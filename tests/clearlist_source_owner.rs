use tradelayer_core::clearlist_source_owner::{ClearlistDbFailure, ClearlistSourceOwner, METHODS};
use tradelayer_core::validity_source::{
    DependencyCall, DependencyReturn, SourceBackend, SourceCall, SourceException, SourceOutcome,
    SourceRuleError, SourceValue as V,
};

fn s(text: &str) -> V {
    V::String(text.into())
}
fn n(number: f64) -> V {
    V::number(number)
}
fn obj(fields: &[(&str, V)]) -> V {
    V::Object(
        fields
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
    )
}
fn text(value: &V) -> String {
    if let V::String(s) = value {
        s.clone()
    } else {
        panic!("expected string: {value:?}")
    }
}
fn array(value: &V) -> Vec<V> {
    if let V::Array(a) = value {
        a.clone()
    } else {
        panic!("expected array: {value:?}")
    }
}
fn evaluate(owner: &mut ClearlistSourceOwner, method: &str, args: Vec<V>) -> SourceOutcome {
    owner
        .evaluate(&SourceCall {
            method: method.into(),
            arguments: args,
        })
        .unwrap()
}
fn returned(outcome: SourceOutcome) -> V {
    match outcome {
        SourceOutcome::Returned(v) => v,
        other => panic!("expected return: {other:?}"),
    }
}
fn outcome_value(outcome: SourceOutcome) -> V {
    match outcome {
        SourceOutcome::Returned(v) => obj(&[("returned", v)]),
        SourceOutcome::Thrown(e) => obj(&[(
            "thrown",
            obj(&[("name", s(&e.name)), ("message", s(&e.message))]),
        )]),
    }
}

// Decode the tagged fixture grammar directly; nesting of its tags is separate
// from bounded SourceValue nesting, so the production JSON limit is unchanged.
struct Diagnostic<'a> {
    s: &'a str,
    at: usize,
}
impl Diagnostic<'_> {
    fn byte(&mut self, b: u8) {
        assert_eq!(self.s.as_bytes().get(self.at), Some(&b));
        self.at += 1;
    }
    fn string(&mut self) -> String {
        let start = self.at;
        self.byte(b'"');
        loop {
            match self.s.as_bytes()[self.at] {
                b'\\' => self.at += 2,
                b'"' => {
                    self.at += 1;
                    break;
                }
                _ => self.at += 1,
            }
        }
        let v =
            tradelayer_core::issuance_intent::JsonValue::parse(&self.s[start..self.at]).unwrap();
        if let tradelayer_core::issuance_intent::JsonValue::String(s) = v {
            s
        } else {
            unreachable!()
        }
    }
    fn value(&mut self, depth: usize) -> V {
        assert!(depth < 64);
        self.byte(b'[');
        let tag = self.string();
        if self.s.as_bytes()[self.at] == b']' {
            self.at += 1;
            return match tag.as_str() {
                "undefined" => V::Undefined,
                "null" => V::Null,
                _ => panic!("unexpected tag {tag}"),
            };
        }
        self.byte(b',');
        let value = match tag.as_str() {
            "string" => V::String(self.string()),
            "number" => {
                let s = self.string();
                V::number(match s.as_str() {
                    "NaN" => f64::NAN,
                    "Infinity" => f64::INFINITY,
                    "-Infinity" => f64::NEG_INFINITY,
                    _ => f64::from_bits(u64::from_str_radix(&s, 16).unwrap()),
                })
            }
            "boolean" => {
                let value = self.s[self.at..].starts_with("true");
                self.at += if value { 4 } else { 5 };
                V::Bool(value)
            }
            "array" => {
                self.byte(b'[');
                let mut values = vec![];
                while self.s.as_bytes()[self.at] != b']' {
                    if !values.is_empty() {
                        self.byte(b',');
                    }
                    values.push(self.value(depth + 1));
                }
                self.byte(b']');
                V::Array(values)
            }
            "object" => {
                self.byte(b'[');
                let mut fields = vec![];
                while self.s.as_bytes()[self.at] != b']' {
                    if !fields.is_empty() {
                        self.byte(b',');
                    }
                    self.byte(b'[');
                    let key = self.string();
                    self.byte(b',');
                    let value = self.value(depth + 1);
                    self.byte(b']');
                    fields.push((key, value));
                }
                self.byte(b']');
                V::Object(fields)
            }
            _ => panic!("unknown tag {tag}"),
        };
        self.byte(b']');
        value
    }
}
fn decode(hex: &str) -> V {
    let bytes = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect::<Vec<_>>();
    let text = std::str::from_utf8(&bytes).unwrap();
    let mut parser = Diagnostic { s: text, at: 0 };
    let v = parser.value(0);
    assert_eq!(parser.at, text.len());
    v
}

