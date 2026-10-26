use core::str::FromStr;
use tradelayer_core::decimal::Decimal;
use tradelayer_core::ledger::{AccountId, PropertyId, TxId};
use tradelayer_core::trade_history::{
    source_break_up_opening_indices, source_weighted_mean, ContractDirection, ContractEventOrigin,
    ContractExecution, ExecutionPnl,
};
use tradelayer_core::trade_history::{
    AddressMatch, InsertOutcome, TradeDetails, TradeHistory, TradeHistoryError, TradeId,
    TradeRecord, MAX_HISTORY_RECORDS,
};

fn decimal(value: &str) -> Decimal {
    Decimal::from_str(value).unwrap()
}
fn execution(
    id: u8,
    height: u64,
    index: u32,
    buyer: &str,
    seller: &str,
    quantity: &str,
    price: &str,
) -> ContractExecution {
    ContractExecution {
        id: TradeId([id; 32]),
        block_height: height,
        event_index: index,
        contract_id: 9,
        buyer: account(buyer),
        seller: account(seller),
        quantity: decimal(quantity),
        price: decimal(price),
        origin: ContractEventOrigin::Orders {
            buy_txid: TxId::from_bytes([id.wrapping_add(1); 32]),
            sell_txid: TxId::from_bytes([id.wrapping_add(2); 32]),
        },
        buyer_pnl: ExecutionPnl::default(),
        seller_pnl: ExecutionPnl::default(),
        retained_fee_atoms: 0,
        collected_loss_atoms: 0,
        paid_claim_atoms: 0,
        unfunded_loss_atoms: 0,
    }
}
fn lifecycle() -> Vec<ContractExecution> {
    vec![
        execution(1, 1, 0, "alice", "bob", "2", "100"),
        execution(2, 2, 0, "alice", "bob", "1", "130"),
        execution(3, 2, 1, "bob", "alice", "0.5", "140"),
        execution(4, 3, 0, "bob", "alice", "3", "150"),
        execution(5, 4, 0, "alice", "bob", "0.25", "140"),
        execution(6, 4, 1, "alice", "bob", "0.25", "145"),
        execution(7, 5, 0, "alice", "bob", "0.1", "160"),
    ]
}

#[test]
fn exact_fractional_position_add_reduce_flip_close_reopen_and_categories() {
    let mut history = TradeHistory::new();
    let mut rows = lifecycle();
    rows.reverse();
    assert_eq!(history.insert_execution_batch(&rows).unwrap(), 7);
    let snapshots = history.position_history(&account("alice"), 9).unwrap();
    let expected = [
        ("2", Some("100")),
        ("3", Some("110")),
        ("2.5", Some("110")),
        ("-0.5", Some("150")),
        ("-0.25", Some("150")),
        ("0", None),
        ("0.1", Some("160")),
    ];
    for (snapshot, (quantity, average)) in snapshots.iter().zip(expected) {
        assert_eq!(snapshot.effect.after, decimal(quantity));
        assert_eq!(snapshot.average_entry_price, average.map(decimal));
    }
    assert_eq!(snapshots[3].effect.closed, decimal("2.5"));
    assert_eq!(snapshots[3].effect.opened, decimal("0.5"));
    assert_eq!(snapshots[3].effect.flip, decimal("0.5"));
    assert!(!snapshots[3].effect.fully_closed);
    assert!(snapshots[5].effect.fully_closed);
    assert_eq!(snapshots[6].effect.before, Decimal::zero());
    let categories = history.categorized_trades(&account("alice"), 9).unwrap();
    assert_eq!(
        categories
            .open_trades
            .iter()
            .map(|row| row.amount.to_string())
            .collect::<Vec<_>>(),
        ["2", "1", "-0.5", "0.1"]
    );
    assert_eq!(
        categories
            .close_trades
            .iter()
            .map(|row| row.amount.to_string())
            .collect::<Vec<_>>(),
        ["-0.5", "-2.5", "0.25", "0.25"]
    );
    assert_eq!(categories.open_trades[2].amount, decimal("-0.5"));
    assert_eq!(categories.close_trades[1].amount, decimal("-2.5"));
    let episodes = history.position_episodes(&account("alice"), 9).unwrap();
    assert_eq!(episodes.len(), 3);
    assert_eq!(episodes[0].direction, ContractDirection::Long);
    assert_eq!(episodes[0].remaining, Decimal::zero());
    assert_eq!(episodes[1].direction, ContractDirection::Short);
    assert_eq!(episodes[1].remaining, Decimal::zero());
    assert_eq!(episodes[2].direction, ContractDirection::Long);
    assert_eq!(episodes[2].remaining, decimal("0.1"));
    assert_eq!(
        history
            .position_history(&account("bob"), 9)
            .unwrap()
            .last()
            .unwrap()
            .effect
            .after,
        decimal("-0.1")
    );
}

