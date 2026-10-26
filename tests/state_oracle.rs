//! Independent source bytes plus immutable real-custody query regressions.
use core::cmp::Ordering;
use tradelayer_core::issuance_intent::{JsonNumber, JsonValue};
use tradelayer_core::ledger::{
    AccountId, ChannelSide, Ledger, Operation, PropertyId, PropertyKind, TxId,
};
use tradelayer_core::state_oracle::{
    bucketize, build_address_balance_payload, build_address_balance_payload_with,
    compare_addresses, decode_balance_payload, encode_balance_payload, payload_hash_from_b64,
    quantize8, StateOracleError, MAX_ADDRESSES, MAX_BASE64_BYTES, MAX_PAYLOAD_BYTES,
};

fn a(text: &str) -> AccountId {
    text.try_into().unwrap()
}
fn p() -> PropertyId {
    7.try_into().unwrap()
}
fn id(value: u8) -> TxId {
    TxId::from_bytes([value; 32])
}
fn unhex(text: &str) -> Vec<u8> {
    if text == "-" {
        return vec![];
    }
    assert!(text.len().is_multiple_of(2));
    (0..text.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&text[at..at + 2], 16).unwrap())
        .collect()
}
fn utf8_hex(text: &str) -> String {
    String::from_utf8(unhex(text)).unwrap()
}
fn float_bits(text: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(text, 16).unwrap())
}
fn b64(bytes: &[u8]) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let word = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for shift in [18, 12, 6, 0] {
            let at = (word >> shift) as usize & 63;
            out.push(char::from(TABLE[at]));
        }
        if chunk.len() < 3 {
            out.pop();
            out.push('=');
        }
        if chunk.len() < 2 {
            let at = out.len() - 2;
            out.replace_range(at..at + 1, "=");
        }
    }
    out
}

