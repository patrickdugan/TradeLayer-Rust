//! Complete-source encoder, decoder and Types composition regression corpus.
//! The complete Types VM uses an identity validity adapter, never chain validation.
use std::collections::{BTreeMap, BTreeSet};
use tradelayer_core::issuance_intent::JsonValue;
use tradelayer_core::legacy_txdecoder_audit::{
    source_decode_historical_method_unverified, source_decode_historical_type_unverified,
    HistoricalDecoderError, LegacyDecoderError, HISTORICAL_DECODER_METHODS,
};
use tradelayer_core::legacy_txencoder_audit::{
    source_encode_method_unverified, LegacyEncoderError, SourceEncoderReturn,
    SOURCE_ENCODER_METHODS,
};
use tradelayer_core::legacy_types_audit::{
    source_generic_concat_unverified, source_inspect_decode_invocation, source_route,
    LegacyTypesError, SourceDecodeOutcome, SourceDecoderInvocation,
};

const VECTORS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/fixtures/wire_roundtrip_vectors.tsv"
));

fn unhex(text: &str) -> String {
    if text == "-" {
        return String::new();
    }
    assert!(text.len().is_multiple_of(2));
    String::from_utf8(
        text.as_bytes()
            .chunks_exact(2)
            .map(|pair| u8::from_str_radix(core::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect(),
    )
    .unwrap()
}

fn json(text: &str) -> JsonValue {
    JsonValue::parse(&unhex(text)).unwrap()
}

fn string(value: &JsonValue) -> &str {
    let JsonValue::String(value) = value else {
        panic!("expected string: {value:?}");
    };
    value
}

fn array(value: &JsonValue) -> &[JsonValue] {
    let JsonValue::Array(value) = value else {
        panic!("expected array: {value:?}");
    };
    value
}

fn rows() -> impl Iterator<Item = Vec<&'static str>> {
    VECTORS
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let fields: Vec<_> = line.split('\t').collect();
            assert_eq!(fields.len(), 18, "{line}");
            assert_eq!(fields[0], "W");
            fields
        })
}

#[test]
fn complete_composition_encoders_and_explicit_typed_restrictions() {
    let mut methods = BTreeSet::new();
    let mut dispositions = BTreeMap::new();
    let mut generic_types = BTreeSet::new();
    for row in rows() {
        *dispositions.entry(row[6]).or_insert(0) += 1;
        let input = json(row[5]);
        let expected = json(row[8]);
        if row[2] == "raw" {
            assert_eq!(row[6], "raw");
            assert_eq!(row[7], "ok");
            assert_eq!(string(&input), string(&expected), "{}", row[1]);
            continue;
        }
        methods.insert(row[4]);
        let result = source_encode_method_unverified(row[4], &input);
        match row[6] {
            // Recorded before the literal dispatcher kept Unicode text
            // (aeebb65) and fractional radix36 (125e63d); both now reproduce
            // the source output exactly.
            "ascii-reject" | "radix-reject" if row[1] != "unsafe-contract-id" => {
                assert_eq!(row[7], "ok", "{}", row[1]);
                assert_eq!(
                    result.unwrap().source_string().unwrap(),
                    string(&expected),
                    "{}",
                    row[1]
                );
            }
            "radix-reject" => {
                assert_eq!(
                    result,
                    Err(LegacyEncoderError::UnsupportedNumberRadix36),
                    "{}",
                    row[1]
                );
                assert_eq!(row[7], "ok", "source emits V8 Number radix36");
            }
            "numeric-reject" => {
                assert_eq!(
                    result,
                    Err(LegacyEncoderError::UnsupportedNumericCoercion),
                    "{}",
                    row[1]
                );
                assert_eq!(row[7], "ok", "source emits nonfinite BigNumber text");
            }
            "source-throw" => {
                assert_eq!(row[7], "throw");
                assert!(
                    result.is_err(),
                    "{} must reject actual source failure",
                    row[1]
                );
                assert_eq!(array(&expected).len(), 2);
            }
            "exact" => {
                if row[2] == "generic" {
                    let type_id: u8 = row[3].parse().unwrap();
                    generic_types.insert(type_id);
                    let inner = result.unwrap();
                    let assembly =
                        source_generic_concat_unverified(type_id, inner.source_string().unwrap());
                    if type_id == 4 {
                        assert_eq!(assembly, Err(LegacyTypesError::MissingSourceEncoder(4)));
                        assert_eq!(row[7], "throw");
                        assert_eq!(string(&array(&expected)[0]), "TypeError");
                    } else {
                        assert_eq!(row[7], "ok", "{}", row[1]);
                        let assembly = assembly.unwrap();
                        assert_eq!(assembly.claimed_ascii(), string(&expected), "{}", row[1]);
                        assert_eq!(
                            assembly.require_encoded_transaction_authority(),
                            Err(LegacyTypesError::TrustedTransactionExecutionForbidden)
                        );
                    }
                } else if row[7] == "undefined" {
                    assert_eq!(result, Ok(SourceEncoderReturn::Undefined));
                    assert_eq!(expected, JsonValue::Null);
                } else {
                    assert_eq!(row[7], "ok", "{}", row[1]);
                    assert_eq!(
                        result.unwrap().source_string().unwrap(),
                        string(&expected),
                        "{}",
                        row[1]
                    );
                }
            }
            other => panic!("unhandled disposition {other}"),
        }
    }
    assert_eq!(methods, SOURCE_ENCODER_METHODS.iter().copied().collect());
    assert_eq!(generic_types, (0..36).collect());
    assert_eq!(dispositions.values().sum::<usize>(), 169);
    assert_eq!(dispositions["ascii-reject"], 2);
    assert_eq!(dispositions["radix-reject"], 2);
    assert_eq!(dispositions["numeric-reject"], 2);
    assert_eq!(dispositions["source-throw"], 3);
    assert_eq!(dispositions["raw"], 11);
}