#[test]
fn lifo_consumes_prior_closes_and_never_uses_future_lots() {
    let mut history = TradeHistory::new();
    history.insert_execution_batch(&lifecycle()).unwrap();
    let owner = account("alice");
    let before_flip = history
        .lifo_entry(&owner, 9, &decimal("1.5"), Some((2, 1)))
        .unwrap();
    assert_eq!(before_flip.covered, decimal("1.5"));
    assert_eq!(before_flip.uncovered, Decimal::zero());
    assert_eq!(before_flip.total_cost, decimal("165"));
    assert_eq!(
        before_flip
            .lots
            .iter()
            .map(|lot| lot.trade_id.0[0])
            .collect::<Vec<_>>(),
        [2, 1]
    );
    assert_eq!(before_flip.lots[0].quantity, decimal("0.5"));
    let after_flip = history
        .lifo_entry(&owner, 9, &decimal("-1"), Some((3, 0)))
        .unwrap();
    assert_eq!(after_flip.covered, decimal("0.5"));
    assert_eq!(after_flip.uncovered, decimal("0.5"));
    assert_eq!(after_flip.total_cost, decimal("75"));
    assert_eq!(after_flip.lots[0].trade_id, TradeId([4; 32]));
    let flat = history
        .lifo_entry(&owner, 9, &decimal("1"), Some((4, 1)))
        .unwrap();
    assert_eq!(flat.covered, Decimal::zero());
    assert_eq!(flat.uncovered, decimal("1"));
    let current = history.lifo_entry(&owner, 9, &decimal("1"), None).unwrap();
    assert_eq!(current.total_cost, decimal("16"));
    assert_eq!(current.covered, decimal("0.1"));
    assert!(history
        .lifo_entry(&owner, 9, &Decimal::zero(), None)
        .unwrap()
        .lots
        .is_empty());
}

#[test]
fn execution_origins_preserve_real_references_and_private_pool_identity() {
    let mut book = execution(1, 1, 0, "alice", "bob", "0.1", "100");
    book.origin = ContractEventOrigin::ForcedBook {
        epoch: TxId::from_bytes([9; 32]),
        fill_index: 2,
        maker_txid: TxId::from_bytes([8; 32]),
    };
    let mut amm = execution(2, 1, 3, "~econ:pool", "alice", "0.05", "101");
    amm.origin = ContractEventOrigin::ForcedAmm {
        epoch: TxId::from_bytes([9; 32]),
        fill_index: 0,
        pool_id: 1,
    };
    let ordinary = execution(3, 2, 0, "alice", "bob", "1", "102");
    let mut history = TradeHistory::new();
    history
        .insert_execution_batch(&[book.clone(), amm.clone(), ordinary.clone()])
        .unwrap();
    let refs = history
        .transaction_references(&account("alice"), 9)
        .unwrap();
    assert_eq!(refs[0].origin, book.origin);
    assert_eq!(refs[1].origin, amm.origin);
    assert_eq!(refs[0].participant_order_txid, None);
    assert_eq!(refs[1].participant_order_txid, None);
    assert_eq!(
        refs[2].participant_order_txid,
        Some(TxId::from_bytes([4; 32]))
    );
    assert_eq!(history.liquidation_executions_at_block(9, 1).len(), 2);
    assert_eq!(
        history
            .executions_for_address(
                9,
                &account("ALICE"),
                AddressMatch::LegacyOriginalOrLowercase
            )
            .len(),
        3
    );
    assert!(history
        .executions_for_address(9, &account("ALICE"), AddressMatch::Exact)
        .is_empty());
    assert_eq!(history.first_trade_block(9), Some(1));
    assert_eq!(history.executions_between_blocks(9, 1, 1).unwrap().len(), 2);
    assert_eq!(
        history.executions_between_blocks(9, 2, 1),
        Err(TradeHistoryError::ReversedBlockRange)
    );
}

