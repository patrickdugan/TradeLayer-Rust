use tradelayer_core::issuance_intent::JsonValue;
use tradelayer_core::legacy_txencoder_audit::{
    source_encode_create_whitelist_unverified, source_encode_method_unverified, LegacyEncoderError,
    SourceEncoderReturn,
};
use tradelayer_core::legacy_types_audit::LegacyTypesError;

fn unhex(raw: &str) -> String {
    if raw == "-" {
        return String::new();
    }
    assert!(raw.len().is_multiple_of(2));
    String::from_utf8(
        raw.as_bytes()
            .chunks_exact(2)
            .map(|v| u8::from_str_radix(std::str::from_utf8(v).unwrap(), 16).unwrap())
            .collect(),
    )
    .unwrap()
}
fn input(rows: &[&str]) -> JsonValue {
    JsonValue::parse(&unhex(rows[3])).unwrap()
}
fn rows() -> impl Iterator<Item = Vec<&'static str>> {
    include_str!("../fixtures/txencoder_unicode_vectors.tsv")
        .lines()
        .map(|line| line.split('\t').collect())
}

#[test]
fn complete_unchanged_encoder_unicode_strings_match_without_normalization_or_input_rewrite() {
    let mut matched = 0;
    for row in rows().filter(|row| row[0] == "V") {
        assert_eq!(row.len(), 6);
        let actual_input = input(&row);
        let before = actual_input.clone();
        let result = source_encode_method_unverified(row[2], &actual_input)
            .unwrap_or_else(|e| panic!("{}: {e:?}", row[1]));
        assert_eq!(row[4], "string");
        assert_eq!(
            result.source_string(),
            Some(unhex(row[5]).as_str()),
            "{}",
            row[1]
        );
        assert_eq!(actual_input, before, "{} mutated input", row[1]);
        if let SourceEncoderReturn::Encoding(encoded) = result {
            assert!(encoded.require_historically_accepted_transaction().is_err());
            assert!(encoded.require_authenticated_list_activation().is_err());
            assert!(encoded.require_signed_channel_revision().is_err());
            assert!(encoded.require_trusted_execution().is_err());
        }
        matched += 1;
    }
    assert_eq!(matched, 63);
}

#[test]
fn first_ten_actual_source_calls_are_a_separate_exact_proof() {
    let full = include_str!("../fixtures/txencoder_unicode_vectors.tsv");
    let proof = include_str!("../fixtures/txencoder_unicode_proof10.tsv");
    assert_eq!(
        proof,
        full.lines().take(10).collect::<Vec<_>>().join("\n") + "\n"
    );
    for row in proof
        .lines()
        .map(|line| line.split('\t').collect::<Vec<_>>())
    {
        let result = source_encode_method_unverified(row[2], &input(&row)).unwrap();
        assert_eq!(result.source_string(), Some(unhex(row[5]).as_str()));
    }
    let composed = rows().find(|row| row[1] == "issue-composed").unwrap();
    let decomposed = rows().find(|row| row[1] == "issue-decomposed").unwrap();
    assert_ne!(
        unhex(composed[5]),
        unhex(decomposed[5]),
        "no Unicode normalization"
    );
}

#[test]
fn literal_unicode_keeps_typed_ascii_wrappers_and_old_generic_concat_closed() {
    assert_eq!(
        source_encode_create_whitelist_unverified("backup", "Ω", "site", "note"),
        Err(LegacyEncoderError::NonPrintableAscii)
    );
    let actual_input = JsonValue::parse(r#"{"name":"Ω","backupAddress":"backup"}"#).unwrap();
    let SourceEncoderReturn::Encoding(encoded) =
        source_encode_method_unverified("encodeCreateWhitelist", &actual_input).unwrap()
    else {
        panic!("source encoding required")
    };
    assert_eq!(
        encoded.source_generic_types_concat_unverified(),
        Err(LegacyTypesError::ClaimNotPrintableAscii),
        "the separate old generic ASCII assembly is not silently widened"
    );
}

#[test]
fn non_unicode_numeric_resource_and_source_failure_guards_stay_explicit() {
    for row in rows().filter(|row| row[0] == "Q") {
        let expected = match row[1] {
            "guard-ascii-control" => LegacyEncoderError::NonPrintableAscii,
            "guard-overlong-unicode" => LegacyEncoderError::SourceReturnTooLong,
            "guard-fractional-number" => LegacyEncoderError::UnsupportedNumberRadix36,
            "guard-missing-source-array" => LegacyEncoderError::MissingOrMalformedField,
            "guard-custom-tostring" => LegacyEncoderError::UnsupportedNumericCoercion,
            other => panic!("unknown declared guard: {other}"),
        };
        let actual_input = input(&row);
        let before = actual_input.clone();
        assert_eq!(
            source_encode_method_unverified(row[2], &actual_input),
            Err(expected),
            "{}",
            row[1]
        );
        assert_eq!(actual_input, before);
    }
    assert!(
        JsonValue::parse(r#""\ud800""#).is_err(),
        "unpaired UTF-16 remains outside JsonValue"
    );
    assert_eq!(
        source_encode_method_unverified("encodePropertyId", &JsonValue::String("a".repeat(32768))),
        Err(LegacyEncoderError::JsonInputLimit)
    );
    assert_eq!(
        source_encode_method_unverified("unknown", &JsonValue::String("Ω".into())),
        Err(LegacyEncoderError::UnknownSourceMethod)
    );
}

#[test]
fn utf16_address_threshold_and_ecmascript_unicode_whitespace_use_real_source_results() {
    let selected = rows()
        .filter(|row| {
            row[1].starts_with("commit-utf16-")
                || row[1].starts_with("transfer-utf16-")
                || row[1].starts_with("synthetic-space-")
                || row[1].starts_with("amm-number-space-")
                || row[1] == "bn-js-space"
        })
        .collect::<Vec<_>>();
    assert_eq!(selected.len(), 21);
    for row in selected {
        let result = source_encode_method_unverified(row[2], &input(&row)).unwrap();
        assert_eq!(
            result.source_string(),
            Some(unhex(row[5]).as_str()),
            "{}",
            row[1]
        );
    }
}
