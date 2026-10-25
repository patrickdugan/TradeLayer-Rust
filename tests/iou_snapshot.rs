use tradelayer_core::decimal::Decimal;
use tradelayer_core::iou::{Claim, IouKey, IouStore, MAX_IOU_CLAIMS, MAX_IOU_RECORDS};
use tradelayer_core::iou_ledger::{
    FundedIouError, FundedIouLedger, IouInput, MatchProfit, PayoutAtomPolicy, MAX_FUNDED_IOU_INPUTS,
};
use tradelayer_core::ledger::{
    AccountId, ChannelSide, Ledger, Operation, PropertyId, PropertyKind, TxId,
};
use tradelayer_core::network_profile::TradeLayerMainnet;

fn a(value: &str) -> AccountId {
    AccountId::try_from(value).unwrap()
}
fn d(value: &str) -> Decimal {
    value.parse().unwrap()
}
fn p() -> PropertyId {
    PropertyId::try_from(1).unwrap()
}
fn key() -> IouKey {
    IouKey::new(42, 1)
}
fn id(n: u8) -> TxId {
    TxId::from_bytes([n; 32])
}
fn input(n: u8, height: u64) -> IouInput {
    IouInput {
        network: TradeLayerMainnet::Bitcoin,
        txid: id(n),
        height,
    }
}
fn ledger() -> Ledger {
    let mut ledger = Ledger::new();
    ledger
        .apply(
            id(250),
            Operation::CreateProperty {
                id: p(),
                issuer: a("payer"),
                kind: PropertyKind::Fixed,
                initial_atoms: 1_000_000_000,
            },
        )
        .unwrap();
    ledger
}
fn claims(rows: &[(&str, &str)]) -> IouStore {
    let mut store = IouStore::new();
    store
        .save_claim_map(
            key(),
            0,
            rows.iter()
                .map(|&(address, amount)| Claim {
                    address: a(address),
                    amount: d(amount),
                })
                .collect(),
        )
        .unwrap();
    store
}
fn funded(policy: PayoutAtomPolicy, store: IouStore) -> FundedIouLedger {
    FundedIouLedger::from_ledger_unverified(TradeLayerMainnet::Bitcoin, policy, ledger(), store)
        .unwrap()
}
fn restore(bytes: &[u8], policy: PayoutAtomPolicy) -> FundedIouLedger {
    FundedIouLedger::from_snapshot_bytes(bytes, TradeLayerMainnet::Bitcoin, policy).unwrap()
}
fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0_u32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & (0_u32.wrapping_sub(crc & 1)));
        }
    }
    !crc
}
fn fix_crc(bytes: &mut [u8]) {
    let at = bytes.len() - 4;
    let crc = crc32(&bytes[..at]);
    bytes[at..].copy_from_slice(&crc.to_be_bytes());
}
fn take_u32(bytes: &[u8], at: &mut usize) -> usize {
    let n = u32::from_be_bytes(bytes[*at..*at + 4].try_into().unwrap()) as usize;
    *at += 4;
    n
}
fn skip_decimal(bytes: &[u8], at: &mut usize) {
    let len = take_u32(bytes, at);
    *at += len;
}
fn iou_map_offsets(bytes: &[u8]) -> (usize, usize, Vec<std::ops::Range<usize>>) {
    let mut at = 5;
    let buckets = take_u32(bytes, &mut at);
    for _ in 0..buckets {
        at += 16;
        for _ in 0..5 {
            skip_decimal(bytes, &mut at);
        }
    }
    let maps_at = at;
    let maps = take_u32(bytes, &mut at);
    let total_at = at;
    at += 4;
    let mut ranges = Vec::new();
    for _ in 0..maps {
        let start = at;
        at += 16;
        let count = take_u32(bytes, &mut at);
        for _ in 0..count {
            let len = bytes[at] as usize;
            at += 1 + len;
            skip_decimal(bytes, &mut at);
        }
        ranges.push(start..at);
    }
    (maps_at, total_at, ranges)
}
fn funded_offsets(bytes: &[u8]) -> (usize, usize, usize, usize) {
    let mut at = 40;
    if bytes[39] == 1 {
        at += 8;
    }
    let ledger_len = take_u32(bytes, &mut at);
    let ledger_at = at;
    at += ledger_len;
    let iou_len = take_u32(bytes, &mut at);
    let iou_at = at;
    at += iou_len;
    let escrows_at = at;
    let count = take_u32(bytes, &mut at);
    for _ in 0..count {
        at += 8;
        let len = bytes[at] as usize;
        at += 1 + len;
    }
    (ledger_at, iou_at, escrows_at, at)
}

