use std::collections::{BTreeMap, BTreeSet};
use tradelayer_core::issuance_intent::JsonValue;
use tradelayer_core::validity_source::{
    evaluate_unverified, DependencyCall, DependencyReturn, SourceBackend, SourceCall,
    SourceEvaluation, SourceException, SourceOutcome, SourceRuleError, SourceValue as V, METHODS,
};
const FIXTURE: &str = include_str!("../fixtures/validity_vectors.tsv");
fn unhex(v: &str) -> String {
    String::from_utf8(
        v.as_bytes()
            .chunks_exact(2)
            .map(|p| u8::from_str_radix(core::str::from_utf8(p).unwrap(), 16).unwrap())
            .collect(),
    )
    .unwrap()
}
fn decode(v: &str) -> V {
    V::from_json(&JsonValue::parse(&unhex(v)).unwrap()).unwrap()
}
fn fields(v: V) -> BTreeMap<String, V> {
    if let V::Object(v) = v {
        v.into_iter().collect()
    } else {
        panic!("expected object")
    }
}
fn list(v: V) -> Vec<V> {
    if let V::Array(v) = v {
        v
    } else {
        panic!("expected array")
    }
}
fn text(v: V) -> String {
    if let V::String(v) = v {
        v
    } else {
        panic!("expected string")
    }
}
struct Snapshot {
    defaults: BTreeMap<String, V>,
    overrides: BTreeMap<String, V>,
    pending: BTreeSet<String>,
    calls: usize,
}
impl Snapshot {
    fn new(overrides: V) -> Self {
        let defaults = FIXTURE
            .lines()
            .find_map(|l| l.strip_prefix("# defaults "))
            .unwrap();
        let pending = FIXTURE
            .lines()
            .find_map(|l| l.strip_prefix("# pending "))
            .unwrap();
        Self {
            defaults: fields(decode(defaults)),
            overrides: fields(overrides),
            pending: list(decode(pending)).into_iter().map(text).collect(),
            calls: 0,
        }
    }
}
impl SourceBackend for Snapshot {
    fn invoke(&mut self, call: &DependencyCall) -> Result<DependencyReturn, SourceRuleError> {
        self.calls += 1;
        let Some(v) = self
            .overrides
            .get(&call.target)
            .or_else(|| self.defaults.get(&call.target))
            .cloned()
        else {
            return Err(SourceRuleError::UnknownDependency);
        };
        let mode = v.get("$mode");
        if let V::String(mode) = mode {
            return Ok(match mode.as_str() {
                "pending" => DependencyReturn::Pending(v.get("value")),
                "immediate" => DependencyReturn::Immediate(v.get("value")),
                "throw" => DependencyReturn::Throw(SourceException {
                    name: text(v.get("name")),
                    message: text(v.get("message")),
                }),
                "pending-throw" => DependencyReturn::PendingThrow(SourceException {
                    name: text(v.get("name")),
                    message: text(v.get("message")),
                }),
                _ => return Err(SourceRuleError::InvalidInput),
            });
        }
        Ok(if self.pending.contains(&call.target) {
            DependencyReturn::Pending(v)
        } else {
            DependencyReturn::Immediate(v)
        })
    }
}
fn trace(e: &SourceEvaluation) -> String {
    let values = e
        .dependencies
        .iter()
        .map(|o| {
            let (mode, value) = match &o.returned {
                DependencyReturn::Immediate(v) => ("immediate", v.diagnostic_json().unwrap()),
                DependencyReturn::Pending(v) => ("pending", v.diagnostic_json().unwrap()),
                DependencyReturn::PendingThrow(e) => (
                    "pending-throw",
                    format!(
                        "[{},{}]",
                        JsonValue::String(e.name.clone()).to_json().unwrap(),
                        JsonValue::String(e.message.clone()).to_json().unwrap()
                    ),
                ),
                DependencyReturn::Throw(e) => (
                    "throw",
                    format!(
                        "[{},{}]",
                        JsonValue::String(e.name.clone()).to_json().unwrap(),
                        JsonValue::String(e.message.clone()).to_json().unwrap()
                    ),
                ),
            };
            format!(
                "[{}, {},{},{}]",
                JsonValue::String(o.call.target.clone()).to_json().unwrap(),
                V::Array(o.call.arguments.clone())
                    .diagnostic_json()
                    .unwrap(),
                JsonValue::String(mode.into()).to_json().unwrap(),
                value
            )
            .replace(", ", ",")
        })
        .collect::<Vec<_>>()
        .join(",");
    format!("[{values}]")
}
#[test]
fn complete_source_rules_match_returns_throws_mutations_and_dependency_order() {
    let mut count = 0;
    let mut covered = BTreeSet::new();
    let mut failures = Vec::new();
    for line in FIXTURE.lines().filter(|l| l.starts_with("V\t")) {
        let r: Vec<_> = line.split('\t').collect();
        assert_eq!(r.len(), 11);
        let args = list(V::from_diagnostic_json(&unhex(r[3])).unwrap());
        let mut backend = Snapshot::new(decode(r[4]));
        let call = SourceCall {
            method: r[2].into(),
            arguments: args,
        };
        let result = evaluate_unverified(&call, &mut backend);
        count += 1;
        covered.insert(r[2]);
        let result = match result {
            Ok(r) => r,
            Err(e) => {
                failures.push(format!("{}: local {e:?}", r[1]));
                continue;
            }
        };
        let actual = match &result.outcome {
            SourceOutcome::Returned(v) => {
                if r[5] != "ok" {
                    failures.push(format!("{} expected throw, returned {:?}", r[1], v));
                    continue;
                }
                v.diagnostic_json().unwrap()
            }
            SourceOutcome::Thrown(e) => {
                if r[5] != "throw" {
                    failures.push(format!("{} expected return, threw {e:?}", r[1]));
                    continue;
                }
                format!(
                    "[{},{}]",
                    JsonValue::String(e.name.clone()).to_json().unwrap(),
                    JsonValue::String(e.message.clone()).to_json().unwrap()
                )
            }
        };
        if actual != unhex(r[6]) {
            failures.push(format!(
                "{} result actual={actual} expected={}",
                r[1],
                unhex(r[6])
            ))
        }
        let args = V::Array(result.arguments_after.clone())
            .diagnostic_json()
            .unwrap();
        if args != unhex(r[7]) {
            failures.push(format!(
                "{} mutated args actual={args} expected={}",
                r[1],
                unhex(r[7])
            ))
        }
        let actual_trace = trace(&result);
        if actual_trace != unhex(r[8]) {
            failures.push(format!(
                "{} trace actual={actual_trace} expected={}",
                r[1],
                unhex(r[8])
            ))
        }
        let deferred = format!(
            "[{}]",
            result
                .deferred_rejections
                .iter()
                .map(|e| format!(
                    "[{},{}]",
                    JsonValue::String(e.name.clone()).to_json().unwrap(),
                    JsonValue::String(e.message.clone()).to_json().unwrap()
                ))
                .collect::<Vec<_>>()
                .join(",")
        );
        if deferred != unhex(r[9]) {
            failures.push(format!(
                "{} deferred actual={deferred} expected={}",
                r[1],
                unhex(r[9])
            ))
        }
        let globals = format!(
            "[{}]",
            result
                .global_writes
                .iter()
                .map(|(k, v)| format!(
                    "[{},{}]",
                    JsonValue::String(k.clone()).to_json().unwrap(),
                    v.diagnostic_json().unwrap()
                ))
                .collect::<Vec<_>>()
                .join(",")
        );
        if globals != unhex(r[10]) {
            failures.push(format!(
                "{} globals actual={globals} expected={}",
                r[1],
                unhex(r[10])
            ))
        }
        assert_eq!(
            result.require_authenticated_admission(),
            Err(SourceRuleError::UnauthenticatedInputAndState)
        );
    }
    assert_eq!(count, 562);
    assert_eq!(covered, METHODS.iter().copied().collect());
    assert!(
        failures.is_empty(),
        "{} mismatches:\n{}",
        failures.len(),
        failures.into_iter().take(30).collect::<Vec<_>>().join("\n")
    );
}

