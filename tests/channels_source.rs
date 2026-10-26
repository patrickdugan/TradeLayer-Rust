use std::collections::BTreeSet;
use tradelayer_core::channels_source::{ChannelsState, SourceChannels, METHODS};
use tradelayer_core::validity_source::{
    DependencyCall, DependencyReturn, SourceBackend, SourceCall, SourceException, SourceRuleError,
    SourceValue as V,
};

fn string(v: &V) -> String {
    if let V::String(s) = v {
        s.clone()
    } else {
        panic!("expected string: {v:?}")
    }
}
fn list(v: &V) -> Vec<V> {
    if let V::Array(s) = v {
        s.clone()
    } else {
        panic!("expected array: {v:?}")
    }
}
fn decode(hex: &str) -> V {
    decode_with_domain(hex, false)
}
fn decode_with_domain(hex: &str, unsupported_allowed: bool) -> V {
    let bytes = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect::<Vec<_>>();
    let s = std::str::from_utf8(&bytes).unwrap();
    let mut p = Diagnostic {
        s,
        at: 0,
        unsupported_allowed,
    };
    let v = p.value(0);
    assert_eq!(p.at, s.len());
    v
}
// The corpus wraps every logical value in a tagged JSON array. Decode that
// grammar directly rather than broadening the production 16-deep JSON parser.
struct Diagnostic<'a> {
    s: &'a str,
    at: usize,
    unsupported_allowed: bool,
}
impl Diagnostic<'_> {
    fn byte(&mut self, b: u8) {
        assert_eq!(self.s.as_bytes().get(self.at), Some(&b));
        self.at += 1
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
                "null" => V::Null,
                "undefined" => V::Undefined,
                "promise" => V::Promise,
                _ => panic!("invalid tag"),
            };
        }
        self.byte(b',');
        let value = match tag.as_str() {
            "string" => V::String(self.string()),
            "bignumber" => V::BigNumber(self.string()),
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
                let b = self.s[self.at..].starts_with("true");
                self.at += if b { 4 } else { 5 };
                V::Bool(b)
            }
            "array" | "array-properties" => {
                self.byte(b'[');
                let mut a = vec![];
                while self.s.as_bytes()[self.at] != b']' {
                    if !a.is_empty() {
                        self.byte(b',')
                    }
                    a.push(self.value(depth + 1))
                }
                self.byte(b']');
                if tag == "array-properties" {
                    assert!(
                        self.unsupported_allowed,
                        "unsupported named-array tag in an exact case"
                    );
                    self.byte(b',');
                    self.byte(b'[');
                    let mut first = true;
                    while self.s.as_bytes()[self.at] != b']' {
                        if !first {
                            self.byte(b',')
                        }
                        first = false;
                        self.byte(b'[');
                        let _key = self.string();
                        self.byte(b',');
                        let _value = self.value(depth + 1);
                        self.byte(b']');
                    }
                    self.byte(b']');
                }
                V::Array(a)
            }
            "sparse-array" => {
                assert!(
                    self.unsupported_allowed,
                    "unsupported sparse-array tag in an exact case"
                );
                let dense = self.value(depth + 1);
                self.byte(b',');
                let _own_indices = self.value(depth + 1);
                self.byte(b',');
                let _named = self.value(depth + 1);
                dense
            }
            "object" => {
                self.byte(b'[');
                let mut a = vec![];
                while self.s.as_bytes()[self.at] != b']' {
                    if !a.is_empty() {
                        self.byte(b',')
                    }
                    self.byte(b'[');
                    let key = self.string();
                    self.byte(b',');
                    let v = self.value(depth + 1);
                    self.byte(b']');
                    a.push((key, v))
                }
                self.byte(b']');
                V::Object(a)
            }
            _ => panic!("invalid tag {tag}"),
        };
        self.byte(b']');
        value
    }
}
fn state(v: &V) -> ChannelsState {
    ChannelsState {
        registry: list(&v.get("registry"))
            .iter()
            .map(|v| {
                let p = list(v);
                (p[0].clone(), p[1].clone())
            })
            .collect(),
        pending_withdrawals: list(&v.get("pendingWithdrawals")),
    }
}
#[derive(Default)]
struct Backend {
    expected: Vec<V>,
    cursor: usize,
}
impl SourceBackend for Backend {
    fn invoke(&mut self, call: &DependencyCall) -> Result<DependencyReturn, SourceRuleError> {
        let Some(row) = self.expected.get(self.cursor) else {
            panic!("unexpected dependency {call:?}")
        };
        self.cursor += 1;
        assert_eq!(
            call.target,
            string(&row.get("target")),
            "dependency{}",
            self.cursor
        );
        assert_eq!(
            call.arguments,
            list(&row.get("arguments")),
            "{} arguments",
            call.target
        );
        let answer = row.get("returned");
        let error = || {
            let e = answer.get("error");
            SourceException {
                name: string(&e.get("name")),
                message: string(&e.get("message")),
            }
        };
        Ok(match string(&answer.get("kind")).as_str() {
            "immediate" => DependencyReturn::Immediate(answer.get("value")),
            "pending" => DependencyReturn::Pending(answer.get("value")),
            "throw" => DependencyReturn::Throw(error()),
            "pending-throw" => DependencyReturn::PendingThrow(error()),
            _ => panic!("bad dependency answer"),
        })
    }
}

