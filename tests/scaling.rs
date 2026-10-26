use tradelayer_core::ledger::{
    AccountId, ChannelSide, Ledger, Operation, PropertyId, PropertyKind, TxId,
};
use tradelayer_core::scaling::*;
fn a(s: &str) -> AccountId {
    AccountId::try_from(s).unwrap()
}
fn id(n: u8) -> TxId {
    TxId::from_bytes([n; 32])
}
fn property() -> PropertyId {
    PropertyId::try_from(1).unwrap()
}
fn config() -> ScalingConfig {
    ScalingConfig {
        activation_23: Some(0),
        activation_31: Some(0),
    }
}
fn descriptor(name: &str) -> ChannelDescriptor {
    ChannelDescriptor {
        address: a(name),
        party_a: a("alice"),
        party_b: a("bob"),
        root: id(99),
    }
}
fn state() -> ScalingState {
    let mut s = ScalingState::new(config()).unwrap();
    s.register_channel_unverified(descriptor("c"), 1).unwrap();
    s
}
fn trade(n: u8, expiry: u64) -> TradeInput {
    TradeInput {
        txid: id(n),
        price: 100.0,
        contracts: 2.0,
        expiry_block: expiry,
    }
}
fn keep(n: u8) -> SettleRequest {
    SettleRequest {
        settle_type: 0,
        reference_1: Some(id(n)),
        reference_2: None,
        mark_price: None,
        net_amount: None,
        column_a_is_seller: None,
        expiry_block: None,
    }
}
fn close(t: u8, k: u8) -> SettleRequest {
    SettleRequest {
        settle_type: 1,
        reference_1: Some(id(t)),
        reference_2: Some(id(k)),
        mark_price: Some(110.0),
        net_amount: None,
        column_a_is_seller: Some(false),
        expiry_block: None,
    }
}
fn net(n: u8) -> SettleRequest {
    SettleRequest {
        settle_type: 2,
        reference_1: Some(id(n)),
        reference_2: None,
        mark_price: None,
        net_amount: Some(-3.0),
        column_a_is_seller: Some(true),
        expiry_block: None,
    }
}
fn king() -> KingRequest {
    KingRequest {
        block_start: 1,
        block_end: 6,
        property_id: 1,
        net_amount: Some(3.0),
        a_pays_b: Some(true),
        channel_root: Some(id(99)),
        total_contracts: 2.0,
    }
}
fn context() -> ValidationContext {
    ValidationContext {
        active_23: true,
        active_31: true,
        channel_exists: true,
        commit_a_present: true,
        commit_b_present: true,
    }
}
fn ledger() -> Ledger {
    let mut l = Ledger::new();
    l.apply(
        id(201),
        Operation::CreateProperty {
            id: property(),
            issuer: a("alice"),
            kind: PropertyKind::Fixed,
            initial_atoms: 1_500_000_000,
        },
    )
    .unwrap();
    l.apply(
        id(202),
        Operation::Transfer {
            id: property(),
            from: a("alice"),
            to: a("bob"),
            amount_atoms: 500_000_000,
        },
    )
    .unwrap();
    for (n, side, owner, amount) in [
        (203, ChannelSide::A, "alice", 1_000_000_000),
        (204, ChannelSide::B, "bob", 500_000_000),
    ] {
        l.apply(
            id(n),
            Operation::ChannelCommit {
                id: property(),
                owner: a(owner),
                channel: a("c"),
                side,
                amount_atoms: amount,
            },
        )
        .unwrap();
    }
    l
}
fn active() -> ScalingState {
    let mut s = state();
    s.register_trade_unverified(&a("c"), trade(1, 99), 2)
        .unwrap();
    s.settle_channel_pnl_unverified(&a("c"), keep(1), 3, id(2), 12345)
        .unwrap();
    s
}
fn bits(s: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(s, 16).unwrap())
}
fn unhex(s: &str) -> String {
    if s == "-" {
        return String::new();
    }
    String::from_utf8(
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect(),
    )
    .unwrap()
}
fn crc(bytes: &[u8]) -> u32 {
    let mut c = !0u32;
    for b in bytes {
        c ^= u32::from(*b);
        for _ in 0..8 {
            c = (c >> 1) ^ (0xedb88320 & 0u32.wrapping_sub(c & 1));
        }
    }
    !c
}
fn repair(bytes: &mut [u8]) {
    let end = bytes.len() - 4;
    let c = crc(&bytes[..end]);
    bytes[end..].copy_from_slice(&c.to_be_bytes());
}

