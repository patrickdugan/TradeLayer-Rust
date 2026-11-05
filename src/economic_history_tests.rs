// Bounded receipt history: the state keeps a window of receipts, the rest live in
// a log committed by a running hash. The consensus constants are lowered through
// the test-only limits so that the window and the capacity bound are crossed in
// a few dozen blocks.
use crate::contract_orderbook::{ContractOrderbook, Order, Side, MAX_CONTRACT_ORDER_IDS};
use crate::economic::*;
use crate::joint::{JointOperation, JointProtocolState, UnverifiedJointTx};
use crate::ledger::{AccountId, Ledger, Operation, PropertyId, PropertyKind, TxId};
use crate::network_profile::TradeLayerMainnet;
use crate::registry::RegistryAction;
use crate::tally::FeeRoute;
use std::collections::BTreeSet;

const A: u128 = 100_000_000;
fn a(s: &str) -> AccountId {
    s.try_into().unwrap()
}
fn p() -> PropertyId {
    7.try_into().unwrap()
}
fn id(n: u32) -> TxId {
    let mut bytes = [0; 32];
    bytes[28..].copy_from_slice(&n.to_be_bytes());
    TxId::from_bytes(bytes)
}
fn config(funding: bool) -> EconomicConfig {
    EconomicConfig {
        native_entitlements: None,
        insurance: None,
        contract_binding: None,
        source_consensus: None,
        network: TradeLayerMainnet::Litecoin,
        contract: 1.try_into().unwrap(),
        collateral: p(),
        inverse: false,
        notional: "1".parse().unwrap(),
        initial_margin_per_contract: "10".parse().unwrap(),
        fee_route: if funding {
            FeeRoute::Oracle(1)
        } else {
            FeeRoute::Native(1)
        },
        policy: SettlementPolicy::ConservingV1,
        funding: funding.then_some(FundingConfig {
            oracle_id: 1,
            max_staleness_blocks: 96,
            policy: FundingPolicy::ConservingFundingV1,
        }),
        marking: None,
        liquidation: None,
        amm: None,
    }
}
fn bootstrap() -> JointProtocolState {
    let mut ledger = Ledger::new();
    ledger
        .apply(
            id(1),
            Operation::CreateProperty {
                id: p(),
                issuer: a("alice"),
                kind: PropertyKind::Fixed,
                initial_atoms: 3000 * A,
            },
        )
        .unwrap();
    ledger
        .apply(
            id(2),
            Operation::Transfer {
                id: p(),
                from: a("alice"),
                to: a("bob"),
                amount_atoms: 1000 * A,
            },
        )
        .unwrap();
    JointProtocolState::from_bootstrap_unverified(
        TradeLayerMainnet::Litecoin,
        a("admin"),
        ledger,
        BTreeSet::from([p()]),
    )
    .unwrap()
}
fn order(n: u32, owner: &str, side: Side, height: u64) -> EconomicInput {
    EconomicInput::Order(Order {
        txid: id(n),
        owner: a(owner),
        contract: 1.try_into().unwrap(),
        height,
        side,
        quantity: "1".parse().unwrap(),
        price: "100".parse().unwrap(),
        post: false,
        is_market: false,
    })
}
fn fresh(funding: bool, policy: AuditPolicy, window: usize, receipt_cap: usize) -> EconomicState {
    let mut state = EconomicState::from_joint_unverified(config(funding), bootstrap())
        .unwrap()
        .with_audit_policy(policy);
    state.set_history_limits_for_test(window, receipt_cap, MAX_CONTRACT_ORDER_IDS);
    state
        .apply_block_unverified(
            1,
            vec![EconomicInput::Joint(UnverifiedJointTx {
                txid: id(10),
                actor: a("admin"),
                operation: JointOperation::Registry(RegistryAction::Activate {
                    types: vec![2, 6, 18],
                    code_hash: [0x71; 32],
                }),
            })],
        )
        .unwrap();
    state
}
/// One fill per block; the sides alternate so no position grows.
fn trade(state: &mut EconomicState, height: u64) -> Result<usize, EconomicError> {
    let n = 1000 + 2 * height as u32;
    let (buyer, seller) = if height.is_multiple_of(2) {
        ("alice", "bob")
    } else {
        ("bob", "alice")
    };
    state.apply_block_unverified(
        height,
        vec![
            order(n, buyer, Side::Buy, height),
            order(n + 1, seller, Side::Sell, height),
        ],
    )
}
fn run(state: &mut EconomicState, from: u64, to: u64) {
    for height in from..=to {
        assert_eq!(trade(state, height), Ok(1), "height {height}");
    }
}
fn decode(bytes: &[u8], funding: bool) -> EconomicState {
    EconomicState::from_snapshot_bytes(bytes, &config(funding), &a("admin"), &BTreeSet::from([p()]))
        .unwrap()
}

