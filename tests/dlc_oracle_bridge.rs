use tradelayer_core::dlc_oracle_bridge::*;
use tradelayer_core::issuance_intent::{JsonNumber, JsonObject, JsonValue};

fn unhex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
        .collect()
}
fn text(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes).unwrap()
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn json(text: &str) -> JsonValue {
    JsonValue::parse(text).unwrap()
}
fn field<'a>(value: &'a JsonValue, key: &str) -> &'a JsonValue {
    let JsonValue::Object(object) = value else {
        panic!("not object")
    };
    object.get(key).unwrap()
}
fn string(value: &JsonValue) -> &str {
    let JsonValue::String(value) = value else {
        panic!("not string")
    };
    value
}
fn set(value: &JsonValue, name: &str, replacement: JsonValue) -> JsonValue {
    let JsonValue::Object(mut object) = value.clone() else {
        panic!()
    };
    object.insert(name.into(), replacement).unwrap();
    JsonValue::Object(object)
}
fn fixture(name: &str) -> JsonValue {
    let line = include_str!("../fixtures/dlc_oracle_bridge_vectors.tsv")
        .lines()
        .find(|line| line.starts_with(&format!("I\t{name}\t")))
        .unwrap();
    json(&text(unhex(line.split('\t').nth(3).unwrap())))
}
fn expectation<'a>(key: &'a [u8], payload: &'a [u8]) -> TrustedRelayExpectation<'a> {
    TrustedRelayExpectation {
        oracle_public_key: key,
        event_id: "event-1",
        outcome: "settled",
        outcome_index: 2.0,
        state_hash: "state-1",
        timestamp: 1_700_000_000.0,
        payload,
    }
}

#[test]
fn complete_genuine_source_corpus_matches_native_bridge() {
    let mut counts = std::collections::BTreeMap::<&str, usize>::new();
    for line in include_str!("../fixtures/dlc_oracle_bridge_vectors.tsv").lines() {
        if line.starts_with('#') {
            continue;
        }
        let cells: Vec<_> = line.split('\t').collect();
        let kind = cells[0];
        let name = cells[1];
        *counts.entry(kind).or_default() += 1;
        if matches!(kind, "I" | "H") {
            continue;
        }
        let input = json(&text(unhex(cells[2])));
        let output = json(&text(unhex(cells[3])));
        let thrown = match &output {
            JsonValue::Object(object) => object.get("throw"),
            _ => None,
        };
        let expected = match &output {
            JsonValue::Object(object) => object.get("value"),
            _ => None,
        };
        match kind {
            "N" => assert_eq!(
                source_from_node_dlc_like(&input)
                    .unwrap()
                    .unwrap_or(JsonValue::Null),
                *expected.unwrap(),
                "{name}"
            ),
            "P" => assert_eq!(
                source_parse_relay_blob(&input)
                    .unwrap()
                    .unwrap_or(JsonValue::Null),
                *expected.unwrap(),
                "{name}"
            ),
            "C" => {
                let actual = canonical_relay_message(&input);
                if thrown.is_some() {
                    assert!(
                        matches!(actual, Err(DlcBridgeError::SourceTypeError(_))),
                        "{name}: {actual:?}"
                    );
                } else {
                    let expected = expected.unwrap();
                    assert_eq!(
                        actual.unwrap(),
                        string(field(expected, "message")),
                        "{name}"
                    );
                    assert_eq!(
                        hex(&relay_message_digest(&input).unwrap()),
                        string(field(expected, "digest")),
                        "{name}"
                    );
                }
            }
            "K" | "S" => {
                let result = if kind == "K" {
                    source_pubkey_buffer(&input)
                } else {
                    source_signature_buffer(&input)
                }
                .unwrap();
                assert_eq!(
                    result.map_or(JsonValue::Null, |v| JsonValue::String(hex(&v))),
                    *expected.unwrap(),
                    "{name}"
                );
            }
            "V" => {
                let actual = source_validate_relay_bundle(
                    field(&input, "bundle"),
                    Some(field(&input, "expected")),
                );
                if let Some(thrown) = thrown {
                    assert_eq!(string(field(thrown, "name")), "TypeError");
                    let expected_message = string(field(thrown, "message"));
                    assert!(
                        matches!(actual, Err(DlcBridgeError::SourceTypeError(_))),
                        "{name}: {actual:?}"
                    );
                    if matches!(expected_message, "Expected Point" | "Expected Signature") {
                        assert_eq!(
                            actual,
                            Err(DlcBridgeError::SourceTypeError(
                                if expected_message == "Expected Point" {
                                    "Expected Point"
                                } else {
                                    "Expected Signature"
                                }
                            )),
                            "{name}"
                        );
                    }
                } else {
                    assert_eq!(
                        json(&actual.unwrap_or_else(|e| panic!("{name}: {e:?}")).to_json()),
                        *expected.unwrap(),
                        "{name}"
                    );
                }
            }
            _ => panic!("unknown fixture kind {kind}"),
        }
    }
    assert!(counts["V"] >= 70);
    assert!(counts["C"] >= 150);
    assert!(counts["P"] >= 20);
    assert_eq!(counts["I"], 1);
}