#[test]
fn source_checkpoint_preserves_signed_zero_records_counters_and_claim_order() {
    let mut store = claims(&[
        ("zeta", "-1.0000000000000000000001"),
        ("10", "0"),
        ("02", "-2"),
        ("2", "3"),
        ("alpha", "4"),
    ]);
    store.add_profit(key(), &d("7"), 2).unwrap();
    store.apply_to_losers(key(), &d("2"), 3).unwrap();
    store
        .add_loss(IouKey::new(43, 1), &d("0.0000000000000000000001"), 4)
        .unwrap();
    store.save_claim_map(IouKey::new(44, 1), 5, vec![]).unwrap();
    let bytes = store.snapshot_bytes().unwrap();
    let decoded = IouStore::from_snapshot_bytes(&bytes).unwrap();
    assert_eq!(decoded, store);
    assert_eq!(decoded.snapshot_bytes().unwrap(), bytes);
    assert_eq!(
        decoded
            .claim_map(key())
            .unwrap()
            .claims()
            .iter()
            .map(|row| row.address.as_str())
            .collect::<Vec<_>>(),
        vec!["2", "10", "zeta", "02", "alpha"]
    );
    assert_eq!(decoded.bucket(key()).unwrap().amount, d("-9"));
    assert_eq!(decoded.bucket(key()).unwrap().block_start_amount, d("-7"));
    assert_eq!(decoded.bucket(key()).unwrap().block_delta, d("-2"));
    assert_eq!(decoded.bucket(key()).unwrap().block_profits, d("2"));
    assert!(decoded.bucket(IouKey::new(43, 1)).unwrap().amount.is_zero());
    assert_eq!(decoded.record_count(), 4);
    assert_eq!(decoded.claim_count(), 5);
}

#[test]
fn complete_funded_cycle_restarts_and_rejects_all_prior_outer_and_internal_ids() {
    let mut state = funded(PayoutAtomPolicy::Exact, IouStore::new());
    state
        .accrue_match_profit_unverified(
            input(1, 10),
            MatchProfit {
                key: key(),
                buyer: a("buyer"),
                seller: a("seller"),
                buyer_pnl: d("6"),
                seller_pnl: d("-2"),
            },
        )
        .unwrap();
    let before_funding = state.snapshot_bytes().unwrap();
    state = restore(&before_funding, PayoutAtomPolicy::Exact);
    state
        .collect_loss_unverified(input(2, 11), key(), a("payer"), 400_000_000)
        .unwrap();
    state = restore(&state.snapshot_bytes().unwrap(), PayoutAtomPolicy::Exact);
    let receipt = state
        .pay_unverified(input(3, 11), key(), 400_000_000)
        .unwrap();
    let done = state.snapshot_bytes().unwrap();
    let mut restarted = restore(&done, PayoutAtomPolicy::Exact);
    assert_eq!(state, restarted);
    assert_eq!(receipt.credited_atoms, 400_000_000);
    assert_eq!(restarted.ious().bucket(key()).unwrap().block_losses, d("4"));
    for n in [1, 2, 3, 250] {
        assert_eq!(
            restarted.collect_loss_unverified(input(n, 11), key(), a("payer"), 1),
            Err(FundedIouError::Replay(id(n)))
        );
    }
    let internal = *restarted
        .ledger()
        .input_txids_unverified()
        .iter()
        .find(|id| **id != crate::id(250))
        .unwrap();
    assert_eq!(
        restarted.collect_loss_unverified(
            IouInput {
                txid: internal,
                ..input(4, 11)
            },
            key(),
            a("payer"),
            1
        ),
        Err(FundedIouError::Replay(internal))
    );
    assert_eq!(restarted.snapshot_bytes().unwrap(), done);
    // Explicitly restoring the parent checkpoint restores its replay domain.
    // Reapplying the same branch yields the exact former state and receipt.
    let mut fork = restore(&before_funding, PayoutAtomPolicy::Exact);
    fork.collect_loss_unverified(input(2, 11), key(), a("payer"), 400_000_000)
        .unwrap();
    assert_eq!(
        fork.pay_unverified(input(3, 11), key(), 400_000_000)
            .unwrap(),
        receipt
    );
    assert_eq!(fork.snapshot_bytes().unwrap(), done);
}