#[test]
fn fills_past_the_receipt_cap_no_longer_halt_the_owner() {
    // Without archiving the capacity bound ends the history at the cap.
    let mut unbounded = fresh(false, AuditPolicy::default(), 1000, 16);
    run(&mut unbounded, 2, 17);
    assert_eq!(unbounded.receipts().len(), 16);
    assert_eq!(
        trade(&mut unbounded, 18),
        Err(EconomicError::Capacity),
        "the control state still stops at the cap"
    );
    // With a window the same cap bounds only what the state holds.
    for policy in [
        AuditPolicy::EveryBlock,
        AuditPolicy::Periodic(1),
        AuditPolicy::Periodic(7),
        AuditPolicy::default(),
    ] {
        let mut state = fresh(false, policy, 4, 16);
        run(&mut state, 2, 121);
        assert_eq!(state.receipt_count(), 120);
        assert_eq!(state.archived_receipt_count(), 116);
        assert_eq!(state.receipts().len(), 4);
        assert_eq!(state.receipt_log().len(), 116);
        assert!(state.receipt_log().is_complete());
        state.verify().unwrap();
        // The journal holds only the window's accruals.
        assert!(state.fees().journal().len() <= 4);
        assert_eq!(state.receipts_since(118).len(), 2);
    }
}

#[test]
fn every_policy_commits_the_same_history_bytes() {
    let mut states: Vec<EconomicState> = [
        AuditPolicy::EveryBlock,
        AuditPolicy::Periodic(1),
        AuditPolicy::Periodic(7),
        AuditPolicy::default(),
    ]
    .into_iter()
    .map(|policy| fresh(false, policy, 4, 16))
    .collect();
    for height in 2..=60 {
        let mut bytes = BTreeSet::new();
        for state in &mut states {
            trade(state, height).unwrap();
            bytes.insert(state.snapshot_bytes().unwrap());
        }
        assert_eq!(bytes.len(), 1, "height {height}");
    }
}

#[test]
fn archive_layout_roundtrips_and_older_layouts_decode() {
    for funding in [false, true] {
        let mut state = fresh(funding, AuditPolicy::default(), 4, 16);
        // Not yet archived: the layout that existed before is kept byte for byte.
        run(&mut state, 2, 5);
        let legacy = state.snapshot_bytes().unwrap();
        assert_eq!(
            &legacy[..5],
            if funding { b"TLEC\x02" } else { b"TLEC\x01" }
        );
        run(&mut state, 6, 40);
        let bytes = state.snapshot_bytes().unwrap();
        assert_eq!(
            &bytes[..6],
            &[b'T', b'L', b'E', b'C', 0x0b, u8::from(funding)]
        );
        let restored = decode(&bytes, funding);
        assert_eq!(restored, state);
        assert_eq!(restored.snapshot_bytes().unwrap(), bytes);
        // The restored node holds no archived receipt but the same commitment.
        assert_eq!(
            restored.receipt_log_commitment(),
            state.receipt_log_commitment()
        );
        assert_eq!(
            restored.receipt_log().base_count(),
            state.archived_receipt_count()
        );
        assert!(restored.receipt_log().is_empty());
        restored.verify().unwrap();
        // Corrupt layouts are rejected.
        for bad in [5_usize, 6] {
            let mut raw = crate::codec::verify_crc(&bytes).unwrap().to_vec();
            raw[bad] ^= 0x7f;
            assert!(EconomicState::from_snapshot_bytes(
                &crate::codec::with_crc(raw).unwrap(),
                &config(funding),
                &a("admin"),
                &BTreeSet::from([p()])
            )
            .is_err());
        }
        // The old layout needs no archive; a new layout without one is not valid.
        let mut raw = crate::codec::verify_crc(&legacy).unwrap().to_vec();
        raw[4] = 0x0b;
        raw.insert(5, u8::from(funding));
        assert!(EconomicState::from_snapshot_bytes(
            &crate::codec::with_crc(raw).unwrap(),
            &config(funding),
            &a("admin"),
            &BTreeSet::from([p()])
        )
        .is_err());
    }
}