#[test]
fn full_unchanged_source_pnl_vectors_preserve_binary64_bits() {
    let mut n = 0;
    for line in include_str!("../fixtures/scaling_vectors.tsv").lines() {
        let f: Vec<_> = line.split('\t').collect();
        if f[0] != "P" {
            continue;
        }
        let p = calculate_pnl(bits(f[2]), bits(f[3]), bits(f[4]), f[5] == "1").unwrap();
        assert_eq!(p.amount.to_bits(), bits(f[6]).to_bits(), "{}", f[1]);
        assert_eq!(
            p.direction,
            if f[7] == "AtoB" {
                Direction::AtoB
            } else {
                Direction::BtoA
            }
        );
        n += 1;
    }
    assert_eq!(n, 136);
}
#[test]
fn full_source_validator_order_and_reasons_are_preserved_separately() {
    let mut s = state();
    s.register_trade_unverified(&a("c"), trade(1, 99), 2)
        .unwrap();
    s.register_trade_unverified(&a("c"), trade(9, 2), 2)
        .unwrap();
    s.settle_channel_pnl_unverified(&a("c"), keep(1), 3, id(2), 0)
        .unwrap();
    s.register_trade_unverified(&a("c"), trade(8, 99), 3)
        .unwrap();
    s.settle_channel_pnl_unverified(&a("c"), keep(8), 3, id(7), 0)
        .unwrap();
    s.neutralize_settlement_unverified(&a("c"), id(7), 3)
        .unwrap();
    let mut n = 0;
    for line in include_str!("../fixtures/scaling_vectors.tsv").lines() {
        let f: Vec<_> = line.split('\t').collect();
        if f[0] != "V" {
            continue;
        }
        let (mut p, mut c) = (keep(1), context());
        match f[1] {
            "unpublished-keep" => p = keep(77),
            "expired-keep" => p = keep(9),
            "valid-close" => p = close(1, 2),
            "close-all-reasons" => {
                p = close(1, 2);
                p.reference_2 = None;
                p.mark_price = Some(0.0);
                p.expiry_block = Some(3);
            }
            "missing-net-fields" => {
                p = net(1);
                p.net_amount = None;
                p.column_a_is_seller = None;
            }
            "zero-expiry-ignored" => {
                p = net(1);
                p.expiry_block = Some(0);
            }
            "equal-expiry" => p.expiry_block = Some(4),
            "neutralized" => p = keep(7),
            "missing-ref" => p.reference_1 = None,
            "invalid-type" => p.settle_type = 3,
            "context-active" => c.active_23 = false,
            "context-exists" => c.channel_exists = false,
            "context-a" => {
                c.commit_a_present = false;
                c.commit_b_present = false;
            }
            _ => panic!("unknown vector"),
        }
        let v = s.validate_settle_channel_pnl(&a("c"), &p, 4, c);
        assert_eq!(v.valid, f[2] == "1", "{}", f[1]);
        assert_eq!(v.reason, unhex(f[3]), "{}", f[1]);
        n += 1;
    }
    assert_eq!(n, 13);
}
#[test]
fn source_king_future_end_and_pending_count_quirks_are_observable() {
    let mut s = state();
    s.register_trade_unverified(&a("c"), trade(1, 99), 2)
        .unwrap();
    s.register_trade_unverified(&a("c"), trade(9, 2), 2)
        .unwrap();
    s.advance_height(3).unwrap();
    for line in include_str!("../fixtures/scaling_vectors.tsv").lines() {
        let f: Vec<_> = line.split('\t').collect();
        if f[0] != "K" {
            continue;
        }
        let mut p = king();
        p.block_start = 2;
        p.block_end = 10;
        p.net_amount = Some(0.0);
        p.a_pays_b = Some(false);
        match f[1] {
            "future-end-allowed" => {}
            "empty-pending" => p.block_start = 4,
            "missing-range" => p.block_start = 0,
            "reversed-range" => p.block_start = 11,
            "future-start" => p.block_start = 8,
            "property" => p.property_id = 0,
            "net-missing" => p.net_amount = None,
            "direction" => p.a_pays_b = None,
            "root" => p.channel_root = None,
            _ => panic!(),
        };
        let v = s.validate_king_settle(&a("c"), &p, 4, context());
        assert_eq!(v.valid, f[2] == "1", "{}", f[1]);
        assert_eq!(v.reason, unhex(f[3]));
        assert_eq!(
            v.neutralized_count,
            if f[4] == "-" {
                None
            } else {
                Some(f[4].parse().unwrap())
            }
        );
    }
}
#[test]
fn lifecycle_closes_nets_sweeps_and_produces_an_unfunded_plan_only() {
    let l = ledger();
    let ledger_before = l.snapshot_bytes().unwrap();
    let mut s = ScalingState::new(config()).unwrap();
    s.register_channel_from_ledger_unverified(&l, a("c"), id(99), 1)
        .unwrap();
    s.register_trade_unverified(&a("c"), trade(1, 99), 2)
        .unwrap();
    s.settle_channel_pnl_unverified(&a("c"), keep(1), 3, id(2), 12345)
        .unwrap();
    let offset = s
        .settle_channel_pnl_unverified(&a("c"), close(1, 2), 4, id(3), 12345)
        .unwrap()
        .unwrap();
    assert_eq!(
        offset.pnl,
        Pnl {
            amount: 20.0,
            direction: Direction::BtoA
        }
    );
    assert_eq!(s.get_trade_status(id(1)), Some(SettleStatus::Neutralized));
    assert!(s.is_settlement_neutralized(&a("c"), id(2)));
    s.settle_channel_pnl_unverified(&a("c"), net(3), 5, id(4), 12345)
        .unwrap();
    assert!(s.is_settlement_neutralized(&a("c"), id(3)));
    let rows = s.get_pending_settlements_in_range(&a("c"), 1, 6);
    assert_eq!(
        rows.iter().map(|r| r.kind).collect::<Vec<_>>(),
        vec![PendingKind::Settlement, PendingKind::Net]
    );
    let plan = s
        .process_king_settle_unverified(&l, &a("c"), king(), 6, id(5), 12345)
        .unwrap();
    assert_eq!(plan.amount_atoms, 300_000_000);
    assert_eq!((plan.after_a, plan.after_b), (700_000_000, 800_000_000));
    assert_eq!(s.channel(&a("c")).unwrap().kings[0].neutralized_count, 2);
    assert!(s.get_pending_settlements_in_range(&a("c"), 1, 6).is_empty());
    assert_eq!(l.snapshot_bytes().unwrap(), ledger_before);
    let bytes = s.checkpoint_bytes().unwrap();
    let restored = ScalingState::from_checkpoint_bytes(&bytes, &config()).unwrap();
    assert_eq!(restored.checkpoint_bytes().unwrap(), bytes);
    assert_eq!(restored, s);
    let before = s.checkpoint_bytes().unwrap();
    s.clear_l2_state(&a("c"), 1, 6).unwrap();
    assert_eq!(s.checkpoint_bytes().unwrap(), before);
}
#[test]
fn expiry_and_replay_repairs_fail_atomically_without_reinterpreting_validation() {
    let mut s = state();
    let before = s.checkpoint_bytes().unwrap();
    assert!(
        s.validate_settle_channel_pnl(&a("c"), &keep(1), 2, context())
            .valid
    );
    assert_eq!(
        s.settle_channel_pnl_unverified(&a("c"), keep(1), 2, id(2), 0),
        Err(ScalingError::UnknownTrade)
    );
    assert_eq!(s.checkpoint_bytes().unwrap(), before);
    s.register_trade_unverified(&a("c"), trade(1, 2), 2)
        .unwrap();
    assert_eq!(s.get_trade_status(id(1)), Some(SettleStatus::Pending));
    s.advance_height(3).unwrap();
    assert_eq!(s.get_trade_status(id(1)), Some(SettleStatus::Expired));
    assert_eq!(
        s.get_pending_settlements_in_range(&a("c"), 1, 4)[0].status,
        SettleStatus::Expired
    );
    let before = s.checkpoint_bytes().unwrap();
    assert!(s
        .settle_channel_pnl_unverified(&a("c"), keep(1), 4, id(2), 0)
        .is_err());
    assert_eq!(s.checkpoint_bytes().unwrap(), before);
    let mut s = active();
    s.advance_height(100).unwrap();
    assert_eq!(s.get_trade_status(id(1)), Some(SettleStatus::Live));
    let before = s.checkpoint_bytes().unwrap();
    assert_eq!(
        s.register_trade_unverified(&a("c"), trade(2, 200), 100),
        Err(ScalingError::Replay)
    );
    assert_eq!(s.checkpoint_bytes().unwrap(), before);
}
#[test]
fn global_source_queries_do_not_authorize_cross_channel_transitions() {
    let mut s = active();
    s.register_channel_unverified(descriptor("other"), 3)
        .unwrap();
    assert!(s.validate_keep_alive_chain(id(1), id(2)));
    assert!(
        s.validate_settle_channel_pnl(&a("other"), &close(1, 2), 4, context())
            .valid
    );
    let before = s.checkpoint_bytes().unwrap();
    assert_eq!(
        s.settle_channel_pnl_unverified(&a("other"), close(1, 2), 4, id(3), 0),
        Err(ScalingError::UnknownTrade)
    );
    assert_eq!(s.checkpoint_bytes().unwrap(), before);
    s.settle_channel_pnl_unverified(&a("c"), net(1), 4, id(3), 0)
        .unwrap();
    assert_eq!(s.get_trade_status(id(1)), Some(SettleStatus::Neutralized));
    let before = s.checkpoint_bytes().unwrap();
    assert!(s
        .settle_channel_pnl_unverified(&a("c"), net(1), 5, id(4), 0)
        .is_err());
    assert_eq!(s.checkpoint_bytes().unwrap(), before);
}
#[test]
fn king_checks_root_range_capacity_and_exact_atomic_amount_before_sweep() {
    let l = ledger();
    let s = active();
    let mut unknown = s.clone();
    let mut request = king();
    request.property_id = 999;
    request.net_amount = Some(0.0);
    assert!(
        unknown
            .validate_king_settle(&a("c"), &request, 6, context())
            .valid
    );
    let before = unknown.checkpoint_bytes().unwrap();
    assert_eq!(
        unknown.process_king_settle_unverified(&l, &a("c"), request, 6, id(5), 0),
        Err(ScalingError::InvalidInput)
    );
    assert_eq!(unknown.checkpoint_bytes().unwrap(), before);
    for expected in [
        ScalingError::InvalidRoot,
        ScalingError::InvalidHeight,
        ScalingError::InsufficientCollateral,
        ScalingError::NonAtomicAmount,
        ScalingError::NonFinite,
    ] {
        let mut trial = s.clone();
        let mut p = king();
        match expected {
            ScalingError::InvalidRoot => p.channel_root = Some(id(88)),
            ScalingError::InvalidHeight => p.block_end = 8,
            ScalingError::InsufficientCollateral => p.net_amount = Some(20.0),
            ScalingError::NonAtomicAmount => p.net_amount = Some(0.000000001),
            ScalingError::NonFinite => p.net_amount = Some(f64::INFINITY),
            _ => unreachable!(),
        };
        let before = trial.checkpoint_bytes().unwrap();
        assert_eq!(
            trial.process_king_settle_unverified(&l, &a("c"), p, 6, id(5), 0),
            Err(expected)
        );
        assert_eq!(trial.checkpoint_bytes().unwrap(), before);
    }
    let mut trial = s;
    let mut p = king();
    p.net_amount = Some(-3.0);
    p.a_pays_b = Some(false);
    let plan = trial
        .process_king_settle_unverified(&l, &a("c"), p, 6, id(5), 0)
        .unwrap();
    assert_eq!((plan.after_a, plan.after_b), (1_300_000_000, 200_000_000));
    assert_eq!(plan.direction, Direction::BtoA);
}
#[test]
fn activation_is_prior_block_and_failed_calls_do_not_expire_or_advance() {
    let cfg = ScalingConfig {
        activation_23: Some(3),
        activation_31: Some(5),
    };
    let mut s = ScalingState::new(cfg).unwrap();
    s.register_channel_unverified(descriptor("c"), 1).unwrap();
    s.register_trade_unverified(&a("c"), trade(1, 20), 2)
        .unwrap();
    let before = s.checkpoint_bytes().unwrap();
    assert!(s
        .settle_channel_pnl_unverified(&a("c"), keep(1), 3, id(2), 0)
        .is_err());
    assert_eq!(s.checkpoint_bytes().unwrap(), before);
    s.settle_channel_pnl_unverified(&a("c"), keep(1), 4, id(2), 0)
        .unwrap();
    let l = ledger();
    assert!(s
        .process_king_settle_unverified(&l, &a("c"), king(), 5, id(5), 0)
        .is_err());
    s.process_king_settle_unverified(&l, &a("c"), king(), 6, id(5), 0)
        .unwrap();
}
#[test]
fn checkpoint_replay_rejects_repaired_crc_projection_action_and_configuration_tampering() {
    let s = active();
    let bytes = s.checkpoint_bytes().unwrap();
    assert_eq!(
        ScalingState::from_checkpoint_bytes(&bytes, &ScalingConfig::default()),
        Err(ScalingError::InvalidSnapshot)
    );
    let mut changed = bytes.clone();
    let at = changed.len() - 5;
    changed[at] ^= 1;
    repair(&mut changed);
    assert!(ScalingState::from_checkpoint_bytes(&changed, &config()).is_err());
    let mut changed = bytes.clone();
    let action_start = 5 + 9 + 9 + 4;
    changed[action_start] = 255;
    repair(&mut changed);
    assert!(ScalingState::from_checkpoint_bytes(&changed, &config()).is_err());
    let mut changed = bytes.clone();
    changed[23..27].copy_from_slice(&u32::MAX.to_be_bytes());
    repair(&mut changed);
    assert_eq!(
        ScalingState::from_checkpoint_bytes(&changed, &config()),
        Err(ScalingError::Capacity)
    );
    for n in [0, 4, 5, 23, bytes.len() / 2, bytes.len() - 1] {
        assert!(ScalingState::from_checkpoint_bytes(&bytes[..n], &config()).is_err());
    }
    let mut changed = bytes;
    changed.push(0);
    assert!(ScalingState::from_checkpoint_bytes(&changed, &config()).is_err());
}
#[test]
fn checkpoints_can_restore_earlier_history_and_resume_identically() {
    let mut s = active();
    let prior = s.checkpoint_bytes().unwrap();
    s.settle_channel_pnl_unverified(&a("c"), close(1, 2), 4, id(3), 12345)
        .unwrap();
    let future = s.checkpoint_bytes().unwrap();
    let mut rolled = ScalingState::from_checkpoint_bytes(&prior, &config()).unwrap();
    assert_eq!(rolled.get_trade_status(id(1)), Some(SettleStatus::Live));
    rolled
        .settle_channel_pnl_unverified(&a("c"), close(1, 2), 4, id(3), 12345)
        .unwrap();
    assert_eq!(rolled.checkpoint_bytes().unwrap(), future);
}
#[test]
fn finite_safe_height_time_and_history_bounds_are_atomic() {
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            calculate_pnl(1.0, 1.0, value, false),
            Err(ScalingError::NonFinite)
        );
    }
    let mut s = state();
    assert_eq!(
        s.advance_height(MAX_SAFE_HEIGHT + 1),
        Err(ScalingError::InvalidHeight)
    );
    let mut t = trade(1, 10);
    t.contracts = f64::MAX;
    t.price = f64::MAX;
    s.register_trade_unverified(&a("c"), t, 2).unwrap();
    s.settle_channel_pnl_unverified(&a("c"), keep(1), 3, id(2), 0)
        .unwrap();
    let mut c = close(1, 2);
    c.mark_price = Some(1.0);
    let before = s.checkpoint_bytes().unwrap();
    assert_eq!(
        s.settle_channel_pnl_unverified(&a("c"), c, 4, id(3), 0),
        Err(ScalingError::NonFinite)
    );
    assert_eq!(s.checkpoint_bytes().unwrap(), before);
    assert_eq!(
        s.settle_channel_pnl_unverified(&a("c"), keep(1), 4, id(4), MAX_SAFE_HEIGHT + 1),
        Err(ScalingError::InvalidTime)
    );
    let mut s = ScalingState::new(config()).unwrap();
    for _ in 0..MAX_ACTIONS {
        s.advance_height(1).unwrap();
    }
    let n = s.action_count();
    assert_eq!(s.advance_height(1), Err(ScalingError::Capacity));
    assert_eq!(s.action_count(), n);
}
