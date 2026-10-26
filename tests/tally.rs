use tradelayer_core::decimal::{Decimal, Rounding};
use tradelayer_core::ledger::PropertyId;
use tradelayer_core::tally::*;

fn d(s: &str) -> Decimal {
    s.parse().unwrap()
}
fn p(id: u32) -> PropertyId {
    PropertyId::try_from(id).unwrap()
}
fn key(id: u32, contract: u32) -> FeeKey {
    FeeKey {
        property: p(id),
        contract,
    }
}
fn buckets(s: &str) -> LegacyBuckets {
    let v: Vec<_> = s.split(',').map(d).collect();
    assert_eq!(v.len(), 5);
    LegacyBuckets {
        available: v[0].clone(),
        reserved: v[1].clone(),
        margin: v[2].clone(),
        vesting: v[3].clone(),
        channel: v[4].clone(),
    }
}
fn tally(s: &str) -> LegacyTally {
    let (amount, b) = s.split_once(',').unwrap();
    LegacyTally {
        amount: d(amount),
        buckets: buckets(b),
    }
}
fn maker(s: &str) -> MakerFlag {
    match s {
        "true" => MakerFlag::True,
        "false" => MakerFlag::False,
        "undefined" => MakerFlag::Missing,
        _ => panic!("flag"),
    }
}
fn signed(s: &str) -> SignedAtoms {
    if let Some(v) = s.strip_prefix('-') {
        SignedAtoms::Credit(v.parse().unwrap())
    } else {
        SignedAtoms::Debit(s.parse().unwrap())
    }
}
fn route(s: &str, contract: u32) -> FeeRoute {
    match s {
        "spot" => FeeRoute::Spot,
        "native" => FeeRoute::Native(contract),
        "oracle" => FeeRoute::Oracle(contract),
        _ => panic!("route"),
    }
}
fn raw_atoms(s: &str) -> Decimal {
    d(s).checked_mul(&Decimal::from(ATOMS_PER_TOKEN)).unwrap()
}

