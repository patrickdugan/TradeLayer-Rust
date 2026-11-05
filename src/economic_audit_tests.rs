// A violation the per-block path cannot see is found at the next full-audit
// height under Periodic, and at once under EveryBlock.
use crate::contract_orderbook::{Order, Side};
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
fn config() -> EconomicConfig {
    EconomicConfig {
        native_entitlements: None,
        insurance: None,
        contract_binding: None,
        source_consensus: None,
        network: TradeLayerMainnet::Litecoin,
        anchor: EconomicConfig::genesis_anchor(TradeLayerMainnet::Litecoin),
        contract: 1.try_into().unwrap(),
        collateral: p(),
        inverse: false,
        notional: "1".parse().unwrap(),
        initial_margin_per_contract: "10".parse().unwrap(),
        fee_route: FeeRoute::Native(1),
        policy: SettlementPolicy::ConservingV1,
        funding: None,
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
/// Block 1 activates, block 2 trades once, so one receipt exists to corrupt.
fn traded(policy: AuditPolicy) -> EconomicState {
    let mut state = EconomicState::from_joint_unverified(config(), bootstrap())
        .unwrap()
        .with_audit_policy(policy);
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
    assert_eq!(
        state
            .apply_block_unverified(
                2,
                vec![
                    order(20, "alice", Side::Buy, 2),
                    order(21, "bob", Side::Sell, 2)
                ]
            )
            .unwrap(),
        1
    );
    state
}

#[test]
fn periodic_policy_finds_untouched_history_corruption_at_the_next_full_audit() {
    let mut state = traded(AuditPolicy::Periodic(8));
    assert!(state.has_audit_marker_for_test());
    state.corrupt_oldest_receipt_for_audit_test();
    assert_eq!(state.verify(), Err(EconomicError::Invariant));
    for height in 3..8 {
        assert_eq!(
            state.apply_block_unverified(height, vec![]),
            Ok(0),
            "height {height} audits only live state and its own additions"
        );
    }
    let before = state.clone();
    assert_eq!(
        state.apply_block_unverified(8, vec![]),
        Err(EconomicError::Invariant),
        "the multiple of 8 audits completely"
    );
    assert_eq!(state, before, "a failed audit publishes nothing");
    assert_eq!(state.finalized_height(), Some(7));
}

#[test]
fn every_block_policy_finds_the_same_corruption_at_the_next_block() {
    let mut state = traded(AuditPolicy::EveryBlock);
    state.corrupt_oldest_receipt_for_audit_test();
    let before = state.clone();
    assert_eq!(
        state.apply_block_unverified(3, vec![]),
        Err(EconomicError::Invariant)
    );
    assert_eq!(state, before);
}

#[test]
fn a_state_no_block_of_this_process_produced_audits_completely_before_its_first_block() {
    let mut state = traded(AuditPolicy::Periodic(8));
    state.corrupt_oldest_receipt_for_audit_test();
    state.clear_audit_marker_for_test();
    assert_eq!(
        state.apply_block_unverified(3, vec![]),
        Err(EconomicError::Invariant)
    );
}

#[test]
fn policy_and_marker_are_runtime_only() {
    let periodic = traded(AuditPolicy::Periodic(8));
    let every = traded(AuditPolicy::EveryBlock);
    assert!(periodic.has_audit_marker_for_test());
    assert!(!every.has_audit_marker_for_test());
    assert_eq!(periodic, every);
    assert_eq!(format!("{periodic:?}"), format!("{every:?}"));
    let bytes = periodic.snapshot_bytes().unwrap();
    assert_eq!(bytes, every.snapshot_bytes().unwrap());
    let decoded =
        EconomicState::from_snapshot_bytes(&bytes, &config(), &a("admin"), &BTreeSet::from([p()]))
            .unwrap();
    assert_eq!(decoded, periodic);
    assert_eq!(decoded.audit_policy(), AuditPolicy::default());
    assert_eq!(AuditPolicy::default(), AuditPolicy::Periodic(64));
    assert!(!decoded.has_audit_marker_for_test());
    let clone = periodic.clone();
    assert_eq!(clone.audit_policy(), AuditPolicy::Periodic(8));
    assert!(clone.has_audit_marker_for_test());
}
