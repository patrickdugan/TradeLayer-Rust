use tradelayer_core::activation::{
    ActivationCall, ActivationObservation, RepairedActivationIntent, SourceActivation,
    SourceActivationSingleton, GENESIS_ADMIN, MAX_ACTIONS, METHODS, REGISTRY_NAMES,
};

use tradelayer_core::validity_source::{
    DependencyCall, DependencyReturn, SourceBackend, SourceException, SourceOutcome,
    SourceRuleError, SourceValue as V,
};
const FIXTURE: &str = include_str!("../fixtures/activation_vectors.tsv");
fn decode(s: &str) -> V {
    let bytes = s
        .as_bytes()
        .chunks_exact(2)
        .map(|b| u8::from_str_radix(core::str::from_utf8(b).unwrap(), 16).unwrap())
        .collect::<Vec<_>>();
    V::from_diagnostic_json(core::str::from_utf8(&bytes).unwrap())
        .unwrap_or_else(|e| panic!("decode {e:?}: {}", String::from_utf8_lossy(&bytes)))
}
fn list(v: V) -> Vec<V> {
    if let V::Array(a) = v {
        a
    } else {
        panic!("expected array {v:?}")
    }
}
fn string(v: V) -> String {
    if let V::String(s) = v {
        s
    } else {
        panic!("expected string {v:?}")
    }
}
fn object(a: Vec<(&str, V)>) -> V {
    V::Object(a.into_iter().map(|(k, v)| (k.into(), v)).collect())
}