#[test]
fn noop_outer_ids_height_and_rounding_receipts_survive_restart() {
    let mut state = funded(
        PayoutAtomPolicy::Down,
        claims(&[("alice", "1"), ("bob", "1"), ("carol", "1")]),
    );
    state
        .collect_loss_unverified(input(1, 1), key(), a("payer"), 100_000_000)
        .unwrap();
    let receipt = state
        .pay_unverified(input(2, 1), key(), 100_000_000)
        .unwrap();
    assert_eq!(receipt.credited_atoms, 99_999_999);
    assert_eq!(
        receipt.allocations[0].source_amount,
        d("0.33333333333333333333")
    );
    assert_eq!(
        receipt.allocations[0].atom_rounding_delta,
        d("-0.00000000333333333333")
    );
    let bytes = state.snapshot_bytes().unwrap();
    let mut restarted = restore(&bytes, PayoutAtomPolicy::Down);
    assert_eq!(state, restarted);
    assert_eq!(
        restarted
            .ledger()
            .available_atoms(p(), &restarted.escrow_for_audit(key()).unwrap()),
        1
    );
    let noop = restarted.pay_unverified(input(3, 2), key(), 1).unwrap();
    assert_eq!(noop.credited_atoms, 0);
    restarted = restore(&restarted.snapshot_bytes().unwrap(), PayoutAtomPolicy::Down);
    assert_eq!(restarted.processed_count(), 3);
    assert_eq!(restarted.last_height(), Some(2));
    assert_eq!(
        restarted.pay_unverified(input(3, 2), key(), 1),
        Err(FundedIouError::Replay(id(3)))
    );
    assert_eq!(
        restarted.pay_unverified(input(4, 1), key(), 1),
        Err(FundedIouError::HeightRegression)
    );
    // Receipts are operation results, not hidden state: checkpointing the
    // pre-payout state and replaying reproduces every rounding metadata field.
    let mut initial = funded(
        PayoutAtomPolicy::Down,
        claims(&[("alice", "1"), ("bob", "1"), ("carol", "1")]),
    );
    initial
        .collect_loss_unverified(input(1, 1), key(), a("payer"), 100_000_000)
        .unwrap();
    let mut replay = restore(&initial.snapshot_bytes().unwrap(), PayoutAtomPolicy::Down);
    assert_eq!(
        replay
            .pay_unverified(input(2, 1), key(), 100_000_000)
            .unwrap(),
        receipt
    );
}

