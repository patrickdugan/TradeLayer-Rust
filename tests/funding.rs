use tradelayer_core::decimal::{Decimal, Rounding};
use tradelayer_core::funding::*;
use tradelayer_core::ledger::AccountId;

fn d(text: &str) -> Decimal {
    text.parse().unwrap()
}
fn a(text: &str) -> AccountId {
    AccountId::try_from(text).unwrap()
}
fn p(owner: &str, quantity: &str, available_atoms: u128) -> FundingPosition {
    FundingPosition {
        owner: a(owner),
        contracts: d(quantity),
        available_atoms,
    }
}
fn payment(owner: &str, atoms: u128) -> FundingPayment {
    FundingPayment {
        owner: a(owner),
        atoms,
    }
}

#[test]
fn all_unchanged_source_rate_notional_and_positive_fractional_payer_vectors() {
    let mut count = 0;
    for line in include_str!("../fixtures/clearing_vectors.tsv")
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
    {
        let f: Vec<_> = line.split('\t').collect();
        assert_eq!(f.len(), 5);
        let (kind, name, input, expected) = (f[0], f[1], f[2], f[3]);
        let values: Vec<_> = input.split(',').collect();
        match kind {
            "average" => {
                let prices: Vec<_> = if input == "-" {
                    Vec::new()
                } else {
                    values.iter().map(|v| d(v)).collect()
                };
                assert_eq!(
                    source_average(&prices).unwrap(),
                    if expected == "null" {
                        None
                    } else {
                        Some(d(expected))
                    },
                    "{name}"
                );
            }
            "rate" => assert_eq!(
                source_rate(&d(values[0]), &d(values[1])).unwrap(),
                d(expected),
                "{name}"
            ),
            "notional" => assert_eq!(
                collateral_notional(
                    &d(values[0]),
                    &d(values[1]),
                    values[2] == "1",
                    values[3] == "1"
                )
                .unwrap(),
                d(expected),
                "{name}"
            ),
            "payer" => {
                let positions = [
                    p("payer", values[0], u128::MAX),
                    p("receiver", &format!("-{}", values[0]), 0),
                ];
                let plan = plan_funding(&positions, &d(values[1]), &d(values[2])).unwrap();
                let expected_atoms: u128 = d(expected)
                    .checked_mul(&Decimal::from(ATOMS_PER_TOKEN))
                    .unwrap()
                    .to_string()
                    .parse()
                    .unwrap();
                assert_eq!(plan.total_atoms, expected_atoms, "{name}");
                assert_eq!(
                    plan.debits.iter().map(|p| p.atoms).sum::<u128>(),
                    expected_atoms
                );
                assert_eq!(
                    plan.credits.iter().map(|p| p.atoms).sum::<u128>(),
                    expected_atoms
                );
            }
            other => panic!("unhandled source vector {other}"),
        }
        count += 1;
    }
    assert_eq!(count, 117);
}

#[test]
fn negative_rate_debits_shorts_once_and_credits_longs_with_no_sign_inversion() {
    let positions = [p("long", "0.3", 0), p("short", "-0.3", 30_000)];
    let before = positions.clone();
    let plan = plan_funding(&positions, &d("1"), &d("-10")).unwrap();
    assert_eq!(plan.total_atoms, 30_000);
    assert_eq!(plan.debits, [payment("short", 30_000)]);
    assert_eq!(plan.credits, [payment("long", 30_000)]);
    assert_eq!(positions, before);
    let positive = [p("long", "0.3", 30_000), p("short", "-0.3", 0)];
    let plan = plan_funding(&positive, &d("1"), &d("10")).unwrap();
    assert_eq!(plan.debits, [payment("long", 30_000)]);
    assert_eq!(plan.credits, [payment("short", 30_000)]);
}

#[test]
fn each_payer_half_up_charge_is_not_rounded_as_one_combined_position() {
    let positions = [
        p("payerB", "0.000005", 1),
        p("payerA", "0.000005", 1),
        p("c", "-1", 0),
        p("b", "-1", 0),
        p("a", "-1", 0),
    ];
    let plan = plan_funding(&positions, &d("1"), &d("10")).unwrap();
    assert_eq!(plan.total_atoms, 2);
    assert_eq!(plan.debits, [payment("payerA", 1), payment("payerB", 1)]);
    assert_eq!(plan.credits, [payment("a", 1), payment("b", 1)]);
}

#[test]
fn hamilton_uses_exact_fractional_remainders_and_ascii_ties() {
    let positions = [
        p("payer", "0.00001", 1),
        p("b", "-0.5", 0),
        p("a", "-0.5", 0),
    ];
    let plan = plan_funding(&positions, &d("1"), &d("10")).unwrap();
    assert_eq!(plan.total_atoms, 1);
    assert_eq!(plan.credits, [payment("a", 1)]);
    let positions = [
        p("payer", "0.00001", 1),
        p("a", "-10000000000000000000.00000000000000000001", 0),
        p("b", "-10000000000000000000.00000000000000000002", 0),
    ];
    let plan = plan_funding(&positions, &d("1"), &d("10")).unwrap();
    // The fractional difference is far below a 20-place rounded ratio. It
    // must still outrank the alphabetical tie-break rather than vanish.
    assert_eq!(plan.credits, [payment("b", 1)]);
    let positions = [p("payer", "0.00001", 1), p("a", "-1", 0), p("A", "-1", 0)];
    assert_eq!(
        plan_funding(&positions, &d("1"), &d("10")).unwrap().credits,
        [payment("A", 1)]
    );
}

