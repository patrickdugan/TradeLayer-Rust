use tradelayer_core::oracle_source_owner::{OracleDbFailure, OracleSourceOwner, METHODS};
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
fn evaluate(owner: &mut OracleSourceOwner, method: &str, args: Vec<V>) -> SourceOutcome {
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

#[derive(Default)]
struct InsuranceBoundary {
    reject: bool,
}
impl SourceBackend for InsuranceBoundary {
    fn invoke(&mut self, call: &DependencyCall) -> Result<DependencyReturn, SourceRuleError> {
        assert_eq!(call.target, "insurance.liquidate");
        assert_eq!(call.arguments.len(), 2);
        assert_eq!(call.arguments[1], V::Bool(true));
        Ok(if self.reject {
            DependencyReturn::PendingThrow(SourceException {
                name: "Error".into(),
                message: "injected insurance rejection".into(),
            })
        } else {
            DependencyReturn::Pending(V::Undefined)
        })
    }
}
#[test]
fn unchanged_full_source_all_methods_rows_results_queries_and_singletons() {
    let mut owner = OracleSourceOwner::new();
    let mut backend = InsuranceBoundary::default();
    let mut count = 0;
    let mut methods = std::collections::BTreeSet::new();
    for line in include_str!("../fixtures/oracle_source_owner_vectors.tsv").lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let f = line.split('\t').collect::<Vec<_>>();
        assert_eq!(f.len(), 7);
        assert_eq!(f[0], "V");
        let case = f[1];
        let action = decode(f[2]);
        let op = text(&action.get("op"));
        let outcome = match op.as_str() {
            "reset" => {
                owner = OracleSourceOwner::new();
                backend = InsuranceBoundary::default();
                SourceOutcome::Returned(V::Undefined)
            }
            "import" => {
                owner
                    .import_rows(&text(&action.get("collection")), array(&action.get("rows")))
                    .unwrap();
                SourceOutcome::Returned(V::Undefined)
            }
            "failure" => {
                let failure = action.get("failure");
                let V::Number(skip) = failure.get("skip") else {
                    panic!()
                };
                owner
                    .fail_database_call(OracleDbFailure {
                        collection: text(&failure.get("collection")),
                        operation: text(&failure.get("operation")),
                        skip: f64::from_bits(skip) as usize,
                        exception: SourceException {
                            name: text(&failure.get("name")),
                            message: text(&failure.get("message")),
                        },
                    })
                    .unwrap();
                SourceOutcome::Returned(V::Undefined)
            }
            "insurance" => {
                backend.reject = action.get("mode") == s("throw");
                SourceOutcome::Returned(V::Undefined)
            }
            "restart" => {
                owner = OracleSourceOwner::restore_durable(&owner.durable_checkpoint().unwrap())
                    .unwrap();
                backend = InsuranceBoundary::default();
                SourceOutcome::Returned(V::Undefined)
            }
            "call" => {
                let method = text(&action.get("method"));
                methods.insert(method.clone());
                owner
                    .evaluate_with_backend(
                        &SourceCall {
                            method,
                            arguments: array(&action.get("arguments")),
                        },
                        &mut backend,
                    )
                    .unwrap_or_else(|e| panic!("{case}: {e:?}"))
            }
            _ => panic!("unknown action {op}"),
        };
        assert_eq!(outcome_value(outcome), decode(f[3]), "outcome: {case}");
        assert_eq!(owner.state_value(), decode(f[4]), "state: {case}");
        assert_eq!(
            V::Array(owner.take_db_calls().iter().map(|c| c.value()).collect()),
            decode(f[5]),
            "DB calls: {case}"
        );
        assert_eq!(
            V::Array(
                owner
                    .take_dependency_calls()
                    .into_iter()
                    .map(|c| obj(&[
                        ("target", s(&c.target)),
                        ("arguments", V::Array(c.arguments))
                    ]))
                    .collect()
            ),
            decode(f[6]),
            "dependencies: {case}"
        );
        let checkpoint = owner.checkpoint().unwrap();
        let restored = OracleSourceOwner::from_checkpoint(&checkpoint).unwrap();
        assert_eq!(restored, owner, "complete owner equality: {case}");
        assert_eq!(
            restored.state_value(),
            owner.state_value(),
            "source state: {case}"
        );
        assert_eq!(
            restored.encode().unwrap(),
            checkpoint,
            "canonical bytes: {case}"
        );
        owner = restored;
        count += 1;
    }
    assert_eq!(count, 311);
    assert_eq!(methods, METHODS.into_iter().map(str::to_string).collect());
}