#[test]
fn unchanged_full_source_all_methods_results_rows_queries_and_singletons() {
    let mut owner = ClearlistSourceOwner::new();
    let mut count = 0;
    let mut methods = std::collections::BTreeSet::new();
    for line in include_str!("../fixtures/clearlist_source_owner_vectors.tsv").lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let fields = line.split('\t').collect::<Vec<_>>();
        assert_eq!(fields.len(), 6);
        assert_eq!(fields[0], "V");
        let case = fields[1];
        let action = decode(fields[2]);
        let op = text(&action.get("op"));
        let outcome = match op.as_str() {
            "reset" => {
                owner = if action.get("library") == s("bitcoin") {
                    ClearlistSourceOwner::with_source_library(
                        tradelayer_core::bip32_public::SourceLibrary::BitcoinFallback,
                    )
                } else {
                    ClearlistSourceOwner::new()
                };
                SourceOutcome::Returned(V::Undefined)
            }
            "import" => {
                owner
                    .import_rows(&text(&action.get("collection")), array(&action.get("rows")))
                    .unwrap();
                SourceOutcome::Returned(V::Undefined)
            }
            "failure" => {
                let f = action.get("failure");
                let V::Number(skip) = f.get("skip") else {
                    panic!()
                };
                owner
                    .fail_database_call(ClearlistDbFailure {
                        collection: text(&f.get("collection")),
                        operation: text(&f.get("operation")),
                        skip: f64::from_bits(skip) as usize,
                        exception: SourceException {
                            name: text(&f.get("name")),
                            message: text(&f.get("message")),
                        },
                    })
                    .unwrap();
                SourceOutcome::Returned(V::Undefined)
            }
            "restart" => {
                owner = ClearlistSourceOwner::restore_durable(&owner.durable_checkpoint().unwrap())
                    .unwrap();
                SourceOutcome::Returned(V::Undefined)
            }
            "call" => {
                let method = text(&action.get("method"));
                methods.insert(method.clone());
                evaluate(&mut owner, &method, array(&action.get("arguments")))
            }
            _ => panic!("unexpected action {op}"),
        };
        assert_eq!(outcome_value(outcome), decode(fields[3]), "outcome: {case}");
        assert_eq!(owner.state_value(), decode(fields[4]), "state: {case}");
        assert_eq!(
            V::Array(owner.take_db_calls().iter().map(|c| c.value()).collect()),
            decode(fields[5]),
            "adapter calls: {case}"
        );
        let bytes = owner.checkpoint().unwrap();
        let restored = ClearlistSourceOwner::from_checkpoint(&bytes).unwrap();
        assert_eq!(
            restored, owner,
            "protocol-state equality after checkpoint: {case}"
        );
        assert_eq!(
            restored.state_value(),
            owner.state_value(),
            "complete checkpoint: {case}"
        );
        assert_eq!(
            restored.checkpoint().unwrap(),
            bytes,
            "canonical encoding: {case}"
        );
        count += 1;
    }
    assert_eq!(count, 200);
    assert_eq!(methods, METHODS.into_iter().map(str::to_string).collect());
}