#[test]
fn source_legacy_fallback_accepts_replaced_payload_but_trusted_policy_refuses_it() {
    let data = fixture("full-source");
    let key = unhex(string(field(&data, "publicKey")));
    let payload = unhex(string(field(&data, "payload")));
    let expected = expectation(&key, &payload);
    let current = field(&data, "good");
    let proof = validate_trusted_payload_bound(current, &expected).unwrap();
    assert_eq!(proof.oracle_public_key.as_slice(), key);
    assert_eq!(
        hex(&proof.message_digest),
        hex(&relay_message_digest(current).unwrap())
    );
    assert_eq!(
        proof.canonical_message,
        canonical_relay_message(current).unwrap()
    );
    let legacy = field(&data, "legacy");
    assert!(source_validate_relay_bundle(legacy, None).unwrap().valid);
    assert_eq!(
        validate_trusted_payload_bound(legacy, &expected),
        Err(DlcBridgeError::TrustedMismatch("signatureHex"))
    );
    let replaced = field(&data, "legacyChanged");
    assert!(source_validate_relay_bundle(replaced, None).unwrap().valid);
    assert_eq!(
        validate_trusted_payload_bound(replaced, &expected),
        Err(DlcBridgeError::TrustedMismatch("payloadHash"))
    );
}

#[test]
fn trusted_inputs_are_independent_and_all_relevant_fields_are_bound() {
    let data = fixture("full-source");
    let key = unhex(string(field(&data, "publicKey")));
    let payload = unhex(string(field(&data, "payload")));
    let good = field(&data, "good");
    let mut expected = expectation(&key, &payload);
    let foreign = field(&data, "foreignOracle");
    assert!(source_validate_relay_bundle(foreign, None).unwrap().valid);
    assert_eq!(
        validate_trusted_payload_bound(foreign, &expected),
        Err(DlcBridgeError::TrustedMismatch("oraclePubkeyHex"))
    );
    expected.event_id = "different";
    assert_eq!(
        validate_trusted_payload_bound(good, &expected),
        Err(DlcBridgeError::TrustedMismatch("eventId"))
    );
    expected = expectation(&key, &payload);
    expected.outcome = "different";
    assert_eq!(
        validate_trusted_payload_bound(good, &expected),
        Err(DlcBridgeError::TrustedMismatch("outcome"))
    );
    expected = expectation(&key, &payload);
    expected.outcome_index = 3.0;
    assert_eq!(
        validate_trusted_payload_bound(good, &expected),
        Err(DlcBridgeError::TrustedMismatch("outcomeIndex"))
    );
    expected = expectation(&key, &payload);
    expected.timestamp += 1.0;
    assert_eq!(
        validate_trusted_payload_bound(good, &expected),
        Err(DlcBridgeError::TrustedMismatch("timestamp"))
    );
    expected = expectation(&key, &payload);
    expected.state_hash = "other";
    assert_eq!(
        validate_trusted_payload_bound(good, &expected),
        Err(DlcBridgeError::TrustedMismatch("stateHash"))
    );
    expected = expectation(&key, b"different");
    assert_eq!(
        validate_trusted_payload_bound(good, &expected),
        Err(DlcBridgeError::TrustedMismatch("payloadHash"))
    );
    for field in [
        "eventId",
        "outcome",
        "stateHash",
        "payloadHash",
        "balancePayloadB64",
        "oraclePubkeyHex",
        "signatureHex",
    ] {
        let changed = set(good, field, JsonValue::Null);
        assert!(
            validate_trusted_payload_bound(&changed, &expectation(&key, &payload)).is_err(),
            "{field}"
        );
    }
    let coerced = set(good, "outcomeIndex", JsonValue::String("2".into()));
    assert!(source_validate_relay_bundle(&coerced, None).unwrap().valid);
    assert_eq!(
        validate_trusted_payload_bound(&coerced, &expectation(&key, &payload)),
        Err(DlcBridgeError::TrustedMismatch("outcomeIndex"))
    );
}