#[test]
fn every_source_method_matches_full_unmodified_source_observations() {
    let mut owner = SourceChannels::new();
    let mut scene = String::new();
    let mut methods = BTreeSet::new();
    let mut observations = 0;
    let mut failures = vec![];
    let (mut exact, mut domains, mut scenes) = (0, 0, 0);
    for line in include_str!("../fixtures/channels_vectors.tsv").lines() {
        let f = line.split('\t').collect::<Vec<_>>();
        match f[0] {
            "S" => {
                scenes += 1;
                scene = f[1].into();
                owner = SourceChannels::from_state_unverified(state(&decode(f[2]))).unwrap()
            }
            "Q" => {
                domains += 1;
                methods.insert(f[2].to_owned());
                let before = owner.clone();
                let expected = decode_with_domain(f[5], true);
                if f[1].starts_with("unsupported-sparse-extension/") {
                    assert_eq!(
                        expected.get("sparseOwnKeysAfterAdjust"),
                        V::Array(vec![V::String("3".into())])
                    );
                    let continuation = expected.get("continuation");
                    let calls = list(&continuation.get("dependencies"));
                    let properties: Vec<_> = calls
                        .iter()
                        .filter(|call| string(&call.get("target")).starts_with("Tally."))
                        .map(|call| list(&call.get("arguments"))[1].clone())
                        .collect();
                    assert_eq!(properties, vec![V::number(3.0); 3]);
                }
                let mut backend = Backend {
                    expected: list(&expected.get("dependencies")),
                    cursor: 0,
                };
                assert_eq!(
                    owner.execute_unverified(
                        &SourceCall {
                            method: f[2].into(),
                            arguments: list(&decode(f[3]))
                        },
                        &mut backend
                    ),
                    Err(SourceRuleError::InvalidInput)
                );
                assert_eq!(owner, before);
                observations += 1;
            }
            "A" => {
                exact += 1;
                let method = f[2];
                methods.insert(method.to_owned());
                let expected = decode(f[5]);
                let mut backend = Backend {
                    expected: list(&expected.get("dependencies")),
                    cursor: 0,
                };
                let checked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let result = owner.execute_unverified(
                        &SourceCall {
                            method: method.into(),
                            arguments: list(&decode(f[3])),
                        },
                        &mut backend,
                    );
                    let observed = result
                        .unwrap_or_else(|e| panic!("{} in scene{}:local {e:?}", f[1], scene))
                        .diagnostic_value(owner.state());
                    assert_eq!(
                        backend.cursor,
                        backend.expected.len(),
                        "{} missing dependency",
                        f[1]
                    );
                    assert_eq!(observed, expected, "{} in scene{}", f[1], scene);
                }));
                if checked.is_err() {
                    eprintln!("SOURCE_CASE_FAILED {}", f[1]);
                    failures.push(f[1].to_owned());
                    owner = SourceChannels::from_state_unverified(state(&expected.get("state")))
                        .unwrap();
                }
                observations += 1
            }
            "M" => {
                assert_eq!(f[1].parse::<usize>().unwrap(), METHODS.len());
                assert_eq!(f[2].split(',').collect::<Vec<_>>(), METHODS)
            }
            _ => {}
        }
    }
    assert_eq!(methods.len(), METHODS.len() + 1);
    assert_eq!((observations, exact, domains, scenes), (491, 445, 46, 480));
    assert!(failures.is_empty(), "source case mismatches: {failures:?}");
}