#[test]
fn imported_channel_and_all_settlement_liabilities_are_retained() {
    let mut l = ledger();
    l.apply(
        id(240),
        Operation::Transfer {
            id: p(),
            from: a("payer"),
            to: a("bob"),
            amount_atoms: 100,
        },
    )
    .unwrap();
    for (n, owner, side) in [(241, "payer", ChannelSide::A), (242, "bob", ChannelSide::B)] {
        l.apply(
            id(n),
            Operation::ChannelCommit {
                id: p(),
                owner: a(owner),
                channel: a("channel"),
                side,
                amount_atoms: 100,
            },
        )
        .unwrap();
    }
    l.apply(
        id(243),
        Operation::ChannelWithdraw {
            id: p(),
            owner: a("payer"),
            channel: a("channel"),
            side: ChannelSide::A,
            amount_atoms: 10,
            unlock_height: 30,
        },
    )
    .unwrap();
    l.apply(
        id(244),
        Operation::ChannelNetClaim {
            id: p(),
            channel: a("channel"),
            payer_side: ChannelSide::A,
            amount_atoms: 5,
            trade_reference: id(230),
            height: 10,
            expiry_height: 20,
        },
    )
    .unwrap();
    let preview = l
        .preview_king_settlement_unverified(&a("channel"), p(), 10, 10, 11)
        .unwrap();
    l.apply(
        id(245),
        Operation::ChannelKingSettle {
            id: p(),
            channel: a("channel"),
            start_height: 10,
            end_height: 10,
            settled_at: 11,
            expected_root: preview.root,
            expected_count: preview.count,
            payer_side: preview.payer_side,
            net_atoms: preview.net_atoms,
        },
    )
    .unwrap();
    l.apply(
        id(246),
        Operation::ChannelNetClaim {
            id: p(),
            channel: a("channel"),
            payer_side: ChannelSide::B,
            amount_atoms: 3,
            trade_reference: id(231),
            height: 12,
            expiry_height: 25,
        },
    )
    .unwrap();
    assert_eq!(l.pending_withdrawal_count(), 1);
    assert_eq!(l.pending_net_settlement_count(), 1);
    assert_eq!(l.swept_net_settlement_count(), 1);
    assert_eq!(l.king_settlement_count(), 1);
    let state = FundedIouLedger::from_ledger_unverified(
        TradeLayerMainnet::Bitcoin,
        PayoutAtomPolicy::Exact,
        l.clone(),
        IouStore::new(),
    )
    .unwrap();
    let restarted = restore(&state.snapshot_bytes().unwrap(), PayoutAtomPolicy::Exact);
    assert_eq!(restarted.ledger(), &l);
    assert_eq!(restarted, state);
}

#[test]
fn crc_corruption_every_truncation_and_trailing_data_are_rejected() {
    let source = claims(&[("a", "-1"), ("b", "0")]);
    let source_bytes = source.snapshot_bytes().unwrap();
    for len in 0..source_bytes.len() {
        assert!(IouStore::from_snapshot_bytes(&source_bytes[..len]).is_err());
    }
    let mut state = funded(PayoutAtomPolicy::Exact, source);
    state
        .collect_loss_unverified(input(1, 1), key(), a("payer"), 1)
        .unwrap();
    let bytes = state.snapshot_bytes().unwrap();
    for len in 0..bytes.len() {
        assert!(FundedIouLedger::from_snapshot_bytes(
            &bytes[..len],
            TradeLayerMainnet::Bitcoin,
            PayoutAtomPolicy::Exact
        )
        .is_err());
    }
    for at in [0, 10, bytes.len() / 2, bytes.len() - 1] {
        let mut bad = bytes.clone();
        bad[at] ^= 1;
        assert!(FundedIouLedger::from_snapshot_bytes(
            &bad,
            TradeLayerMainnet::Bitcoin,
            PayoutAtomPolicy::Exact
        )
        .is_err());
    }
    for (is_source, bytes) in [(true, source_bytes), (false, bytes)] {
        let mut bad = bytes;
        let at = bad.len() - 4;
        bad.insert(at, 0);
        fix_crc(&mut bad);
        if is_source {
            assert!(IouStore::from_snapshot_bytes(&bad).is_err());
        } else {
            assert!(FundedIouLedger::from_snapshot_bytes(
                &bad,
                TradeLayerMainnet::Bitcoin,
                PayoutAtomPolicy::Exact
            )
            .is_err());
        }
    }
}