struct Snapshot {
    entries: Vec<V>,
    overrides: V,
    defaults: V,
    calls: usize,
}
impl Snapshot {
    fn new(initial: V) -> Self {
        Self {
            entries: list(initial),
            overrides: V::object(),
            defaults: decode(
                FIXTURE
                    .lines()
                    .find_map(|l| l.strip_prefix("# defaults "))
                    .unwrap(),
            ),
            calls: 0,
        }
    }
}
impl SourceBackend for Snapshot {
    fn invoke(&mut self, call: &DependencyCall) -> Result<DependencyReturn, SourceRuleError> {
        self.calls += 1;
        let override_value = if let V::Object(a) = &self.overrides {
            a.iter()
                .find(|(k, _)| k == &call.target)
                .map(|(_, v)| v.clone())
        } else {
            None
        };
        let value = override_value.unwrap_or_else(|| {
            if call.target == "DB.findAsync" {
                V::Array(self.entries.clone())
            } else {
                self.defaults.get(&call.target)
            }
        });
        let (mode, value) = if let V::String(mode) = value.get("$mode") {
            (mode, value.get("value"))
        } else {
            (
                if call.target == "Timer.setTimeout" {
                    "immediate"
                } else {
                    "pending"
                }
                .into(),
                value.clone(),
            )
        };
        if mode == "throw" || mode == "pending-throw" {
            let descriptor = self.overrides.get(&call.target);
            if call.target == "DB.updateAsync"
                && descriptor.get("$commitBeforeThrow") == V::Bool(true)
            {
                let value = call.arguments[1].get("$set").get("value");
                if let Some(row) = self
                    .entries
                    .iter_mut()
                    .find(|v| v.get("_id") == V::String("activationsList".into()))
                {
                    row.insert("value", value)?;
                } else {
                    self.entries.push(object(vec![
                        ("_id", V::String("activationsList".into())),
                        ("value", value),
                    ]));
                }
            }
            let error = SourceException {
                name: string(descriptor.get("name")),
                message: string(descriptor.get("message")),
            };
            return Ok(if mode == "throw" {
                DependencyReturn::Throw(error)
            } else {
                DependencyReturn::PendingThrow(error)
            });
        }
        if call.target == "DB.updateAsync" {
            let value = call.arguments[1].get("$set").get("value");
            if let Some(row) = self
                .entries
                .iter_mut()
                .find(|v| v.get("_id") == V::String("activationsList".into()))
            {
                row.insert("value", value)?;
            } else {
                self.entries.push(object(vec![
                    ("_id", V::String("activationsList".into())),
                    ("value", value),
                ]));
            }
        }
        Ok(match mode.as_str() {
            "immediate" => DependencyReturn::Immediate(value),
            "pending" => DependencyReturn::Pending(value),
            _ => return Err(SourceRuleError::InvalidInput),
        })
    }
}
fn observation(e: &ActivationObservation) -> V {
    let outcome = match &e.outcome {
        SourceOutcome::Returned(value) => object(vec![
            ("status", V::String("returned".into())),
            ("value", value.clone()),
        ]),
        SourceOutcome::Thrown(e) => object(vec![
            ("status", V::String("thrown".into())),
            ("name", V::String(e.name.clone())),
            ("message", V::String(e.message.clone())),
        ]),
    };
    let dependencies = e
        .dependencies
        .iter()
        .map(|o| {
            let (mode, value) = match &o.returned {
                DependencyReturn::Immediate(v) => ("immediate", v.clone()),
                DependencyReturn::Pending(v) => ("pending", v.clone()),
                DependencyReturn::Throw(e) => (
                    "throw",
                    V::Array(vec![
                        V::String(e.name.clone()),
                        V::String(e.message.clone()),
                    ]),
                ),
                DependencyReturn::PendingThrow(e) => (
                    "pending-throw",
                    V::Array(vec![
                        V::String(e.name.clone()),
                        V::String(e.message.clone()),
                    ]),
                ),
            };
            V::Array(vec![
                V::String(o.call.target.clone()),
                V::Array(o.call.arguments.clone()),
                V::String(mode.into()),
                value,
            ])
        })
        .collect();
    object(vec![
        ("outcome", outcome),
        ("state", e.state_after.diagnostic_value()),
        ("dependencies", V::Array(dependencies)),
        (
            "swallowed",
            V::Array(
                e.swallowed
                    .iter()
                    .map(|(stage, name)| {
                        V::Array(vec![V::String(stage.clone()), V::String(name.clone())])
                    })
                    .collect(),
            ),
        ),
        (
            "deferred",
            V::Array(
                e.deferred_rejections
                    .iter()
                    .map(|e| {
                        object(vec![
                            ("name", V::String(e.name.clone())),
                            ("message", V::String(e.message.clone())),
                        ])
                    })
                    .collect(),
            ),
        ),
        ("arrayProperties", e.state_after.array_properties_value()),
    ])
}
#[test]
fn complete_unmodified_source_methods_match_results_partial_state_and_ordered_dependencies() {
    let mut source = SourceActivation::new();
    let mut backend = Snapshot::new(V::Array(Vec::new()));
    let mut count = 0;
    let mut scenes = 0;
    let mut methods = std::collections::BTreeSet::new();
    for line in FIXTURE
        .lines()
        .filter(|l| l.starts_with("S\t") || l.starts_with("A\t"))
    {
        let f = line.split('\t').collect::<Vec<_>>();
        match f[0] {
            "S" => {
                source = SourceActivation::new();
                backend = Snapshot::new(decode(f[2]));
                scenes += 1;
            }
            "A" => {
                let action = decode(f[2]);
                let method = string(action.get("method"));
                methods.insert(method.clone());
                backend.overrides = action.get("overrides");
                let result = source
                    .execute_unverified(
                        ActivationCall {
                            method,
                            arguments: list(action.get("args")),
                        },
                        &mut backend,
                    )
                    .unwrap_or_else(|e| panic!("{} local error {e:?}", f[1]));
                assert_eq!(observation(&result), decode(f[3]), "{}", f[1]);
                assert_eq!(
                    result.require_authenticated_admission(),
                    Err(SourceRuleError::UnauthenticatedInputAndState)
                );
                count += 1;
            }
            _ => panic!("invalid row"),
        }
    }
    assert_eq!(scenes, 259);
    assert_eq!(count, 670);
    assert_eq!(methods, METHODS.into_iter().map(str::to_owned).collect());
    assert_eq!(
        decode(
            FIXTURE
                .lines()
                .find_map(|l| l.strip_prefix("# names "))
                .unwrap()
        ),
        V::Array(
            REGISTRY_NAMES
                .into_iter()
                .map(|s| V::String(s.into()))
                .collect()
        )
    );
}
#[test]
fn singleton_ignores_constructor_chain_and_reuses_mutated_metadata() {
    let mut singleton = SourceActivationSingleton::default();
    let mut backend = Snapshot::new(V::Array(Vec::new()));
    let initial = singleton.construct(V::String("BTC".into()));
    assert_eq!(initial.state().chain, None);
    initial
        .execute_unverified(
            ActivationCall {
                method: "init".into(),
                arguments: vec![V::String("BTC".into())],
            },
            &mut backend,
        )
        .unwrap();
    let again = singleton.get_instance(V::String("DOGE".into()));
    assert_eq!(again.action_count(), 1);
    assert_eq!(
        again.state().admin_address,
        Some(V::String("MTmoypkhRQoJ172ZqxcsVumPZfJ8KCrQCB".into()))
    );
    assert_ne!(
        again.state().admin_address,
        Some(V::String(GENESIS_ADMIN.into()))
    );
}
#[test]
fn bounded_repaired_intents_are_metadata_and_do_not_grant_native_genesis() {
    let p = RepairedActivationIntent::new(vec![0, 2, 18], 0, Some([3; 32])).unwrap();
    assert!(p.requires_native_genesis_policy());
    assert_eq!(p.block, 0);
    for types in [vec![], vec![36], vec![2, 2], vec![255]] {
        assert_eq!(
            RepairedActivationIntent::new(types, 1, None),
            Err(SourceRuleError::InvalidInput)
        );
    }
    assert_eq!(
        RepairedActivationIntent::new(vec![2], 9_007_199_254_740_992, None),
        Err(SourceRuleError::InvalidInput)
    );
    assert!(!RepairedActivationIntent::new(vec![35], 1, None)
        .unwrap()
        .requires_native_genesis_policy());
}
#[test]
fn local_capacity_failures_are_preflighted_and_metadata_atomic() {
    let mut source = SourceActivation::new();
    let mut backend = Snapshot::new(V::Array(Vec::new()));
    let before = source.clone();
    let call = |method: &str, arguments| ActivationCall {
        method: method.into(),
        arguments,
    };
    assert_eq!(
        source.execute_unverified(call("noSuchMethod", vec![]), &mut backend),
        Err(SourceRuleError::UnknownMethod)
    );
    assert_eq!(
        source.execute_unverified(call("activate", vec![V::Undefined; 9]), &mut backend),
        Err(SourceRuleError::Capacity)
    );
    assert_eq!(
        source.execute_unverified(
            call("init", vec![V::String("x".repeat(1_048_577))]),
            &mut backend
        ),
        Err(SourceRuleError::Capacity)
    );
    assert_eq!(source, before);
    assert_eq!(backend.calls, 0);
    for _ in 0..MAX_ACTIONS {
        source
            .execute_unverified(call("getNextTxTypeId", vec![]), &mut backend)
            .unwrap();
    }
    let before = source.clone();
    assert_eq!(
        source.execute_unverified(call("saveActivationsList", vec![]), &mut backend),
        Err(SourceRuleError::Capacity)
    );
    assert_eq!(source, before);
    assert_eq!(backend.calls, 0);
}
#[test]
fn unsupported_prototype_and_surrogate_domains_remain_explicit() {
    let mut source = SourceActivation::new();
    let mut backend = Snapshot::new(V::Array(Vec::new()));
    let before = source.clone();
    for json in ["{\"__proto__\":{}}", "{\"toString\":0}", "\"\\ud800\""] {
        assert_eq!(
            source.execute_unverified(
                ActivationCall {
                    method: "isValidJSON".into(),
                    arguments: vec![V::String(json.into())]
                },
                &mut backend
            ),
            Err(SourceRuleError::InvalidInput)
        );
        assert_eq!(source, before);
    }
    let r = source
        .execute_unverified(
            ActivationCall {
                method: "isValidJSON".into(),
                arguments: vec![V::String("{\"same\":1,\"same\":2}".into())],
            },
            &mut backend,
        )
        .unwrap();
    assert_eq!(r.outcome, SourceOutcome::Returned(V::Bool(true)));
}