#[test]
fn all_source_number_payload_generic_encoding_and_loose_hash_vectors() {
    let mut counts = [0_usize; 4];
    let mut unsupported = 0;
    for line in include_str!("../fixtures/state_oracle_vectors.tsv")
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
    {
        let fields: Vec<_> = line.split('\t').collect();
        match fields[0] {
            "N" => {
                let number =
                    JsonValue::Number(JsonNumber::from_f64(float_bits(fields[2])).unwrap());
                assert_eq!(
                    number.to_json().unwrap(),
                    utf8_hex(fields[3]),
                    "{}",
                    fields[1]
                );
                let array = JsonValue::Array(vec![number]);
                assert_eq!(
                    encode_balance_payload(Some(&array)).unwrap(),
                    fields[4],
                    "{}",
                    fields[1]
                );
                assert_eq!(
                    payload_hash_from_b64(fields[4]).unwrap(),
                    fields[5],
                    "{}",
                    fields[1]
                );
                counts[0] += 1;
            }
            "P" => {
                let property: u32 = fields[2].parse().unwrap();
                let bucket = if fields[3] == "default" {
                    1.0
                } else {
                    float_bits(fields[3])
                };
                let observations: Vec<_> = if fields[4] == "-" {
                    vec![]
                } else {
                    fields[4]
                        .split(';')
                        .map(|entry| {
                            let (address, value) = entry.split_once(':').unwrap();
                            (
                                utf8_hex(address),
                                match value {
                                    "m" | "e" | "n" => None,
                                    bits => Some(float_bits(bits)),
                                },
                            )
                        })
                        .collect()
                };
                let json = utf8_hex(fields[5]);
                let generic = JsonValue::parse(&json).unwrap();
                assert_eq!(
                    encode_balance_payload(Some(&generic)).unwrap(),
                    fields[6],
                    "{}",
                    fields[1]
                );
                assert_eq!(
                    payload_hash_from_b64(fields[6]).unwrap(),
                    fields[7],
                    "{}",
                    fields[1]
                );
                let addresses = observations
                    .iter()
                    .map(|(address, _)| AccountId::try_from(address.as_str()))
                    .collect::<Result<Vec<_>, _>>();
                match addresses {
                    Err(_) => {
                        assert!(decode_balance_payload(fields[6]).is_err(), "{}", fields[1]);
                        unsupported += 1;
                    }
                    Ok(addresses) => {
                        let mut index = 0;
                        let payload = build_address_balance_payload_with(
                            property,
                            &addresses,
                            bucket,
                            |address, requested| {
                                assert_eq!(requested, property);
                                assert_eq!(address.as_str(), observations[index].0);
                                let available = observations[index].1;
                                index += 1;
                                Ok(available)
                            },
                        );
                        match payload {
                            Ok(payload) => {
                                assert_eq!(index, observations.len());
                                assert_eq!(payload.property_id(), property);
                                assert_eq!(
                                    payload.canonical_json().unwrap(),
                                    json,
                                    "{}",
                                    fields[1]
                                );
                                assert_eq!(
                                    payload.encode_base64().unwrap(),
                                    fields[6],
                                    "{}",
                                    fields[1]
                                );
                                assert_eq!(
                                    payload.sha256_hex().unwrap(),
                                    fields[7],
                                    "{}",
                                    fields[1]
                                );
                                assert_eq!(
                                    decode_balance_payload(fields[6]).unwrap(),
                                    payload,
                                    "{}",
                                    fields[1]
                                );
                            }
                            Err(StateOracleError::NonFinite) => {
                                let nonfinite_input = !bucket.is_finite()
                                    || observations.iter().any(|(_, value)| {
                                        value.is_some_and(|value| !value.is_finite())
                                    });
                                assert!(
                                    fields[1].starts_with("unsupported-")
                                        && (json.contains(":null") || nonfinite_input),
                                    "unexpected nonfinite rejection: {line}"
                                );
                                unsupported += 1;
                            }
                            other => panic!(
                                "unexpected supported-domain result for {}: {other:?}",
                                fields[1]
                            ),
                        }
                    }
                }
                counts[1] += 1;
            }
            "E" => {
                let input = JsonValue::parse(&utf8_hex(fields[2])).unwrap();
                let actual = encode_balance_payload(Some(&input)).unwrap();
                assert_eq!(actual, fields[4], "{}", fields[1]);
                assert_eq!(b64(utf8_hex(fields[3]).as_bytes()), actual, "{}", fields[1]);
                assert_eq!(
                    payload_hash_from_b64(&actual).unwrap(),
                    fields[5],
                    "{}",
                    fields[1]
                );
                counts[2] += 1;
            }
            "H" => {
                let input = utf8_hex(fields[2]);
                let decoded_bytes = unhex(fields[3]);
                assert_eq!(
                    payload_hash_from_b64(&input).unwrap(),
                    fields[4],
                    "{}",
                    fields[1]
                );
                assert_eq!(
                    payload_hash_from_b64(&b64(&decoded_bytes)).unwrap(),
                    fields[4],
                    "{}",
                    fields[1]
                );
                counts[3] += 1;
            }
            other => panic!("unknown source fixture {other}"),
        }
    }
    assert_eq!(counts, [2136, 91, 17, 75]);
    assert_eq!(unsupported, 8);
}

fn custody() -> Ledger {
    let mut ledger = Ledger::new();
    ledger
        .apply(
            id(1),
            Operation::CreateProperty {
                id: p(),
                issuer: a("alice"),
                kind: PropertyKind::Fixed,
                initial_atoms: 2_000_000_000,
            },
        )
        .unwrap();
    ledger
        .apply(
            id(2),
            Operation::Transfer {
                id: p(),
                from: a("alice"),
                to: a("bob"),
                amount_atoms: 700_000_001,
            },
        )
        .unwrap();
    ledger
        .apply(
            id(3),
            Operation::ChannelCommit {
                id: p(),
                owner: a("alice"),
                channel: a("channel"),
                side: ChannelSide::A,
                amount_atoms: 300_000_000,
            },
        )
        .unwrap();
    ledger
}