#[test]
fn standalone_crc_valid_noncanonical_decimals_counts_keys_and_claim_order_fail() {
    let source = claims(&[("2", "1"), ("3", "0")]);
    let bytes = source.snapshot_bytes().unwrap();
    let (maps_at, total_at, maps) = iou_map_offsets(&bytes);
    let map_at = maps[0].start;
    let first = map_at + 20;
    // Both account keys have one byte, hence fixed-size 7-byte records here.
    let second = first + 7;
    let mut variants = Vec::new();
    for (offset, count) in [
        (5, MAX_IOU_RECORDS + 1),
        (maps_at, MAX_IOU_RECORDS + 1),
        (total_at, MAX_IOU_CLAIMS + 1),
        (map_at + 16, MAX_IOU_CLAIMS + 1),
    ] {
        let mut bad = bytes.clone();
        bad[offset..offset + 4].copy_from_slice(&(count as u32).to_be_bytes());
        variants.push(bad);
    }
    let mut bad = bytes.clone();
    bad[total_at..total_at + 4].copy_from_slice(&1_u32.to_be_bytes());
    variants.push(bad);
    let mut bad = bytes.clone();
    bad.swap(first + 1, second + 1);
    variants.push(bad);
    let mut bad = bytes.clone();
    bad[second + 1] = b'2';
    variants.push(bad);
    let mut bad = bytes.clone();
    bad[map_at + 8..map_at + 16].copy_from_slice(&u64::MAX.to_be_bytes());
    variants.push(bad);
    // Canonical "1" becomes numerically equal "01", with lengths and CRC fixed.
    let mut bad = bytes.clone();
    bad[first + 2..first + 6].copy_from_slice(&2_u32.to_be_bytes());
    bad.insert(first + 6, b'0');
    variants.push(bad);
    let mut bad = bytes.clone();
    bad[first + 2..first + 6].copy_from_slice(&u32::MAX.to_be_bytes());
    variants.push(bad);
    // Duplicate full claim-map key, with both header counts made consistent.
    let mut bad = bytes.clone();
    let extra = bad[maps[0].clone()].to_vec();
    let at = bad.len() - 4;
    bad.splice(at..at, extra);
    bad[maps_at..maps_at + 4].copy_from_slice(&2_u32.to_be_bytes());
    bad[total_at..total_at + 4].copy_from_slice(&4_u32.to_be_bytes());
    variants.push(bad);
    for mut bad in variants {
        fix_crc(&mut bad);
        assert!(IouStore::from_snapshot_bytes(&bad).is_err());
    }
}

