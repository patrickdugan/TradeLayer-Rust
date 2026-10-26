use tradelayer_core::decimal::{Decimal, Rounding};
use tradelayer_core::mark_basis::*;

fn d(value: &str) -> Decimal {
    value.parse().unwrap()
}
fn r(value: &str) -> ExactRatio {
    ExactRatio::from_decimal(&d(value)).unwrap()
}
fn config(inverse: bool, notional: &str) -> BasisConfig {
    BasisConfig {
        inverse,
        notional: d(notional),
    }
}
fn trade(delta: &str, price: &str) -> BasisAction {
    BasisAction::Trade {
        delta: d(delta),
        price: d(price),
    }
}
fn mark(price: &str) -> BasisAction {
    BasisAction::Mark { price: d(price) }
}
fn step(position: &mut BasisPosition, cfg: &BasisConfig, action: BasisAction) -> BasisTransition {
    let transition = position.transition(cfg, &action).unwrap();
    *position = transition.position.clone();
    transition
}

#[test]
fn genuine_unchanged_source_raw_and_rounded_helpers_match_supported_domain() {
    let mut count = 0;
    for line in include_str!("../fixtures/mark_basis_vectors.tsv")
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        let fields: Vec<_> = line.split('\t').collect();
        assert_eq!(fields.len(), 4);
        let values: Vec<_> = fields[2].split(',').collect();
        let result = source_clearing_pnl(
            &d(values[0]),
            &d(values[1]),
            &d(values[2]),
            values[3] == "1",
            &d(values[4]),
        )
        .unwrap();
        let result = if matches!(fields[0], "rounded" | "delta") {
            result.quantize(8, Rounding::HalfUp).unwrap()
        } else {
            assert!(matches!(fields[0], "raw" | "new"));
            result
        };
        assert_eq!(result, d(fields[3]), "{} {}", fields[0], fields[1]);
        count += 1;
    }
    assert_eq!(count, 130);
}

#[test]
fn repeated_marks_and_close_at_the_paid_mark_never_pay_original_entry_again() {
    for inverse in [false, true] {
        let cfg = config(inverse, "2.5");
        let mut p = BasisPosition::default();
        assert_eq!(step(&mut p, &cfg, trade("0.5", "100")).atoms.atoms, 0);
        let paid = step(&mut p, &cfg, mark("110"));
        assert!(!paid.pnl.is_zero());
        let carry = p.carry.clone();
        let basis = p.entry_basis.clone();
        let repeated = step(&mut p, &cfg, mark("110"));
        assert!(repeated.pnl.is_zero());
        assert_eq!(repeated.atoms, SignedAtoms::default());
        assert_eq!(p.entry_basis, basis);
        assert_eq!(p.carry, carry);
        let close = step(&mut p, &cfg, trade("-0.5", "110"));
        assert!(close.pnl.is_zero());
        assert_eq!(close.atoms, SignedAtoms::default());
        assert!(p.quantity.is_zero());
        assert!(p.entry_basis.is_zero());
        assert_eq!(p.carry, carry);
    }
}

#[test]
fn linear_add_reduce_flip_and_mark_use_remaining_basis_not_trade_average() {
    let cfg = config(false, "1");
    let mut p = BasisPosition::default();
    step(&mut p, &cfg, trade("0.5", "100"));
    assert_eq!(step(&mut p, &cfg, mark("110")).pnl, r("5"));
    assert_eq!(step(&mut p, &cfg, trade("0.25", "120")).pnl, r("0"));
    assert_eq!(p.entry_basis, r("85"));
    assert_eq!(step(&mut p, &cfg, mark("120")).pnl, r("5"));
    assert_eq!(step(&mut p, &cfg, trade("-0.25", "124")).pnl, r("1"));
    assert_eq!(p.quantity, d("0.5"));
    assert_eq!(p.entry_basis, r("60"));
    assert_eq!(step(&mut p, &cfg, trade("-0.75", "130")).pnl, r("5"));
    assert_eq!(p.quantity, d("-0.25"));
    assert_eq!(p.entry_basis, r("-32.5"));
    assert_eq!(step(&mut p, &cfg, mark("120")).pnl, r("2.5"));
    assert_eq!(step(&mut p, &cfg, trade("0.25", "120")).pnl, r("0"));
    assert_eq!(p, BasisPosition::default());
}

