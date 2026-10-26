use tradelayer_core::issuance_intent::{
    IntentError, IssuanceIntentStore, JsonNumber, JsonObject, JsonValue, MAX_INTENTS,
    MAX_INTENT_WRITES, MAX_JSON_BYTES, MAX_JSON_DEPTH, MAX_JSON_NODES, MAX_SAFE_MILLIS,
    MAX_SELECTOR_BYTES,
};

fn string_json(text: &str) -> String {
    let JsonValue::String(value) = JsonValue::parse(text).unwrap() else {
        panic!("string fixture");
    };
    value
}
fn documents_json<'a>(documents: impl IntoIterator<Item = &'a JsonObject>) -> String {
    let values = documents
        .into_iter()
        .map(|document| JsonValue::Object(document.clone()))
        .collect();
    JsonValue::Array(values).to_json().unwrap()
}
fn checkpoint_roundtrip(store: &IssuanceIntentStore) {
    let bytes = store.checkpoint_bytes().unwrap();
    let restored = IssuanceIntentStore::from_checkpoint_bytes(&bytes).unwrap();
    assert_eq!(&restored, store);
    assert_eq!(restored.checkpoint_bytes().unwrap(), bytes);
}

#[test]
fn full_source_adapter_vectors_cover_every_method_and_mutation_order() {
    let mut store = IssuanceIntentStore::new();
    let mut counts = [0_usize; 7];
    for line in include_str!("../fixtures/issuance_intent_vectors.tsv")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let fields: Vec<_> = line.split('\t').collect();
        match fields[0] {
            "RESET" => {
                checkpoint_roundtrip(&store);
                store = IssuanceIntentStore::new();
                counts[0] += 1;
            }
            "WRITE" => {
                let selector = string_json(fields[2]);
                let intent = JsonObject::parse(fields[4]).unwrap();
                let before = store.clone();
                let actual = store.record_intent(&selector, &intent, fields[3].parse().unwrap());
                match fields[5] {
                    "ok" => assert_eq!(actual.unwrap().to_json().unwrap(), fields[6], "{line}"),
                    error => {
                        let expected = match error {
                            "MissingSelector" => IntentError::MissingSelector,
                            "ImmutableId" => IntentError::ImmutableId,
                            "DuplicateId" => IntentError::DuplicateId,
                            _ => panic!("unexpected fixture error {error}"),
                        };
                        assert_eq!(actual, Err(expected), "{line}");
                        assert_eq!(store, before);
                    }
                }
                counts[1] += 1;
            }
            "GET" => {
                let selector = string_json(fields[2]);
                let actual = store
                    .get_intent(&selector)
                    .map(|document| document.to_json().unwrap())
                    .unwrap_or_else(|| "null".into());
                assert_eq!(actual, fields[3], "{line}");
                counts[2] += 1;
            }
            "ADDRESS" => {
                let address = string_json(fields[2]);
                let actual = documents_json(store.get_intents_by_address(&address));
                assert_eq!(actual, fields[3], "{line}");
                counts[3] += 1;
            }
            "DOCUMENTS" => {
                assert_eq!(documents_json(store.documents()), fields[2], "{line}");
                counts[4] += 1;
            }
            "NUMBER" => {
                assert_eq!(
                    JsonValue::parse(fields[1]).unwrap().to_json().unwrap(),
                    fields[2],
                    "{line}"
                );
                counts[5] += 1;
            }
            "OBS" => {
                // Actual source DB-router and non-object spread observations
                // are not successful operations of the bounded object store.
                counts[6] += 1;
            }
            other => panic!("unknown source vector {other}"),
        }
    }
    assert_eq!(counts, [3, 19, 5, 12, 3, 270, 9]);
    checkpoint_roundtrip(&store);
}

