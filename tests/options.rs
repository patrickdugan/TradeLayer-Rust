use tradelayer_core::decimal::Decimal;
use tradelayer_core::options::{
    compute_reduce_flip, intrinsic, is_option_ticker, mtm_exposure, naked_maintenance,
    parse_ticker, portfolio_maintenance, price_eu_approx, rpnl_for_close, OptionPosition,
    OptionsError, PositionSide, SOURCE_COMMIT, SOURCE_LF_SHA256,
};

fn decimal(text: &str) -> Decimal {
    text.parse().unwrap()
}

fn number(text: &str) -> f64 {
    match text {
        "NaN" => f64::NAN,
        "Infinity" => f64::INFINITY,
        "-Infinity" => f64::NEG_INFINITY,
        _ => text.parse().unwrap(),
    }
}

fn assert_number(actual: f64, expected: &str, case: &str) {
    let expected = number(expected);
    if expected.is_nan() {
        assert!(actual.is_nan(), "{case}: expected NaN, got {actual}");
    } else {
        assert_eq!(actual, expected, "{case}");
    }
}

fn source_side(side: Option<PositionSide>) -> &'static str {
    side.map_or("null", PositionSide::as_source_name)
}

fn decode_hex(hex: &str) -> String {
    let bytes: Vec<_> = hex
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(core::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect();
    assert_eq!(hex.len() % 2, 0);
    String::from_utf8(bytes).unwrap()
}

fn positions(text: &str, with_expiry: bool) -> Vec<OptionPosition> {
    text.split(';')
        .filter(|leg| !leg.is_empty())
        .map(|leg| {
            let fields: Vec<_> = leg.split(':').collect();
            assert_eq!(fields.len(), if with_expiry { 4 } else { 3 });
            let offset = usize::from(with_expiry);
            OptionPosition {
                option_type: fields[0].to_owned(),
                expiry_block: if with_expiry { number(fields[1]) } else { 0.0 },
                strike: decimal(fields[1 + offset]),
                qty: decimal(fields[2 + offset]),
            }
        })
        .collect()
}

#[test]
fn source_identity_and_all_ticker_vectors_are_exact() {
    assert_eq!(SOURCE_COMMIT, "dca0888b827c5bc28fa9189bc9e1d507f5c5670c");
    assert_eq!(
        SOURCE_LF_SHA256,
        "02277caaece5339a9ea0dac15437d14ce3fac8dc5ee92a727a0f544efcdbb703"
    );
    let mut count = 0;
    for line in include_str!("../fixtures/options_tickers.tsv").lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let f: Vec<_> = line.split('\t').collect();
        assert_eq!(f.len(), 7);
        let ticker = decode_hex(f[1]);
        assert_eq!(is_option_ticker(&ticker), f[2] == "1", "{}", f[0]);
        let parsed = parse_ticker(&ticker);
        if f[3] == "null" {
            assert!(parsed.is_none(), "{}: {ticker:?}", f[0]);
        } else {
            let parsed = parsed.unwrap();
            assert_eq!(parsed.series_id, decode_hex(f[3]), "{}", f[0]);
            assert_number(parsed.expiry_block, f[4], f[0]);
            assert_eq!(parsed.option_type.as_source_name(), f[5], "{}", f[0]);
            if f[6] == "null" {
                assert_eq!(parsed.strike, None, "{}", f[0]);
            } else {
                assert_number(parsed.strike.unwrap(), f[6], f[0]);
            }
            assert_eq!(parsed.raw, ticker);
        }
        count += 1;
    }
    assert_eq!(count, 36);
}