#[test]
fn selected_bitcoin_fallback_survives_complete_checkpoint() {
    use tradelayer_core::bip32_public::SourceLibrary;
    let owner = ClearlistSourceOwner::with_source_library(SourceLibrary::BitcoinFallback);
    let restored = ClearlistSourceOwner::from_checkpoint(&owner.checkpoint().unwrap()).unwrap();
    assert_eq!(restored.source_library(), SourceLibrary::BitcoinFallback);
    let restarted = ClearlistSourceOwner::restore_durable_with_source_library(
        &owner.durable_checkpoint().unwrap(),
        SourceLibrary::BitcoinFallback,
    )
    .unwrap();
    assert_eq!(restarted.source_library(), SourceLibrary::BitcoinFallback);
    assert_eq!(
        ClearlistSourceOwner::restore_durable(&owner.durable_checkpoint().unwrap())
            .unwrap()
            .source_library(),
        SourceLibrary::Litecoin
    );
}

#[test]
fn full_checkpoint_continuation_and_db_restart_have_distinct_source_behavior() {
    let mut owner = ClearlistSourceOwner::new();
    returned(evaluate(
        &mut owner,
        "createClearlist",
        vec![s("a"), s("name"), V::Null, s("d"), s("b"), n(1.0)],
    ));
    returned(evaluate(&mut owner, "loadClearlists", vec![]));
    returned(evaluate(
        &mut owner,
        "updateAdmin",
        vec![n(1.0), s("new"), V::Bool(false)],
    ));
    let checkpoint = owner.checkpoint().unwrap();
    let mut restored = ClearlistSourceOwner::from_checkpoint(&checkpoint).unwrap();
    assert_eq!(
        returned(evaluate(
            &mut restored,
            "verifyAdmin",
            vec![n(1.0), s("new")]
        )),
        V::Bool(true)
    );
    let mut restarted =
        ClearlistSourceOwner::restore_durable(&owner.durable_checkpoint().unwrap()).unwrap();
    assert!(matches!(
        evaluate(&mut restarted, "verifyAdmin", vec![n(1.0), s("new")]),
        SourceOutcome::Thrown(_)
    ));
    assert_eq!(
        returned(evaluate(&mut restarted, "getClearlistById", vec![n(1.0)])).get("admin"),
        s("a")
    );
    assert!(ClearlistSourceOwner::restore_durable(&checkpoint).is_err());
    let mut corrupted = checkpoint.clone();
    let last = corrupted.len() - 1;
    corrupted[last] ^= 1;
    assert!(ClearlistSourceOwner::from_checkpoint(&corrupted).is_err());
    let mut trailing = checkpoint;
    trailing.push(0);
    assert!(ClearlistSourceOwner::from_checkpoint(&trailing).is_err());
}

#[test]
fn backend_returns_actual_owned_rows_and_undefined_source_getter() {
    let mut owner = ClearlistSourceOwner::new();
    returned(evaluate(
        &mut owner,
        "addAttestation",
        vec![n(0.0), s("alice"), obj(&[("country", s(" ca "))]), n(10.0)],
    ));
    let country = owner
        .invoke(&DependencyCall {
            target: "ClearList.getCountryCodeByAddress".into(),
            arguments: vec![s("alice")],
        })
        .unwrap();
    assert_eq!(
        country,
        DependencyReturn::Pending(obj(&[
            ("address", s("alice")),
            ("countryCode", s("CA")),
            ("blockHeight", n(10.0))
        ]))
    );
    returned(evaluate(
        &mut owner,
        "setBanlist",
        vec![V::Array(vec![s("CA")]), n(11.0)],
    ));
    assert_eq!(
        owner
            .invoke(&DependencyCall {
                target: "ClearList.getBanlist".into(),
                arguments: vec![]
            })
            .unwrap(),
        DependencyReturn::Pending(V::Undefined)
    );
    assert_eq!(
        owner.rows("clearlists").unwrap()[0].get("data"),
        V::Array(vec![s("CA")])
    );
    assert_eq!(
        owner
            .invoke(&DependencyCall {
                target: "ClearList.isAddressInClearlist".into(),
                arguments: vec![n(0.0), s("alice")]
            })
            .unwrap(),
        DependencyReturn::Pending(V::Bool(true))
    );
    assert_eq!(
        owner.invoke(&DependencyCall {
            target: "fabricated.answer".into(),
            arguments: vec![]
        }),
        Err(SourceRuleError::UnknownDependency)
    );
}