#[test]
fn booked_pnl_offsets_collections_and_event_wide_payouts_are_distinct() {
    let mut event = execution(1, 1, 0, "alice", "bob", "1", "100");
    event.buyer_pnl = ExecutionPnl {
        booked: decimal("10"),
        loss_offset_atoms: 200_000_000,
        ..ExecutionPnl::default()
    };
    event.seller_pnl = ExecutionPnl {
        booked: decimal("-10"),
        collected_atoms: 300_000_000,
        claim_offset_atoms: 100_000_000,
        loss_offset_atoms: 0,
    };
    event.collected_loss_atoms = 300_000_000;
    event.unfunded_loss_atoms = 600_000_000;
    event.paid_claim_atoms = 700_000_000;
    let mut second = event.clone();
    second.id = TradeId([2; 32]);
    second.event_index = 1;
    let mut history = TradeHistory::new();
    history.insert_execution_batch(&[event, second]).unwrap();
    let alice = history.pnl_summary(&account("alice"), 9).unwrap();
    assert_eq!(alice.booked, decimal("20"));
    assert_eq!(alice.collected_atoms, 0);
    assert_eq!(alice.loss_offset_atoms, 400_000_000);
    let bob = history.pnl_summary(&account("bob"), 9).unwrap();
    assert_eq!(bob.booked, decimal("-20"));
    assert_eq!(bob.collected_atoms, 600_000_000);
    assert_eq!(bob.claim_offset_atoms, 200_000_000);
    let rows = history.pnl_history(&account("alice"), 9);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].event_paid_claim_atoms, 700_000_000);
    assert_eq!(history.pnl_history(&account("third-party"), 9).len(), 0);
}

#[test]
fn exact_batches_conflicts_and_last_invalid_leg_are_atomic() {
    let mut history = TradeHistory::new();
    history.insert(contract(9, 1, 0)).unwrap();
    let before = history.clone();
    let mut conflicting = execution(1, 1, 0, "alice", "bob", "1", "100");
    assert!(matches!(
        history.insert_execution(conflicting.clone()),
        Err(TradeHistoryError::ConflictingPosition { .. })
    ));
    assert_eq!(history, before);
    conflicting.block_height = 2;
    conflicting.id = TradeId([9; 32]);
    assert_eq!(
        history.insert_execution(conflicting),
        Err(TradeHistoryError::ConflictingId(TradeId([9; 32])))
    );
    assert_eq!(history, before);
    let good = execution(1, 2, 0, "alice", "bob", "0.5", "100");
    let mut bad = execution(2, 3, 0, "alice", "bob", "0.5", "100");
    bad.buyer_pnl.booked = decimal("-1");
    assert!(history
        .insert_execution_batch(&[good.clone(), bad])
        .is_err());
    assert_eq!(history, before);
    assert_eq!(
        history.insert_execution(good.clone()).unwrap(),
        InsertOutcome::Inserted
    );
    assert_eq!(
        history.insert_execution(good).unwrap(),
        InsertOutcome::AlreadyPresent
    );
    let before = history.clone();
    let legacy = contract(1, 3, 0);
    assert_eq!(
        history.insert(legacy),
        Err(TradeHistoryError::ConflictingId(TradeId([1; 32])))
    );
    assert_eq!(history, before);
    for mut invalid in [
        execution(3, 3, 0, "alice", "alice", "1", "100"),
        execution(4, 3, 0, "alice", "bob", "0", "100"),
        execution(5, 3, 0, "alice", "bob", "1", "-1"),
    ] {
        assert!(history.insert_execution(invalid.clone()).is_err());
        assert_eq!(history, before);
        invalid.quantity = decimal("1e-21");
        assert!(history.insert_execution(invalid).is_err());
        assert_eq!(history, before);
    }
}

