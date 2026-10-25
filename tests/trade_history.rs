use tradelayer_core::ledger::{AccountId, PropertyId, TxId};
use tradelayer_core::trade_history::{
    AddressMatch, InsertOutcome, TradeDetails, TradeHistory, TradeHistoryError, TradeId,
    TradeRecord, MAX_HISTORY_RECORDS,
};

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
    version[4] = 2;
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
}