#[test]
fn replay_from_complete_checkpoint_preserves_generic_rows_and_source_effects() {
    let mut owner = ClearlistSourceOwner::new();
    let arbitrary = obj(&[(
        "value",
        V::Array(vec![
            V::Undefined,
            V::Null,
            V::number(f64::NAN),
            V::number(-0.0),
            obj(&[("nested", s("東"))]),
        ]),
    )]);
    returned(evaluate(
        &mut owner,
        "addAttestation",
        vec![s("0"), s("alice"), arbitrary, n(0.0)],
    ));
    let snapshot = owner.checkpoint().unwrap();
    let mut other = ClearlistSourceOwner::from_checkpoint(&snapshot).unwrap();
    let calls = vec![
        SourceCall {
            method: "revokeAttestation".into(),
            arguments: vec![n(999.0), s("alice"), V::Undefined, n(42.0)],
        },
        SourceCall {
            method: "getAttestations".into(),
            arguments: vec![s("0")],
        },
    ];
    assert_eq!(owner.replay(&calls).unwrap(), other.replay(&calls).unwrap());
    assert_eq!(owner.checkpoint().unwrap(), other.checkpoint().unwrap());
    assert_eq!(
        owner.rows("attestations").unwrap()[0]
            .get("data")
            .get("revokeReason"),
        V::Undefined
    );
}

#[test]
fn private_key_and_reference_identity_domains_fail_explicitly() {
    let mut owner = ClearlistSourceOwner::new();
    assert_eq!(owner.evaluate(&SourceCall{method:"deriveAddressesFromXpub".into(),arguments:vec![s("xprv9s21ZrQH143K3QTDL4LXw2F7HEK3wJUD2nW2nRk4stbPy6cq3jPPqjiChkVvvNKmPGJxWUtg6LnF5kejMRNNU3TGtRBeJgk33yuGBxrMPHi")]}),Err(SourceRuleError::UnsupportedNumericDomain));
    // Object-reference Map keys cannot be encoded as SourceValue identity.
    assert_eq!(
        owner.evaluate(&SourceCall {
            method: "getClearlistById".into(),
            arguments: vec![obj(&[("id", n(1.0))])]
        }),
        Err(SourceRuleError::InvalidInput)
    );
    assert_eq!(
        owner.import_rows(
            "attestations",
            vec![
                obj(&[("_id", s("a")), ("data", obj(&[]))]),
                obj(&[("_id", s("a")), ("data", obj(&[]))])
            ]
        ),
        Err(SourceRuleError::InvalidInput)
    );
    let before = owner.checkpoint().unwrap();
    let metadata = V::Array((0..1023).map(|_| V::Array(vec![V::Null; 7])).collect());
    metadata.validate().unwrap();
    assert_eq!(
        owner.evaluate(&SourceCall {
            method: "addAttestation".into(),
            arguments: vec![n(1.0), s("oversized-row"), metadata, n(1.0)]
        }),
        Err(SourceRuleError::Capacity)
    );
    assert_eq!(
        owner.checkpoint().unwrap(),
        before,
        "a local capacity error leaves a valid owner"
    );
    owner
        .import_rows(
            "attestations",
            (0..65)
                .map(|i| {
                    obj(&[
                        ("_id", s(&format!("row-{i}"))),
                        (
                            "data",
                            obj(&[
                                ("address", s("alice")),
                                ("listId", n(1.0)),
                                (
                                    "timestamp",
                                    if i % 3 == 0 {
                                        V::Undefined
                                    } else {
                                        n(i as f64)
                                    },
                                ),
                            ]),
                        ),
                    ])
                })
                .collect(),
        )
        .unwrap();
    let before = owner.checkpoint().unwrap();
    assert_eq!(
        owner.evaluate(&SourceCall {
            method: "getAttestationHistory".into(),
            arguments: vec![s("alice"), n(1.0)]
        }),
        Err(SourceRuleError::UnsupportedNumericDomain)
    );
    assert_eq!(owner.checkpoint().unwrap(), before);
}