#[test]
fn exact_checkpoint_replays_positions_preserves_v1_and_canonical_version_selection() {
    let mut legacy = TradeHistory::new();
    legacy.insert(contract(90, 1, 0)).unwrap();
    let legacy_bytes = legacy.encode_checkpoint().unwrap();
    assert_eq!(&legacy_bytes[..5], b"TLTH\x01");
    let mut a = legacy.clone();
    let mut rows = lifecycle();
    for row in &mut rows {
        row.block_height += 2;
    }
    a.insert_execution_batch(&rows).unwrap();
    let mut b = TradeHistory::new();
    rows.reverse();
    b.insert_execution_batch(&rows).unwrap();
    b.insert(contract(90, 1, 0)).unwrap();
    let bytes = a.encode_checkpoint().unwrap();
    assert_eq!(a.entries().len(), 8);
    assert_eq!(a.contract_entries(9).len(), 8);
    assert_eq!(a.contract_entries_between_blocks(9, 1, 1).unwrap().len(), 1);
    assert_eq!(
        a.contract_entries_for_address(9, &account("alice"), AddressMatch::Exact)
            .len(),
        7
    );
    assert_eq!(&bytes[..5], b"TLTH\x02");
    assert_eq!(bytes, b.encode_checkpoint().unwrap());
    let restored = TradeHistory::decode_checkpoint(&bytes).unwrap();
    assert_eq!(restored, a);
    assert_eq!(
        restored.position_history(&account("alice"), 9).unwrap(),
        a.position_history(&account("alice"), 9).unwrap()
    );
    assert_eq!(a.rollback_after(1), 7);
    assert_eq!(a.encode_checkpoint().unwrap(), legacy_bytes);
    assert_eq!(a, legacy);
    a.insert_execution(execution(1, 2, 0, "alice", "bob", "0.1", "99"))
        .unwrap();
    assert_eq!(a.execution(TradeId([1; 32])).unwrap().price, decimal("99"));
}

#[test]
fn exact_checkpoint_rejects_every_prefix_and_repaired_crc_forged_derived_positions() {
    let mut history = TradeHistory::new();
    history
        .insert_execution(execution(1, 1, 0, "alice", "bob", "1.25", "100"))
        .unwrap();
    let bytes = history.encode_checkpoint().unwrap();
    for end in 0..bytes.len() {
        assert!(
            TradeHistory::decode_checkpoint(&bytes[..end]).is_err(),
            "prefix {end}"
        );
    }
    let original = bytes[..bytes.len() - 4].to_vec();
    let mut forged = original.clone();
    let offset = forged
        .windows(5)
        .rposition(|part| part == b"-1.25")
        .unwrap();
    forged[offset + 4] = b'6';
    assert_eq!(
        TradeHistory::decode_checkpoint(&with_crc(forged)),
        Err(TradeHistoryError::InvalidCheckpoint("replayed positions"))
    );
    let mut noncanonical = original.clone();
    let offset = noncanonical
        .windows(4)
        .position(|part| part == b"1.25")
        .unwrap();
    noncanonical[offset + 3] = b'0';
    assert_eq!(
        TradeHistory::decode_checkpoint(&with_crc(noncanonical)),
        Err(TradeHistoryError::InvalidCheckpoint("noncanonical decimal"))
    );
    let mut bad_origin = original.clone();
    bad_origin[99] = 255;
    assert!(TradeHistory::decode_checkpoint(&with_crc(bad_origin)).is_err());
    let mut bad_count = original.clone();
    bad_count[22..26].copy_from_slice(&0u32.to_be_bytes());
    assert!(TradeHistory::decode_checkpoint(&with_crc(bad_count)).is_err());
    let mut excessive = original.clone();
    excessive[22..26].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(TradeHistory::decode_checkpoint(&with_crc(excessive)).is_err());
    let mut trailing = original;
    trailing.push(0);
    assert!(TradeHistory::decode_checkpoint(&with_crc(trailing)).is_err());
}

