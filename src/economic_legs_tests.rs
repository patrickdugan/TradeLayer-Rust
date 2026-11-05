// Internal legs are identified by the owner's sequence high-water mark: no
// replay marker of the ledger or the joint state holds them, an external id
// cannot collide with them or use them up, and a snapshot of the earlier
// layout (leg ids stored among the ledger's replay markers) decodes, re-encodes
// byte for byte, and moves to the current layout with its first block.
use crate::contract_orderbook::{Order, Side};
use crate::economic::*;
use crate::joint::{JointError, JointOperation, JointProtocolState, UnverifiedJointTx};
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
fn order(txid: TxId, owner: &str, side: Side, height: u64) -> EconomicInput {
    EconomicInput::Order(Order {
        txid,
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
fn fresh() -> EconomicState {
    let mut state = EconomicState::from_joint_unverified(config(), bootstrap()).unwrap();
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
fn trade_with(state: &mut EconomicState, height: u64, buy: TxId, sell: TxId) {
    let (buyer, seller) = if height.is_multiple_of(2) {
        ("alice", "bob")
    } else {
        ("bob", "alice")
    };
    assert_eq!(
        state.apply_block_unverified(
            height,
            vec![
                order(buy, buyer, Side::Buy, height),
                order(sell, seller, Side::Sell, height),
            ],
        ),
        Ok(1),
        "height {height}"
    );
}
fn trade(state: &mut EconomicState, height: u64) {
    let n = 1000 + 2 * height as u32;
    trade_with(state, height, id(n), id(n + 1));
}
fn decode(bytes: &[u8]) -> EconomicState {
    EconomicState::from_snapshot_bytes(bytes, &config(), &a("admin"), &BTreeSet::from([p()]))
        .unwrap()
}

#[test]
fn the_high_water_mark_rejects_replayed_legs_and_accepts_every_new_one() {
    let mut state = fresh();
    assert_eq!(state.internal_sequence, 0);
    let mut seen = BTreeSet::new();
    for height in 2..=40 {
        let before = state.internal_sequence;
        trade(&mut state, height);
        assert!(state.internal_sequence > before, "height {height}");
        for sequence in before..state.internal_sequence {
            assert!(seen.insert(state.leg_id(sequence)), "leg ids are distinct");
        }
    }
    let legs = state.internal_sequence;
    assert!(legs >= 3 * 39);
    for sequence in 0..legs {
        assert!(state.leg_is_processed(sequence));
        let mut copy = state.clone();
        assert_eq!(
            copy.apply_leg(sequence, a("alice"), a("~econ:x"), 1),
            Err(EconomicError::Joint(JointError::Replay(
                state.leg_id(sequence)
            )))
        );
        assert_eq!(copy, state);
    }
    assert!(!state.leg_is_processed(legs));
    let mut gap = state.clone();
    assert_eq!(
        gap.apply_leg(legs + 1, a("alice"), a("~econ:x"), 1),
        Err(EconomicError::Invariant)
    );
    // The ledger and the joint state hold no marker for any leg.
    for sequence in 0..legs {
        assert!(!state.ledger().is_processed(state.leg_id(sequence)));
    }
    assert_eq!(state.ledger().processed_count(), 2);
    state.verify().unwrap();
}

#[test]
fn an_external_id_equal_to_a_leg_id_cannot_collide_with_or_exhaust_the_legs() {
    let mut state = fresh();
    trade(&mut state, 2);
    // External ids drawn from the leg namespace: the next leg, a past leg, and a
    // far future leg. The legs of the following blocks are unaffected.
    let next = state.internal_sequence;
    let ids = [
        state.leg_id(next),
        state.leg_id(0),
        state.leg_id(next + 1_000),
    ];
    trade_with(&mut state, 3, ids[0], ids[1]);
    trade_with(&mut state, 4, ids[2], id(5000));
    for height in 5..=12 {
        trade(&mut state, height);
    }
    state.verify().unwrap();
    // The external ids are replay-protected as external ids.
    let mut again = state.clone();
    assert!(again
        .apply_block_unverified(13, vec![order(ids[0], "alice", Side::Buy, 13)])
        .is_err());
    assert_eq!(again, state);
    // A snapshot round trip keeps them.
    let decoded = decode(&state.snapshot_bytes().unwrap());
    assert_eq!(decoded, state);
}

#[test]
fn the_earlier_layout_decodes_reencodes_and_migrates_with_the_first_block() {
    let mut state = fresh();
    for height in 2..=12 {
        trade(&mut state, height);
    }
    let legs = state.internal_sequence;
    let current = state.snapshot_bytes().unwrap();
    let earlier = state.snapshot_bytes_with_stored_legs().unwrap();
    assert_ne!(current, earlier);
    // One 32-byte marker per leg.
    assert_eq!(earlier.len(), current.len() + 32 * legs as usize);
    // The earlier layout decodes to the same state and writes itself back.
    let mut decoded = decode(&earlier);
    assert_eq!(decoded, state);
    assert_eq!(decoded.snapshot_bytes().unwrap(), earlier);
    decoded.verify().unwrap();
    // The current layout round trips and is distinguished from it.
    let reloaded = decode(&current);
    assert_eq!(reloaded, state);
    assert_eq!(reloaded.snapshot_bytes().unwrap(), current);
    // The first block moves the decoded state to the current layout, and the
    // result is the state of a node that never stored legs.
    let mut node = state.clone();
    trade(&mut node, 13);
    trade(&mut decoded, 13);
    assert_eq!(
        decoded.snapshot_bytes().unwrap(),
        node.snapshot_bytes().unwrap()
    );
    let mut explicit = decode(&earlier);
    explicit.migrate_leg_history_unverified();
    assert_eq!(explicit.snapshot_bytes().unwrap(), current);
    // Ids that do not match the sequence are refused.
    let mut wrong = state.clone();
    wrong.internal_sequence = legs - 1;
    let domain = wrong.leg_domain();
    wrong.joint.remember_stored_legs_unverified(&domain, legs);
    assert!(wrong.snapshot_bytes().is_err());
}