#[test]
fn exact_composed_bodies_match_all_historical_decoder_exports() {
    let mut methods = BTreeSet::new();
    let mut seen = BTreeMap::new();
    for row in rows() {
        *seen.entry(row[12]).or_insert(0) += 1;
        if row[12] == "unreached" {
            assert!(matches!(row[7], "throw" | "undefined"));
            assert_eq!(row[11], "-");
            assert_eq!(json(row[13]), JsonValue::Null);
            continue;
        }
        let body = unhex(row[11]);
        let result = source_decode_historical_method_unverified(row[10], &body);
        match row[12] {
            "ok" => {
                methods.insert(row[10]);
                let observation = result.unwrap_or_else(|error| panic!("{}: {error:?}", row[1]));
                assert_eq!(
                    observation.diagnostic_json().unwrap(),
                    unhex(row[13]),
                    "{}",
                    row[1]
                );
                assert_eq!(
                    observation.require_trusted_execution(),
                    Err(LegacyDecoderError::TrustedEconomicExecutionForbidden)
                );
                assert_eq!(
                    observation.require_historically_accepted_transaction(),
                    Err(LegacyDecoderError::HistoricalTransactionNotAuthenticated)
                );
            }
            "throw" => {
                methods.insert(row[10]);
                let expected = json(row[13]);
                let expected = array(&expected);
                assert_eq!(expected.len(), 2);
                match result {
                    Err(HistoricalDecoderError::SourceThrow { name, message }) => {
                        assert_eq!(name, string(&expected[0]), "{}", row[1]);
                        assert_eq!(message, string(&expected[1]), "{}", row[1]);
                    }
                    other => panic!("{} expected exact source throw; got {other:?}", row[1]),
                }
            }
            "missing" => {
                assert_eq!(
                    result,
                    Err(HistoricalDecoderError::MissingMethod(row[10].to_owned()))
                );
                assert_eq!(string(&json(row[13])), row[10]);
            }
            other => panic!("unhandled decoder status {other}"),
        }
    }
    assert_eq!(
        methods,
        HISTORICAL_DECODER_METHODS.iter().copied().collect()
    );
    assert_eq!(seen.values().sum::<usize>(), 169);
    assert!(seen["ok"] > 140);
    assert!(seen["throw"] > 0);
    assert!(seen["missing"] > 0);
}