#[test]
fn an_older_snapshot_migrates_and_reaches_the_same_state() {
    for funding in [false, true] {
        // A state from before the archive existed: every receipt is in the state.
        let mut old = fresh(funding, AuditPolicy::default(), 1000, 1000);
        run(&mut old, 2, 41);
        let old_bytes = old.snapshot_bytes().unwrap();
        assert_eq!(
            &old_bytes[..5],
            if funding { b"TLEC\x02" } else { b"TLEC\x01" }
        );
        assert_eq!(old.receipts().len(), 40);

        let mut migrated = decode(&old_bytes, funding);
        assert_eq!(migrated.snapshot_bytes().unwrap(), old_bytes);
        migrated.set_history_limits_for_test(4, 64, MAX_CONTRACT_ORDER_IDS);
        // A node that ran the new software from the start.
        let mut continuous = fresh(funding, AuditPolicy::default(), 4, 64);
        run(&mut continuous, 2, 41);
        assert_ne!(continuous.snapshot_bytes().unwrap(), old_bytes);

        run(&mut migrated, 42, 80);
        run(&mut continuous, 42, 80);
        assert_eq!(migrated.receipt_count(), 79);
        assert_eq!(migrated.receipts().len(), 4);
        assert_eq!(
            migrated.snapshot_bytes().unwrap(),
            continuous.snapshot_bytes().unwrap(),
            "identical consensus state after the first block"
        );
        assert_eq!(migrated, continuous);
        assert_eq!(migrated.ledger(), continuous.ledger());
        assert_eq!(migrated.positions(), continuous.positions());
        assert_eq!(migrated.fees(), continuous.fees());
        assert_eq!(
            migrated.receipt_log_commitment(),
            continuous.receipt_log_commitment()
        );
        migrated.verify().unwrap();

        // The explicit migration gives the same state at once.
        let mut explicit = decode(&old_bytes, funding);
        explicit.set_history_limits_for_test(4, 64, MAX_CONTRACT_ORDER_IDS);
        explicit.migrate_receipt_history_unverified().unwrap();
        assert_eq!(explicit.receipts().len(), 4);
        assert_eq!(&explicit.snapshot_bytes().unwrap()[..5], b"TLEC\x0b");
        explicit.verify().unwrap();
        assert!(explicit.receipt_log().is_complete());
        assert_eq!(explicit.receipt_log().len(), 36);
        run(&mut explicit, 42, 80);
        assert_eq!(explicit, continuous);
    }
}

#[test]
fn a_replayed_old_txid_is_still_rejected_after_receipts_archive() {
    let mut state = fresh(false, AuditPolicy::default(), 4, 16);
    run(&mut state, 2, 60);
    assert!(state.archived_receipt_count() > 50);
    let before = state.clone();
    // The first order and a cancellation id were admitted before any archiving.
    let replay = order(1000 + 4, "alice", Side::Buy, 61);
    assert!(state.apply_block_unverified(61, vec![replay]).is_err());
    assert_eq!(state, before);
    let cancel = EconomicInput::Cancel {
        txid: id(1000 + 5),
        owner: a("bob"),
        criteria: crate::contract_orderbook::CancelCriteria::All,
    };
    assert!(state.apply_block_unverified(61, vec![cancel]).is_err());
    assert_eq!(state, before);
    // The same holds after a snapshot round trip.
    let mut restored = decode(&state.snapshot_bytes().unwrap(), false);
    assert!(restored
        .apply_block_unverified(61, vec![order(1000 + 6, "alice", Side::Buy, 61)])
        .is_err());
    assert_eq!(trade(&mut restored, 61), Ok(1));
}

#[test]
fn the_log_hash_detects_a_tampered_archived_receipt_at_the_next_full_audit() {
    let mut state = fresh(false, AuditPolicy::Periodic(8), 4, 16);
    run(&mut state, 2, 30);
    state.verify().unwrap();
    state.corrupt_archived_receipt_for_test();
    assert_eq!(state.verify(), Err(EconomicError::Invariant));
    // Blocks that are not multiples of 8 audit only their own additions.
    let mut height = state.finalized_height().unwrap() + 1;
    while !height.is_multiple_of(8) {
        assert_eq!(trade(&mut state, height), Ok(1), "height {height}");
        height += 1;
    }
    let before = state.clone();
    assert_eq!(trade(&mut state, height), Err(EconomicError::Invariant));
    assert_eq!(state, before, "a failed audit publishes nothing");

    let mut every = fresh(false, AuditPolicy::EveryBlock, 4, 16);
    run(&mut every, 2, 30);
    every.corrupt_archived_receipt_for_test();
    assert_eq!(trade(&mut every, 31), Err(EconomicError::Invariant));
}