#[test]
fn genuine_source_weighted_mean_and_breakup_corpus_match_exact_helper_paths() {
    let mut averages = 0;
    let mut groups = 0;
    let mut methods = std::collections::BTreeSet::new();
    for line in include_str!("../fixtures/trade_history_method_vectors.tsv")
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
    {
        let cells: Vec<_> = line.split('\t').collect();
        match cells[0] {
            "A" => {
                assert_eq!(cells.len(), 4);
                let entries = if cells[2] == "_" {
                    Vec::new()
                } else {
                    cells[2]
                        .split(';')
                        .map(|entry| {
                            let (price, amount) = entry.split_once(',').unwrap();
                            (decimal(price), decimal(amount))
                        })
                        .collect()
                };
                assert_eq!(
                    source_weighted_mean(&entries).unwrap().to_bits(),
                    u64::from_str_radix(cells[3], 16).unwrap(),
                    "{}",
                    cells[1]
                );
                averages += 1;
            }
            "B" => {
                assert_eq!(cells.len(), 5);
                let heights = |text: &str| {
                    if text == "_" {
                        Vec::new()
                    } else {
                        text.split(',')
                            .map(|height| {
                                if height == "-" {
                                    None
                                } else {
                                    Some(height.parse().unwrap())
                                }
                            })
                            .collect()
                    }
                };
                let expected: Vec<Vec<usize>> = cells[4]
                    .split(';')
                    .map(|group| {
                        if group == "_" {
                            Vec::new()
                        } else {
                            group
                                .split(',')
                                .map(|index| index.parse().unwrap())
                                .collect()
                        }
                    })
                    .collect();
                assert_eq!(
                    source_break_up_opening_indices(&heights(cells[2]), &heights(cells[3]))
                        .unwrap(),
                    expected,
                    "{}",
                    cells[1]
                );
                groups += 1;
            }
            "M" => {
                assert_eq!(cells.len(), 7);
                methods.insert(cells[2]);
            }
            other => panic!("unknown source row {other}"),
        }
    }
    assert_eq!((averages, groups, methods.len()), (102, 103, 18));
    assert_eq!(
        source_weighted_mean(&[
            (decimal("-1e-20"), decimal("1")),
            (Decimal::zero(), decimal("100"))
        ])
        .unwrap()
        .to_bits(),
        (-0.0_f64).to_bits()
    );
    assert!(source_weighted_mean(&[(decimal("1e41"), decimal("1"))]).is_err());
    assert!(source_break_up_opening_indices(&[Some(u64::MAX)], &[]).is_err());
    assert_eq!(
        source_break_up_opening_indices(&[], &[]).unwrap(),
        vec![Vec::<usize>::new()]
    );
}

fn account(value: &str) -> AccountId {
    AccountId::try_from(value).unwrap()
}

fn property(value: u32) -> PropertyId {
    PropertyId::try_from(value).unwrap()
}

fn contract(id: u8, height: u64, index: u32) -> TradeRecord {
    TradeRecord {
        id: TradeId([id; 32]),
        block_height: height,
        event_index: index,
        buyer: account("a"),
        seller: account("b"),
        buyer_tx: TxId::from_bytes([id.wrapping_add(1); 32]),
        seller_tx: TxId::from_bytes([id.wrapping_add(2); 32]),
        details: TradeDetails::Contract {
            contract_id: 9,
            amount_contracts: u64::MAX,
            price_numerator: u128::MAX,
            price_denominator: 3,
            liquidation: false,
        },
    }
}

fn token(id: u8, height: u64, index: u32) -> TradeRecord {
    let mut record = contract(id, height, index);
    record.details = TradeDetails::Token {
        property1: property(5),
        property2: property(6),
        amount1_atoms: u128::MAX,
        amount2_atoms: 1,
    };
    record
}

fn ids(rows: Vec<&TradeRecord>) -> String {
    let result = rows
        .iter()
        .map(|r| r.id.0[0].to_string())
        .collect::<Vec<_>>()
        .join(",");
    if result.is_empty() {
        "-".to_string()
    } else {
        result
    }
}

#[test]
fn pinned_js_query_vectors_match_typed_store() {
    let fixture = include_str!("../fixtures/trade_history_vectors.tsv");
    let mut records = Vec::new();
    for line in fixture.lines().filter(|line| line.starts_with("R\t")) {
        let c: Vec<_> = line.split('\t').collect();
        assert_eq!(c.len(), 10);
        let mut row = contract(
            c[1].parse().unwrap(),
            c[5].parse().unwrap(),
            c[6].parse().unwrap(),
        );
        row.buyer = account(c[7]);
        row.seller = account(c[8]);
        row.details = if c[2] == "token" {
            TradeDetails::Token {
                property1: property(c[3].parse().unwrap()),
                property2: property(c[4].parse().unwrap()),
                amount1_atoms: 11,
                amount2_atoms: 13,
            }
        } else {
            TradeDetails::Contract {
                contract_id: c[3].parse().unwrap(),
                amount_contracts: 2,
                price_numerator: 13,
                price_denominator: 11,
                liquidation: c[9].parse().unwrap(),
            }
        };
        records.push(row);
    }
    // Deliberately reverse source insertion order: Rust canonicalizes query
    // output; the JS fixture DB was supplied in that canonical order.
    records.reverse();
    let mut history = TradeHistory::new();
    assert_eq!(history.insert_batch(&records).unwrap(), 10);
    let mut queries = 0;
    for line in fixture.lines().filter(|line| line.starts_with("Q\t")) {
        let c: Vec<_> = line.split('\t').collect();
        assert_eq!(c.len(), 7);
        let a = c[2].parse().unwrap();
        let mode = if c[5] == "exact" {
            AddressMatch::Exact
        } else {
            AddressMatch::LegacyOriginalOrLowercase
        };
        let result = match c[1] {
            "token" => ids(history.token_trades_for_address(
                property(a),
                property(c[3].parse().unwrap()),
                &account(c[4]),
                mode,
            )),
            "contract" => ids(history.contract_trades_for_address(a, &account(c[4]), mode)),
            "all" => ids(history.contract_trades(a)),
            "range" => ids(history
                .contract_trades_between_blocks(a, c[3].parse().unwrap(), c[4].parse().unwrap())
                .unwrap()),
            "liquidation" => ids(history.liquidation_trades_at_block(a, c[3].parse().unwrap())),
            "first" => history
                .first_trade_block(a)
                .map_or_else(|| "-".to_string(), |h| h.to_string()),
            kind => panic!("unknown fixture query: {kind}"),
        };
        assert_eq!(result, c[6], "{line}");
        queries += 1;
    }
    assert_eq!(queries, 25);
    assert_eq!(
        history,
        TradeHistory::decode_checkpoint(&history.encode_checkpoint().unwrap()).unwrap()
    );
}

