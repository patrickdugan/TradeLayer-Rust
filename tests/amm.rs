use std::collections::BTreeMap;
use tradelayer_core::amm::*;

fn number(text: &str) -> f64 {
    text.parse().unwrap()
}
fn assert_number(actual: f64, expected: &str, context: &str) {
    let expected = number(expected);
    if expected.is_nan() {
        assert!(actual.is_nan(), "{context}: actual {actual}");
    } else {
        assert_eq!(actual.to_bits(), expected.to_bits(), "{context}");
    }
}
fn side(text: &str) -> Side {
    match text {
        "buy" => Side::Buy,
        "sell" => Side::Sell,
        _ => panic!("invalid side {text}"),
    }
}
fn contract() -> CapitalAsset {
    CapitalAsset::Contract("1".to_owned())
}
fn token_pair() -> CapitalAsset {
    CapitalAsset::TokenPair {
        first: "1".to_owned(),
        second: "2".to_owned(),
    }
}
fn vectors() -> impl Iterator<Item = Vec<&'static str>> {
    include_str!("../fixtures/amm_vectors.tsv")
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| line.split('\t').collect())
}

#[test]
fn actual_js_capital_share_and_ordered_balance_journal_vectors() {
    let mut pools = BTreeMap::new();
    let mut changes = 0;
    for c in vectors() {
        match c[0] {
            "P" => {
                pools.insert(
                    c[1],
                    AmmPool::new(
                        number(c[2]),
                        number(c[3]),
                        number(c[4]),
                        c[5].parse().unwrap(),
                    )
                    .unwrap(),
                );
            }
            "A" | "R" => {
                let asset = if c[5] == "1" {
                    CapitalAsset::Contract(c[3].to_owned())
                } else {
                    CapitalAsset::TokenPair {
                        first: c[3].to_owned(),
                        second: c[6].to_owned(),
                    }
                };
                let pool = pools.get_mut(c[1]).unwrap();
                let receipt = if c[0] == "A" {
                    pool.add_capital(
                        c[2],
                        &asset,
                        number(c[4]),
                        number(c[7]),
                        c[8].parse().unwrap(),
                    )
                } else {
                    pool.redeem_capital(
                        c[2],
                        &asset,
                        number(c[4]),
                        number(c[7]),
                        c[8].parse().unwrap(),
                    )
                }
                .unwrap();
                assert_number(pool.position(), c[9], &c.join(" "));
                assert_number(pool.share(c[2]), c[10], &c.join(" "));
                let expected: Vec<_> = c[11].split(';').collect();
                assert_eq!(receipt.len(), expected.len());
                for (movement, expected) in receipt.iter().zip(expected) {
                    let expected: Vec<_> = expected.split(':').collect();
                    assert_eq!(movement.address, c[2]);
                    assert_eq!(movement.property, expected[0]);
                    assert_number(movement.amount, expected[1], &c.join(" "));
                    assert_eq!(movement.kind.as_source_name(), expected[2]);
                    assert_eq!(movement.block, c[8].parse::<u64>().unwrap());
                }
                assert_eq!(pool.lp_addresses().join(","), c[12]);
                changes += 1;
            }
            _ => {}
        }
    }
    assert_eq!((pools.len(), changes), (4, 17));
    assert_eq!(pools["c"].lp_addresses(), Vec::<&str>::new());
    assert_eq!(pools["keys"].lp_addresses(), ["2", "10", "z", "01"]);
    assert_eq!(pools["keys"].total_position_percent(None), 10.0);
    assert_eq!(pools["keys"].total_position_percent(Some("2")), 3.0);
    let empty = AmmPool::new(0.0, 0.0, 0.0, 0).unwrap();
    let empty_positive = AmmPool::new(0.0, 100.0, 0.0, 0).unwrap();
    let mut queries = 0;
    for c in vectors().filter(|c| c[0] == "H") {
        let pool = match c[1] {
            "keys" => &pools["keys"],
            "emptypositive" => &empty_positive,
            _ => &empty,
        };
        let owner = (c[2] != "*").then_some(c[2]);
        assert_number(pool.total_position_percent(owner), c[3], &c.join(" "));
        queries += 1;
    }
    assert_eq!(queries, 7);
}