#[test]
fn arbitrary_nested_fields_are_replaced_instead_of_guessed_or_merged() {
    let mut store = IssuanceIntentStore::new();
    let first = JsonObject::parse(r#"{"recipientAddress":"r","unanticipated":{"nested":[null,true,2.5,{"a.b":"x","$custom":"data","__proto__":{"value":1}}]},"empty":[]}"#).unwrap();
    let result = store.record_intent("tx", &first, 1).unwrap();
    assert_eq!(result.get("unanticipated"), first.get("unanticipated"));
    let second = JsonObject::parse(r#"{"recipientAddress":"other","onlyNew":false}"#).unwrap();
    store.record_intent("tx", &second, 2).unwrap();
    assert!(store
        .get_intent("tx")
        .unwrap()
        .get("unanticipated")
        .is_none());
    assert!(store.get_intents_by_address("r").is_empty());
    assert_eq!(store.get_intents_by_address("other").len(), 1);
    assert_eq!(store.journal()[0].intent, first);
    checkpoint_roundtrip(&store);
}

#[test]
fn spread_overwrites_keep_source_key_slots_and_integer_enumeration() {
    let intent = JsonObject::parse(r#"{"updatedAt":{"discarded":true},"z":1,"_id":"alias","10":"ten","2":"two","01":"one","4294967294":"last-index","4294967295":"ordinary"}"#).unwrap();
    let mut store = IssuanceIntentStore::new();
    let actual = store
        .record_intent("selector", &intent, 777)
        .unwrap()
        .clone();
    assert_eq!(
        actual.to_json().unwrap(),
        r#"{"2":"two","10":"ten","4294967294":"last-index","_id":"alias","updatedAt":777,"z":1,"01":"one","4294967295":"ordinary"}"#
    );
    assert!(store.get_intent("selector").is_none());
    assert_eq!(store.get_intent("alias").unwrap(), &actual);
    checkpoint_roundtrip(&store);
}

#[test]
fn id_mismatches_duplicate_and_immutable_failures_leave_all_metadata_atomic() {
    let mut store = IssuanceIntentStore::new();
    let intent = JsonObject::parse(r#"{"_id":"stored","recipientAddress":"r"}"#).unwrap();
    store.record_intent("selector", &intent, 1).unwrap();
    let before = store.clone();
    assert_eq!(
        store.record_intent("selector", &intent, 2),
        Err(IntentError::DuplicateId)
    );
    assert_eq!(store, before);
    let change_id = JsonObject::parse(r#"{"_id":"different","recipientAddress":"s"}"#).unwrap();
    assert_eq!(
        store.record_intent("stored", &change_id, 3),
        Err(IntentError::ImmutableId)
    );
    assert_eq!(store, before);
    store
        .record_intent(
            "stored",
            &JsonObject::parse(r#"{"recipientAddress":"s"}"#).unwrap(),
            4,
        )
        .unwrap();
    assert!(store.get_intents_by_address("r").is_empty());
    assert_eq!(store.get_intents_by_address("s").len(), 1);
    checkpoint_roundtrip(&store);
}

#[test]
fn nonstring_and_empty_stored_ids_remain_observable_metadata() {
    let mut store = IssuanceIntentStore::new();
    for (selector, text) in [
        ("null", r#"{"_id":null}"#),
        ("number", r#"{"_id":7}"#),
        ("object", r#"{"_id":{"part":1}}"#),
        ("array", r#"{"_id":[]}"#),
        ("empty", r#"{"_id":""}"#),
    ] {
        store
            .record_intent(selector, &JsonObject::parse(text).unwrap(), 0)
            .unwrap();
        assert!(store.get_intent(selector).is_none());
    }
    assert_eq!(store.documents().len(), 5);
    assert!(store.get_intent("").is_some());
    assert_eq!(
        store.record_intent("another", &JsonObject::parse(r#"{"_id":[]}"#).unwrap(), 1),
        Err(IntentError::DuplicateId)
    );
    checkpoint_roundtrip(&store);
}

#[test]
fn address_index_matches_array_members_once_and_keeps_original_row_order() {
    let mut store = IssuanceIntentStore::new();
    for selector in ["z", "a", "m"] {
        store
            .record_intent(
                selector,
                &JsonObject::parse(r#"{"recipientAddress":["r","r",null,"s"]}"#).unwrap(),
                0,
            )
            .unwrap();
    }
    let order = |store: &IssuanceIntentStore| {
        store
            .get_intents_by_address("r")
            .iter()
            .map(|document| document.get("_id").unwrap().clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        order(&store),
        vec![
            JsonValue::from("z"),
            JsonValue::from("a"),
            JsonValue::from("m")
        ]
    );
    store
        .record_intent(
            "a",
            &JsonObject::parse(r#"{"recipientAddress":"s"}"#).unwrap(),
            1,
        )
        .unwrap();
    assert_eq!(
        order(&store),
        vec![JsonValue::from("z"), JsonValue::from("m")]
    );
    store
        .record_intent(
            "a",
            &JsonObject::parse(r#"{"recipientAddress":"r"}"#).unwrap(),
            2,
        )
        .unwrap();
    assert_eq!(
        order(&store),
        vec![
            JsonValue::from("z"),
            JsonValue::from("a"),
            JsonValue::from("m")
        ]
    );
    checkpoint_roundtrip(&store);
}

#[test]
fn address_index_recurses_only_through_arrays_and_replays_replacements() {
    let mut store = IssuanceIntentStore::new();
    let nested = JsonObject::parse(r#"{"recipientAddress":["s",[["r","r"],["r2",{"ignored":"r3"}]]],"unrelated":{"recipientAddress":"r3"}}"#).unwrap();
    store.record_intent("nested", &nested, 1).unwrap();
    assert_eq!(store.get_intents_by_address("r").len(), 1);
    assert_eq!(store.get_intents_by_address("r2").len(), 1);
    assert!(store.get_intents_by_address("r3").is_empty());
    let bytes = store.checkpoint_bytes().unwrap();
    let mut store = IssuanceIntentStore::from_checkpoint_bytes(&bytes).unwrap();
    assert_eq!(store.get_intents_by_address("r").len(), 1);
    store
        .record_intent(
            "nested",
            &JsonObject::parse(r#"{"recipientAddress":[["other"]]}"#).unwrap(),
            2,
        )
        .unwrap();
    assert!(store.get_intents_by_address("r").is_empty());
    assert!(store.get_intents_by_address("r2").is_empty());
    assert_eq!(store.get_intents_by_address("other").len(), 1);
    checkpoint_roundtrip(&store);
}

#[test]
fn json_number_serialization_and_negative_zero_are_exact_finite_source_values() {
    for (text, expected) in [
        ("-0", "0"),
        ("-0.0e+10", "0"),
        ("1e-999", "0"),
        ("9007199254740993", "9007199254740992"),
        ("1e21", "1e+21"),
        ("1e-7", "1e-7"),
        ("0.000001", "0.000001"),
    ] {
        let value = JsonValue::parse(text).unwrap();
        assert_eq!(value.to_json().unwrap(), expected);
        assert_eq!(JsonValue::parse(&value.to_json().unwrap()).unwrap(), value);
    }
    assert_eq!(
        JsonNumber::from_f64(-0.0).unwrap(),
        JsonNumber::from_f64(0.0).unwrap()
    );
    assert!(JsonNumber::from_f64(f64::NAN).is_err());
    assert!(JsonNumber::from_f64(f64::INFINITY).is_err());
    assert!(JsonValue::parse("1e999").is_err());
}

#[test]
fn duplicate_json_keys_keep_first_slot_and_last_value_like_source_parse() {
    let value = JsonValue::parse(r#"{"z":1,"a":2,"z":3,"3":"x","1":"y","a":4}"#).unwrap();
    assert_eq!(value.to_json().unwrap(), r#"{"1":"y","3":"x","z":3,"a":4}"#);
    let mut object = JsonObject::new();
    object.insert("z".into(), JsonValue::Null).unwrap();
    object.insert("2".into(), JsonValue::from("two")).unwrap();
    object.insert("z".into(), JsonValue::Bool(true)).unwrap();
    assert_eq!(object.to_json().unwrap(), r#"{"2":"two","z":true}"#);
}

#[test]
fn malformed_json_and_unsupported_utf16_surrogates_are_explicit_errors() {
    for text in [
        "",
        "[",
        "{",
        "01",
        "-",
        ".1",
        "1.",
        "1e",
        "1e+",
        "NaN",
        "Infinity",
        "null trailing",
        "[1,]",
        "{\"a\":1,}",
        "{\"a\" 1}",
        "\"raw\nnewline\"",
        "\"\\x41\"",
    ] {
        assert!(JsonValue::parse(text).is_err(), "{text}");
    }
    for text in [r#""\ud800""#, r#""\udc00""#, r#""\ud800x""#] {
        assert_eq!(
            JsonValue::parse(text),
            Err(IntentError::UnsupportedSurrogate)
        );
    }
    assert_eq!(
        JsonValue::parse(r#""\ud83d\ude00""#)
            .unwrap()
            .to_json()
            .unwrap(),
        "\"😀\""
    );
    let value = JsonValue::parse(r#"{"text":"雪\u0000\b\f\n\r\t\"\\/"}"#).unwrap();
    assert_eq!(JsonValue::parse(&value.to_json().unwrap()).unwrap(), value);
    assert!(JsonObject::parse("[]").is_err());
}

#[test]
fn depth_node_and_projection_limits_are_checked_without_partial_mutation() {
    let depth = format!(
        "{}null{}",
        "[".repeat(MAX_JSON_DEPTH + 1),
        "]".repeat(MAX_JSON_DEPTH + 1)
    );
    assert_eq!(JsonValue::parse(&depth), Err(IntentError::DepthLimit));
    let nodes = format!("[{}]", vec!["null"; MAX_JSON_NODES].join(","));
    assert_eq!(JsonValue::parse(&nodes), Err(IntentError::Capacity));
    let mut object = JsonObject::new();
    let before = object.clone();
    assert_eq!(
        object.insert(
            "large".into(),
            JsonValue::String("x".repeat(MAX_JSON_BYTES + 1))
        ),
        Err(IntentError::Capacity)
    );
    assert_eq!(object, before);
    let mut store = IssuanceIntentStore::new();
    let before = store.clone();
    let mut near_cap = JsonObject::new();
    near_cap
        .insert(
            "memo".into(),
            JsonValue::String("x".repeat(MAX_JSON_BYTES - 20)),
        )
        .unwrap();
    assert_eq!(
        store.record_intent("selector", &near_cap, 1),
        Err(IntentError::Capacity)
    );
    assert_eq!(store, before);
    assert_eq!(
        store.record_intent("", &object, 0),
        Err(IntentError::MissingSelector)
    );
    assert_eq!(
        store.record_intent(&"x".repeat(MAX_SELECTOR_BYTES + 1), &object, 0),
        Err(IntentError::SelectorLimit)
    );
    assert_eq!(
        store.record_intent("x", &object, MAX_SAFE_MILLIS + 1),
        Err(IntentError::InvalidMillis)
    );
    assert_eq!(store, before);
    store.record_intent("x", &object, MAX_SAFE_MILLIS).unwrap();
    checkpoint_roundtrip(&store);
}

#[test]
fn row_and_journal_caps_reject_atomically_but_existing_rows_can_update() {
    let empty = JsonObject::new();
    let mut store = IssuanceIntentStore::new();
    for index in 0..MAX_INTENTS {
        store
            .record_intent(&format!("row{index}"), &empty, 0)
            .unwrap();
    }
    let before = store.clone();
    assert_eq!(
        store.record_intent("overflow", &empty, 0),
        Err(IntentError::Capacity)
    );
    assert_eq!(store, before);
    store.record_intent("row0", &empty, 1).unwrap();
    let mut history = IssuanceIntentStore::new();
    for _ in 0..MAX_INTENT_WRITES {
        history.record_intent("same", &empty, 0).unwrap();
    }
    let before = history.clone();
    assert_eq!(
        history.record_intent("same", &empty, 0),
        Err(IntentError::Capacity)
    );
    assert_eq!(history, before);
    assert_eq!(history.documents().len(), 1);
    checkpoint_roundtrip(&store);
}

#[test]
fn aggregate_nested_journal_budget_cannot_be_bypassed_by_replacing_one_row() {
    let mut intent = JsonObject::new();
    intent
        .insert("nodes".into(), JsonValue::Array(vec![JsonValue::Null; 500]))
        .unwrap();
    let mut store = IssuanceIntentStore::new();
    let mut accepted = 0;
    loop {
        let before = store.clone();
        match store.record_intent("same", &intent, 0) {
            Ok(_) => accepted += 1,
            Err(IntentError::Capacity) => {
                assert_eq!(store, before);
                break;
            }
            result => panic!("unexpected budget result {result:?}"),
        }
        assert!(accepted < MAX_INTENT_WRITES);
    }
    assert!(accepted > 1);
    assert_eq!(store.documents().len(), 1);
    checkpoint_roundtrip(&store);
}

fn fix_crc(bytes: &mut [u8]) {
    let at = bytes.len() - 4;
    let mut crc = !0_u32;
    for byte in &bytes[..at] {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & 0_u32.wrapping_sub(crc & 1));
        }
    }
    bytes[at..].copy_from_slice(&(!crc).to_be_bytes());
}

#[test]
fn checkpoint_crc_and_semantic_replay_both_reject_damage() {
    let mut store = IssuanceIntentStore::new();
    store
        .record_intent(
            "x",
            &JsonObject::parse(r#"{"recipientAddress":"r","n":1}"#).unwrap(),
            1,
        )
        .unwrap();
    let original = store.checkpoint_bytes().unwrap();
    for end in [0, 4, 5, original.len() - 1] {
        assert!(IssuanceIntentStore::from_checkpoint_bytes(&original[..end]).is_err());
    }
    let mut corrupt = original.clone();
    corrupt[0] ^= 1;
    assert!(IssuanceIntentStore::from_checkpoint_bytes(&corrupt).is_err());
    let mut wrong_projection = original.clone();
    let needle = b"\"updatedAt\":1";
    let at = wrong_projection
        .windows(needle.len())
        .position(|window| window == needle)
        .unwrap();
    wrong_projection[at + needle.len() - 1] = b'2';
    fix_crc(&mut wrong_projection);
    assert_eq!(
        IssuanceIntentStore::from_checkpoint_bytes(&wrong_projection),
        Err(IntentError::InvalidCheckpoint)
    );
    let mut overflow = original.clone();
    overflow[5..9].copy_from_slice(&u32::MAX.to_be_bytes());
    fix_crc(&mut overflow);
    assert!(IssuanceIntentStore::from_checkpoint_bytes(&overflow).is_err());
    let mut trailing = original.clone();
    let at = trailing.len() - 4;
    trailing.insert(at, 0);
    fix_crc(&mut trailing);
    assert!(IssuanceIntentStore::from_checkpoint_bytes(&trailing).is_err());
    checkpoint_roundtrip(&store);
}

#[test]
fn checkpoints_require_canonical_json_and_valid_supplied_millis() {
    let mut store = IssuanceIntentStore::new();
    store
        .record_intent("x", &JsonObject::parse(r#"{"n":1}"#).unwrap(), 1)
        .unwrap();
    let original = store.checkpoint_bytes().unwrap();
    // Magic5,count4,selector-length4,selector1,millis8,intent-length4.
    let length_at = 5 + 4 + 4 + 1 + 8;
    let text_at = length_at + 4;
    let old_len = u32::from_be_bytes(original[length_at..text_at].try_into().unwrap()) as usize;
    let noncanonical = br#"{"n":1.0}"#;
    let mut changed = original[..length_at].to_vec();
    changed.extend_from_slice(&(noncanonical.len() as u32).to_be_bytes());
    changed.extend_from_slice(noncanonical);
    changed.extend_from_slice(&original[text_at + old_len..]);
    fix_crc(&mut changed);
    assert!(IssuanceIntentStore::from_checkpoint_bytes(&changed).is_err());
    let mut invalid_time = original.clone();
    invalid_time[14..22].copy_from_slice(&(MAX_SAFE_MILLIS + 1).to_be_bytes());
    fix_crc(&mut invalid_time);
    assert!(IssuanceIntentStore::from_checkpoint_bytes(&invalid_time).is_err());
    let mut invalid_utf8 = original.clone();
    invalid_utf8[text_at + 2] = 0xff;
    fix_crc(&mut invalid_utf8);
    assert!(IssuanceIntentStore::from_checkpoint_bytes(&invalid_utf8).is_err());
}
