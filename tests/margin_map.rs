use std::collections::BTreeMap;
use tradelayer_core::contract_registry::ContractId;
use tradelayer_core::decimal::Decimal;
use tradelayer_core::ledger::AccountId;
use tradelayer_core::margin_map::*;

fn d(s: &str) -> Decimal {
    s.parse().unwrap()
}
fn a(s: &str) -> AccountId {
    AccountId::try_from(s).unwrap()
}
fn map() -> MarginMap {
    MarginMap::new(ContractId::try_from(1).unwrap())
}
fn opt(s: &str) -> Option<Decimal> {
    if s == "null" {
        None
    } else {
        Some(d(s))
    }
}
fn p(fields: &[&str]) -> Position {
    let mut p = Position::new(a("alice"));
    p.contracts = d(fields[0]);
    p.margin = d(fields[1]);
    p.avg_price = Some(d(fields[2]));
    p.unrealized_pnl = d(fields[3]);
    p
}
fn state(p: &Position, expected: &str, name: &str) {
    let f: Vec<_> = expected.split(',').collect();
    assert_eq!(f.len(), 8);
    assert_eq!(p.contracts, d(f[0]), "{name} contracts");
    assert_eq!(p.margin, d(f[1]), "{name} margin");
    assert_eq!(p.unrealized_pnl, d(f[2]), "{name} upnl");
    assert_eq!(p.avg_price, opt(f[3]), "{name} avg");
    assert_eq!(p.liquidation_price, opt(f[4]), "{name} liq");
    assert_eq!(p.bankruptcy_price, opt(f[5]), "{name} bankruptcy");
    assert_eq!(p.realized_pnl, d(f[6]), "{name} rpnl");
    assert_eq!(p.iou_claim, d(f[7]), "{name} iou");
}
fn context() -> ContractContext {
    ContractContext {
        inverse: false,
        notional: d("1"),
        available: d("10"),
    }
}