#[test]
fn complete_source_vm_numeric_and_state_corpus() {
    let mut counts = std::collections::BTreeMap::new();
    for (index, line) in include_str!("../fixtures/tally_vectors.tsv")
        .lines()
        .enumerate()
    {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let f: Vec<_> = line.split('\t').collect();
        *counts.entry(f[0]).or_insert(0) += 1;
        match f[0] {
            "TOTAL" => assert_eq!(
                source_calculate_total(&buckets(f[1])).unwrap(),
                d(f[2]),
                "line {}",
                index + 1
            ),
            "PROPERTY" => assert_eq!(
                source_total_for_property(&[buckets(f[1]), buckets(f[2])]).unwrap(),
                d(f[3]),
                "line {}",
                index + 1
            ),
            "TALLY" => assert_eq!(
                source_total_tally(&[tally(f[1]), tally(f[2])]).unwrap(),
                tally(f[3]),
                "line {}",
                index + 1
            ),
            "TOKENS" => {
                let v: Vec<_> = f[1].split(',').map(d).collect();
                let a = LegacyBuckets {
                    available: v[0].clone(),
                    reserved: v[1].clone(),
                    ..Default::default()
                };
                let b = LegacyBuckets {
                    available: v[1].clone(),
                    reserved: v[0].clone(),
                    ..Default::default()
                };
                assert_eq!(
                    source_total_tokens(&[a, b]).unwrap(),
                    d(f[2]),
                    "line {}",
                    index + 1
                );
            }
            "UPDATE" | "CHANNEL" => {
                let result = if f[0] == "UPDATE" {
                    source_update_projection(&tally(f[1]), &buckets(f[2]))
                } else {
                    source_update_channel_projection(&tally(f[1]), &d(f[2]))
                }
                .unwrap();
                assert_eq!(result.after, tally(f[3]), "line {}", index + 1);
                let rejection = if result.vesting_rejected {
                    "vesting"
                } else {
                    match result.rejected_bucket {
                        None => "none",
                        Some(BalanceBucket::Available) => "available",
                        Some(BalanceBucket::Reserved) => "reserved",
                        Some(BalanceBucket::Margin) => "margin",
                        Some(BalanceBucket::Channel) => "Error",
                    }
                };
                assert_eq!(rejection, f[4], "line {}", index + 1);
                assert_eq!(
                    usize::from(result.delta_emitted),
                    f[5].parse::<usize>().unwrap()
                );
            }
            "MATCH" => {
                let result = calculate_match_fees(&MatchFeeInput {
                    amount_buy: d(f[1]),
                    amount_sell: d(f[2]),
                    buy_maker: maker(f[3]),
                    sell_maker: maker(f[4]),
                    inverse: f[5] == "true",
                    last_mark: d(f[6]),
                    notional_value: d(f[7]),
                    channel: f[8] == "true",
                })
                .unwrap();
                assert_eq!(result.buyer, signed(f[9]), "line {}", index + 1);
                assert_eq!(result.seller, signed(f[10]), "line {}", index + 1);
                assert_eq!(result.retained_atoms, f[11].parse::<u128>().unwrap());
                assert_eq!(
                    result.buyer.debit_atoms() + result.seller.debit_atoms(),
                    result.retained_atoms
                        + result.buyer.credit_atoms()
                        + result.seller.credit_atoms()
                );
            }
            "ACCRUE" => {
                let conversion = source_fee_conversion(&d(f[4])).unwrap();
                let raw = raw_atoms(f[4]);
                let error = if conversion.atoms > 0 && !conversion.fractional_sats.is_zero() {
                    "TypeError"
                } else {
                    "none"
                };
                assert_eq!(
                    error,
                    f[8],
                    "source NeDB get/put mismatch, line {}",
                    index + 1
                );
                if raw > Decimal::zero() {
                    assert_eq!(
                        conversion
                            .fractional_sats
                            .checked_add(&Decimal::from(conversion.atoms))
                            .unwrap(),
                        raw
                    );
                }
                let mut book = FeeBook::new();
                if error == "none" {
                    let plan = book
                        .accrue_atoms(
                            p(f[1].parse().unwrap()),
                            route(f[2], f[3].parse().unwrap()),
                            conversion.atoms,
                            11,
                        )
                        .unwrap();
                    assert_eq!(plan.value_added_atoms, f[5].parse::<u128>().unwrap());
                    assert_eq!(plan.stash_added_atoms, f[6].parse::<u128>().unwrap());
                    assert_eq!(plan.insurance_atoms, f[7].parse::<u128>().unwrap());
                    assert_eq!(plan.collection_atoms, conversion.atoms);
                } else {
                    // Source throws before saving its first row or insurance deposit.
                    assert_eq!((f[5], f[6], f[7]), ("0", "0", "0"));
                }
            }
            "QUEUE" => {
                let property = p(f[1].parse().unwrap());
                let amount = d(f[2])
                    .quantize(8, Rounding::HalfUp)
                    .unwrap()
                    .checked_mul(&Decimal::from(ATOMS_PER_TOKEN))
                    .unwrap()
                    .to_string()
                    .parse::<u128>()
                    .unwrap();
                // Source erases Spot identity. Reproduce its observed route using
                // Native(1); the repaired API retains Spot in the next assertion.
                let mut source_route = FeeBook::new();
                let plan = source_route
                    .accrue_atoms(property, FeeRoute::Native(1), amount, 11)
                    .unwrap();
                assert_eq!(plan.value_added_atoms, f[3].parse::<u128>().unwrap());
                assert_eq!(plan.stash_added_atoms, f[4].parse::<u128>().unwrap());
                assert_eq!(plan.insurance_atoms, f[5].parse::<u128>().unwrap());
                let mut repaired = FeeBook::new();
                repaired
                    .queue_repaired(property, FeeRoute::Spot, amount, 11)
                    .unwrap();
                let plans = repaired.flush_repaired(11).unwrap();
                if amount > 0 {
                    let mut direct = FeeBook::new();
                    direct
                        .accrue_atoms(property, FeeRoute::Spot, amount, 11)
                        .unwrap();
                    assert_eq!(
                        repaired.rows().collect::<Vec<_>>(),
                        direct.rows().collect::<Vec<_>>()
                    );
                    assert_eq!(plans[0].collection_atoms, 0);
                }
            }
            "DRAW" => {
                let row = FeeRow {
                    value_atoms: f[1].parse().unwrap(),
                    stash_atoms: f[2].parse().unwrap(),
                };
                let mut book = FeeBook::from_snapshot(FeeBookSnapshot {
                    rows: vec![(key(1, 1), row)],
                    ..Default::default()
                })
                .unwrap();
                let maximum = if f[3] == "none" {
                    None
                } else {
                    Some(f[3].parse().unwrap())
                };
                let plan = book
                    .draw_source_order(key(1, 1), maximum, f[4] == "true", 11)
                    .unwrap();
                assert_eq!(plan.spent_atoms().unwrap(), f[5].parse::<u128>().unwrap());
                assert_eq!(
                    book.row(key(1, 1)),
                    FeeRow {
                        value_atoms: f[6].parse().unwrap(),
                        stash_atoms: f[7].parse().unwrap()
                    }
                );
            }
            _ => panic!("unconsumed source vector {line}"),
        }
    }
    assert_eq!(counts.values().sum::<usize>(), 248);
    assert_eq!(counts["MATCH"], 120);
    assert_eq!(counts["ACCRUE"], 40);
    assert_eq!(counts.len(), 10);
}