#[test]
fn strict_policy_rejects_high_s_and_accepts_the_same_uncompressed_key() {
    let data = fixture("full-source");
    let key = unhex(string(field(&data, "publicKey")));
    let payload = unhex(string(field(&data, "payload")));
    let expected = expectation(&key, &payload);
    let high = field(&data, "highS");
    assert!(source_validate_relay_bundle(high, None).unwrap().valid);
    assert_eq!(
        validate_trusted_payload_bound(high, &expected),
        Err(DlcBridgeError::TrustedMismatch("signatureHex"))
    );
    let uncompressed = set(
        field(&data, "good"),
        "oraclePubkeyHex",
        field(&data, "uncompressed").clone(),
    );
    assert!(validate_trusted_payload_bound(&uncompressed, &expected).is_ok());
    let truncated_key = set(
        field(&data, "good"),
        "oraclePubkeyHex",
        JsonValue::String(format!("{}zz{}", hex(&key), "0".repeat(62))),
    );
    assert!(
        source_validate_relay_bundle(&truncated_key, None)
            .unwrap()
            .valid
    );
    assert_eq!(
        validate_trusted_payload_bound(&truncated_key, &expected),
        Err(DlcBridgeError::TrustedMismatch("oraclePubkeyHex"))
    );
    let invalid_key = [0u8; 33];
    assert_eq!(
        validate_trusted_payload_bound(field(&data, "good"), &expectation(&invalid_key, &payload)),
        Err(DlcBridgeError::InvalidExpectation)
    );
}

#[test]
fn source_invalid_verdicts_are_distinct_from_native_encoding_exceptions() {
    let data = fixture("full-source");
    let good = field(&data, "good");
    let short = set(good, "signatureHex", JsonValue::String("00".into()));
    assert_eq!(
        source_validate_relay_bundle(&short, None).unwrap().reason,
        Some("Invalid oracle pubkey/signature encoding")
    );
    let bad = set(good, "signatureHex", JsonValue::String("zz".repeat(64)));
    assert_eq!(
        source_validate_relay_bundle(&bad, None),
        Err(DlcBridgeError::SourceTypeError("Expected Signature"))
    );
    let zero = set(good, "signatureHex", JsonValue::String("00".repeat(64)));
    assert_eq!(
        source_validate_relay_bundle(&zero, None).unwrap().reason,
        Some("Invalid oracle relay signature")
    );
    assert_eq!(
        source_validate_relay_bundle(&JsonValue::Null, None)
            .unwrap()
            .reason,
        Some("Missing relay bundle")
    );
}