#[test]
fn pinned_source_calculations_and_state_transitions() {
    let mut count = 0;
    let mut options = map();
    let mut index = map();
    for line in include_str!("../fixtures/margin_map_vectors.tsv")
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
    {
        let row: Vec<_> = line.split('\t').collect();
        assert_eq!(row.len(), 4);
        let (kind, name, input, expected) = (row[0], row[1], row[2], row[3]);
        let f: Vec<_> = input.split(',').collect();
        let mut m = map();
        match kind {
            "estimate" => assert_eq!(
                estimate_pnl(&d(f[0]), &d(f[1]), &d(f[2]), f[3] == "true", &d(f[4])).unwrap(),
                d(expected),
                "{name}"
            ),
            "price" => assert_eq!(
                price_delta(
                    &d(f[0]),
                    &d(f[1]),
                    &d(f[2]),
                    f[3] == "true",
                    &d(f[4]),
                    f[5] == "true"
                )
                .unwrap(),
                d(expected),
                "{name}"
            ),
            "requirement" => assert_eq!(
                calculate_margin_requirement(&d(f[0]), &d(f[1]), f[2] == "true").unwrap(),
                d(expected),
                "{name}"
            ),
            "liquidation" => {
                let result = calculate_liquidation_price(
                    &d(f[0]),
                    &d(f[1]),
                    &d(f[2]),
                    &d(f[3]),
                    f[4] == "true",
                    f[5] == "true",
                    &d(f[6]),
                    &d(f[7]),
                )
                .unwrap();
                let e: Vec<_> = expected.split(',').collect();
                assert_eq!(result.bankruptcy_price, opt(e[0]), "{name}");
                assert_eq!(result.liquidation_price, opt(e[1]), "{name}");
            }
            "average" => {
                let mut p = Position::new(a("alice"));
                p.contracts = d(f[0]);
                p.avg_price = Some(d(f[1]));
                assert_eq!(
                    update_average_price(&p, &d(f[2]), &d(f[3]), f[4] == "true").unwrap(),
                    opt(expected),
                    "{name}"
                );
            }
            "maintenance" => {
                let mut p = Position::new(a("alice"));
                p.contracts = d(input);
                m.write_position(p).unwrap();
                assert_eq!(
                    m.compute_maintenance_margin_requirement(&a("alice"), &d("2"))
                        .unwrap(),
                    d(expected),
                    "{name}"
                );
            }
            "trade" => {
                m.write_position(p(&f)).unwrap();
                let trade = PositionTrade {
                    address: a("alice"),
                    amount: d(f[4]),
                    price: d(f[5]),
                    sell: f[6] == "true",
                    close: d(f[7]),
                    flip: d(f[8]),
                    in_clearing: false,
                    initial: false,
                    block: 10,
                };
                let out = m.update_contract_balances(&trade, &context()).unwrap();
                state(&out, expected, name);
            }
            "initial" | "update" | "fee" | "clear" => {
                m.write_position(p(&f)).unwrap();
                let value = d(f[4]);
                let out = match kind {
                    "initial" => m.set_initial_margin(&a("alice"), &value, 10),
                    "update" => m.update_margin(&a("alice"), &value, 10),
                    "fee" => m.fee_margin_reduce(&a("alice"), &value, 10),
                    "clear" => m.clear(&a("alice"), &value, &d("12"), 10, &d("14"), &d("11")),
                    _ => unreachable!(),
                }
                .unwrap();
                state(&out, expected, name);
            }
            "reduce" => {
                m.write_position(p(&f)).unwrap();
                let (reduction, s) = expected.split_once(';').unwrap();
                assert_eq!(
                    m.reduce_margin(&a("alice"), &d("2"), f[4] == "true", &d(f[5]), 10)
                        .unwrap(),
                    d(reduction),
                    "{name}"
                );
                state(m.get(&a("alice")).unwrap(), s, name);
            }
            "realize" => {
                m.write_position(p(&f)).unwrap();
                let out = m
                    .realize_pnl(
                        &a("alice"),
                        &d("2"),
                        &d("13"),
                        &d("10"),
                        f[4] == "true",
                        &d("2.5"),
                        f[5] == "true",
                    )
                    .unwrap();
                state(&out, expected, name);
            }
            "settle" => {
                m.write_position(p(&f)).unwrap();
                let (pnl, s) = expected.split_once(';').unwrap();
                assert_eq!(
                    m.settle_pnl(
                        &a("alice"),
                        &d("2"),
                        &d("13"),
                        Some(&d("11")),
                        f[4] == "true",
                        &d("2.5"),
                        10
                    )
                    .unwrap(),
                    d(pnl),
                    "{name}"
                );
                state(m.get(&a("alice")).unwrap(), s, name);
            }
            "option" => {
                let out = options
                    .apply_option_trade(&a("alice"), "1-100-C-20", &d(f[0]), &d(f[1]), &d(f[2]), 10)
                    .unwrap();
                let e: Vec<_> = expected.split(',').collect();
                assert_eq!(out.position.contracts, d(e[0]), "{name}");
                assert_eq!(out.position.avg_price, d(e[1]), "{name}");
                assert_eq!(out.position.margin, d(e[2]), "{name}");
                assert_eq!(out.closed_quantity, d(e[3]), "{name}");
                assert_eq!(out.flip_quantity, d(e[4]), "{name}");
                assert_eq!(
                    options.option_open_interest("1-100-C-20"),
                    e[5].parse().unwrap()
                );
            }
            "adl" => {
                for (address, contracts, margin, mark) in [
                    ("bob", "-3", "20", "10"),
                    ("carol", "-2", "15", "11"),
                    ("liquidator", "5", "30", "10"),
                ] {
                    let mut p = Position::new(a(address));
                    p.contracts = d(contracts);
                    p.margin = d(margin);
                    p.avg_price = Some(d("10"));
                    p.last_mark = Some(d(mark));
                    m.write_position(p).unwrap();
                }
                let request = DeleverageRequest {
                    liquidating_address: a("liquidator"),
                    unfilled: d("4"),
                    sell: true,
                    inverse: f[0] == "true",
                    notional: d("2.5"),
                    mark: d(f[1]),
                    initial_per_contract: d("2"),
                    liquidation_pool: d("7"),
                    ledger_shortfalls: BTreeMap::new(),
                    block: 10,
                };
                let out = m.simple_deleverage(&request).unwrap();
                let e: Vec<_> = expected.split('|').collect();
                assert_eq!(out.total_deleveraged, d(e[0]));
                for (cp, want) in out.counterparties.iter().zip(e[1].split(';')) {
                    let w: Vec<_> = want.split(',').collect();
                    assert_eq!(cp.address, a(w[0]));
                    assert_eq!(cp.matched, d(w[1]));
                    assert_eq!(cp.after, d(w[2]));
                    assert_eq!(m.get(&cp.address).unwrap().margin, d(w[3]));
                    assert_eq!(m.get(&cp.address).unwrap().avg_price, Some(d(w[4])));
                }
                for (cp, want) in out.counterparties.iter().zip(e[2].split(';')) {
                    let w: Vec<_> = want.split(',').collect();
                    assert_eq!(cp.address, a(w[0]));
                    assert_eq!(cp.pool_share, d(w[1]), "{name}");
                }
            }
            "index" => {
                index.touch_expiry_index(&d(f[0]), &d(f[1]), f[2]).unwrap();
                let (tickers, oi) = expected.split_once('|').unwrap();
                let expected_tickers: Vec<_> =
                    tickers.split(';').filter(|s| !s.is_empty()).collect();
                assert_eq!(
                    index.get_expiring_tickers_up_to(300),
                    expected_tickers,
                    "{name}"
                );
                assert_eq!(
                    index.option_open_interest(f[2]),
                    oi.parse().unwrap(),
                    "{name}"
                );
            }
            "protection" => {
                let mut p = Position::new(a("alice"));
                p.contracts = d(f[0]);
                p.options.insert(
                    f[2].to_owned(),
                    OptionPosition {
                        contracts: d(f[3]),
                        avg_price: d("0"),
                        margin: d("0"),
                    },
                );
                m.write_position(p).unwrap();
                assert_eq!(
                    m.calc_liquidation_with_options(&a("alice"), &d(f[1]))
                        .unwrap(),
                    d(expected),
                    "{name}"
                );
            }
            "mint" => {
                m.write_position(p(&[f[0], f[1], "10", "0"])).unwrap();
                let out = m
                    .move_margin_and_contracts_for_mint(&a("alice"), 2, &d(f[2]), &d(f[3]), 10)
                    .unwrap();
                let e: Vec<_> = expected.split('|').collect();
                let r: Vec<_> = e[0].split(',').collect();
                assert_eq!(out.contracts, d(r[0]));
                assert_eq!(out.margin, d(r[1]));
                assert_eq!(out.excess, d(r[2]));
                state(m.get(&a("alice")).unwrap(), e[1], name);
                let v: Vec<_> = e[2].split(',').collect();
                let vault = m.get(&out.vault_address).unwrap();
                assert_eq!(vault.contracts, d(v[0]));
                assert_eq!(vault.margin, d(v[1]));
                assert_eq!(vault.avg_price, opt(v[3]));
            }
            "order" => {
                let mut p = Position::new(a("alice"));
                p.contracts = d(f[0]);
                p.margin = d(f[1]);
                p.available = d(f[2]);
                m.write_position(p).unwrap();
                let out = m
                    .generate_liquidation_order(&a("alice"), &d(f[3]), &d("99.125"), 10)
                    .unwrap();
                if expected == "null" {
                    assert!(out.is_none());
                } else {
                    let out = out.unwrap();
                    let e: Vec<_> = expected.split(',').collect();
                    assert_eq!(out.amount, d(e[0]));
                    assert_eq!(out.price, d(e[1]));
                    assert_eq!(out.sell, e[2] == "true");
                    assert_eq!(e[3], "true");
                    assert_eq!(e[4], "true");
                }
            }
            "lookup" => {
                let mut p = Position::new(a("BC1QABC"));
                p.contracts = d("2");
                p.margin = d("10");
                m.write_position(p).unwrap();
                let out = m.get_position_for_address(&a(input));
                let e: Vec<_> = expected.split(',').collect();
                assert_eq!(out.contracts, d(e[0]));
                assert_eq!(out.margin, d(e[1]));
                assert_eq!(out.unrealized_pnl, d(e[2]));
            }
            "merge" => {
                let mut funded = Position::new(a("funded"));
                funded.margin = d("100");
                let mut claim = Position::new(a("claim"));
                claim.iou_claim = d("1");
                let mut active = Position::new(a("active"));
                active.contracts = d("2");
                active.margin = d("10");
                m.merge_positions(&[funded, claim, active], input == "true")
                    .unwrap();
                assert_eq!(
                    m.positions()
                        .map(|p| p.address.as_str())
                        .collect::<Vec<_>>()
                        .join(","),
                    expected
                );
            }
            "adjust" => {
                m.write_position(p(&[f[0], "20", "10", "0"])).unwrap();
                m.adjust_deleveraging(&a("alice"), &d(f[1]), &d("2"), &d("0"), 10)
                    .unwrap();
                state(m.get(&a("alice")).unwrap(), expected, name);
            }
            _ => panic!("unknown vector {kind}"),
        }
        count += 1;
    }
    assert_eq!(count, 246);
}