#[test]
fn complete_continuation_and_process_restart_keep_distinct_lifecycles() {
    let mut owner = OracleSourceOwner::new();
    returned(evaluate(
        &mut owner,
        "createOracle",
        vec![s("feed"), s("admin")],
    ));
    returned(evaluate(
        &mut owner,
        "publishData",
        vec![n(1.0), s("100"), n(120.0), n(80.0), n(99.0), n(10.0)],
    ));
    let mut continued = OracleSourceOwner::decode(&owner.encode().unwrap()).unwrap();
    let mut restarted =
        OracleSourceOwner::restore_durable(&owner.durable_checkpoint().unwrap()).unwrap();
    let call = SourceCall {
        method: "publishData".into(),
        arguments: vec![
            n(1.0),
            s("200"),
            V::Undefined,
            V::Undefined,
            V::Undefined,
            n(10.0),
        ],
    };
    assert_eq!(owner.evaluate(&call), continued.evaluate(&call));
    assert_eq!(owner, continued);
    returned(restarted.evaluate(&call).unwrap());
    assert_eq!(
        returned(evaluate(&mut owner, "getOraclePrice", vec![n(1.0)])),
        s("100")
    );
    assert_eq!(
        returned(evaluate(&mut restarted, "getOraclePrice", vec![n(1.0)])),
        n(105.0)
    );
    returned(evaluate(&mut owner, "getAllOracles", vec![]));
    let info = returned(evaluate(&mut owner, "getOracleInfo", vec![n(1.0)]));
    assert_eq!(info.get("data"), obj(&[]));
    assert_eq!(info.get("lastPublishedBlock"), V::Undefined);
}

#[test]
fn closure_forwards_exact_buggy_arguments_and_retains_partial_effects() {
    let mut owner = OracleSourceOwner::new();
    returned(evaluate(
        &mut owner,
        "createOracle",
        vec![s("feed"), s("a")],
    ));
    let error = owner
        .evaluate(&SourceCall {
            method: "closeOracle".into(),
            arguments: vec![n(1.0)],
        })
        .unwrap_err();
    assert_eq!(error, SourceRuleError::UnknownDependency);
    assert_eq!(
        owner.rows("oracleList").unwrap()[0].get("closed"),
        V::Bool(true)
    );
    assert_eq!(
        owner.take_dependency_calls(),
        vec![DependencyCall {
            target: "insurance.liquidate".into(),
            arguments: vec![s("a"), V::Bool(true)]
        }]
    );
    let mut backend = InsuranceBoundary { reject: true };
    assert!(matches!(
        owner
            .invoke_source_with_backend("closeOracle", &[n(1.0)], &mut backend)
            .unwrap(),
        DependencyReturn::PendingThrow(_)
    ));
    assert_eq!(
        owner.rows("oracleList").unwrap()[0].get("closed"),
        V::Bool(true)
    );
}