#[test]
fn projections_require_caller_custody_and_exclude_vesting() {
    let first = CustodyAtoms {
        available: 10,
        reserved: 20,
        margin: 30,
        channel: 40,
        vesting: d("9000000000000000000000000.0000000001"),
    };
    let second = CustodyAtoms {
        available: 2,
        ..Default::default()
    };
    assert_eq!(first.owned_atoms(), Ok(100));
    let total = aggregate_custody([&first, &second]).unwrap();
    assert_eq!(total.owned_atoms(), Ok(102));
    assert_eq!(total.vesting, first.vesting);
    for (bucket, required, shortfall) in [
        (BalanceBucket::Available, 11, 1),
        (BalanceBucket::Reserved, 20, 0),
        (BalanceBucket::Margin, 99, 69),
        (BalanceBucket::Channel, 1, 0),
    ] {
        assert_eq!(
            first.sufficient(bucket, required),
            Sufficiency {
                has_sufficient: shortfall == 0,
                shortfall_atoms: shortfall
            }
        );
    }
    assert_eq!(
        aggregate_custody([&CustodyAtoms {
            vesting: d("-1"),
            ..Default::default()
        }]),
        Err(TallyError::InvalidInput)
    );
    assert_eq!(
        aggregate_custody([&CustodyAtoms {
            available: u128::MAX,
            reserved: 1,
            ..Default::default()
        }]),
        Err(TallyError::AmountOverflow)
    );
}