#[test]
fn real_ledger_projection_reads_available_only_without_mutating_custody_or_ids() {
    let ledger = custody();
    let before = ledger.clone();
    let bytes = ledger.snapshot_bytes().unwrap();
    let payload =
        build_address_balance_payload(&ledger, p(), &[a("bob"), a("missing"), a("alice")], 1.0)
            .unwrap();
    assert_eq!(payload.property_id(), 7);
    assert_eq!(payload.bucket_size(), 1.0);
    assert_eq!(
        ledger.channel_atoms(p(), &a("channel"), ChannelSide::A),
        300_000_000
    );
    let rows = payload.rows();
    assert_eq!(
        rows.iter()
            .map(|row| row.address.as_str())
            .collect::<Vec<_>>(),
        ["alice", "bob", "missing"]
    );
    // Binary64 multiplication floors this projected scalar down one atom;
    // actual owned custody remains 999,999,999 atoms below.
    assert_eq!(rows[0].available, 9.99999998);
    assert_eq!(rows[0].bucket_min, 9.0);
    assert_eq!(rows[0].bucket_max, 10.0);
    assert_eq!(rows[1].available, 7.00000001);
    assert_eq!(rows[2].available, 0.0);
    assert_eq!(ledger, before);
    assert_eq!(ledger.snapshot_bytes().unwrap(), bytes);
    assert_eq!(ledger.processed_count(), 3);
    ledger.verify_conservation().unwrap();
    let decoded = decode_balance_payload(&payload.encode_base64().unwrap()).unwrap();
    assert_eq!(decoded, payload);
    assert_eq!(ledger, before);
    let unknown =
        build_address_balance_payload(&ledger, 8.try_into().unwrap(), &[a("alice")], 1.0).unwrap();
    assert_eq!(unknown.rows()[0].available, 0.0);
    assert_eq!(ledger, before);
}

#[test]
fn source_queries_run_in_supplied_order_and_stable_duplicate_rows_survive_sorting() {
    let addresses = [a("b"), a("a"), a("a"), a("A")];
    let mut visits = Vec::new();
    let payload = build_address_balance_payload_with(0, &addresses, 1.0, |address, property| {
        assert_eq!(property, 0);
        visits.push(address.clone());
        Ok(Some(visits.len() as f64))
    })
    .unwrap();
    assert_eq!(visits, addresses);
    assert_eq!(
        payload
            .rows()
            .iter()
            .map(|row| (row.address.as_str(), row.available))
            .collect::<Vec<_>>(),
        [("a", 2.0), ("a", 3.0), ("A", 4.0), ("b", 1.0)]
    );
    let mut queried = 0;
    assert_eq!(
        build_address_balance_payload_with(7, &addresses, 1.0, |_, _| {
            queried += 1;
            if queried == 2 {
                Err(StateOracleError::Arithmetic)
            } else {
                Ok(Some(1.0))
            }
        }),
        Err(StateOracleError::Arithmetic)
    );
    assert_eq!(queried, 2);
}

#[test]
fn full_printable_account_domain_and_locale_case_ties_are_supported() {
    let addresses: Vec<_> = (0x21_u8..=0x7e)
        .map(|byte| a(&char::from(byte).to_string()))
        .collect();
    let payload = build_address_balance_payload_with(1, &addresses, 1.0, |_, _| Ok(None)).unwrap();
    assert_eq!(payload.rows().len(), 94);
    for pair in payload.rows().windows(2) {
        assert_eq!(
            compare_addresses(&pair[0].address, &pair[1].address),
            Ordering::Less
        );
    }
    assert_eq!(compare_addresses(&a("a"), &a("A")), Ordering::Less);
    assert_eq!(compare_addresses(&a("aB"), &a("Ab")), Ordering::Less);
    assert_eq!(
        compare_addresses(&a("prefix"), &a("prefixLong")),
        Ordering::Less
    );
    assert_eq!(compare_addresses(&a("same"), &a("same")), Ordering::Equal);
    assert_eq!(
        decode_balance_payload(&payload.encode_base64().unwrap()).unwrap(),
        payload
    );
}