#[test]
fn strategy_intents_feed_existing_option_maintenance_without_opening_positions() {
    use tradelayer_core::decimal::Decimal;
    let pool = AmmPool::new(10.0, 100.0, 3.0, 7).unwrap();
    let baseline = pool.clone();
    let orders = pool
        .build_option_strategy(OptionStrategy::IronFly, 100.0, 100)
        .unwrap();
    let positions: Vec<_> = orders
        .iter()
        .map(|order| order.as_option_position().unwrap())
        .collect();
    assert_eq!(positions[0].qty, Decimal::from(-3i32));
    assert_eq!(positions[2].qty, Decimal::from(3u32));
    assert_eq!(positions[0].option_type, "Call");
    assert_eq!(positions[1].option_type, "Put");
    // Preserve the existing helper's source formula, including its offsets.
    assert_eq!(
        tradelayer_core::options::portfolio_maintenance(&positions, &Decimal::from(100u32))
            .unwrap()
            .to_string(),
        "58.8"
    );
    assert_eq!(pool, baseline);
    let mut invalid = orders[0].clone();
    invalid.strike = f64::INFINITY;
    assert_eq!(invalid.as_option_position(), Err(AmmError::NonFinite));
    assert_eq!(
        pool.build_option_strategy(OptionStrategy::Straddle, 1.0, MAX_SAFE_BLOCK)
            .unwrap_err(),
        AmmError::InvalidBlock
    );
    assert!(pool
        .build_option_strategy(OptionStrategy::None, 1.0, MAX_SAFE_BLOCK)
        .unwrap()
        .is_empty());
}

#[test]
fn actual_js_quote_vectors_retain_zero_and_negative_bid_prices() {
    let mut cases = 0;
    for c in vectors().filter(|c| c[0] == "Q") {
        let pool = AmmPool::new(number(c[1]), 100.0, number(c[2]), 7).unwrap();
        let quotes = pool
            .quote_contract_orders(number(c[3]), c[5].parse().unwrap(), number(c[4]))
            .unwrap();
        assert_eq!(quotes.len(), c[6].parse::<usize>().unwrap());
        for (quote, expected) in quotes.iter().zip(c[7..].chunks(4)) {
            assert_number(quote.amount, expected[0], &c.join(" "));
            assert_number(quote.price, expected[1], &c.join(" "));
            assert_eq!(quote.side, side(expected[2]));
            assert_eq!(quote.txid, expected[3]);
            assert_eq!(quote.sender(), "amm");
            assert_eq!(quote.initial_margin(), 0.0);
        }
        cases += 1;
    }
    assert_eq!(cases, 17);
    let pool = AmmPool::new(10.0, 100.0, 4.0, 7).unwrap();
    for spread in [10000.0, 15000.0, f64::INFINITY] {
        let quotes = pool.quote_contract_orders(100.0, 1, spread).unwrap();
        assert_eq!(
            quotes[0].validate_executable(),
            Err(AmmError::NonExecutableQuote)
        );
    }
    for quote in pool.quote_contract_orders(100.0, 1, 40.0).unwrap() {
        quote.validate_executable().unwrap();
    }
}

#[test]
fn actual_js_exact_binary_to_fixed_vectors_include_ties_and_subnormals() {
    let mut cases = 0;
    for c in vectors().filter(|c| c[0] == "F") {
        assert_number(to_fixed_eight(number(c[1])).unwrap(), c[2], &c.join(" "));
        cases += 1;
    }
    assert_eq!(cases, 146);
    // Both are exact binary halfway points: ties go away from zero, not even.
    assert_eq!(to_fixed_eight(0.001953125).unwrap(), 0.00195313);
    assert_eq!(to_fixed_eight(-0.001953125).unwrap(), -0.00195313);
}