#[test]
fn mixed_routes_same_physical_key_are_retained_without_double_collection() {
    let mut book = FeeBook::new();
    let first = book.queue_repaired(p(1), FeeRoute::Spot, 3, 10).unwrap();
    let second = book
        .queue_repaired(p(1), FeeRoute::Native(1), 7, 10)
        .unwrap();
    assert_eq!(
        (first.custody_after_atoms, second.custody_after_atoms),
        (3, 10)
    );
    assert_eq!(book.expected_custody_atoms(key(1, 1)), Ok(10));
    let plans = book.flush_repaired(10).unwrap();
    assert_eq!(plans.len(), 2);
    assert_eq!(
        (plans[0].custody_before_atoms, plans[0].custody_after_atoms),
        (10, 9)
    );
    assert_eq!(
        (plans[1].custody_before_atoms, plans[1].custody_after_atoms),
        (9, 9)
    );
    assert!(plans.iter().all(|p| p.collection_atoms == 0));
    assert_eq!(
        book.row(key(1, 1)),
        FeeRow {
            value_atoms: 7,
            stash_atoms: 2
        }
    );
    assert_eq!(book.expected_custody_atoms(key(1, 1)), Ok(9));
    assert_eq!(book.verify_custody([(key(1, 1), 9)]), Ok(()));
    assert_eq!(
        book.verify_custody([(key(1, 1), 10)]),
        Err(TallyError::CustodyMismatch)
    );
    assert!(book.flush_repaired(10).unwrap().is_empty());
    assert_eq!(FeeBook::from_snapshot(book.snapshot()).unwrap(), book);
}

#[test]
fn coalesced_queue_and_cross_block_pending_custody_are_accounted_once() {
    let mut book = FeeBook::new();
    book.queue_repaired(p(2), FeeRoute::Oracle(9), 3, 5)
        .unwrap();
    book.queue_repaired(p(2), FeeRoute::Oracle(9), 4, 5)
        .unwrap();
    book.queue_repaired(p(2), FeeRoute::Oracle(9), 10, 6)
        .unwrap();
    assert_eq!(book.queued().count(), 2);
    let plan = book.flush_repaired(5).unwrap()[0];
    assert_eq!(
        (
            plan.amount_atoms,
            plan.insurance_atoms,
            plan.stash_added_atoms
        ),
        (7, 3, 4)
    );
    assert_eq!(
        (plan.custody_before_atoms, plan.custody_after_atoms),
        (17, 14)
    );
    assert_eq!(book.expected_custody(), Ok(vec![(key(2, 9), 14)]));
    let plan = book.flush_repaired(6).unwrap()[0];
    assert_eq!(
        (plan.custody_before_atoms, plan.custody_after_atoms),
        (14, 9)
    );
    assert_eq!(book.row(key(2, 9)).stash_atoms, 9);
}

#[test]
fn plan_conservation_covers_every_route_odd_splits_and_zero() {
    for property in [1, 2] {
        for route in [
            FeeRoute::Spot,
            FeeRoute::Native(1),
            FeeRoute::Native(7),
            FeeRoute::Oracle(9),
        ] {
            let mut book = FeeBook::new();
            for amount in [0, 1, 2, 3, 100, u64::MAX as u128] {
                let plan = book.accrue_atoms(p(property), route, amount, 0).unwrap();
                assert_eq!(
                    plan.custody_before_atoms + amount,
                    plan.custody_after_atoms + plan.insurance_atoms
                );
                assert_eq!(
                    amount,
                    plan.value_added_atoms + plan.stash_added_atoms + plan.insurance_atoms
                );
                book.verify_custody(book.expected_custody().unwrap())
                    .unwrap();
            }
            assert_eq!(book.journal().len(), 5);
            assert_eq!(FeeBook::from_snapshot(book.snapshot()).unwrap(), book);
        }
    }
}

#[test]
fn source_value_first_draw_does_not_top_up_from_stash() {
    let mut book = FeeBook::new();
    book.accrue_atoms(p(1), FeeRoute::Native(1), 2, 0).unwrap();
    book.accrue_atoms(p(1), FeeRoute::Spot, 15, 0).unwrap();
    let first = book.draw_source_order(key(1, 1), Some(5), true, 1).unwrap();
    assert_eq!((first.value_atoms, first.stash_atoms), (2, 0));
    let second = book.draw_source_order(key(1, 1), Some(5), true, 1).unwrap();
    assert_eq!((second.value_atoms, second.stash_atoms), (0, 5));
    assert_eq!(
        (second.custody_before_atoms, second.custody_after_atoms),
        (8, 3)
    );
    assert_eq!(FeeBook::from_snapshot(book.snapshot()).unwrap(), book);
}