#[test]
fn every_calculator_matches_pinned_real_bignumber_oracle() {
    let mut counts = [0; 6];
    for line in include_str!("../fixtures/options_calculations.tsv").lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let f: Vec<_> = line.split('\t').collect();
        match f[1] {
            "reduce" => {
                assert_eq!(f.len(), 12);
                let actual = compute_reduce_flip(number(f[2]), number(f[3]));
                assert_number(actual.existing, f[4], f[0]);
                assert_number(actual.delta, f[5], f[0]);
                assert_number(actual.after, f[6], f[0]);
                assert_eq!(source_side(actual.existing_side), f[7], "{}", f[0]);
                assert_eq!(source_side(actual.delta_side), f[8], "{}", f[0]);
                assert_eq!(source_side(actual.after_side), f[9], "{}", f[0]);
                assert_number(actual.closed_qty, f[10], f[0]);
                assert_number(actual.flip_qty, f[11], f[0]);
                counts[0] += 1;
            }
            "rpnl" => {
                assert_eq!(f.len(), 7);
                let side = if f[2] == "null" { None } else { Some(f[2]) };
                let actual = rpnl_for_close(side, &decimal(f[3]), &decimal(f[4]), &decimal(f[5]));
                assert_eq!(actual.unwrap(), decimal(f[6]), "{}", f[0]);
                counts[1] += 1;
            }
            "intrinsic" => {
                assert_eq!(f.len(), 6);
                let actual = intrinsic(f[2], &decimal(f[3]), &decimal(f[4]));
                assert_eq!(actual.unwrap(), decimal(f[5]), "{}", f[0]);
                counts[2] += 1;
            }
            "naked" => {
                assert_eq!(f.len(), 6);
                let actual = naked_maintenance(f[2], &decimal(f[3]), &decimal(f[4]));
                assert_eq!(actual.unwrap(), decimal(f[5]), "{}", f[0]);
                counts[3] += 1;
            }
            "price" => {
                assert_eq!(f.len(), 8);
                let actual = price_eu_approx(
                    f[2],
                    &decimal(f[3]),
                    &decimal(f[4]),
                    number(f[5]),
                    number(f[6]),
                );
                assert_eq!(actual.unwrap(), decimal(f[7]), "{}", f[0]);
                counts[4] += 1;
            }
            "mtm" => {
                assert_eq!(f.len(), 9);
                let actual = mtm_exposure(
                    &positions(f[6], false),
                    &decimal(f[2]),
                    number(f[3]),
                    number(f[4]),
                    number(f[5]),
                )
                .unwrap();
                assert_eq!(actual.premium, decimal(f[7]), "{}", f[0]);
                assert_eq!(actual.intrinsic, decimal(f[8]), "{}", f[0]);
                counts[5] += 1;
            }
            method => panic!("unexpected source fixture method {method}"),
        }
    }
    assert_eq!(counts, [15, 8, 8, 8, 13, 8]);
}

#[test]
fn all_source_portfolio_vectors_preserve_grouping_rounding_and_input_order() {
    let mut count = 0;
    for line in include_str!("../fixtures/options_portfolios.tsv").lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let f: Vec<_> = line.split('\t').collect();
        assert_eq!(f.len(), 4);
        let actual = portfolio_maintenance(&positions(f[2], true), &decimal(f[1])).unwrap();
        assert_eq!(actual, decimal(f[3]), "{}", f[0]);
        count += 1;
    }
    assert_eq!(count, 22);
}

#[test]
fn source_order_sensitivity_is_not_replaced_with_a_spread_optimizer() {
    let spot = decimal("100");
    let first = positions("Call:10:90:-1;Call:10:100:1;Call:10:200:1", true);
    let second = positions("Call:10:90:-1;Call:10:200:1;Call:10:100:1", true);
    assert_eq!(portfolio_maintenance(&first, &spot).unwrap(), decimal("9"));
    assert_eq!(portfolio_maintenance(&second, &spot).unwrap(), decimal("0"));
    // Sharing a hedge across two shorts consumes it only once.
    let consumed = positions("Call:10:90:-1;Call:10:110:-1;Call:10:120:1", true);
    assert_eq!(
        portfolio_maintenance(&consumed, &spot).unwrap(),
        decimal("17")
    );
}

#[test]
fn expiry_grouping_matches_numeric_source_default_before_stringification() {
    let mixed = positions("Call:NaN:90:-1;Call:0:110:1", true);
    assert_eq!(
        portfolio_maintenance(&mixed, &decimal("100")).unwrap(),
        decimal("8")
    );
    let separated = positions("Call:Infinity:90:-1;Call:-Infinity:110:1", true);
    assert_eq!(
        portfolio_maintenance(&separated, &decimal("100")).unwrap(),
        decimal("10")
    );
}

#[test]
fn nonfinite_pricing_bridge_is_explicit_and_short_circuits_match_source() {
    let spot = decimal("100");
    let strike = decimal("90");
    assert_eq!(
        price_eu_approx("Call", &spot, &strike, f64::INFINITY, 30.0),
        Err(OptionsError::NonFinitePricingInput)
    );
    assert_eq!(
        price_eu_approx("Call", &spot, &strike, 0.2, f64::INFINITY),
        Err(OptionsError::NonFinitePricingInput)
    );
    assert_eq!(
        price_eu_approx("Call", &spot, &strike, f64::INFINITY, 0.0).unwrap(),
        decimal("10")
    );
    assert_eq!(
        price_eu_approx("Call", &spot, &strike, 0.0, f64::INFINITY).unwrap(),
        decimal("10")
    );
    assert_eq!(
        price_eu_approx("Call", &spot, &strike, f64::NAN, 30.0).unwrap(),
        decimal("10")
    );
}

#[test]
fn parser_and_regex_have_deliberately_distinct_source_contracts() {
    assert!(is_option_ticker("a-b-2-C"));
    assert_eq!(parse_ticker("a-b-2-C"), None);
    assert!(!is_option_ticker("x-12tail-C-1.5junk"));
    let parsed = parse_ticker("x-12tail-C-1.5junk-extra").unwrap();
    assert_eq!(parsed.expiry_block, 12.0);
    assert_eq!(parsed.strike, Some(1.5));
    assert!(!is_option_ticker("x-12-C-NaN"));
    assert!(parse_ticker("x-12-C-NaN").unwrap().strike.unwrap().is_nan());
}
