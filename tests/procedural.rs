use std::collections::BTreeMap;
use tradelayer_core::procedural::{
    normalize_hash, ContextRejection, ProceduralDocument, ProceduralError, ProceduralRegistry,
    ProceduralState, ProceduralValue as V, MAX_JS_SAFE_INTEGER, MAX_PROCEDURAL_DOCUMENT_BYTES,
    MAX_PROCEDURAL_RECORDS, MAX_PROCEDURAL_TEXT_BYTES, MAX_PROCEDURAL_VALUE_DEPTH,
    MAX_PROCEDURAL_VALUE_NODES,
};

fn text(value: &V) -> &str {
    match value {
        V::Text(value) => value,
        _ => panic!("expected string"),
    }
}

fn object(value: &V) -> &ProceduralDocument {
    match value {
        V::Object(value) => value,
        _ => panic!("expected document"),
    }
}

fn doc(fields: &[(&str, V)]) -> ProceduralDocument {
    fields
        .iter()
        .map(|(key, value)| ((*key).into(), value.clone()))
        .collect()
}

fn context_result(result: Result<ProceduralDocument, ContextRejection>) -> V {
    match result {
        Ok(mut fields) => {
            fields.insert("valid".into(), V::Bool(true));
            V::Object(fields)
        }
        Err(error) => V::Object(doc(&[
            ("valid", V::Bool(false)),
            ("reason", error.to_string().into()),
        ])),
    }
}

fn run(registry: &mut ProceduralRegistry, operation: &str, args: &[V]) -> V {
    let written = match operation {
        "template" => Some(registry.upsert_template(text(&args[0]), object(&args[1]))),
        "contract" => Some(registry.upsert_contract(
            text(&args[0]),
            text(&args[1]),
            Some(text(&args[2])),
            object(&args[3]),
        )),
        "contractDefault" => {
            Some(registry.upsert_contract(text(&args[0]), text(&args[1]), None, object(&args[2])))
        }
        "transition" => {
            Some(registry.transition_contract(text(&args[0]), text(&args[1]), object(&args[2])))
        }
        _ => None,
    };
    if let Some(result) = written {
        return match result {
            Ok(value) => V::Object(value),
            Err(ProceduralError::ImmutableRecordId) => {
                V::Object(doc(&[("error", "IMMUTABLE_ID".into())]))
            }
            Err(error) => panic!("unexpected typed-domain error {error}"),
        };
    }
    match operation {
        "getTemplate" => registry
            .get_template(text(&args[0]))
            .cloned()
            .map(V::Object)
            .unwrap_or(V::Null),
        "getContract" => registry
            .get_contract(text(&args[0]))
            .cloned()
            .map(V::Object)
            .unwrap_or(V::Null),
        "normalize" => normalize_hash(text(&args[0])).into(),
        "issuance" => context_result(
            registry
                .ensure_issuance_context(
                    text(&args[0]),
                    text(&args[1]),
                    text(&args[2]),
                    text(&args[3]),
                )
                .map(|context| {
                    doc(&[
                        ("contract", V::Object(context.contract.clone())),
                        ("template", V::Object(context.template.clone())),
                    ])
                }),
        ),
        "redemption" => context_result(
            registry
                .ensure_redemption_context(text(&args[0]), text(&args[1]), text(&args[2]))
                .map(|context| doc(&[("contract", V::Object(context.contract.clone()))])),
        ),
        "constants" => V::Object(
            [
                ProceduralState::Template,
                ProceduralState::Funded,
                ProceduralState::Open,
                ProceduralState::Settled,
                ProceduralState::Closed,
                ProceduralState::Disputed,
            ]
            .into_iter()
            .map(|state| (state.as_str().into(), state.as_str().into()))
            .collect(),
        ),
        "all" => V::Object(
            registry
                .records()
                .map(|(key, value)| (key.into(), V::Object(value.clone())))
                .collect(),
        ),
        _ => panic!("unknown fixture operation {operation}"),
    }
}

// A tiny typed fixture decoder, not a production JSON parser. Strings use UTF-8
// hex, so whitespace, Unicode and delimiter characters remain unambiguous.
fn decode(input: &str) -> V {
    fn take<'a>(input: &mut &'a str, delimiter: char) -> &'a str {
        let (prefix, suffix) = input.split_once(delimiter).unwrap();
        *input = suffix;
        prefix
    }
    fn value(input: &mut &str) -> V {
        let tag = input.as_bytes()[0];
        *input = &input[1..];
        match tag {
            b'z' => V::Null,
            b'b' => {
                let truth = input.as_bytes()[0] == b'1';
                *input = &input[1..];
                V::Bool(truth)
            }
            b'n' => V::Integer(take(input, ';').parse().unwrap()),
            b's' => {
                let bytes = take(input, ';')
                    .as_bytes()
                    .chunks_exact(2)
                    .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
                    .collect();
                V::Text(String::from_utf8(bytes).unwrap())
            }
            b'a' => {
                let count = take(input, ':').parse().unwrap();
                V::Array((0..count).map(|_| value(input)).collect())
            }
            b'o' => {
                let count = take(input, ':').parse().unwrap();
                V::Object(
                    (0..count)
                        .map(|_| {
                            let key = value(input);
                            (text(&key).to_owned(), value(input))
                        })
                        .collect(),
                )
            }
            _ => panic!("unknown fixture tag"),
        }
    }
    let mut rest = input;
    let result = value(&mut rest);
    assert!(rest.is_empty(), "unconsumed fixture data");
    result
}