#[test]
fn trade_pair_rolls_back_both_sides_and_deltas_on_second_failure() {
    let mut m = map();
    m.write_position(p(&["2", "10", "100", "0"])).unwrap();
    let before = m.clone();
    let trade = PositionTrade {
        address: a("alice"),
        amount: d("1"),
        price: d("101"),
        sell: false,
        close: d("0"),
        flip: d("0"),
        in_clearing: false,
        initial: false,
        block: 10,
    };
    let mut seller = trade.clone();
    seller.address = a("missing");
    assert_eq!(
        m.update_contract_balances_with_match(&trade, &context(), &seller, &context()),
        Err(MarginError::MissingPosition)
    );
    assert_eq!(m, before);
}

#[test]
fn expiry_source_helper_preserves_insertion_order_and_counts_addresses() {
    let mut m = map();
    for ticker in ["1-200-C-10", "1-100-P-9", "1-200-P-8"] {
        m.touch_expiry_index(&d("0"), &d("1"), ticker).unwrap();
    }
    m.touch_expiry_index(&d("0"), &d("-8"), "1-200-C-10")
        .unwrap();
    assert_eq!(m.option_open_interest("1-200-C-10"), 2);
    assert_eq!(
        m.get_expiring_tickers_up_to(300),
        ["1-200-C-10", "1-200-P-8", "1-100-P-9"]
    );
    m.touch_expiry_index(&d("1"), &d("0"), "1-200-C-10")
        .unwrap();
    assert_eq!(m.option_open_interest("1-200-C-10"), 1);
    m.cleanup_expired_tickers_up_to(100);
    assert_eq!(
        m.get_expiring_tickers_up_to(300),
        ["1-200-C-10", "1-200-P-8"]
    );
}

