use tradelayer_core::issuance_intent::JsonValue;
use tradelayer_core::legacy_txencoder_audit::{
    source_encode_method_unverified, source_encode_token_issue_unverified, ClaimedTokenIssue,
    LegacyEncoderError, SourceEncoderReturn, JS_SAFE_INTEGER,
};

fn unhex(raw: &str) -> String {
    if raw == "-" {
        return String::new();
    }
    assert!(raw.len().is_multiple_of(2));
    String::from_utf8(
        raw.as_bytes()
            .chunks_exact(2)
            .map(|part| u8::from_str_radix(std::str::from_utf8(part).unwrap(), 16).unwrap())
            .collect(),
    )
    .unwrap()
}
fn rows() -> impl Iterator<Item = Vec<&'static str>> {
    include_str!("../fixtures/txencoder_fraction_vectors.tsv")
        .lines()
        .map(|line| line.split('\t').collect())
}
fn assert_source_row(row: &[&str]) -> String {
    assert_eq!(row.len(), 6);
    assert_eq!(row[4], "string");
    let input = JsonValue::parse(&unhex(row[3])).unwrap();
    let before = input.clone();
    let result = source_encode_method_unverified(row[2], &input)
        .unwrap_or_else(|error| panic!("{}: {error:?}", row[1]));
    assert_eq!(
        result.source_string(),
        Some(unhex(row[5]).as_str()),
        "{} raw={}",
        row[1],
        unhex(row[3])
    );
    assert_eq!(input, before, "{} changed input", row[1]);
    if let SourceEncoderReturn::Encoding(encoded) = &result {
        assert!(encoded.require_historically_accepted_transaction().is_err());
        assert!(encoded.require_authenticated_list_activation().is_err());
        assert!(encoded.require_signed_channel_revision().is_err());
        assert!(encoded.require_trusted_execution().is_err());
    }
    result.source_string().unwrap().to_owned()
}

#[test]
fn complete_original_encoder_binary64_fraction_and_payload_results_match_exactly() {
    let mut count = 0;
    for row in rows().filter(|row| row[0] == "V") {
        assert_source_row(&row);
        count += 1;
    }
    assert_eq!(count, 4470);
}

#[test]
fn first_ten_source_calls_preserve_subnormals_negative_zero_and_neighbor_rounding() {
    let full = include_str!("../fixtures/txencoder_fraction_vectors.tsv");
    let proof = include_str!("../fixtures/txencoder_fraction_proof10.tsv");
    assert_eq!(
        proof,
        full.lines().take(10).collect::<Vec<_>>().join("\n") + "\n"
    );
    for row in proof
        .lines()
        .map(|line| line.split('\t').collect::<Vec<_>>())
    {
        assert_source_row(&row);
    }
    assert_eq!(
        assert_source_row(&rows().find(|row| row[1] == "half").unwrap()),
        "0.i"
    );
    assert_eq!(
        assert_source_row(&rows().find(|row| row[1] == "negative-zero").unwrap()),
        "0"
    );
    assert_ne!(
        assert_source_row(&rows().find(|row| row[1] == "above-one").unwrap()),
        "1"
    );
    assert_ne!(
        assert_source_row(&rows().find(|row| row[1] == "minimum-subnormal").unwrap()),
        "0"
    );
}

#[test]
fn nonfinite_unsafe_integer_and_custom_coercion_guards_remain_separate() {
    let mut count = 0;
    for row in rows().filter(|row| row[0] == "Q") {
        let expected = if row[1] == "guard-custom-coercion" {
            LegacyEncoderError::UnsupportedNumericCoercion
        } else {
            LegacyEncoderError::UnsupportedNumberRadix36
        };
        let input = JsonValue::parse(&unhex(row[3])).unwrap();
        let before = input.clone();
        assert_eq!(
            source_encode_method_unverified(row[2], &input),
            Err(expected),
            "{}",
            row[1]
        );
        assert_eq!(input, before);
        count += 1;
    }
    assert_eq!(count, 4);
}

#[test]
fn typed_atom_wrapper_and_string_radix_semantics_stay_unchanged() {
    assert_eq!(
        source_encode_token_issue_unverified(ClaimedTokenIssue {
            initial_amount: JS_SAFE_INTEGER + 1,
            ticker: "TK",
            whitelists: &[],
            managed: false,
            backup_address: "backup",
            nft: false,
            colored_coin_hybrid: false,
            procedural_type: None,
        }),
        Err(LegacyEncoderError::InvalidBase36Number)
    );
    let string =
        source_encode_method_unverified("encodePropertyId", &JsonValue::String("0.5".into()))
            .unwrap();
    assert_eq!(
        string.source_string(),
        Some("0.5"),
        "String.toString ignores radix"
    );
    let number =
        source_encode_method_unverified("encodePropertyId", &JsonValue::parse("0.5").unwrap())
            .unwrap();
    assert_eq!(number.source_string(), Some("0.i"));
}

#[test]
fn actual_source_payload_paths_preserve_big_number_scaling_and_framing() {
    let names = [
        "send-fractional-property",
        "issue-fractional-fields",
        "multisend-fractional-properties",
        "scaled-number-half",
        "commit-fractional-property",
        "whitelist-fractional-id",
        "admin-fractional-id",
        "amm-fractional-numbers",
        "oracle-fractional-lag",
    ];
    for name in names {
        assert_source_row(&rows().find(|row| row[1] == name).unwrap());
    }
    assert_eq!(
        assert_source_row(
            &rows()
                .find(|row| row[1] == "send-fractional-property")
                .unwrap()
        ),
        "tl20;alice;0.i;1"
    );
}