#[test]
fn a_pruned_log_is_consensus_equal_and_the_prefix_reattaches_exactly() {
    let mut state = fresh(false, AuditPolicy::default(), 4, 16);
    run(&mut state, 2, 40);
    let complete: Vec<_> = state.receipt_log().iter().cloned().collect();
    assert_eq!(complete.len(), 35);
    let mut pruned = state.clone();
    pruned.prune_receipt_log();
    assert_eq!(pruned, state);
    pruned.verify().unwrap();
    // Both nodes keep agreeing on every later block.
    run(&mut pruned, 41, 50);
    run(&mut state, 41, 50);
    assert_eq!(
        pruned.snapshot_bytes().unwrap(),
        state.snapshot_bytes().unwrap()
    );
    // Only the exact prefix chains to the pruned base.
    assert_eq!(pruned.receipt_log().base_count(), 35);
    let mut wrong = complete.clone();
    wrong[3].retained_fee_atoms += 1;
    assert_eq!(
        pruned.attach_receipt_log_prefix(wrong),
        Err(EconomicError::InvalidInput)
    );
    assert_eq!(
        pruned.attach_receipt_log_prefix(complete[1..].to_vec()),
        Err(EconomicError::InvalidInput)
    );
    pruned.attach_receipt_log_prefix(complete).unwrap();
    assert!(pruned.receipt_log().is_complete());
    assert_eq!(pruned.receipt_log().len(), pruned.archived_receipt_count());
    pruned.verify().unwrap();
    // A tampered committed total is found even without a log.
    let mut forged = state.clone();
    forged.prune_receipt_log();
    forged.receipt_archive.collected_atoms += 1;
    assert_eq!(forged.verify(), Err(EconomicError::Invariant));
    let mut forged = state.clone();
    forged.receipt_archive.insurance_atoms += 1;
    assert_eq!(forged.verify(), Err(EconomicError::Invariant));
}

#[test]
fn rollback_keeps_the_log_that_still_chains_to_the_restored_head() {
    let mut state = fresh(false, AuditPolicy::default(), 4, 16);
    run(&mut state, 2, 30);
    let earlier = state.snapshot_bytes().unwrap();
    let archived_then = state.archived_receipt_count();
    run(&mut state, 31, 45);
    let mut restored = decode(&earlier, false);
    restored.set_history_limits_for_test(4, 16, MAX_CONTRACT_ORDER_IDS);
    assert_eq!(restored.receipt_log().base_count(), archived_then);
    restored.inherit_receipt_log(&state);
    assert!(restored.receipt_log().is_complete());
    assert_eq!(restored.receipt_log().len(), archived_then);
    restored.verify().unwrap();
    run(&mut restored, 31, 45);
    assert_eq!(restored, state);
}

const _: () = assert!(MAX_CONTRACT_ORDER_IDS > 65_536);

#[test]
fn order_ids_past_the_old_bound_are_admitted() {
    let contract = 1.try_into().unwrap();
    let mut book = ContractOrderbook::new(contract);
    let mut snapshot = book.snapshot();
    snapshot.seen = (0..70_000_u64)
        .map(|n| {
            let mut bytes = [0; 32];
            bytes[..8].copy_from_slice(&n.to_be_bytes());
            TxId::from_bytes(bytes)
        })
        .collect();
    book = ContractOrderbook::from_snapshot(snapshot).unwrap();
    let fresh_order = Order {
        txid: id(9_999_999),
        owner: a("alice"),
        contract,
        height: 1,
        side: Side::Buy,
        quantity: "1".parse().unwrap(),
        price: "100".parse().unwrap(),
        post: false,
        is_market: false,
    };
    book.queue(fresh_order.clone()).unwrap();
    // Every one of the 70,000 earlier ids is still refused.
    let mut replay = fresh_order;
    replay.txid = {
        let mut bytes = [0; 32];
        bytes[..8].copy_from_slice(&41_234_u64.to_be_bytes());
        TxId::from_bytes(bytes)
    };
    assert!(book.queue(replay).is_err());
}