#[test]
fn actual_js_scalar_and_inventory_curve_vectors() {
    let mut counts = [0; 3];
    for c in vectors() {
        match c[0] {
            "S" => {
                assert_number(
                    calculate_order_size(number(c[1]), number(c[2]), number(c[3])),
                    c[4],
                    &c.join(" "),
                );
                counts[0] += 1;
            }
            "V" => {
                assert_number(
                    calculate_redemption_value(
                        number(c[1]),
                        c[2] == "1",
                        number(c[3]),
                        number(c[4]),
                        number(c[5]),
                        number(c[6]),
                    ),
                    c[7],
                    &c.join(" "),
                );
                counts[1] += 1;
            }
            "G" => {
                let mut pool = AmmPool::new(number(c[2]), 100.0, number(c[3]), 7).unwrap();
                let curve = pool
                    .generate_contract_order(
                        if c[1] == "linear" {
                            ContractType::Linear
                        } else {
                            ContractType::Inverse
                        },
                        number(c[4]),
                        number(c[5]),
                        number(c[6]),
                    )
                    .unwrap();
                assert_number(pool.position(), c[7], &c.join(" "));
                assert_number(curve.price, c[8], &c.join(" "));
                assert_number(curve.size, c[9], &c.join(" "));
                assert_eq!(curve.side, side(c[10]));
                counts[2] += 1;
            }
            _ => {}
        }
    }
    assert_eq!(counts, [9, 6, 18]);
}

#[test]
fn actual_js_bound_static_strategy_and_liquidity_vectors() {
    let mut counts = [0; 2];
    for c in vectors() {
        match c[0] {
            "T" => {
                let pool = AmmPool::new(0.0, 100.0, number(c[4]), 7).unwrap();
                let strategy = match c[1] {
                    "straddle" => OptionStrategy::Straddle,
                    "ironFly" => OptionStrategy::IronFly,
                    "calendar" => OptionStrategy::Calendar,
                    _ => OptionStrategy::None,
                };
                let orders = pool
                    .build_option_strategy(strategy, number(c[2]), c[3].parse().unwrap())
                    .unwrap();
                let expected: Vec<_> = c[5].split(';').filter(|s| !s.is_empty()).collect();
                assert_eq!(orders.len(), expected.len());
                for (order, expected) in orders.iter().zip(expected) {
                    let fields: Vec<_> = expected.split(',').collect();
                    assert_eq!(order.side, side(fields[0]));
                    assert_eq!(order.option_type.as_source_name(), fields[1]);
                    assert_number(order.strike, fields[2], &c.join(" "));
                    assert_eq!(order.expiry, fields[3].parse::<u64>().unwrap());
                    assert_number(order.size, fields[4], &c.join(" "));
                }
                counts[0] += 1;
            }
            "L" => {
                let mut pool = AmmPool::new(number(c[1]), number(c[2]), 4.0, 7).unwrap();
                let accepted = pool
                    .request_liquidity(LiquidityOrder {
                        size: number(c[3]),
                        side: side(c[4]),
                    })
                    .unwrap();
                assert_eq!(accepted.is_some(), c[5] == "1");
                assert_number(pool.position(), c[6], &c.join(" "));
                assert_eq!(
                    pool.snapshot().amm_orders.len(),
                    c[7].parse::<usize>().unwrap()
                );
                counts[1] += 1;
            }
            _ => {}
        }
    }
    assert_eq!(counts, [12, 10]);
}

#[test]
fn actual_js_token_curve_intents_require_explicit_external_placement() {
    let mut cases = 0;
    for c in vectors().filter(|c| c[0] == "X") {
        let orders =
            token_curve_intents("1", "2", number(c[1]), c[2].parse().unwrap(), number(c[3]))
                .unwrap();
        let expected: Vec<_> = c[4].split(';').filter(|s| !s.is_empty()).collect();
        assert_eq!(orders.len(), expected.len());
        for (order, expected) in orders.iter().zip(expected) {
            let fields: Vec<_> = expected.split(',').collect();
            assert_eq!(order.offered_property, fields[0]);
            assert_eq!(order.desired_property, fields[1]);
            assert_number(order.amount_offered, fields[2], &c.join(" "));
            assert_number(order.amount_expected, fields[3], &c.join(" "));
            assert_number(order.price, fields[4], &c.join(" "));
        }
        cases += 1;
    }
    assert_eq!(cases, 4);
    assert!(token_curve_intents("1", "2", 100.0, 4, 10.0)
        .unwrap()
        .iter()
        .any(|order| order.amount_expected < 0.0));
}

