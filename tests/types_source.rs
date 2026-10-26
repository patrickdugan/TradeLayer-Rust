use std::collections::{BTreeMap, BTreeSet};
use tradelayer_core::issuance_intent::JsonValue;
use tradelayer_core::types_source::*;
use tradelayer_core::validity_source::{
    DependencyCall, DependencyReturn, SourceBackend, SourceCall, SourceEvaluation, SourceException,
    SourceOutcome, SourceRuleError, SourceValue as V,
};

const FIXTURE: &str = include_str!("../fixtures/types_vectors.tsv");
fn unhex(v: &str) -> String {
    String::from_utf8(
        v.as_bytes()
            .chunks_exact(2)
            .map(|p| u8::from_str_radix(core::str::from_utf8(p).unwrap(), 16).unwrap())
            .collect(),
    )
    .unwrap()
}
fn list(v: V) -> Vec<V> {
    if let V::Array(v) = v {
        v
    } else {
        panic!("array expected")
    }
}
pub struct RecordingValidity {
    pub mode: String,
    pub calls: usize,
}
impl TypesValidity for RecordingValidity {
    fn validate(&mut self, call: &SourceCall) -> Result<ValiditySettlement, SourceRuleError> {
        self.calls += 1;
        let mut arguments = call.arguments.clone();
        let mut value = arguments.get(1).cloned().unwrap_or(V::Undefined);
        if self.mode.ends_with("mutate") && matches!(value, V::Object(_)) {
            value.insert("valid", V::Bool(true))?;
            value.insert("reason", V::String("adapter-observation".into()))?;
            value.insert("adapterOnly", V::Bool(true))?;
            arguments[1] = value.clone();
        }
        if self.mode.ends_with("null") {
            value = V::Null;
        }
        if self.mode.ends_with("undefined") {
            value = V::Undefined;
        }
        if self.mode.ends_with("replacement") {
            value = V::Object(vec![
                ("valid".into(), V::Bool(false)),
                ("reason".into(), V::String("replacement".into())),
            ]);
        }
        if self.mode.ends_with("bad-valid") {
            value = V::Object(vec![
                (
                    "valid".into(),
                    V::Object(vec![("toString".into(), V::number(0.0))]),
                ),
                ("reason".into(), V::String(String::new())),
            ]);
        }
        let outcome = if self.mode.ends_with("throw") {
            SourceOutcome::Thrown(SourceException {
                name: "RangeError".into(),
                message: "validity adapter failure".into(),
            })
        } else {
            SourceOutcome::Returned(value)
        };
        Ok(ValiditySettlement {
            pending: !self.mode.starts_with("immediate"),
            evaluation: SourceEvaluation {
                outcome,
                arguments_after: arguments,
                dependencies: vec![],
                deferred_rejections: vec![],
                global_writes: vec![],
            },
        })
    }
}
struct Snapshot {
    defaults: BTreeMap<String, V>,
    overrides: BTreeMap<String, V>,
    pending: BTreeSet<String>,
}
fn fields(v: V) -> BTreeMap<String, V> {
    if let V::Object(v) = v {
        v.into_iter().collect()
    } else {
        panic!("object expected")
    }
}
fn plain(hex: &str) -> V {
    V::from_json(&JsonValue::parse(&unhex(hex)).unwrap()).unwrap()
}
fn text(v: V) -> String {
    if let V::String(v) = v {
        v
    } else {
        panic!("string expected")
    }
}
impl Snapshot {
    fn new(overrides: &str) -> Self {
        let defaults = FIXTURE
            .lines()
            .find_map(|l| l.strip_prefix("# source-defaults "))
            .unwrap();
        let pending = FIXTURE
            .lines()
            .find_map(|l| l.strip_prefix("# source-pending "))
            .unwrap();
        Self {
            defaults: fields(plain(defaults)),
            overrides: fields(plain(overrides)),
            pending: list(plain(pending)).into_iter().map(text).collect(),
        }
    }
}
impl SourceBackend for Snapshot {
    fn invoke(&mut self, call: &DependencyCall) -> Result<DependencyReturn, SourceRuleError> {
        let v = self
            .overrides
            .get(&call.target)
            .or_else(|| self.defaults.get(&call.target))
            .cloned()
            .ok_or(SourceRuleError::UnknownDependency)?;
        if let V::String(mode) = v.get("$mode") {
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

#[test]
fn full_unchanged_source_corpus_matches_decoding_mutation_arguments_errors_and_awaited_settlements()
{
    let mut count = 0;
    let mut covered = BTreeSet::new();
    let mut failures = Vec::new();
    for line in FIXTURE.lines().filter(|l| l.starts_with("T\t")) {
        let r: Vec<_> = line.split('\t').collect();
        assert_eq!(r.len(), 8);
        let args = list(
            V::from_diagnostic_json(&unhex(r[2]))
                .unwrap_or_else(|e| panic!("{} input {e:?}", r[1])),
        );
        let mut backend = RecordingValidity {
            mode: r[3].into(),
            calls: 0,
        };
        let actual = if r[3].starts_with("source") {
            decode_payload_unverified(&args, &mut Snapshot::new(r[6]))
        } else {
            decode_with_validity_unverified(&args, &mut backend)
        };
        count += 1;
        if r[5] == "unsupported-utf16-exception" {
            assert_eq!(actual, Err(TypesError::UnsupportedUtf16Exception));
            continue;
        }
        if let Some(V::Number(bits)) = args.get(1) {
            let n = f64::from_bits(*bits);
            if n.fract() == 0.0 && (0.0..36.0).contains(&n) {
                covered.insert(n as u8);
            }
        }
        let actual = match actual {
            Ok(v) => v,
            Err(e) => {
                failures.push(format!("{} local {e:?}", r[1]));
                continue;
            }
        };
        let diagnostic = actual.diagnostic_json().unwrap();
        let expected = unhex(r[4]);
        if diagnostic != expected {
            failures.push(format!(
                "{}\nactual:{diagnostic}\nexpected:{expected}",
                r[1]
            ));
        }
        assert_eq!(
            actual.require_authenticated_admission(),
            Err(TypesError::UnauthenticatedInputAndState)
        );
        if !r[3].starts_with("source") {
            assert_eq!(backend.calls, usize::from(actual.validity.is_some()));
        }
    }
    assert_eq!(count, 1228);
    assert_eq!(covered, (0..36).collect());
    assert!(
        failures.is_empty(),
        "{} failures\n{}",
        failures.len(),
        failures.into_iter().take(12).collect::<Vec<_>>().join("\n")
    );
}

#[test]
fn borrowed_caps_reject_before_validity_and_source_helper_retains_reference_precedence() {
    let mut backend = RecordingValidity {
        mode: "pending-identity".into(),
        calls: 0,
    };
    assert_eq!(
        decode_with_validity_unverified(&vec![V::Null; 17], &mut backend),
        Err(TypesError::Capacity)
    );
    assert_eq!(
        decode_with_validity_unverified(&[V::String("x".repeat(1_048_577))], &mut backend),
        Err(TypesError::Capacity)
    );
    let duplicates = V::Object(vec![("x".into(), V::Null), ("x".into(), V::Null)]);
    assert_eq!(
        decode_with_validity_unverified(&[duplicates], &mut backend),
        Err(TypesError::InvalidInput)
    );
    assert_eq!(backend.calls, 0);
    let refs = V::Array(vec![
        V::Null,
        V::Object(vec![("address".into(), V::String(String::new()))]),
        V::Object(vec![("address".into(), V::String("first".into()))]),
    ]);
    assert_eq!(
        resolve_reference_address_unverified(&refs),
        Ok(V::String("first".into()))
    );
    assert_eq!(
        resolve_reference_address_unverified(&V::Object(vec![
            ("address".into(), V::Bool(true)),
            ("senderAddress".into(), V::String("fallback".into()))
        ])),
        Ok(V::String("fallback".into()))
    );
}

struct InactiveBackend {
    calls: Vec<DependencyCall>,
}
impl SourceBackend for InactiveBackend {
    fn invoke(&mut self, call: &DependencyCall) -> Result<DependencyReturn, SourceRuleError> {
        self.calls.push(call.clone());
        match call.target.as_str() {
            "Activation.isTxTypeActive" => Ok(DependencyReturn::Pending(V::Bool(false))),
            "TxUtils.validateAddressWrapper" => Ok(DependencyReturn::Pending(V::Object(vec![(
                "isvalid".into(),
                V::Bool(true),
            )]))),
            _ => Err(SourceRuleError::UnknownDependency),
        }
    }
}
#[test]
fn default_entry_really_composes_full_source_validity_instead_of_accepting_identity_results() {
    let args = vec![
        V::String("tx".into()),
        V::number(7.0),
        V::String("tl".into()),
        V::String("backup,List,url,desc".into()),
        V::String("admin".into()),
        V::Array(vec![]),
        V::number(0.0),
        V::number(0.0),
        V::number(101.0),
    ];
    let mut backend = InactiveBackend { calls: vec![] };
    let result = decode_payload_unverified(&args, &mut backend).unwrap();
    assert_eq!(backend.calls.len(), 2);
    let TypesOutcome::Returned(value) = result.outcome else {
        panic!("expected source return")
    };
    assert_eq!(value.get("valid"), V::Bool(false));
    assert!(!result
        .validity
        .unwrap()
        .settlement
        .evaluation
        .dependencies
        .is_empty());
}