#[test]
fn complete_checkpoints_are_canonical_bounded_and_corruption_detecting() {
    let mut owner = OracleSourceOwner::new();
    owner
        .import_rows(
            "oracleList",
            vec![obj(&[
                ("_id", s("oracle-1")),
                ("id", n(1.0)),
                (
                    "name",
                    obj(&[("type", s("full")), ("whitelists", V::Array(vec![n(9.0)]))]),
                ),
                ("unknown", V::Undefined),
            ])],
        )
        .unwrap();
    returned(evaluate(&mut owner, "load", vec![]));
    owner
        .fail_database_call(OracleDbFailure {
            collection: "oracleData".into(),
            operation: "findAsync".into(),
            skip: 2,
            exception: SourceException {
                name: "Error".into(),
                message: "queued".into(),
            },
        })
        .unwrap();
    let bytes = owner.encode().unwrap();
    assert_eq!(OracleSourceOwner::decode(&bytes).unwrap(), owner);
    for at in 0..bytes.len() {
        let mut bad = bytes.clone();
        bad[at] ^= 1;
        assert!(
            OracleSourceOwner::decode(&bad).is_err(),
            "corrupted byte {at}"
        );
    }
    for len in 0..bytes.len() {
        assert!(OracleSourceOwner::decode(&bytes[..len]).is_err());
    }
    let mut extra = bytes.clone();
    extra.push(0);
    assert!(OracleSourceOwner::decode(&extra).is_err());
    assert!(OracleSourceOwner::restore_durable(&bytes).is_err());
    assert!(OracleSourceOwner::decode(&owner.durable_checkpoint().unwrap()).is_err());
    let before = owner.clone();
    assert!(owner
        .import_rows(
            "oracleList",
            vec![obj(&[("_id", s("same"))]), obj(&[("_id", s("same"))])]
        )
        .is_err());
    assert_eq!(owner, before);
    assert_eq!(
        owner.evaluate(&SourceCall {
            method: "close".into(),
            arguments: vec![]
        }),
        Err(SourceRuleError::UnknownMethod)
    );
    assert_eq!(owner, before);
    assert_eq!(
        owner
            .invoke(&DependencyCall {
                target: "Oracle.getOraclePrice".into(),
                arguments: vec![n(99.0)]
            })
            .unwrap(),
        DependencyReturn::Pending(n(1.0))
    );
}

fn with_crc(mut bytes: Vec<u8>) -> Vec<u8> {
    let mut crc = 0xffff_ffffu32;
    for byte in &bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ if crc & 1 != 0 { 0xedb8_8320 } else { 0 };
        }
    }
    bytes.extend_from_slice(&(!crc).to_be_bytes());
    bytes
}
#[test]
fn well_checksummed_hostile_checkpoint_sections_are_rejected() {
    let empty = OracleSourceOwner::new().encode().unwrap();
    assert_eq!(empty.len(), 30);
    let raw = empty[..empty.len() - 4].to_vec();
    let mut extra = raw.clone();
    extra.push(0);
    assert!(OracleSourceOwner::decode(&with_crc(extra)).is_err());
    let mut excessive = raw.clone();
    excessive[6..10].copy_from_slice(&1025u32.to_be_bytes());
    assert!(OracleSourceOwner::decode(&with_crc(excessive)).is_err());
    let mut version = raw.clone();
    version[4] = 2;
    assert!(OracleSourceOwner::decode(&with_crc(version)).is_err());
    // Two Map keys equal under SameValueZero, including the -0 alias.
    let mut duplicate = raw[..14].to_vec();
    duplicate.extend_from_slice(&2u32.to_be_bytes());
    for bits in [0.0f64.to_bits(), (-0.0f64).to_bits()] {
        duplicate.push(4);
        duplicate.extend_from_slice(&bits.to_be_bytes());
        duplicate.push(0);
    }
    duplicate.extend_from_slice(&0u32.to_be_bytes());
    duplicate.extend_from_slice(&0u32.to_be_bytes());
    assert!(OracleSourceOwner::decode(&with_crc(duplicate)).is_err());
    // A lone -0 Map key is noncanonical because JS stores it as +0.
    let mut minus_zero = raw[..14].to_vec();
    minus_zero.extend_from_slice(&1u32.to_be_bytes());
    minus_zero.push(4);
    minus_zero.extend_from_slice(&(-0.0f64).to_bits().to_be_bytes());
    minus_zero.push(0);
    minus_zero.extend_from_slice(&0u32.to_be_bytes());
    minus_zero.extend_from_slice(&0u32.to_be_bytes());
    assert!(OracleSourceOwner::decode(&with_crc(minus_zero)).is_err());
    let mut nan = raw[..14].to_vec();
    nan.extend_from_slice(&1u32.to_be_bytes());
    nan.push(4);
    nan.extend_from_slice(&0x7ff8_0000_0000_0001u64.to_be_bytes());
    nan.push(0);
    nan.extend_from_slice(&0u32.to_be_bytes());
    nan.extend_from_slice(&0u32.to_be_bytes());
    assert!(OracleSourceOwner::decode(&with_crc(nan)).is_err());
}