#[test]
fn snapshot_import_rejects_malformed_rows_routes_and_journal_math() {
    let mut book = FeeBook::new();
    book.accrue_atoms(p(1), FeeRoute::Spot, 7, 1).unwrap();
    book.queue_repaired(p(1), FeeRoute::Native(1), 9, 2)
        .unwrap();
    let original = book.snapshot();
    let mut bad = original.clone();
    bad.rows.push(bad.rows[0]);
    assert_eq!(
        FeeBook::from_snapshot(bad),
        Err(TallyError::InvalidSnapshot)
    );
    let mut bad = original.clone();
    bad.queued[0].key.contract = 99;
    assert_eq!(
        FeeBook::from_snapshot(bad),
        Err(TallyError::InvalidSnapshot)
    );
    let mut bad = original.clone();
    bad.queued.push(bad.queued[0]);
    assert_eq!(
        FeeBook::from_snapshot(bad),
        Err(TallyError::InvalidSnapshot)
    );
    let mut bad = original.clone();
    bad.queued[0].block = MAX_SAFE_BLOCK + 1;
    assert_eq!(FeeBook::from_snapshot(bad), Err(TallyError::InvalidInput));
    let mut bad = original.clone();
    if let FeeJournalEntry::Accrued(ref mut plan) = bad.journal[0] {
        plan.insurance_atoms += 1;
    }
    assert_eq!(
        FeeBook::from_snapshot(bad),
        Err(TallyError::InvalidSnapshot)
    );
    let mut bad = original.clone();
    if let FeeJournalEntry::Accrued(ref mut plan) = bad.journal[0] {
        plan.custody_before_atoms = 50;
    }
    assert_eq!(
        FeeBook::from_snapshot(bad),
        Err(TallyError::InvalidSnapshot)
    );
    let mut bad = original.clone();
    bad.rows[0].1.value_atoms = u128::MAX;
    assert_eq!(FeeBook::from_snapshot(bad), Err(TallyError::AmountOverflow));
    assert_eq!(FeeBook::from_snapshot(original).unwrap(), book);
}

#[test]
fn failed_updates_and_flush_leave_original_metadata_unchanged() {
    let mut book = FeeBook::from_snapshot(FeeBookSnapshot {
        rows: vec![(
            key(2, 1),
            FeeRow {
                value_atoms: u128::MAX,
                stash_atoms: 0,
            },
        )],
        ..Default::default()
    })
    .unwrap();
    let original = book.clone();
    assert_eq!(
        book.accrue_atoms(p(2), FeeRoute::Spot, 1, 1),
        Err(TallyError::AmountOverflow)
    );
    assert_eq!(book, original);
    assert_eq!(
        book.queue_repaired(p(2), FeeRoute::Spot, 1, 1),
        Err(TallyError::AmountOverflow)
    );
    assert_eq!(book, original);
    assert_eq!(
        book.accrue_atoms(p(2), FeeRoute::Native(0), 1, 1),
        Err(TallyError::InvalidInput)
    );
    assert_eq!(book, original);
    // Even if splitting out insurance would make the final row fit, the full
    // pre-split funded amount and its journal arithmetic must fit atom bounds.
    let mut split_overflow = FeeBook::from_snapshot(FeeBookSnapshot {
        rows: vec![(
            key(1, 1),
            FeeRow {
                value_atoms: 0,
                stash_atoms: u128::MAX - 1,
            },
        )],
        ..Default::default()
    })
    .unwrap();
    let original_split = split_overflow.clone();
    assert_eq!(
        split_overflow.accrue_atoms(p(1), FeeRoute::Spot, 2, 1),
        Err(TallyError::AmountOverflow)
    );
    assert_eq!(split_overflow, original_split);
    let mut bounded = FeeBook::new();
    bounded.queue_repaired(p(1), FeeRoute::Spot, 3, 1).unwrap();
    bounded.queue_repaired(p(2), FeeRoute::Spot, 3, 1).unwrap();
    let mut snapshot = bounded.snapshot();
    snapshot
        .journal
        .resize(MAX_FEE_JOURNAL - 1, snapshot.journal[0]);
    let mut bounded = FeeBook::from_snapshot(snapshot).unwrap();
    let original = bounded.clone();
    assert_eq!(bounded.flush_repaired(1), Err(TallyError::Capacity));
    assert_eq!(bounded, original);
}