#[test]
fn genuine_out_of_domain_observations_are_retained_without_false_parity() {
    let mut count = 0;
    for line in FIXTURE.lines().filter(|l| l.starts_with("Q\t")) {
        let f = line.split('\t').collect::<Vec<_>>();
        let action = decode(f[2]);
        let mut source = SourceActivation::new();
        let before = source.clone();
        let mut backend = Snapshot::new(V::Array(Vec::new()));
        let expected = match f[3] {
            "InvalidInput" => SourceRuleError::InvalidInput,
            "UnsupportedNumericDomain" => SourceRuleError::UnsupportedNumericDomain,
            _ => panic!("unrecognized domain"),
        };
        assert_eq!(
            source.execute_unverified(
                ActivationCall {
                    method: string(action.get("method")),
                    arguments: list(action.get("args"))
                },
                &mut backend
            ),
            Err(expected),
            "{}",
            f[1]
        );
        assert_eq!(source, before);
        assert_eq!(backend.calls, 0);
        let actual_source = decode(f[4]);
        assert_eq!(actual_source.get("status"), V::String("returned".into()));
        assert_eq!(
            actual_source.get("value"),
            match f[1] {
                "own-toString-typeerror" => V::Bool(false),
                "over128bit-hex-parseint" => V::Undefined,
                _ => V::Bool(true),
            }
        );
        count += 1;
    }
    assert_eq!(count, 5);
}