#[test]
fn binary64_floor_and_minimum_step_preserve_negative_dust_and_stored_zero_step() {
    assert_eq!(quantize8(-0.0).unwrap().to_bits(), 0);
    assert_eq!(quantize8(-0.000000001).unwrap(), -0.00000001);
    assert_eq!(quantize8(0.000000001).unwrap(), 0.0);
    let bucket = bucketize(-0.000000001, 0.0).unwrap();
    assert_eq!(bucket.bucket_min, -0.00000001);
    assert_eq!(bucket.bucket_max, 0.0);
    let payload =
        build_address_balance_payload_with(1, &[a("a")], 0.0, |_, _| Ok(Some(0.0))).unwrap();
    assert_eq!(payload.bucket_size(), 0.0);
    assert_eq!(payload.rows()[0].bucket_min, 0.0);
    assert_eq!(payload.rows()[0].bucket_max, 0.00000001);
    let negative_step =
        build_address_balance_payload_with(1, &[a("a")], -1.0, |_, _| Ok(None)).unwrap();
    assert_eq!(negative_step.bucket_size(), -1.0);
    assert_eq!(negative_step.rows()[0].bucket_max, 0.00000001);
}

#[test]
fn nonfinite_and_overflow_inputs_fail_before_publishing_a_partial_payload() {
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, f64::MAX] {
        assert_eq!(quantize8(value), Err(StateOracleError::NonFinite));
        assert_eq!(bucketize(0.0, value), Err(StateOracleError::NonFinite));
        let mut visited = 0;
        assert_eq!(
            build_address_balance_payload_with(7, &[a("a")], value, |_, _| {
                visited += 1;
                Ok(Some(0.0))
            }),
            Err(StateOracleError::NonFinite)
        );
        assert_eq!(visited, 0);
        assert_eq!(
            build_address_balance_payload_with(7, &[a("a")], 1.0, |_, _| Ok(Some(value))),
            Err(StateOracleError::NonFinite)
        );
    }
    let ledger = custody();
    let before = ledger.clone();
    assert_eq!(
        build_address_balance_payload(&ledger, p(), &[a("alice")], f64::NAN),
        Err(StateOracleError::NonFinite)
    );
    assert_eq!(ledger, before);
}

#[test]
fn row_and_encoded_byte_caps_apply_before_querying_or_decoding() {
    let addresses = vec![a("same"); MAX_ADDRESSES + 1];
    let mut queried = false;
    assert_eq!(
        build_address_balance_payload_with(7, &addresses, 1.0, |_, _| {
            queried = true;
            Ok(None)
        }),
        Err(StateOracleError::Capacity)
    );
    assert!(!queried);
    let over = "A".repeat(MAX_BASE64_BYTES + 1);
    assert_eq!(
        payload_hash_from_b64(&over),
        Err(StateOracleError::Capacity)
    );
    assert_eq!(
        decode_balance_payload(&over),
        Err(StateOracleError::Capacity)
    );
    // This fits the encoded-character cap but decodes beyond 2MiB.
    let decoded_too_large = "A".repeat((MAX_PAYLOAD_BYTES + 1).div_ceil(3) * 4);
    assert!(decoded_too_large.len() <= MAX_BASE64_BYTES);
    assert_eq!(
        payload_hash_from_b64(&decoded_too_large),
        Err(StateOracleError::Capacity)
    );
}

#[test]
fn permissive_source_hash_is_separate_from_strict_balance_decoding() {
    let payload =
        build_address_balance_payload_with(7, &[a("alice")], 1.0, |_, _| Ok(Some(2.0))).unwrap();
    let canonical = payload.encode_base64().unwrap();
    let hash = payload.sha256_hex().unwrap();
    let whitespace = format!(" {}\n", canonical);
    assert_eq!(payload_hash_from_b64(&whitespace).unwrap(), hash);
    assert_eq!(
        decode_balance_payload(&whitespace),
        Err(StateOracleError::InvalidBase64)
    );
    let padded = build_address_balance_payload_with(7, &[], 1.0, |_, _| Ok(None)).unwrap();
    let padded_b64 = padded.encode_base64().unwrap();
    assert!(padded_b64.ends_with('='));
    let unpadded = padded_b64.trim_end_matches('=');
    assert_eq!(
        payload_hash_from_b64(unpadded).unwrap(),
        padded.sha256_hex().unwrap()
    );
    assert_eq!(
        decode_balance_payload(unpadded),
        Err(StateOracleError::InvalidBase64)
    );
    let junk = format!("{canonical}!!!");
    assert_eq!(payload_hash_from_b64(&junk).unwrap(), hash);
    assert!(decode_balance_payload(&junk).is_err());
    assert_eq!(
        payload_hash_from_b64("").unwrap(),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        payload_hash_from_b64("a").unwrap(),
        payload_hash_from_b64("").unwrap()
    );
    assert!(decode_balance_payload("e30=").is_err());
}