#[test]
fn metadata_bounds_and_custody_input_are_validated() {
    let mut book = FeeBook::new();
    assert_eq!(
        book.accrue_atoms(p(1), FeeRoute::Spot, 1, MAX_SAFE_BLOCK + 1),
        Err(TallyError::InvalidInput)
    );
    assert_eq!(
        book.verify_custody([(key(1, 1), 1)]),
        Err(TallyError::CustodyMismatch)
    );
    assert_eq!(
        book.verify_custody([(key(1, 1), 0), (key(1, 1), 0)]),
        Err(TallyError::CustodyMismatch)
    );
    assert_eq!(
        book.verify_custody([(key(1, 0), 0)]),
        Err(TallyError::CustodyMismatch)
    );
    assert_eq!(
        FeeBook::from_snapshot(FeeBookSnapshot {
            rows: vec![(key(1, 1), FeeRow::default()); MAX_FEE_ROWS + 1],
            ..Default::default()
        }),
        Err(TallyError::Capacity)
    );
    assert_eq!(
        FeeBook::from_snapshot(FeeBookSnapshot {
            journal: vec![
                FeeJournalEntry::Queued(FeeQueuePlan {
                    entry: QueuedFee {
                        block: 1,
                        key: key(1, 1),
                        route: FeeRoute::Spot,
                        atoms: 1
                    },
                    custody_before_atoms: 0,
                    custody_after_atoms: 1
                });
                MAX_FEE_JOURNAL + 1
            ],
            ..Default::default()
        }),
        Err(TallyError::Capacity)
    );
}

#[test]
fn fee_limits_reject_nonfinite_and_out_of_domain_results() {
    let base = MatchFeeInput {
        amount_buy: d("1"),
        amount_sell: d("1"),
        buy_maker: MakerFlag::False,
        sell_maker: MakerFlag::True,
        inverse: false,
        last_mark: d("1"),
        notional_value: d("0"),
        channel: false,
    };
    assert_eq!(calculate_match_fees(&base), Err(TallyError::Arithmetic));
    let huge = MatchFeeInput {
        notional_value: d("1"),
        amount_buy: d("1e100"),
        ..base
    };
    assert_eq!(calculate_match_fees(&huge), Err(TallyError::AmountOverflow));
    assert_eq!(
        source_calculate_total(&LegacyBuckets {
            available: d("1e1000"),
            ..Default::default()
        }),
        Err(TallyError::NonFinite)
    );
    assert_eq!(
        source_fee_conversion(&d("0.000000009")).unwrap(),
        FeeConversion {
            atoms: 0,
            fractional_sats: d("0.9"),
            source_discards_all: true
        }
    );
    assert_eq!(
        source_fee_conversion(&d("0.000000015")).unwrap(),
        FeeConversion {
            atoms: 1,
            fractional_sats: d("0.5"),
            source_discards_all: false
        }
    );
}

#[test]
fn journals_can_be_drained_without_changing_custody_metadata() {
    let mut book = FeeBook::new();
    book.accrue_atoms(p(1), FeeRoute::Spot, 9, 2).unwrap();
    let custody = book.expected_custody().unwrap();
    let journal = book.take_journal();
    assert_eq!(journal.len(), 1);
    assert!(book.journal().is_empty());
    assert_eq!(book.expected_custody().unwrap(), custody);
    assert_eq!(FeeBook::from_snapshot(book.snapshot()).unwrap(), book);
}
