use std::collections::{BTreeSet, HashSet};
use std::str::FromStr;

use tradelayer_core::decimal::{
    Decimal, DecimalError, Rounding, MAX_DECIMAL_DIGITS, MAX_DECIMAL_INPUT_BYTES, MAX_DECIMAL_SCALE,
};

fn d(text: &str) -> Decimal {
    Decimal::from_str(text).unwrap()
}

#[test]
fn canonical_equivalence_agrees_with_hashing_and_total_order() {
    let equivalents = ["1", "+1.0", "0001.000", "10e-1", "100E-2", "1e+000"];
    let values: Vec<_> = equivalents.iter().map(|text| d(text)).collect();
    assert!(values.iter().all(|value| value == &values[0]));
    assert!(values.iter().all(|value| value.cmp(&values[0]).is_eq()));
    assert_eq!(values.iter().collect::<HashSet<_>>().len(), 1);
    assert_eq!(values.iter().collect::<BTreeSet<_>>().len(), 1);

    for text in ["0", "-0", "+0.0000", "-000e4096", "0e-4096"] {
        let zero = d(text);
        assert_eq!(zero, Decimal::zero());
        assert!(!zero.is_negative());
        assert_eq!(zero.decimal_places(), 0);
        assert_eq!(zero.coefficient_digits(), 0);
        assert_eq!(zero.to_string(), "0");
        assert_eq!(zero.negated(), zero);
    }
}

#[test]
fn total_order_distinguishes_extreme_scales_and_reverses_negative_magnitudes() {
    let near_tenth = format!("0.1{}1", "0".repeat(MAX_DECIMAL_SCALE as usize - 2));
    let positive = [
        d("1e-4096"),
        d("1e-4095"),
        d("0.09"),
        d("0.1"),
        d(&near_tenth),
        d("1"),
        d("10"),
        d("1e4095"),
    ];
    for (i, left) in positive.iter().enumerate() {
        assert!(left > &Decimal::zero());
        for (j, right) in positive.iter().enumerate() {
            assert_eq!(left.cmp(right), i.cmp(&j));
            assert_eq!(left.negated().cmp(&right.negated()), j.cmp(&i));
            assert!(left.negated() < *right);
        }
    }
}

#[test]
fn rounding_uses_the_first_discarded_digit_and_has_no_negative_zero() {
    for (text, places, down, half_up) in [
        ("0.000000000000000000005", 20, "0", "0.00000000000000000001"),
        ("0.00049", 3, "0", "0"),
        ("0.00050", 3, "0", "0.001"),
        ("0.00051", 3, "0", "0.001"),
        ("0.00050", 2, "0", "0"),
        ("0.999999", 2, "0.99", "1"),
        ("9.999999", 2, "9.99", "10"),
        ("99.5", 0, "99", "100"),
        ("100.0001", 0, "100", "100"),
    ] {
        for negative in [false, true] {
            let value = if negative { d(text).negated() } else { d(text) };
            for (rounding, expected) in [(Rounding::Down, down), (Rounding::HalfUp, half_up)] {
                let expected = if negative {
                    d(expected).negated()
                } else {
                    d(expected)
                };
                let actual = value.quantize(places, rounding).unwrap();
                assert_eq!(actual, expected, "{value}, places={places}, {rounding:?}");
                assert!(!actual.is_zero() || !actual.is_negative());
            }
        }
    }
}

#[test]
fn division_rounds_away_from_zero_at_minimum_representable_quantum() {
    let tiny = d("1e-4096");
    for numerator_negative in [false, true] {
        for denominator_negative in [false, true] {
            let numerator = if numerator_negative {
                tiny.negated()
            } else {
                tiny.clone()
            };
            let denominator = if denominator_negative {
                d("-2")
            } else {
                d("2")
            };
            assert_eq!(
                numerator.checked_div(&denominator, MAX_DECIMAL_SCALE, Rounding::Down),
                Ok(Decimal::zero())
            );
            let expected = if numerator_negative != denominator_negative {
                tiny.negated()
            } else {
                tiny.clone()
            };
            assert_eq!(
                numerator.checked_div(&denominator, MAX_DECIMAL_SCALE, Rounding::HalfUp),
                Ok(expected)
            );
        }
    }
    assert_eq!(
        tiny.checked_div(&tiny, MAX_DECIMAL_SCALE, Rounding::HalfUp),
        Ok(d("1"))
    );
    assert_eq!(
        d("1").checked_div(&tiny, 0, Rounding::Down),
        Err(DecimalError::LimitExceeded)
    );
}