#[test]
fn exact_fractional_positions_are_not_silently_coerced_to_source_number() {
    let quantity = "9007199254740993";
    let plan = plan_funding(
        &[p("payer", quantity, u128::MAX), p("receiver", "-1", 0)],
        &d("1"),
        &d("10"),
    )
    .unwrap();
    assert_eq!(plan.total_atoms, 900_719_925_474_099_300_000);
}

#[test]
fn last_insufficient_payer_and_receiver_overflow_reject_entire_plan() {
    let positions = [
        p("a", "0.5", 50_000),
        p("z", "0.5", 49_999),
        p("receiver", "-1", 0),
    ];
    let before = positions.clone();
    assert_eq!(
        plan_funding(&positions, &d("1"), &d("10")),
        Err(FundingError::InsufficientAvailable(a("z")))
    );
    assert_eq!(positions, before);
    let positions = [p("payer", "0.00001", 1), p("receiver", "-1", u128::MAX)];
    assert_eq!(
        plan_funding(&positions, &d("1"), &d("10")),
        Err(FundingError::AmountOverflow(a("receiver")))
    );
}

#[test]
fn empty_flat_zero_rate_missing_sides_and_tiny_rounding_have_explicit_results() {
    for positions in [vec![], vec![p("flat", "0", 0)]] {
        let plan = plan_funding(&positions, &d("1"), &d("12.5")).unwrap();
        assert_eq!(plan.total_atoms, 0);
        assert!(plan.debits.is_empty() && plan.credits.is_empty());
    }
    assert!(plan_funding(&[p("onlylong", "1", 0)], &d("1"), &d("0"))
        .unwrap()
        .debits
        .is_empty());
    for positions in [
        vec![p("onlylong", "1", u128::MAX)],
        vec![p("onlyshort", "-1", u128::MAX)],
    ] {
        assert_eq!(
            plan_funding(&positions, &d("1"), &d("10")),
            Err(FundingError::MissingCounterparty)
        );
    }
    let plan = plan_funding(
        &[p("payer", "0.00000001", 0), p("receiver", "-1", 0)],
        &d("1"),
        &d("0.0001"),
    )
    .unwrap();
    assert_eq!(plan.total_atoms, 0);
    assert!(plan.debits.is_empty() && plan.credits.is_empty());
}

#[test]
fn limits_duplicates_invalid_domains_and_total_overflow_fail_closed() {
    let positions = [p("same", "1", 0), p("same", "-1", 0)];
    assert_eq!(
        plan_funding(&positions, &d("1"), &d("0")),
        Err(FundingError::DuplicateOwner(a("same")))
    );
    let positions: Vec<_> = (0..=MAX_FUNDING_POSITIONS)
        .map(|n| p(&format!("p{n}"), "0", 0))
        .collect();
    assert_eq!(
        plan_funding(&positions, &d("1"), &d("0")),
        Err(FundingError::Capacity)
    );
    for rate in ["12.5001", "-12.5001", "0.00001"] {
        assert_eq!(
            plan_funding(&[], &d("1"), &d(rate)),
            Err(FundingError::InvalidRate)
        );
    }
    assert!(source_rate(&d("-1"), &d("100")).is_err());
    assert!(collateral_notional(&d("1"), &d("0"), true, false).is_err());
    assert!(plan_funding(
        &[p("payer", "1", 0), p("receiver", "-1", 0)],
        &d("0"),
        &d("1")
    )
    .is_err());
    assert!(plan_funding(
        &[p("payer", "0.000000000000000000001", 0)],
        &d("1"),
        &d("0")
    )
    .is_err());
    let each = (u128::MAX / 2) + 1;
    let quantity = Decimal::from(each)
        .checked_div(&Decimal::from(100_000_u32), 20, Rounding::HalfUp)
        .unwrap();
    let positions = [
        FundingPosition {
            owner: a("a"),
            contracts: quantity.clone(),
            available_atoms: u128::MAX,
        },
        FundingPosition {
            owner: a("b"),
            contracts: quantity,
            available_atoms: u128::MAX,
        },
        p("r", "-1", 0),
    ];
    assert_eq!(
        plan_funding(&positions, &d("1"), &d("10")),
        Err(FundingError::Arithmetic)
    );
}

#[test]
fn permutations_preserve_conservation_and_deterministic_payments_for_both_signs() {
    for rate in ["0.0001", "1.2345", "12.5", "-0.0001", "-1.2345", "-12.5"] {
        for quantity in ["0.00001", "0.125", "0.3", "100000.00000001"] {
            let positions = [
                p("longB", quantity, u128::MAX / 4),
                p("shortA", "-0.5", u128::MAX / 4),
                p("longA", "0.5", u128::MAX / 4),
                p("shortB", &format!("-{quantity}"), u128::MAX / 4),
            ];
            let plan = plan_funding(&positions, &d("2.5"), &d(rate)).unwrap();
            let reverse: Vec<_> = positions.into_iter().rev().collect();
            assert_eq!(plan_funding(&reverse, &d("2.5"), &d(rate)).unwrap(), plan);
            assert_eq!(
                plan.debits.iter().map(|p| p.atoms).sum::<u128>(),
                plan.total_atoms
            );
            assert_eq!(
                plan.credits.iter().map(|p| p.atoms).sum::<u128>(),
                plan.total_atoms
            );
        }
    }
}