#[test]
fn registry_create_vectors_and_atomic_validated_state_replacement() {
    let mut registry = AmmRegistry::new();
    let mut cases = 0;
    for c in vectors().filter(|c| c[0] == "D") {
        let definition = registry
            .create(
                Some(c[2].parse().unwrap()),
                Some(c[3].parse().unwrap()),
                None,
                None,
                None,
                c[4].parse().unwrap(),
            )
            .unwrap();
        assert_eq!(definition.amm_id, c[1].parse::<u32>().unwrap());
        assert_eq!(definition.created_at, c[4].parse::<u64>().unwrap());
        let state = registry.load_state(definition.amm_id).unwrap();
        assert_number(state.position, c[5], &c.join(" "));
        assert_number(state.pnl, c[6], &c.join(" "));
        assert_eq!(state.updated_at, c[7].parse::<u64>().unwrap());
        assert_eq!(registry.load_registry(definition.amm_id), Some(&definition));
        cases += 1;
    }
    assert_eq!(cases, 2);
    let mut state = registry.load_state(1).unwrap().clone();
    state.position = 3.0;
    state.pnl = -2.0;
    registry.save_state(1, state.clone(), 2000).unwrap();
    assert_eq!(registry.load_state(1).unwrap().updated_at, 2000);
    let baseline = registry.clone();
    state.pnl = f64::NAN;
    assert_eq!(
        registry.save_state(1, state.clone(), 3000),
        Err(AmmError::NonFinite)
    );
    assert_eq!(registry, baseline);
    assert_eq!(
        registry.save_state(99, state, 3000),
        Err(AmmError::NotFound)
    );
    assert_eq!(registry, baseline);
    assert_eq!(registry.load_registry(99), None);
    assert_eq!(registry.load_state(99), None);
}

#[test]
fn invalid_capital_changes_are_atomic_including_source_partial_mutation_cases() {
    let mut pool = AmmPool::new(0.0, 100.0, 10.0, 7).unwrap();
    pool.add_capital("alice", &token_pair(), 5.0, 7.0, 1)
        .unwrap();
    let baseline = pool.clone();
    for second in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert_eq!(
            pool.add_capital("alice", &token_pair(), 2.0, second, 2),
            Err(AmmError::InvalidSecondAmount)
        );
        assert_eq!(pool, baseline);
        assert_eq!(
            pool.redeem_capital("alice", &token_pair(), 2.0, second, 2),
            Err(AmmError::InvalidSecondAmount)
        );
        assert_eq!(pool, baseline);
    }
    for amount in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert_eq!(
            pool.add_capital("alice", &contract(), amount, 0.0, 2),
            Err(AmmError::InvalidAmount)
        );
        assert_eq!(
            pool.redeem_capital("alice", &contract(), amount, 0.0, 2),
            Err(AmmError::InvalidAmount)
        );
        assert_eq!(pool, baseline);
    }
    assert_eq!(
        pool.redeem_capital("missing", &contract(), 1.0, 0.0, 2),
        Err(AmmError::InsufficientShares)
    );
    assert_eq!(
        pool.redeem_capital("alice", &contract(), 6.0, 0.0, 2),
        Err(AmmError::InsufficientShares)
    );
    assert_eq!(
        pool.add_capital("alice", &contract(), 96.0, 0.0, 2),
        Err(AmmError::MaximumPosition)
    );
    assert_eq!(
        pool.add_capital("alice", &contract(), 1.0, 0.0, MAX_SAFE_BLOCK + 1),
        Err(AmmError::InvalidBlock)
    );
    assert_eq!(pool, baseline);
}

#[test]
fn importing_state_rejects_nonfinite_duplicate_or_prototype_shares() {
    let pool = AmmPool::new(0.0, 100.0, 10.0, 7).unwrap();
    for (position, limit, quote, error) in [
        (f64::NAN, 1.0, 1.0, AmmError::NonFinite),
        (0.0, -1.0, 1.0, AmmError::InvalidLimits),
        (0.0, 1.0, -1.0, AmmError::InvalidLimits),
    ] {
        assert_eq!(AmmPool::new(position, limit, quote, 7), Err(error));
    }
    let mut snapshot = pool.snapshot().clone();
    snapshot.lp_addresses = vec![
        LpShare {
            address: "a".into(),
            amount: 1.0
        };
        2
    ];
    assert_eq!(
        AmmPool::from_snapshot(snapshot.clone()),
        Err(AmmError::InvalidSnapshot)
    );
    snapshot.lp_addresses.truncate(1);
    for amount in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        snapshot.lp_addresses[0].amount = amount;
        assert_eq!(
            AmmPool::from_snapshot(snapshot.clone()),
            Err(AmmError::InvalidSnapshot)
        );
    }
    snapshot.lp_addresses[0].amount = 1.0;
    for owner in ["__proto__", "constructor", "toString", "", "a\n"] {
        snapshot.lp_addresses[0].address = owner.to_owned();
        assert_eq!(
            AmmPool::from_snapshot(snapshot.clone()),
            Err(AmmError::InvalidIdentifier)
        );
    }
}