#[test]
fn source_merge_drops_funded_flat_positions_but_keeps_iou() {
    let mut m = map();
    let mut funded = Position::new(a("funded"));
    funded.margin = d("100");
    let mut claim = Position::new(a("claim"));
    claim.iou_claim = d("1");
    m.merge_positions(&[funded, claim], true).unwrap();
    assert!(m.get(&a("funded")).is_none());
    assert!(m.get(&a("claim")).is_some());
    assert_eq!(m.cleanup_flat_positions(), 0);
}

#[test]
fn bech32_fallback_depends_on_query_prefix_case() {
    let mut m = map();
    let mut p = Position::new(a("BC1QABC"));
    p.contracts = d("2");
    m.write_position(p).unwrap();
    assert_eq!(m.get_position_for_address(&a("bc1qabc")).contracts, d("2"));
    assert_eq!(m.get_position_for_address(&a("Bc1qabc")).contracts, d("0"));
}

#[test]
fn deliberate_iou_repair_is_explicit_and_does_not_move_funded_margin() {
    let mut m = map();
    m.write_position(p(&["1", "10", "100", "0"])).unwrap();
    let before = m.clone();
    m.apply_legacy_iou_claim_delta();
    assert_eq!(m, before);
    let shares = m
        .allocate_iou_claims(&a("alice"), &a("absent"), &d("1"), &d("2"), &d("3"))
        .unwrap();
    assert_eq!(shares, vec![(a("alice"), d("1"))]);
    assert_eq!(m.get(&a("alice")).unwrap().margin, d("10"));
    assert_eq!(m.get(&a("alice")).unwrap().iou_claim, d("1"));
}