#[test]
fn local_capacity_rejections_restore_owner_and_failure_schedule() {
    use tradelayer_core::oracle_source_owner::MAX_ROWS;
    let mut owner = OracleSourceOwner::new();
    owner
        .import_rows(
            "oracleList",
            (0..MAX_ROWS)
                .map(|i| obj(&[("_id", s(&format!("oracle-{i}"))), ("id", n(i as f64))]))
                .collect(),
        )
        .unwrap();
    owner
        .fail_database_call(OracleDbFailure {
            collection: "oracleList".into(),
            operation: "getDatabase".into(),
            skip: 2,
            exception: SourceException {
                name: "Error".into(),
                message: "still scheduled".into(),
            },
        })
        .unwrap();
    let before = owner.encode().unwrap();
    assert_eq!(
        owner.evaluate(&SourceCall {
            method: "createOracle".into(),
            arguments: vec![s("overflow"), s("a")]
        }),
        Err(SourceRuleError::Capacity)
    );
    assert_eq!(owner.encode().unwrap(), before);
    assert_eq!(
        owner.import_rows("oracleList", vec![obj(&[("_id", s("x"))]); MAX_ROWS + 1]),
        Err(SourceRuleError::Capacity)
    );
    assert_eq!(owner.encode().unwrap(), before);
}

#[test]
fn nested_value_counts_have_symmetric_import_encode_and_decode_limits() {
    use tradelayer_core::oracle_source_owner::MAX_ROWS;
    let mut owner = OracleSourceOwner::new();
    let legal = obj(&[
        ("_id", s("oracle-1")),
        ("extra", V::Array(vec![V::Null; MAX_ROWS])),
    ]);
    owner.import_rows("oracleList", vec![legal]).unwrap();
    assert_eq!(
        OracleSourceOwner::decode(&owner.encode().unwrap()).unwrap(),
        owner
    );
    let legal_fields = (0..MAX_ROWS).map(|i| (format!("f{i}"), V::Null)).collect();
    owner
        .import_rows(
            "oracleList",
            vec![obj(&[
                ("_id", s("oracle-1")),
                ("extra", V::Object(legal_fields)),
            ])],
        )
        .unwrap();
    assert_eq!(
        OracleSourceOwner::decode(&owner.encode().unwrap()).unwrap(),
        owner
    );
    let before = owner.encode().unwrap();
    assert_eq!(
        owner.import_rows(
            "oracleList",
            vec![obj(&[
                ("_id", s("oracle-1")),
                ("extra", V::Array(vec![V::Null; MAX_ROWS + 1]))
            ])]
        ),
        Err(SourceRuleError::Capacity)
    );
    assert_eq!(owner.encode().unwrap(), before);
    let illegal_fields = (0..=MAX_ROWS).map(|i| (format!("f{i}"), V::Null)).collect();
    assert_eq!(
        owner.import_rows(
            "oracleList",
            vec![obj(&[
                ("_id", s("oracle-1")),
                ("extra", V::Object(illegal_fields))
            ])]
        ),
        Err(SourceRuleError::Capacity)
    );
    assert_eq!(owner.encode().unwrap(), before);
}

#[test]
fn aggregate_value_budget_rejects_forged_children_during_parsing() {
    fn string(bytes: &mut Vec<u8>, text: &str) {
        bytes.push(5);
        bytes.extend_from_slice(&(text.len() as u32).to_be_bytes());
        bytes.extend_from_slice(text.as_bytes());
    }
    // Individually valid 1024-null child arrays would otherwise all be built
    // before a final aggregate SourceValue validation notices the node limit.
    let mut raw = b"TLOS\x01\x01".to_vec();
    raw.extend_from_slice(&1u32.to_be_bytes());
    raw.push(7);
    raw.extend_from_slice(&2u32.to_be_bytes());
    raw.extend_from_slice(&3u32.to_be_bytes());
    raw.extend_from_slice(b"_id");
    string(&mut raw, "oracle-1");
    raw.extend_from_slice(&5u32.to_be_bytes());
    raw.extend_from_slice(b"extra");
    raw.push(6);
    raw.extend_from_slice(&9u32.to_be_bytes());
    for _ in 0..9 {
        raw.push(6);
        raw.extend_from_slice(&1024u32.to_be_bytes());
        raw.extend(std::iter::repeat_n(1u8, 1024));
    }
    for _ in 0..4 {
        raw.extend_from_slice(&0u32.to_be_bytes());
    }
    assert_eq!(
        OracleSourceOwner::decode(&with_crc(raw)).unwrap_err(),
        SourceRuleError::Capacity
    );
}