#[test]
fn capacity_failure_and_float_overflow_never_partially_mutate_state() {
    let mut snapshot = AmmPool::new(0.0, f64::MAX, f64::MAX, 7)
        .unwrap()
        .snapshot()
        .clone();
    snapshot.lp_addresses = (0..MAX_LPS)
        .map(|i| LpShare {
            address: format!("owner-{i}"),
            amount: 1.0,
        })
        .collect();
    let mut pool = AmmPool::from_snapshot(snapshot).unwrap();
    let baseline = pool.clone();
    assert_eq!(
        pool.add_capital("extra", &contract(), 1.0, 0.0, 1),
        Err(AmmError::CapacityExceeded)
    );
    assert_eq!(pool, baseline);
    pool.add_capital("owner-0", &contract(), 1.0, 0.0, 1)
        .unwrap();
    pool.redeem_capital("owner-1", &contract(), 1.0, 0.0, 1)
        .unwrap();
    pool.add_capital("extra", &contract(), 1.0, 0.0, 1).unwrap();
    let mut snapshot = AmmPool::new(f64::MAX, f64::MAX, f64::MAX, 7)
        .unwrap()
        .snapshot()
        .clone();
    snapshot.amm_orders = vec![
        LiquidityOrder {
            size: 1.0,
            side: Side::Buy
        };
        MAX_AMM_ORDERS
    ];
    let mut pool = AmmPool::from_snapshot(snapshot).unwrap();
    let baseline = pool.clone();
    assert_eq!(
        pool.add_capital("alice", &contract(), f64::MAX, 0.0, 1),
        Err(AmmError::NonFinite)
    );
    assert_eq!(
        pool.request_liquidity(LiquidityOrder {
            size: 0.0,
            side: Side::Buy
        }),
        Err(AmmError::CapacityExceeded)
    );
    assert_eq!(
        pool.generate_contract_order(ContractType::Linear, 1.0, 1.0, f64::MAX),
        Err(AmmError::NonFinite)
    );
    assert_eq!(pool, baseline);
    assert_eq!(
        token_curve_intents("1", "2", 100.0, MAX_AMM_ORDERS / 2 + 1, 10.0),
        Err(AmmError::CapacityExceeded)
    );
    assert_eq!(
        token_curve_intents("1", "2", -1.0, 1, 10.0),
        Err(AmmError::NonFinite)
    );
    assert_eq!(
        token_curve_intents("1", "2", 100.0, 1, 0.0),
        Err(AmmError::NonFinite)
    );
}

#[test]
fn registry_metadata_and_state_capacities_are_transactional() {
    let mut registry = AmmRegistry::new();
    let oversized = Some("x".repeat(MAX_METADATA_BYTES + 1));
    assert_eq!(
        registry.create(None, None, oversized, None, None, 1),
        Err(AmmError::CapacityExceeded)
    );
    assert_eq!(registry, AmmRegistry::new());
    for expected in 1..=MAX_AMMS {
        let definition = registry
            .create(None, None, Some(String::new()), None, None, 1)
            .unwrap();
        assert_eq!(definition.amm_id as usize, expected);
        assert_eq!(definition.options_maker, None);
    }
    let baseline = registry.clone();
    assert_eq!(
        registry.create(None, None, None, None, None, 1),
        Err(AmmError::CapacityExceeded)
    );
    let mut state = registry.load_state(1).unwrap().clone();
    state.orders = vec![
        LiquidityOrder {
            size: 1.0,
            side: Side::Buy
        };
        MAX_AMM_ORDERS + 1
    ];
    assert_eq!(
        registry.save_state(1, state, 2),
        Err(AmmError::CapacityExceeded)
    );
    assert_eq!(registry, baseline);
}