#[test]
fn repaired_claim_rounding_cannot_exceed_imbalance_budget() {
    let mut m = map();
    for address in ["alice", "bob"] {
        m.write_position(Position::new(a(address))).unwrap();
    }
    let shares = m
        .allocate_iou_claims(&a("alice"), &a("bob"), &d("1"), &d("1"), &d("0.00000001"))
        .unwrap();
    assert_eq!(
        shares,
        vec![(a("alice"), d("0.00000001")), (a("bob"), d("0"))]
    );
    let total = m
        .get(&a("alice"))
        .unwrap()
        .iou_claim
        .checked_add(&m.get(&a("bob")).unwrap().iou_claim)
        .unwrap();
    assert_eq!(total, d("0.00000001"));
    let before = m.clone();
    m.allocate_iou_claims(&a("alice"), &a("bob"), &d("1"), &d("1"), &d("0.000000009"))
        .unwrap();
    assert_eq!(m, before, "sub-atom imbalance remains unallocated");
    // An existing high-magnitude claim must not round the actual increment up
    // through binary64, either: the repaired claim is an exact decimal field.
    let mut p = m.get(&a("alice")).unwrap().clone();
    p.iou_claim = d("10000000000000000");
    m.write_position(p).unwrap();
    m.allocate_iou_claims(&a("alice"), &a("bob"), &d("1"), &d("0"), &d("1"))
        .unwrap();
    assert_eq!(
        m.get(&a("alice")).unwrap().iou_claim,
        d("10000000000000001")
    );
}

#[test]
fn nonfinite_boundary_rejects_and_rolls_back() {
    let mut m = map();
    m.write_position(p(&["1", "10", "100", "0"])).unwrap();
    let before = m.clone();
    assert_eq!(
        m.set_initial_margin(&a("alice"), &d("1e1000"), 10),
        Err(MarginError::NonFinite)
    );
    assert_eq!(m, before);
}

#[test]
fn expired_options_still_protect_per_actual_source_helper() {
    let mut m = map();
    m.write_position(p(&["1", "10", "100", "0"])).unwrap();
    m.apply_option_trade(&a("alice"), "1-1-P-90", &d("0.001"), &d("1"), &d("0"), 100)
        .unwrap();
    assert_eq!(
        m.calc_liquidation_with_options(&a("alice"), &d("80"))
            .unwrap(),
        d("90")
    );
}

#[test]
fn maintenance_ignores_unrealized_pnl_and_compares_strictly() {
    let mut m = map();
    m.write_position(p(&["2", "2", "100", "-999"])).unwrap();
    assert_eq!(
        m.compute_maintenance_margin_requirement(&a("alice"), &d("2"))
            .unwrap(),
        d("2")
    );
    assert_eq!(
        m.is_below_maintenance_margin(&a("alice"), &d("2")).unwrap(),
        Some(false)
    );
    assert_eq!(
        m.is_below_maintenance_margin(&a("absent"), &d("2"))
            .unwrap(),
        None
    );
}

#[test]
fn deleverage_noop_preserves_map_and_flat_reset_keeps_avg_price() {
    let mut m = map();
    m.write_position(p(&["2", "10", "100", "-1"])).unwrap();
    let before = m.clone();
    m.adjust_deleveraging(&a("alice"), &d("0"), &d("2"), &d("0"), 10)
        .unwrap();
    assert_eq!(m, before);
    let out = m
        .adjust_deleveraging(&a("alice"), &d("100"), &d("2"), &d("0"), 10)
        .unwrap();
    assert_eq!(out.after, d("0"));
    assert_eq!(out.collateral_return, d("10"));
    assert_eq!(m.get(&a("alice")).unwrap().avg_price, Some(d("100")));
    assert_eq!(
        m.deltas().iter().filter(|d| d.mode == "deleverage").count(),
        2
    );
}