#[test]
fn idempotent_duplicates_and_conflicting_ids_or_positions_never_mutate() {
    let row = contract(1, 100, 7);
    let mut history = TradeHistory::new();
    assert_eq!(history.insert(row.clone()), Ok(InsertOutcome::Inserted));
    let before = history.clone();
    assert_eq!(
        history.insert(row.clone()),
        Ok(InsertOutcome::AlreadyPresent)
    );
    let mut changed = row.clone();
    changed.block_height += 1;
    assert_eq!(
        history.insert(changed),
        Err(TradeHistoryError::ConflictingId(row.id))
    );
    let mut changed = row.clone();
    changed.id = TradeId([2; 32]);
    assert_eq!(
        history.insert(changed),
        Err(TradeHistoryError::ConflictingPosition {
            block_height: 100,
            event_index: 7
        })
    );
    let mut changed = row.clone();
    changed.seller_tx = TxId::from_bytes([99; 32]);
    assert_eq!(
        history.insert(changed),
        Err(TradeHistoryError::ConflictingId(row.id))
    );
    assert_eq!(history, before);
    assert_eq!(history.get(row.id), Some(&row));
}

#[test]
fn batch_is_atomic_for_validation_and_conflicts_with_prior_or_batch_rows() {
    let mut history = TradeHistory::new();
    history.insert(contract(1, 1, 0)).unwrap();
    let before = history.clone();
    let mut invalid = token(3, 2, 1);
    if let TradeDetails::Token { amount1_atoms, .. } = &mut invalid.details {
        *amount1_atoms = 0;
    }
    for batch in [
        vec![contract(2, 2, 0), invalid],
        vec![contract(2, 2, 0), contract(2, 2, 1)],
        vec![contract(2, 2, 0), contract(1, 3, 0)],
        vec![contract(2, 2, 0), contract(3, 2, 0)],
    ] {
        assert!(history.insert_batch(&batch).is_err());
        assert_eq!(history, before);
    }
    assert_eq!(
        history.insert_batch(&[contract(2, 2, 0), contract(2, 2, 0)]),
        Ok(1)
    );
    assert_eq!(history.len(), 2);
}

#[test]
fn economics_validation_preserves_full_integer_precision() {
    let mut history = TradeHistory::new();
    history.insert(token(1, 0, 0)).unwrap();
    history.insert(contract(2, u64::MAX, u32::MAX)).unwrap();
    let before = history.clone();
    let bad_details = [
        TradeDetails::Token {
            property1: property(5),
            property2: property(5),
            amount1_atoms: 1,
            amount2_atoms: 1,
        },
        TradeDetails::Token {
            property1: property(5),
            property2: property(6),
            amount1_atoms: 1,
            amount2_atoms: 0,
        },
        TradeDetails::Contract {
            contract_id: 0,
            amount_contracts: 1,
            price_numerator: 1,
            price_denominator: 1,
            liquidation: false,
        },
        TradeDetails::Contract {
            contract_id: 9,
            amount_contracts: 0,
            price_numerator: 1,
            price_denominator: 1,
            liquidation: false,
        },
        TradeDetails::Contract {
            contract_id: 9,
            amount_contracts: 1,
            price_numerator: 0,
            price_denominator: 1,
            liquidation: false,
        },
        TradeDetails::Contract {
            contract_id: 9,
            amount_contracts: 1,
            price_numerator: 1,
            price_denominator: 0,
            liquidation: false,
        },
    ];
    for details in bad_details {
        let mut invalid = contract(3, 2, 0);
        invalid.details = details;
        assert!(matches!(
            history.insert(invalid),
            Err(TradeHistoryError::InvalidRecord(_))
        ));
        assert_eq!(history, before);
    }
    let encoded = history.encode_checkpoint().unwrap();
    assert_eq!(TradeHistory::decode_checkpoint(&encoded), Ok(before));
}