#[test]
fn exact_proportional_basis_survives_repeating_partial_close_fraction() {
    let cfg = config(false, "1");
    let mut p = BasisPosition::default();
    step(&mut p, &cfg, trade("2", "1"));
    step(&mut p, &cfg, trade("1", "2"));
    let one = step(&mut p, &cfg, trade("-1", "2"));
    assert_eq!(one.pnl, ExactRatio::from_parts(d("2"), d("3")).unwrap());
    assert_eq!(
        p.entry_basis,
        ExactRatio::from_parts(d("8"), d("3")).unwrap()
    );
    let two = step(&mut p, &cfg, trade("-1", "2"));
    assert_eq!(two.pnl, one.pnl);
    let three = step(&mut p, &cfg, trade("-1", "2"));
    assert_eq!(three.pnl, one.pnl);
    assert_eq!(
        one.atoms.atoms + two.atoms.atoms + three.atoms.atoms,
        200_000_000
    );
    assert_eq!(p, BasisPosition::default());
}

#[test]
fn inverse_and_linear_closed_cashflows_are_invariant_under_inserted_marks() {
    let actions = [
        trade("1.5", "3"),
        trade("-0.5", "7"),
        trade("0.25", "11"),
        trade("-2", "17"),
        trade("0.75", "19"),
    ];
    for inverse in [false, true] {
        let cfg = config(inverse, "2.5");
        let mut expected = ExactRatio::default();
        for action in &actions {
            if let BasisAction::Trade { delta, price } = action {
                expected = expected
                    .checked_sub(
                        &r(&delta.to_string())
                            .checked_mul(&coordinate(price, inverse).unwrap())
                            .unwrap()
                            .checked_mul(&r("2.5"))
                            .unwrap(),
                    )
                    .unwrap();
            }
        }
        for marks in [false, true] {
            let mut p = BasisPosition::default();
            let mut payouts = ExactRatio::default();
            let mut pnls = ExactRatio::default();
            for (i, action) in actions.iter().enumerate() {
                let t = step(&mut p, &cfg, action.clone());
                payouts = payouts
                    .checked_add(&r(&t.atoms.to_decimal().to_string()))
                    .unwrap();
                pnls = pnls.checked_add(&t.pnl).unwrap();
                if marks {
                    let t = step(&mut p, &cfg, mark(["5", "9", "13", "18", "19"][i]));
                    payouts = payouts
                        .checked_add(&r(&t.atoms.to_decimal().to_string()))
                        .unwrap();
                    pnls = pnls.checked_add(&t.pnl).unwrap();
                }
            }
            assert!(p.quantity.is_zero());
            assert!(p.entry_basis.is_zero());
            assert_eq!(pnls, expected);
            assert_eq!(payouts.checked_add(&p.carry).unwrap(), expected);
        }
    }
}

#[test]
fn signed_subatom_carry_accumulates_reverses_and_survives_flat_reopen() {
    let cfg = config(false, "1");
    let mut p = BasisPosition::default();
    step(&mut p, &cfg, trade("1", "1"));
    assert_eq!(step(&mut p, &cfg, mark("1.000000006")).atoms.atoms, 0);
    assert_eq!(p.carry, r("0.000000006"));
    assert_eq!(step(&mut p, &cfg, mark("1.000000012")).atoms.atoms, 1);
    assert_eq!(p.carry, r("0.000000002"));
    assert_eq!(step(&mut p, &cfg, mark("1.000000008")).atoms.atoms, 0);
    assert_eq!(p.carry, r("-0.000000002"));
    step(&mut p, &cfg, trade("-1", "1.000000008"));
    assert!(p.quantity.is_zero());
    assert_eq!(p.carry, r("-0.000000002"));
    step(&mut p, &cfg, trade("1", "2"));
    assert_eq!(step(&mut p, &cfg, mark("2.000000002")).atoms.atoms, 0);
    assert!(p.carry.is_zero());
}