#[test]
fn aliases_and_canonical_field_presence_preserve_source_meaning() {
    let alias = json(
        r#"{"eventId":0,"oracleEventId":"alias","outcomes":[false,"ignored"],"outcomeIndex":false,"outcome_index":9,"state_hash":"s","eventMaturityEpoch":3}"#,
    );
    let normalized = source_from_node_dlc_like(&alias).unwrap().unwrap();
    assert_eq!(
        field(&normalized, "eventId"),
        &JsonValue::String("alias".into())
    );
    assert_eq!(
        field(&normalized, "outcome"),
        &JsonValue::String(String::new())
    );
    assert_eq!(field(&normalized, "outcomeIndex"), &JsonValue::Bool(false));
    let no_hash = canonical_relay_message(&json("{}")).unwrap();
    let empty_hash = canonical_relay_message(&json(r#"{"payloadHash":""}"#)).unwrap();
    assert_ne!(no_hash, empty_hash);
    assert_eq!(
        no_hash,
        canonical_relay_message(&json(r#"{"payloadHash":null}"#)).unwrap()
    );
    assert!(
        canonical_relay_message(&json(r#"{"outcomeIndex":"Infinity","timestamp":"bad"}"#))
            .unwrap()
            .contains("\"outcomeIndex\":null")
    );
}

#[test]
fn parse_requires_json_and_preserves_scalar_and_array_source_fallbacks() {
    assert_eq!(
        source_parse_relay_blob(&json(r#"{"eventId":"already-object"}"#)).unwrap(),
        None
    );
    assert_eq!(
        source_parse_relay_blob(&JsonValue::String("false".into())).unwrap(),
        Some(JsonValue::Bool(false))
    );
    assert_eq!(
        source_parse_relay_blob(&JsonValue::String("null".into())).unwrap(),
        Some(JsonValue::Null)
    );
    assert_eq!(
        source_parse_relay_blob(&JsonValue::String("1e999".into())),
        Err(DlcBridgeError::UnsupportedNumber)
    );
    assert_eq!(
        source_parse_relay_blob(&JsonValue::String("\"\\ud800\"".into())),
        Err(DlcBridgeError::UnsupportedUtf16)
    );
    assert!(
        source_parse_relay_blob(&JsonValue::String("b64:e30".into()))
            .unwrap()
            .is_some()
    );
}

#[test]
fn bounds_precede_clones_and_trusted_configuration_is_checked() {
    let too_many = JsonValue::Array(vec![JsonValue::Null; 4097]);
    assert_eq!(
        source_from_node_dlc_like(&too_many),
        Err(DlcBridgeError::Capacity)
    );
    let huge = JsonValue::String("x".repeat(MAX_RELAY_JSON_BYTES + 1));
    assert_eq!(
        source_parse_relay_blob(&huge),
        Err(DlcBridgeError::Capacity)
    );
    assert_eq!(
        source_signature_buffer(&huge),
        Err(DlcBridgeError::Capacity)
    );
    let data = fixture("full-source");
    let key = unhex(string(field(&data, "publicKey")));
    let payload = unhex(string(field(&data, "payload")));
    let good = field(&data, "good");
    for index in [f64::NAN, f64::INFINITY, -1.0, 0.5, 9_007_199_254_740_992.0] {
        let mut expected = expectation(&key, &payload);
        expected.outcome_index = index;
        assert_eq!(
            validate_trusted_payload_bound(good, &expected),
            Err(DlcBridgeError::InvalidExpectation)
        );
    }
    let oversized = vec![0; MAX_PAYLOAD_BYTES + 1];
    assert_eq!(
        validate_trusted_payload_bound(good, &expectation(&key, &oversized)),
        Err(DlcBridgeError::InvalidExpectation)
    );
    let mut object = JsonObject::new();
    object
        .insert(
            "outcomeIndex".into(),
            JsonValue::Number(JsonNumber::from_f64(2.0).unwrap()),
        )
        .unwrap();
    assert!(canonical_relay_message(&JsonValue::Object(object)).is_ok());
}