#[test]
fn strict_decode_rejects_reformatted_reordered_and_nonfinite_json() {
    let prefix =
        r#"{"schema":"tl-state-oracle-balance-v1","propertyId":7,"bucketSize":1,"rows":[]}"#;
    assert!(decode_balance_payload(&b64(prefix.as_bytes())).is_ok());
    for json in [
        prefix.replace("propertyId", "property"),
        prefix.replace(":7", ":07"),
        prefix.replace(":1,", ":1.0,"),
        prefix.replace(":1,", ":1e0,"),
        prefix.replace(":1,", ":NaN,"),
        prefix.replace(":1,", ":1e999,"),
        prefix.replace("\"rows\":[]", "\"rows\":[],\"extra\":1"),
        prefix.replace(
            "\"propertyId\":7,\"bucketSize\":1",
            "\"bucketSize\":1,\"propertyId\":7",
        ),
        format!(" {prefix}"),
        format!("{prefix}x"),
    ] {
        assert!(
            decode_balance_payload(&b64(json.as_bytes())).is_err(),
            "{json}"
        );
    }
    let invalid_utf8 = [0xff, 0xfe];
    assert!(decode_balance_payload(&b64(&invalid_utf8)).is_err());
    let unsorted = r#"{"schema":"tl-state-oracle-balance-v1","propertyId":7,"bucketSize":1,"rows":[{"address":"z","available":0,"bucketMin":0,"bucketMax":1},{"address":"a","available":0,"bucketMin":0,"bucketMax":1}]}"#;
    assert_eq!(
        decode_balance_payload(&b64(unsorted.as_bytes())),
        Err(StateOracleError::NonCanonical)
    );
}

#[test]
fn a_new_fingerprint_does_not_make_a_canonical_claim_true() {
    let ledger = custody();
    let before = ledger.clone();
    let actual = build_address_balance_payload(&ledger, p(), &[a("alice")], 1.0).unwrap();
    let truthful = actual.canonical_json().unwrap();
    let fiction = truthful.replace("\"available\":9.99999998", "\"available\":999");
    assert_ne!(fiction, truthful);
    let fiction_b64 = b64(fiction.as_bytes());
    let parsed = decode_balance_payload(&fiction_b64).unwrap();
    assert_eq!(parsed.rows()[0].available, 999.0);
    assert_ne!(parsed.sha256_hex().unwrap(), actual.sha256_hex().unwrap());
    assert_eq!(ledger, before);
    assert_eq!(ledger.available_atoms(p(), &a("alice")), 999_999_999);
}

#[test]
fn generic_encoding_preserves_source_falsy_fallback_and_arbitrary_json_order() {
    assert_eq!(encode_balance_payload(None).unwrap(), "e30=");
    for input in ["null", "false", "0", "-0", "\"\""] {
        assert_eq!(
            encode_balance_payload(Some(&JsonValue::parse(input).unwrap())).unwrap(),
            "e30=",
            "{input}"
        );
    }
    for (input, expected) in [
        ("[]", "W10="),
        ("true", "dHJ1ZQ=="),
        ("1", "MQ=="),
        (r#""hello""#, "ImhlbGxvIg=="),
    ] {
        assert_eq!(
            encode_balance_payload(Some(&JsonValue::parse(input).unwrap())).unwrap(),
            expected
        );
    }
    let input = JsonValue::parse(r#"{"z":1,"2":"two","a":[null,true,0.0000001]}"#).unwrap();
    assert_eq!(
        encode_balance_payload(Some(&input)).unwrap(),
        b64(br#"{"2":"two","z":1,"a":[null,true,1e-7]}"#)
    );
}