#[test]
fn funded_crc_valid_network_policy_height_escrow_and_replay_malleability_fail() {
    let mut state = funded(PayoutAtomPolicy::Exact, claims(&[("a", "1")]));
    state
        .collect_loss_unverified(input(1, 1), key(), a("payer"), 1)
        .unwrap();
    state.pay_unverified(input(2, 2), key(), 1).unwrap();
    let bytes = state.snapshot_bytes().unwrap();
    let (ledger_at, _, escrow_at, replay_at) = funded_offsets(&bytes);
    let mut variants = Vec::new();
    let mut bad = bytes.clone();
    bad[5] = TradeLayerMainnet::Litecoin.tag();
    variants.push(bad);
    let mut bad = bytes.clone();
    bad[6] ^= 1;
    variants.push(bad);
    let mut bad = bytes.clone();
    bad[38] = 2;
    variants.push(bad);
    let mut bad = bytes.clone();
    bad[39] = 2;
    variants.push(bad);
    let mut bad = bytes.clone();
    bad[40..48].copy_from_slice(&u64::MAX.to_be_bytes());
    variants.push(bad);
    let mut bad = bytes.clone();
    bad[40..48].copy_from_slice(&0_u64.to_be_bytes());
    variants.push(bad);
    let mut bad = bytes.clone();
    bad[escrow_at..escrow_at + 4].copy_from_slice(&u32::MAX.to_be_bytes());
    variants.push(bad);
    let mut bad = bytes.clone();
    bad[escrow_at + 14] ^= 1;
    variants.push(bad);
    let mut bad = bytes.clone();
    bad.drain(escrow_at + 4..replay_at);
    bad[escrow_at..escrow_at + 4].copy_from_slice(&0_u32.to_be_bytes());
    variants.push(bad);
    let mut bad = bytes.clone();
    let extra = bad[escrow_at + 4..replay_at].to_vec();
    bad.splice(replay_at..replay_at, extra);
    bad[escrow_at..escrow_at + 4].copy_from_slice(&2_u32.to_be_bytes());
    variants.push(bad);
    let mut bad = bytes.clone();
    bad[replay_at..replay_at + 4]
        .copy_from_slice(&((MAX_FUNDED_IOU_INPUTS + 1) as u32).to_be_bytes());
    variants.push(bad);
    let mut bad = bytes.clone();
    bad[replay_at + 36..replay_at + 68].copy_from_slice(&id(1).as_bytes());
    variants.push(bad);
    let mut bad = bytes.clone();
    bad[replay_at + 4..replay_at + 36].copy_from_slice(&id(250).as_bytes());
    bad[replay_at + 36..replay_at + 68].copy_from_slice(&id(251).as_bytes());
    variants.push(bad);
    // Inner ledger also has its own CRC, verified independently.
    let mut bad = bytes.clone();
    bad[ledger_at + 1] ^= 1;
    variants.push(bad);
    for mut bad in variants {
        fix_crc(&mut bad);
        assert!(FundedIouLedger::from_snapshot_bytes(
            &bad,
            TradeLayerMainnet::Bitcoin,
            PayoutAtomPolicy::Exact
        )
        .is_err());
    }
    for network in [TradeLayerMainnet::Litecoin, TradeLayerMainnet::Dogecoin] {
        assert!(
            FundedIouLedger::from_snapshot_bytes(&bytes, network, PayoutAtomPolicy::Exact).is_err()
        );
    }
    for policy in [PayoutAtomPolicy::Down, PayoutAtomPolicy::HalfUp] {
        assert!(
            FundedIouLedger::from_snapshot_bytes(&bytes, TradeLayerMainnet::Bitcoin, policy)
                .is_err()
        );
    }
}

#[test]
fn empty_and_imported_zero_height_state_roundtrip_in_each_network_and_policy() {
    for network in [
        TradeLayerMainnet::Bitcoin,
        TradeLayerMainnet::Litecoin,
        TradeLayerMainnet::Dogecoin,
    ] {
        for policy in [
            PayoutAtomPolicy::Exact,
            PayoutAtomPolicy::Down,
            PayoutAtomPolicy::HalfUp,
        ] {
            for store in [
                IouStore::new(),
                claims(&[]),
                claims(&[("a", "0"), ("b", "-1")]),
            ] {
                let state =
                    FundedIouLedger::from_ledger_unverified(network, policy, ledger(), store)
                        .unwrap();
                let bytes = state.snapshot_bytes().unwrap();
                assert_eq!(
                    state,
                    FundedIouLedger::from_snapshot_bytes(&bytes, network, policy).unwrap()
                );
            }
        }
    }
}

#[test]
fn decimal_limits_and_preallocation_size_limits_apply_to_checkpoints() {
    let maximum = format!(
        "-{}",
        "9".repeat(tradelayer_core::decimal::MAX_DECIMAL_DIGITS)
    );
    let tiny = format!(
        "0.{}1",
        "0".repeat(tradelayer_core::decimal::MAX_DECIMAL_SCALE as usize - 1)
    );
    let source = claims(&[("max", &maximum), ("tiny", &tiny)]);
    let bytes = source.snapshot_bytes().unwrap();
    assert_eq!(source, IouStore::from_snapshot_bytes(&bytes).unwrap());
    let oversized = vec![0; 64 * 1024 * 1024 + 1];
    assert!(IouStore::from_snapshot_bytes(&oversized).is_err());
    assert!(FundedIouLedger::from_snapshot_bytes(
        &oversized,
        TradeLayerMainnet::Bitcoin,
        PayoutAtomPolicy::Exact
    )
    .is_err());
}