#[test]
fn bounded_values_and_calls_preflight_without_backend_effects() {
    let mut b = Snapshot::new(V::object());
    let bad = SourceCall {
        method: "absent".into(),
        arguments: vec![],
    };
    assert_eq!(
        evaluate_unverified(&bad, &mut b),
        Err(SourceRuleError::UnknownMethod)
    );
    assert_eq!(b.calls, 0);
    let prefixed = FIXTURE
        .lines()
        .find(|l| l.starts_with("Q\tunsupported-prefixed-bignumber\t"))
        .unwrap();
    let q: Vec<_> = prefixed.split('\t').collect();
    assert_eq!(q[5], "ok");
    let args = list(V::from_diagnostic_json(&unhex(q[3])).unwrap());
    let call = SourceCall {
        method: q[2].into(),
        arguments: args,
    };
    assert_eq!(
        evaluate_unverified(&call, &mut b),
        Err(SourceRuleError::UnsupportedNumericDomain)
    );
    assert_eq!(b.calls, 0);
    let source_quirk = FIXTURE
        .lines()
        .find(|l| l.starts_with("Q\tunsupported-own-toString-coercion\t"))
        .unwrap();
    let q: Vec<_> = source_quirk.split('\t').collect();
    assert_eq!(q[5], "throw");
    assert!(unhex(q[6]).contains("Cannot convert object to primitive value"));
    let args = list(V::from_diagnostic_json(&unhex(q[3])).unwrap());
    let call = SourceCall {
        method: q[2].into(),
        arguments: args,
    };
    assert_eq!(
        evaluate_unverified(&call, &mut b),
        Err(SourceRuleError::InvalidInput)
    );
    assert_eq!(b.calls, 0);
    let too_many = SourceCall {
        method: "isValidNumber".into(),
        arguments: vec![V::Null; 9],
    };
    assert_eq!(
        evaluate_unverified(&too_many, &mut b),
        Err(SourceRuleError::Capacity)
    );
    assert_eq!(b.calls, 0);
    assert_eq!(
        V::Object(vec![("x".into(), V::Null), ("x".into(), V::Bool(true))]).validate(),
        Err(SourceRuleError::InvalidInput)
    );
    let mut deep = V::Null;
    for _ in 0..25 {
        deep = V::Array(vec![deep]);
    }
    assert_eq!(deep.validate(), Err(SourceRuleError::Capacity));
    assert_ne!(V::number(-0.0), V::number(0.0));
    assert_eq!(
        V::number(-0.0).diagnostic_json().unwrap(),
        "[\"number\",\"8000000000000000\"]"
    );
}