#[test]
fn ranges_include_both_endpoints_and_extreme_heights() {
    let mut history = TradeHistory::new();
    history
        .insert_batch(&[contract(1, 0, 0), contract(2, u64::MAX, u32::MAX)])
        .unwrap();
    assert_eq!(
        ids(history
            .contract_trades_between_blocks(9, 0, u64::MAX)
            .unwrap()),
        "1,2"
    );
    assert_eq!(
        ids(history
            .contract_trades_between_blocks(9, u64::MAX, u64::MAX)
            .unwrap()),
        "2"
    );
    assert_eq!(
        history.contract_trades_between_blocks(9, 2, 1),
        Err(TradeHistoryError::ReversedBlockRange)
    );
    assert_eq!(history.first_trade_block(9), Some(0));
    assert_eq!(history.first_trade_block(99), None);
    assert_eq!(history.rollback_after(u64::MAX), 0);
}

#[test]
fn rollback_releases_ids_and_positions_and_preserves_retained_block() {
    let mut history = TradeHistory::new();
    let retained = contract(2, 10, 0);
    let orphan = contract(3, 11, 0);
    history
        .insert_batch(&[contract(1, 0, 0), retained.clone(), orphan.clone()])
        .unwrap();
    assert_eq!(history.rollback_after(10), 1);
    assert_eq!(history.get(retained.id), Some(&retained));
    assert_eq!(history.get(orphan.id), None);
    let mut replacement = orphan;
    replacement.details = token(3, 11, 0).details;
    assert_eq!(history.insert(replacement), Ok(InsertOutcome::Inserted));
    assert_eq!(history.rollback_after(0), 2);
    assert_eq!(history.len(), 1);
    assert_eq!(history.rollback_after(0), 0);
    assert_eq!(
        history,
        TradeHistory::decode_checkpoint(&history.encode_checkpoint().unwrap()).unwrap()
    );
}

#[test]
fn checkpoints_are_canonical_for_insertion_order_and_exact_rational_economics() {
    let rows = [contract(1, 3, 9), token(2, 2, 0), contract(3, 3, 0)];
    let mut forward = TradeHistory::new();
    forward.insert_batch(&rows).unwrap();
    let mut reverse = TradeHistory::new();
    reverse
        .insert_batch(&rows.into_iter().rev().collect::<Vec<_>>())
        .unwrap();
    assert_eq!(forward, reverse);
    assert_eq!(forward.encode_checkpoint(), reverse.encode_checkpoint());
    let empty = TradeHistory::new();
    assert!(empty.is_empty());
    assert_eq!(
        TradeHistory::decode_checkpoint(&empty.encode_checkpoint().unwrap()),
        Ok(empty)
    );
}