#[test]
fn complete_unmodified_source_metadata_vectors() {
    let mut current_scenario = "";
    let mut registry = ProceduralRegistry::new();
    let mut count = 0;
    for line in include_str!("../fixtures/procedural_vectors.tsv")
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
    {
        let fields: Vec<_> = line.split('\t').collect();
        assert_eq!(fields.len(), 5);
        if fields[0] != current_scenario {
            current_scenario = fields[0];
            registry = ProceduralRegistry::new();
        }
        let V::Array(args) = decode(fields[3]) else {
            panic!("input must be array");
        };
        let expected = decode(fields[4]);
        assert_eq!(
            run(&mut registry, fields[2], &args),
            expected,
            "scenario {} step {} operation {}",
            fields[0],
            fields[1],
            fields[2]
        );
        count += 1;
    }
    assert_eq!(count, 154);
}

#[test]
fn rejected_writes_preserve_registry() {
    let mut registry = ProceduralRegistry::new();
    registry
        .upsert_template("t", &doc(&[("hash", "h".into())]))
        .unwrap();
    let before = registry.clone();
    for (data, expected) in [
        (
            doc(&[("_id", "wrong".into())]),
            ProceduralError::ImmutableRecordId,
        ),
        (
            doc(&[("bad.key", "x".into())]),
            ProceduralError::UnsupportedFieldName,
        ),
        (
            doc(&[("$bad", "x".into())]),
            ProceduralError::UnsupportedFieldName,
        ),
        (
            doc(&[("constructor", "x".into())]),
            ProceduralError::UnsupportedFieldName,
        ),
        (
            doc(&[("number", V::Integer(MAX_JS_SAFE_INTEGER + 1))]),
            ProceduralError::UnsafeInteger,
        ),
        (
            doc(&[("number", V::Integer(i64::MIN))]),
            ProceduralError::UnsafeInteger,
        ),
        (
            doc(&[("long", "x".repeat(MAX_PROCEDURAL_TEXT_BYTES + 1).into())]),
            ProceduralError::TextLimit,
        ),
    ] {
        assert_eq!(registry.upsert_template("t", &data), Err(expected));
        assert_eq!(registry, before);
    }
}

#[test]
fn depth_nodes_and_merged_document_are_bounded_atomically() {
    let mut registry = ProceduralRegistry::new();
    let mut nested = V::Null;
    for _ in 0..=MAX_PROCEDURAL_VALUE_DEPTH {
        nested = V::Array(vec![nested]);
    }
    assert_eq!(
        registry.upsert_template("deep", &doc(&[("v", nested)])),
        Err(ProceduralError::DepthLimit)
    );
    assert_eq!(
        registry.upsert_template(
            "many",
            &doc(&[("v", V::Array(vec![V::Null; MAX_PROCEDURAL_VALUE_NODES]))])
        ),
        Err(ProceduralError::ValueLimit)
    );
    let initial: BTreeMap<_, _> = (0..10)
        .map(|i| {
            (
                format!("field{i}"),
                "x".repeat(MAX_PROCEDURAL_TEXT_BYTES).into(),
            )
        })
        .collect();
    registry.upsert_template("t", &initial).unwrap();
    let before = registry.clone();
    let added: BTreeMap<_, _> = (10..20)
        .map(|i| {
            (
                format!("field{i}"),
                "x".repeat(MAX_PROCEDURAL_TEXT_BYTES).into(),
            )
        })
        .collect();
    assert!(initial.len() * MAX_PROCEDURAL_TEXT_BYTES < MAX_PROCEDURAL_DOCUMENT_BYTES);
    assert_eq!(
        registry.upsert_template("t", &added),
        Err(ProceduralError::DocumentLimit)
    );
    assert_eq!(registry, before);
    assert_eq!(registry.len(), 1);
}

#[test]
fn capacity_allows_updates_and_rejects_additions_atomically() {
    let mut registry = ProceduralRegistry::new();
    assert!(registry.is_empty());
    for i in 0..MAX_PROCEDURAL_RECORDS {
        registry
            .upsert_template(&i.to_string(), &ProceduralDocument::new())
            .unwrap();
    }
    let before = registry.clone();
    assert_eq!(
        registry.transition_contract("new", "open", &ProceduralDocument::new()),
        Err(ProceduralError::Capacity)
    );
    assert_eq!(registry, before);
    registry
        .upsert_template("0", &doc(&[("hash", "h".into())]))
        .unwrap();
    assert_eq!(registry.len(), MAX_PROCEDURAL_RECORDS);
    assert_eq!(registry.records().count(), MAX_PROCEDURAL_RECORDS);
}

#[test]
fn context_input_resource_limits_precede_source_checks() {
    let registry = ProceduralRegistry::new();
    let oversized = "x".repeat(MAX_PROCEDURAL_TEXT_BYTES + 1);
    assert_eq!(
        registry.ensure_issuance_context("", "", "", &oversized),
        Err(ContextRejection::InputLimit)
    );
    assert_eq!(
        registry.ensure_redemption_context("", &oversized, ""),
        Err(ContextRejection::InputLimit)
    );
}

#[test]
fn returned_patch_and_stored_set_have_distinct_lifetimes() {
    let mut registry = ProceduralRegistry::new();
    registry
        .upsert_template("t", &doc(&[("hash", "h".into())]))
        .unwrap();
    let patch = registry
        .upsert_template("t", &doc(&[("new", V::Bool(true))]))
        .unwrap();
    assert!(!patch.contains_key("hash"));
    assert_eq!(
        registry.get_template("t").unwrap().get("hash"),
        Some(&"h".into())
    );
    let mut returned = patch;
    returned.insert("hash".into(), "mutated outside".into());
    assert_eq!(
        registry.get_template("t").unwrap().get("hash"),
        Some(&"h".into())
    );
}
