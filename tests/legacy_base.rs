use tradelayer_core::legacy_base::{self as base, LegacyBaseError, MAX_INPUT_CODE_UNITS};

fn unhex(input: &str) -> String {
    assert_eq!(input.len() % 2, 0);
    let bytes: Vec<_> = input
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let digits = core::str::from_utf8(pair).unwrap();
            u8::from_str_radix(digits, 16).unwrap()
        })
        .collect();
    String::from_utf8(bytes).unwrap()
}

fn run(operation: &str, input: Option<&str>) -> Result<Option<String>, LegacyBaseError> {
    let s = || input.expect("this operation requires a string fixture");
    match operation {
        "94.toBase94" => base::base94_from_decimal(s()).map(Some),
        "94.decimalToBase94" => base::base94_encode_decimal(input),
        "94.fromBase94" => base::base94_to_decimal(input),
        "94.validateBase94Input" => base::base94_validate_input(s()).map(|()| Some(String::new())),
        "94.hexToDecimal" => base::base94_hex_to_decimal(s()).map(Some),
        "94.decimalToHex" => base::base94_decimal_to_hex(s()).map(Some),
        "94.hexToBase94" => base::base94_hex_to_base94(s()).map(Some),
        "94.base94ToHex" => base::base94_to_hex(input).map(Some),
        "256.toBase256" => base::base256_from_decimal(s()).map(Some),
        "256.fromBase256" => base::base256_to_decimal(s()).map(Some),
        "256.hexToDecimal" => base::base256_hex_to_decimal(input).map(Some),
        "256.decimalToHex" => base::base256_decimal_to_hex(s()).map(Some),
        "256.hexToBase256" => base::base256_hex_to_base256(input).map(Some),
        "256.base256ToHex" => base::base256_to_hex(input).map(Some),
        _ => panic!("unknown fixture operation {operation}"),
    }
}

#[test]
fn actual_pinned_javascript_golden_vectors() {
    let mut count = 0;
    for line in include_str!("../fixtures/legacy_base_vectors.tsv").lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let fields: Vec<_> = line.split('\t').collect();
        assert_eq!(fields.len(), 5);
        let [label, operation, raw_input, status, raw_output] =
            <[&str; 5]>::try_from(fields).unwrap();
        let input = (raw_input != "-").then(|| unhex(raw_input));
        let expected = unhex(raw_output);
        let actual = run(operation, input.as_deref());
        match status {
            "ok" => assert_eq!(actual, Ok(Some(expected)), "{label}: {operation}"),
            "null" => assert_eq!(actual, Ok(None), "{label}: {operation}"),
            "error" => assert_eq!(
                actual.expect_err(label).to_string(),
                expected,
                "{label}: {operation}"
            ),
            _ => panic!("unknown status {status}"),
        }
        count += 1;
    }
    assert_eq!(count, 1221);
}

#[test]
fn preserves_ambiguous_delimiters_and_lost_scale() {
    assert_eq!(base::base94_from_decimal("91").unwrap(), "|");
    assert_eq!(
        base::base94_to_decimal(Some("|")).unwrap(),
        Some("15".into())
    );
    assert_eq!(
        base::base94_to_decimal(Some("||ignored")).unwrap(),
        Some("15".into())
    );
    let encoded = base::base94_encode_decimal(Some("1.001")).unwrap();
    assert_eq!(encoded.as_deref(), Some("\"|\""));
    assert_eq!(
        base::base94_to_decimal(encoded.as_deref()).unwrap(),
        Some("1.1".into())
    );
    // Validation accepts | as a legal digit and cannot establish reversibility.
    assert_eq!(base::base94_validate_input("|"), Ok(()));
    assert_eq!(base::base256_decimal_to_hex("-16").unwrap(), "0-10");
}

#[test]
fn alphabet_is_unicode_text_and_does_not_supply_safe_wire_framing() {
    let alphabet = base::custom_base256_alphabet();
    assert_eq!(alphabet.chars().count(), 256);
    assert_eq!(alphabet.chars().last(), Some('\u{0143}'));
    assert!(alphabet.len() > 256);
    assert!(alphabet.contains([',', ';', '|']));
    for c in ['\u{007f}', '\u{0085}', '\u{00a0}', '\u{00ad}'] {
        assert!(!alphabet.contains(c));
    }
    // Neither zero nor leading zeroes carry the original hash width.
    assert_eq!(base::base256_hex_to_base256(Some("000001")).unwrap(), "\"");
    assert_eq!(base::base256_to_hex(Some("\"")).unwrap(), "01");
    assert_eq!(base::base94_hex_to_base94("000001").unwrap(), "\"");
    assert_eq!(base::base94_to_hex(Some("\"")).unwrap(), "1");
}

#[test]
fn resources_are_bounded_before_normalization_or_component_splitting() {
    let too_long = "0".repeat(MAX_INPUT_CODE_UNITS + 1);
    assert_eq!(
        base::base94_from_decimal(&too_long),
        Err(LegacyBaseError::InputTooLong)
    );
    assert_eq!(
        base::base256_from_decimal(&too_long),
        Err(LegacyBaseError::InputTooLong)
    );
    assert_eq!(
        base::base94_to_decimal(Some(&too_long)),
        Err(LegacyBaseError::InputTooLong)
    );
    assert_eq!(
        base::base256_to_hex(Some(&too_long)),
        Err(LegacyBaseError::InputTooLong)
    );
    let ignored = format!("!||{}", "!".repeat(MAX_INPUT_CODE_UNITS));
    assert_eq!(
        base::base94_to_decimal(Some(&ignored)),
        Err(LegacyBaseError::InputTooLong)
    );
    let spaces = " ".repeat(MAX_INPUT_CODE_UNITS + 1);
    assert_eq!(
        base::base256_from_decimal(&spaces),
        Err(LegacyBaseError::InputTooLong)
    );
    assert_eq!(
        base::base94_from_decimal(&"0".repeat(MAX_INPUT_CODE_UNITS)).unwrap(),
        "!"
    );
}

#[test]
fn internal_decimal_expansion_does_not_reapply_external_input_cap() {
    let encoded = "\u{0143}".repeat(MAX_INPUT_CODE_UNITS);
    let decimal = base::base256_to_decimal(&encoded).unwrap();
    assert!(decimal.len() > MAX_INPUT_CODE_UNITS);
    assert_eq!(
        base::base256_to_hex(Some(&encoded)).unwrap(),
        "ff".repeat(MAX_INPUT_CODE_UNITS)
    );
    let hex = "f".repeat(MAX_INPUT_CODE_UNITS);
    assert!(base::base256_hex_to_decimal(Some(&hex)).unwrap().len() > MAX_INPUT_CODE_UNITS);
    assert!(base::base256_hex_to_base256(Some(&hex)).is_ok());
    assert!(base::base94_hex_to_base94(&hex).is_ok());
}

#[test]
fn source_utf16_error_that_rust_cannot_represent_is_explicit() {
    assert_eq!(
        base::base94_from_decimal("😀"),
        Err(LegacyBaseError::UnrepresentableUtf16Error)
    );
    assert_eq!(
        base::base256_to_decimal("😀"),
        Err(LegacyBaseError::UnrepresentableUtf16Error)
    );
    // validateBase94Input uses JS for-of rather than split(''), so its error
    // contains the complete scalar value and can be preserved exactly.
    assert_eq!(
        base::base94_validate_input("😀"),
        Err(LegacyBaseError::InvalidBase94Character('😀'))
    );
}