#[test]
fn equal_opposite_fractional_owners_have_opposite_atoms_and_exact_carries() {
    for inverse in [false, true] {
        let cfg = config(inverse, "0.00000001");
        let mut long = BasisPosition::default();
        let mut short = BasisPosition::default();
        for action in [
            trade("0.33333333333333333333", "3"),
            mark("7"),
            mark("11"),
            trade("-0.1", "13"),
            mark("17"),
            trade("-0.23333333333333333333", "19"),
        ] {
            let opposite = match &action {
                BasisAction::Trade { delta, price } => BasisAction::Trade {
                    delta: delta.negated(),
                    price: price.clone(),
                },
                BasisAction::Mark { price } => BasisAction::Mark {
                    price: price.clone(),
                },
            };
            let left = step(&mut long, &cfg, action);
            let right = step(&mut short, &cfg, opposite);
            assert_eq!(left.pnl, right.pnl.negated());
            assert_eq!(left.atoms.atoms, right.atoms.atoms);
            assert_eq!(
                left.atoms.negative,
                left.atoms.atoms != 0 && !right.atoms.negative
            );
            assert_eq!(long.carry, short.carry.negated());
        }
    }
}

#[test]
fn ratio_canonical_restore_and_resource_bounds_reject_malleability() {
    assert_eq!(
        ExactRatio::from_parts(d("2"), d("4")),
        Err(BasisError::InvalidRatio)
    );
    assert_eq!(
        ExactRatio::from_parts(d("0"), d("2")),
        Err(BasisError::InvalidRatio)
    );
    assert_eq!(
        ExactRatio::from_parts(d("1"), d("-2")),
        Err(BasisError::InvalidRatio)
    );
    assert_eq!(
        ExactRatio::from_parts(d("1.1"), d("2")),
        Err(BasisError::InvalidRatio)
    );
    assert_eq!(
        ExactRatio::from_parts(d("1"), d("0")),
        Err(BasisError::InvalidRatio)
    );
    assert_eq!(
        ExactRatio::from_parts(d(&"9".repeat(513)), d("1")),
        Err(BasisError::Capacity)
    );
    let third = ExactRatio::from_parts(d("1"), d("3")).unwrap();
    assert_eq!(
        third
            .checked_add(&ExactRatio::from_parts(d("1"), d("6")).unwrap())
            .unwrap(),
        r("0.5")
    );
    assert_eq!(third.checked_div(&r("0")), Err(BasisError::InvalidRatio));
    let large = ExactRatio::from_parts(d(&"9".repeat(300)), d("1")).unwrap();
    assert_eq!(large.checked_mul(&large), Err(BasisError::Capacity));
}

#[test]
fn invalid_action_or_last_atom_overflow_leaves_original_position_unchanged() {
    let cfg = config(false, "1");
    let p = BasisPosition::default();
    let saved = p.clone();
    for action in [
        trade("0", "1"),
        trade("1", "0"),
        mark("-1"),
        trade("0.000000000000000000001", "1"),
        trade(&"9".repeat(41), "1"),
    ] {
        assert!(p.transition(&cfg, &action).is_err());
        assert_eq!(p, saved);
    }
    let huge_cfg = config(false, "1000000000000000000000000000000000000000");
    let opened = p
        .transition(&huge_cfg, &trade("100000000000000000000", "1"))
        .unwrap()
        .position;
    assert_eq!(
        opened.transition(&huge_cfg, &mark("100000000000000000000")),
        Err(BasisError::AmountOverflow)
    );
    assert_eq!(opened.quantity, d("100000000000000000000"));
    let invalid_carry = BasisPosition {
        carry: r("0.00000001"),
        ..BasisPosition::default()
    };
    assert_eq!(
        invalid_carry.validate(&cfg),
        Err(BasisError::InvalidPosition)
    );
    let invalid_basis = BasisPosition {
        quantity: d("1"),
        entry_basis: r("-1"),
        carry: r("0"),
    };
    assert_eq!(
        invalid_basis.validate(&cfg),
        Err(BasisError::InvalidPosition)
    );
    assert_eq!(
        p.validate(&config(false, "0")),
        Err(BasisError::InvalidNotional)
    );
}

#[test]
fn fractional_quantities_above_number_precision_and_zero_reciprocals_are_preserved() {
    let cfg = config(false, "1");
    let mut p = BasisPosition::default();
    step(&mut p, &cfg, trade("9007199254740993", "1"));
    assert_eq!(
        step(&mut p, &cfg, mark("1.00000001")).atoms.atoms,
        9_007_199_254_740_993
    );
    let cfg = config(true, "1");
    let mut inverse = BasisPosition::default();
    step(&mut inverse, &cfg, trade("0.5", "1000000000000000000000"));
    assert!(inverse.entry_basis.is_zero());
    assert_eq!(
        step(&mut inverse, &cfg, mark("2000000000000000000000")).atoms,
        SignedAtoms::default()
    );
    inverse.validate(&cfg).unwrap();
}