#[test]
fn body_transforms_are_explicit_and_do_not_repair_generic_double_prefixes() {
    for row in rows().filter(|row| row[7] == "ok") {
        let encoding = json(row[8]);
        let encoding = string(&encoding);
        let prefix = (row[3] != "-").then(|| {
            let id: u8 = row[3].parse().unwrap();
            format!("tl{}", char::from_digit(u32::from(id), 36).unwrap())
        });
        let peeled = prefix
            .as_deref()
            .and_then(|prefix| encoding.strip_prefix(prefix))
            .unwrap_or(encoding);
        let expected = match row[9] {
            "strip-one" => encoding.get(3..).unwrap(),
            "strip-one-if-present" => peeled,
            "identity" | "raw" => encoding,
            "truncate" => {
                assert!(
                    peeled.is_ascii(),
                    "all truncated source arguments are ASCII"
                );
                &peeled[..peeled.len().min(2)]
            }
            "append" => {
                assert_eq!(unhex(row[11]), format!("{peeled},extra;|,0"), "{}", row[1]);
                continue;
            }
            other => panic!("unknown body policy {other}"),
        };
        assert_eq!(unhex(row[11]), expected, "{}", row[1]);
    }
}

#[test]
fn source_dispatch_traces_bind_static_routes_without_claiming_validation() {
    let mut traced_types = BTreeSet::new();
    let mut returned_errors = 0;
    for row in rows() {
        let trace = json(row[16]);
        let trace = array(&trace);
        if row[14] == "unreached" {
            assert!(trace.is_empty());
            assert!(row[3] == "-" || row[7] != "ok");
            continue;
        }
        let type_id: u8 = row[3].parse().unwrap();
        let route = source_route(type_id).unwrap();
        for entry in trace {
            let entry = array(entry);
            assert_eq!(entry.len(), 2);
            assert_eq!(Some(string(&entry[0])), route.validator, "{}", row[1]);
            assert!(route.validator_invoked_by_source);
            let arguments = array(&entry[1]);
            assert_eq!(string(&arguments[0]), "array");
            let arguments = array(&arguments[1]);
            assert_eq!(
                arguments[1],
                json(row[15]),
                "identity adapter returns exact params"
            );
            traced_types.insert(type_id);
        }
        if !route.decoder_exported && row[14] == "throw" {
            assert!(matches!(
                source_decode_historical_type_unverified(type_id, &unhex(row[11])),
                Err(HistoricalDecoderError::MissingMethod(_))
            ));
        }
        if row[14] == "error" {
            returned_errors += 1;
            let call = if row[1].starts_with("txindex-three-args") {
                SourceDecoderInvocation::TxIndexThreeArguments {
                    payload_after_marker: if row[1].ends_with("-tl") {
                        "tl"
                    } else {
                        "11,2,1"
                    },
                }
            } else {
                assert_eq!(row[1], "bad-marker-returned-error");
                SourceDecoderInvocation::FullNineArguments {
                    type_id,
                    marker: "xx",
                    type3_both_output_refs_claimed: false,
                }
            };
            let observation = source_inspect_decode_invocation(call);
            assert!(matches!(
                observation.outcome(),
                SourceDecodeOutcome::ReturnedErrorObjectInvalidMarker
                    | SourceDecodeOutcome::ReturnedErrorObjectUnknownType
            ));
            assert_eq!(
                observation.require_trusted_execution(),
                Err(LegacyTypesError::TrustedTransactionExecutionForbidden)
            );
        }
    }
    assert_eq!(returned_errors, 3);
    assert_eq!(traced_types.len(), 30);
    assert!(
        traced_types.contains(&15),
        "source close calls publish validator"
    );
    assert!(!traced_types.contains(&32));
    assert!(!traced_types.contains(&33));
}