#[test]
fn normalization_happens_before_final_coefficient_and_scale_limits() {
    // The raw coefficient/scale and arithmetic intermediates can exceed the
    // final representation's bounds while their normalized values fit.
    let padded_one = format!("1.{}", "0".repeat(MAX_DECIMAL_INPUT_BYTES - 2));
    assert_eq!(padded_one.len(), MAX_DECIMAL_INPUT_BYTES);
    assert_eq!(d(&padded_one), d("1"));

    let all_nines = "9".repeat(MAX_DECIMAL_DIGITS);
    let large = d(&all_nines);
    assert_eq!(large.coefficient_digits(), MAX_DECIMAL_DIGITS);
    assert_eq!(
        large.checked_div(&d("1"), MAX_DECIMAL_SCALE, Rounding::HalfUp),
        Ok(large.clone())
    );
    assert_eq!(large.checked_add(&d("1")), Err(DecimalError::LimitExceeded));

    let almost_ten = format!("9.{}", "9".repeat(MAX_DECIMAL_DIGITS - 1));
    let quantum = d("1e-4095");
    assert_eq!(d(&almost_ten).checked_add(&quantum), Ok(d("10")));
    assert_eq!(large.checked_sub(&large), Ok(Decimal::zero()));

    let multiplier = d("1e4095");
    let fractional = d(&format!("0.{}", "1".repeat(MAX_DECIMAL_DIGITS)));
    let expected = d(&format!("{}.1", "1".repeat(MAX_DECIMAL_DIGITS - 1)));
    assert_eq!(multiplier.checked_mul(&fractional), Ok(expected));
    assert_eq!(multiplier.checked_mul(&d("1e-4096")), Ok(d("0.1")));
    assert_eq!(
        d("1e-4096").checked_mul(&d("0.1")),
        Err(DecimalError::LimitExceeded)
    );
}

#[test]
fn integer_conversions_cover_signed_minimum_and_unsigned_maximum() {
    for value in [i128::MIN, -1, 0, 1, i128::MAX] {
        let converted = Decimal::from(value);
        assert_eq!(converted, d(&value.to_string()));
        assert_eq!(converted.to_string(), value.to_string());
        assert_eq!(
            converted.checked_add(&converted.negated()),
            Ok(Decimal::zero())
        );
    }
    for value in [0_u128, 1, u128::MAX] {
        let converted = Decimal::from(value);
        assert_eq!(converted, d(&value.to_string()));
        assert_eq!(converted.to_string(), value.to_string());
    }
}

#[test]
fn parser_checks_exponents_whitespace_and_final_resource_limits() {
    assert_eq!(
        d("\u{feff}\u{00a0}\u{2028}-001.2500e+0002\u{3000}"),
        d("-125")
    );
    assert_eq!(d(".5e+1"), d("5"));
    assert_eq!(d("1.e-1"), d("0.1"));
    for text in [
        "",
        ".",
        "+.",
        "1..0",
        "1e",
        "1e+-2",
        "1e--2",
        "1e2e2",
        "1 2",
        "−1",
        "١",
        "\u{0085}1",
        "1\0",
    ] {
        assert_eq!(
            Decimal::from_str(text),
            Err(DecimalError::InvalidSyntax),
            "{text:?}"
        );
    }
    for text in [
        "1e4096".to_owned(),
        "1e-4097".to_owned(),
        "1e9223372036854775808".to_owned(),
        "1e-9223372036854775808".to_owned(),
        "1".repeat(MAX_DECIMAL_INPUT_BYTES + 1),
    ] {
        assert_eq!(Decimal::from_str(&text), Err(DecimalError::LimitExceeded));
    }
    for rounding in [Rounding::Down, Rounding::HalfUp] {
        assert_eq!(
            d("1").quantize(MAX_DECIMAL_SCALE + 1, rounding),
            Err(DecimalError::LimitExceeded)
        );
        assert_eq!(
            d("1").checked_div(&d("1"), MAX_DECIMAL_SCALE + 1, rounding),
            Err(DecimalError::LimitExceeded)
        );
        assert_eq!(
            d("1").checked_div(&Decimal::zero(), 0, rounding),
            Err(DecimalError::DivisionByZero)
        );
    }
}