fn with_crc(mut payload: Vec<u8>) -> Vec<u8> {
    let mut crc = !0u32;
    for &byte in &payload {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    payload.extend_from_slice(&(!crc).to_be_bytes());
    payload
}

fn payload(row: TradeRecord) -> Vec<u8> {
    let mut history = TradeHistory::new();
    history.insert(row).unwrap();
    let mut bytes = history.encode_checkpoint().unwrap();
    bytes.truncate(bytes.len() - 4);
    bytes
}

#[test]
fn checkpoint_decoder_rejects_damage_truncation_trailing_data_and_excess_counts() {
    let original = payload(contract(1, 1, 0));
    for length in 0..original.len() {
        assert!(
            TradeHistory::decode_checkpoint(&with_crc(original[..length].to_vec())).is_err(),
            "prefix {length}"
        );
    }
    let mut damaged = with_crc(original.clone());
    damaged[20] ^= 1;
    assert_eq!(
        TradeHistory::decode_checkpoint(&damaged),
        Err(TradeHistoryError::InvalidCheckpoint("checksum"))
    );
    let mut trailing = original.clone();
    trailing.push(0);
    assert_eq!(
        TradeHistory::decode_checkpoint(&with_crc(trailing)),
        Err(TradeHistoryError::InvalidCheckpoint("trailing bytes"))
    );
    let mut excessive = original.clone();
    excessive[5..9].copy_from_slice(&((MAX_HISTORY_RECORDS + 1) as u32).to_be_bytes());
    assert_eq!(
        TradeHistory::decode_checkpoint(&with_crc(excessive)),
        Err(TradeHistoryError::CapacityExceeded)
    );
    let mut version = original;
    version[4] = 3;
    assert_eq!(
        TradeHistory::decode_checkpoint(&with_crc(version)),
        Err(TradeHistoryError::InvalidCheckpoint("magic/version"))
    );
}

#[test]
fn valid_checksum_does_not_authorize_invalid_fields_or_noncanonical_records() {
    // With one-byte addresses, byte 121 is the kind discriminator. These
    // offsets intentionally exercise the wire layout, independently of decode.
    let original = payload(contract(1, 1, 0));
    for (offset, value) in [(53, 0), (53, 91), (54, 0), (54, 255), (121, 2), (166, 2)] {
        let mut invalid = original.clone();
        invalid[offset] = value;
        assert!(
            TradeHistory::decode_checkpoint(&with_crc(invalid)).is_err(),
            "offset {offset}"
        );
    }
    for range in [122..126, 126..134, 134..150, 150..166] {
        let mut invalid = original.clone();
        invalid[range].fill(0);
        assert!(TradeHistory::decode_checkpoint(&with_crc(invalid)).is_err());
    }
    let mut bad_token = payload(token(2, 2, 0));
    bad_token[122..126].fill(0);
    assert!(TradeHistory::decode_checkpoint(&with_crc(bad_token)).is_err());

    let second = payload(contract(2, 2, 0));
    let duplicate_id = payload(contract(1, 2, 0));
    let mut header = original[..9].to_vec();
    header[5..9].copy_from_slice(&2u32.to_be_bytes());
    let assemble = |first: &[u8], last: &[u8]| {
        let mut bytes = header.clone();
        bytes.extend_from_slice(&first[9..]);
        bytes.extend_from_slice(&last[9..]);
        with_crc(bytes)
    };
    assert_eq!(
        TradeHistory::decode_checkpoint(&assemble(&second, &original)),
        Err(TradeHistoryError::InvalidCheckpoint(
            "noncanonical record order"
        ))
    );
    assert_eq!(
        TradeHistory::decode_checkpoint(&assemble(&original, &original)),
        Err(TradeHistoryError::InvalidCheckpoint(
            "noncanonical record order"
        ))
    );
    assert_eq!(
        TradeHistory::decode_checkpoint(&assemble(&original, &duplicate_id)),
        Err(TradeHistoryError::ConflictingId(TradeId([1; 32])))
    );
}

#[test]
fn record_capacity_duplicate_at_capacity_and_rollback_recovery() {
    let mut history = TradeHistory::new();
    let template = contract(0, 1, 0);
    for index in 0..MAX_HISTORY_RECORDS {
        let mut row = template.clone();
        row.id.0[..4].copy_from_slice(&(index as u32).to_be_bytes());
        row.event_index = index as u32;
        history.insert(row).unwrap();
    }
    assert_eq!(history.len(), MAX_HISTORY_RECORDS);
    let exact_extra = execution(254, 2, 1, "alice", "bob", "0.5", "100");
    assert_eq!(
        history.insert_execution(exact_extra.clone()),
        Err(TradeHistoryError::CapacityExceeded)
    );
    assert_eq!(history.len(), MAX_HISTORY_RECORDS);
    assert_eq!(history.execution(exact_extra.id), None);
    assert_eq!(history.insert(template), Ok(InsertOutcome::AlreadyPresent));
    let mut extra = contract(255, 2, 0);
    extra.id.0[..4].copy_from_slice(&(MAX_HISTORY_RECORDS as u32).to_be_bytes());
    assert_eq!(
        history.insert(extra.clone()),
        Err(TradeHistoryError::CapacityExceeded)
    );
    assert_eq!(history.rollback_after(0), MAX_HISTORY_RECORDS);
    assert!(history.is_empty());
    assert_eq!(history.insert(extra), Ok(InsertOutcome::Inserted));
    assert_eq!(
        history.insert_execution(exact_extra),
        Ok(InsertOutcome::Inserted)
    );
}