#[test]
fn complete_real_db_source_has_the_activation_category_and_async_methods() {
    let facts = decode(
        FIXTURE
            .lines()
            .find_map(|l| l.strip_prefix("# real-db "))
            .unwrap(),
    );
    assert_eq!(facts.get("categoryPresent"), V::Bool(true));
    assert_eq!(facts.get("findAsync"), V::String("function".into()));
    assert_eq!(facts.get("updateAsync"), V::String("function".into()));
    assert_eq!(facts.get("missingCategory"), V::Undefined);
    assert_eq!(facts.get("initialized"), V::Bool(true));
    assert_eq!(facts.get("test"), V::Bool(false));
    let categories = list(facts.get("categories"));
    assert_eq!(
        categories
            .iter()
            .filter(|v| **v == V::String("activations".into()))
            .count(),
        1
    );
}

#[test]
fn unavailable_backend_is_not_a_swallowed_source_error_or_false_activation() {
    struct Unavailable {
        calls: usize,
    }
    impl SourceBackend for Unavailable {
        fn invoke(&mut self, c: &DependencyCall) -> Result<DependencyReturn, SourceRuleError> {
            self.calls += 1;
            match c.target.as_str() {
                "Client.getInstance" => Ok(DependencyReturn::Pending(V::Null)),
                "Client.getChain" => Ok(DependencyReturn::Pending(V::String("BTC".into()))),
                _ => Err(SourceRuleError::UnknownDependency),
            }
        }
    }
    let mut backend = Unavailable { calls: 0 };
    let mut source = SourceActivation::new();
    let before = source.clone();
    assert_eq!(
        source.execute_unverified(
            ActivationCall {
                method: "init".into(),
                arguments: Vec::new()
            },
            &mut backend
        ),
        Err(SourceRuleError::UnknownDependency)
    );
    assert_eq!(backend.calls, 3);
    assert_eq!(source, before);
}