#[test]
fn capacity_and_unknown_method_preserve_complete_state() {
    let mut owner = SourceChannels::new();
    let before = owner.clone();
    let mut backend = Backend::default();
    assert_eq!(
        owner.execute_unverified(
            &SourceCall {
                method: "not-a-method".into(),
                arguments: vec![]
            },
            &mut backend
        ),
        Err(SourceRuleError::UnknownMethod)
    );
    assert_eq!(owner, before);
    assert_eq!(
        owner.execute_unverified(
            &SourceCall {
                method: "setChannel".into(),
                arguments: vec![V::Null; 17]
            },
            &mut backend
        ),
        Err(SourceRuleError::Capacity)
    );
    assert_eq!(owner, before);
    assert_eq!(backend.cursor, 0)
}

#[test]
fn metadata_never_grants_channel_admission() {
    let mut owner = SourceChannels::new();
    let mut backend = Backend::default();
    let evaluation = owner
        .execute_unverified(
            &SourceCall {
                method: "constructor".into(),
                arguments: vec![],
            },
            &mut backend,
        )
        .unwrap();
    assert_eq!(
        evaluation.require_authenticated_admission(),
        Err(SourceRuleError::UnauthenticatedInputAndState)
    )
}

#[test]
fn composed_inputs_and_state_are_bounded_before_external_calls() {
    let mut owner = SourceChannels::new();
    let before = owner.clone();
    let mut backend = Backend::default();
    let text = V::String("x".repeat(900_000));
    let args = vec![text.clone(); 10];
    assert_eq!(
        owner.execute_unverified(
            &SourceCall {
                method: "setChannel".into(),
                arguments: args
            },
            &mut backend
        ),
        Err(SourceRuleError::Capacity)
    );
    assert_eq!(owner, before);
    assert_eq!(backend.cursor, 0);
    let state = ChannelsState {
        registry: (0..10)
            .map(|n| (V::String(n.to_string()), text.clone()))
            .collect(),
        pending_withdrawals: vec![],
    };
    assert_eq!(
        SourceChannels::from_state_unverified(state),
        Err(SourceRuleError::Capacity)
    );
}

#[test]
fn invalid_backend_values_and_exhausted_action_budget_do_not_publish() {
    struct Oversized;
    impl SourceBackend for Oversized {
        fn invoke(&mut self, _: &DependencyCall) -> Result<DependencyReturn, SourceRuleError> {
            Ok(DependencyReturn::Pending(V::String("x".repeat(1_048_577))))
        }
    }
    let mut owner = SourceChannels::new();
    let before = owner.clone();
    assert_eq!(
        owner.execute_unverified(
            &SourceCall {
                method: "loadChannelsRegistry".into(),
                arguments: vec![]
            },
            &mut Oversized
        ),
        Err(SourceRuleError::Capacity)
    );
    assert_eq!(owner, before);
    let call = SourceCall {
        method: "constructor".into(),
        arguments: vec![],
    };
    let mut backend = Backend::default();
    for _ in 0..tradelayer_core::channels_source::MAX_ACTIONS {
        owner.execute_unverified(&call, &mut backend).unwrap();
    }
    let before = owner.clone();
    assert_eq!(
        owner.execute_unverified(&call, &mut backend),
        Err(SourceRuleError::Capacity)
    );
    assert_eq!(owner, before);
}

#[test]
fn derived_collections_and_tie_work_are_bounded_before_allocation() {
    use tradelayer_core::channels_source::{MAX_RECORDS, MAX_TIE_SCAN};
    let mut owner = SourceChannels::new();
    let before = owner.clone();
    let mut backend = Backend::default();
    for call in [
        SourceCall {
            method: "toChannelPropMap".into(),
            arguments: vec![V::String("x".repeat(MAX_RECORDS + 1))],
        },
        SourceCall {
            method: "isChannelEmpty".into(),
            arguments: vec![V::Object(vec![
                (
                    "participants".into(),
                    V::Object(vec![("A".into(), V::String("alice".into()))]),
                ),
                ("A".into(), V::String("x".repeat(MAX_RECORDS + 1))),
                ("B".into(), V::Object(vec![])),
            ])],
        },
        SourceCall {
            method: "tieBreakerByBackChar".into(),
            arguments: vec![
                V::String("B".repeat(MAX_TIE_SCAN + 1)),
                V::String("B".repeat(MAX_TIE_SCAN + 1)),
                V::String("A".into()),
            ],
        },
    ] {
        assert_eq!(
            owner.execute_unverified(&call, &mut backend),
            Err(SourceRuleError::Capacity)
        );
        assert_eq!(owner, before);
        assert_eq!(backend.cursor, 0);
    }
}
