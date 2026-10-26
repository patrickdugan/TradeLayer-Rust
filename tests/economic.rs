//! Complete caller-driven contract execution against the existing sole Ledger.
//! ConservingV1 is an explicit execution policy; all actors/blocks are unverified.
use std::collections::BTreeSet;
use tradelayer_core::contract_orderbook::{CancelCriteria, Order, Side};
use tradelayer_core::contract_registry::ContractId;
use tradelayer_core::decimal::Decimal;
use tradelayer_core::economic::{EconomicConfig, EconomicInput, EconomicState, SettlementPolicy};
use tradelayer_core::iou::IouKey;
use tradelayer_core::joint::{JointOperation, JointProtocolState, UnverifiedJointTx};
use tradelayer_core::ledger::{AccountId, Ledger, Operation, PropertyId, PropertyKind, TxId};
use tradelayer_core::network_profile::TradeLayerMainnet;
use tradelayer_core::registry::RegistryAction;
use tradelayer_core::tally::FeeRoute;

const ATOMS: u128 = 100_000_000;
fn d(s: &str) -> Decimal {
    s.parse().unwrap()
}
fn a(s: &str) -> AccountId {
    AccountId::try_from(s).unwrap()
}
fn tx(n: u8) -> TxId {
    TxId::from_bytes([n; 32])
}
fn property() -> PropertyId {
    PropertyId::try_from(7).unwrap()
}
fn contract() -> ContractId {
    ContractId::try_from(1).unwrap()
}
fn config() -> EconomicConfig {
    EconomicConfig {
        native_entitlements: None,
        insurance: None,
        contract_binding: None,
        source_consensus: None,
        network: TradeLayerMainnet::Litecoin,
        contract: contract(),
        collateral: property(),
        inverse: false,
        notional: d("1"),
        initial_margin_per_contract: d("10"),
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
            tx(1),
            Operation::CreateProperty {
                id: property(),
                issuer: a("alice"),
                kind: PropertyKind::Fixed,
                initial_atoms: 4010 * ATOMS,
            },
        )
        .unwrap();
    for (n, owner, tokens) in [
        (2, "bob", 1000),
        (3, "carol", 1000),
        (4, "dave", 1000),
        (5, "poor", 10),
    ] {
        ledger
            .apply(
                tx(n),
                Operation::Transfer {
                    id: property(),
                    from: a("alice"),
                    to: a(owner),
                    amount_atoms: tokens * ATOMS,
                },
            )
            .unwrap();
    }
    JointProtocolState::from_bootstrap_unverified(
        TradeLayerMainnet::Litecoin,
        a("admin"),
        ledger,
        BTreeSet::from([property()]),
    )
    .unwrap()
}
fn activate() -> EconomicInput {
    EconomicInput::Joint(UnverifiedJointTx {
        txid: tx(10),
        actor: a("admin"),
        operation: JointOperation::Registry(RegistryAction::Activate {
            types: vec![2, 6, 18],
            code_hash: [0x71; 32],
        }),
    })
}
fn new() -> EconomicState {
    let mut state = EconomicState::from_joint_unverified(config(), bootstrap()).unwrap();
    assert_eq!(
        state.apply_block_unverified(1, vec![activate()]).unwrap(),
        0
    );
    state
}
fn order(
    n: u8,
    owner: &str,
    side: Side,
    quantity: &str,
    price: &str,
    height: u64,
) -> EconomicInput {
    EconomicInput::Order(Order {
        txid: tx(n),
        owner: a(owner),
        contract: contract(),
        height,
        side,
        quantity: d(quantity),
        price: d(price),
        post: false,
        is_market: false,
    })
}
fn pair(
    state: &mut EconomicState,
    height: u64,
    first: u8,
    buyer: &str,
    seller: &str,
    quantity: &str,
    price: &str,
) {
    assert_eq!(
        state
            .apply_block_unverified(
                height,
                vec![
                    order(first, buyer, Side::Buy, quantity, price, height),
                    order(first + 1, seller, Side::Sell, quantity, price, height)
                ]
            )
            .unwrap(),
        1
    );
    assert_conserved(state);
}
fn send(n: u8, from: &str, to: &str, tokens: u128) -> EconomicInput {
    EconomicInput::Joint(UnverifiedJointTx {
        txid: tx(n),
        actor: a(from),
        operation: JointOperation::Send {
            id: property(),
            recipient: a(to),
            amount_atoms: tokens * ATOMS,
        },
    })
}
fn position(
    state: &EconomicState,
    owner: &str,
    quantity: &str,
    average: Option<&str>,
    margin_tokens: &str,
) {
    let p = state.positions().get(&a(owner)).unwrap();
    assert_eq!(p.contracts, d(quantity), "{owner} quantity");
    assert_eq!(p.avg_price, average.map(d), "{owner} average");
    assert_eq!(p.margin, d(margin_tokens), "{owner} position margin");
    let margin_atoms: u128 = d(margin_tokens)
        .checked_mul(&Decimal::from(ATOMS))
        .unwrap()
        .to_string()
        .parse()
        .unwrap();
    assert_eq!(
        state.margin_atoms(&a(owner)),
        margin_atoms,
        "{owner} funded margin"
    );
}
fn assert_conserved(state: &EconomicState) {
    state.verify().unwrap();
    state.ledger().verify_conservation().unwrap();
    let available: u128 = ["alice", "bob", "carol", "dave", "poor"]
        .iter()
        .map(|owner| state.ledger().available_atoms(property(), &a(owner)))
        .sum();
    let margin: u128 = state
        .positions()
        .positions()
        .map(|p| state.margin_atoms(&p.address))
        .sum();
    let reserve: u128 = state
        .book()
        .resting()
        .iter()
        .map(|o| state.reserve_atoms(o.order.txid))
        .sum();
    let fee = state
        .fees()
        .expected_custody_atoms(state.config().fee_route.key(property()).unwrap())
        .unwrap();
    assert_eq!(
        available
            + margin
            + reserve
            + fee
            + state.loss_pool_atoms()
            + state.insurance_custody_atoms(),
        state
            .ledger()
            .property(property())
            .unwrap()
            .total_supply_atoms
    );
}

#[test]
fn fractional_open_increase_reduce_close_and_flip_move_real_collateral() {
    let mut state = new();
    pair(&mut state, 2, 20, "alice", "bob", "1.25", "100");
    position(&state, "alice", "1.25", Some("100"), "12.5");
    position(&state, "bob", "-1.25", Some("100"), "12.5");
    pair(&mut state, 3, 30, "alice", "bob", "0.75", "120");
    position(&state, "alice", "2", Some("107.5"), "20");
    position(&state, "bob", "-2", Some("107.5"), "20");
    pair(&mut state, 4, 40, "bob", "alice", "0.5", "130");
    position(&state, "alice", "1.5", Some("107.5"), "15");
    position(&state, "bob", "-1.5", Some("107.5"), "15");
    assert_eq!(state.receipts()[2].seller_pnl, d("11.25"));
    assert_eq!(state.receipts()[2].buyer_pnl, d("-11.25"));
    pair(&mut state, 5, 50, "bob", "alice", "1.5", "110");
    position(&state, "alice", "0", None, "0");
    position(&state, "bob", "0", None, "0");
    assert_eq!(
        state.positions().get(&a("alice")).unwrap().realized_pnl,
        d("15")
    );
    pair(&mut state, 6, 60, "alice", "bob", "2", "90");
    pair(&mut state, 7, 70, "bob", "alice", "3", "95");
    position(&state, "alice", "-1", Some("95"), "10");
    position(&state, "bob", "1", Some("95"), "10");
    assert_eq!(
        state.positions().get(&a("alice")).unwrap().realized_pnl,
        d("25")
    );
    assert_eq!(
        state.positions().get(&a("bob")).unwrap().realized_pnl,
        d("-25")
    );
    assert_eq!(state.receipts().len(), 6);
    assert_eq!(state.loss_pool_atoms(), 0);
    assert_eq!(state.unfunded_loss_atoms(), 0);
}

#[test]
fn unmarked_legacy_receipts_cannot_fabricate_reported_owners() {
    use tradelayer_core::economic_reporting::{EconomicHistory, EconomicReportingError};
    let mut state = new();
    assert_eq!(
        EconomicHistory::from_state(&state)
            .unwrap()
            .history()
            .executions()
            .count(),
        0
    );
    pair(&mut state, 2, 40, "alice", "bob", "0.5", "100");
    let bytes = state.snapshot_bytes().unwrap();
    assert!(matches!(
        EconomicHistory::from_state(&state),
        Err(EconomicReportingError::MissingOwnerEvidence)
    ));
    assert_eq!(state.snapshot_bytes().unwrap(), bytes);
}

#[test]
fn resting_maker_rebate_is_paid_only_from_funded_taker_fee() {
    let mut state = new();
    state
        .apply_block_unverified(2, vec![order(20, "alice", Side::Buy, "1.25", "100", 2)])
        .unwrap();
    assert_eq!(state.reserve_atoms(tx(20)), 1_250_000_000);
    let alice_before = state.ledger().available_atoms(property(), &a("alice"));
    let bob_before = state.ledger().available_atoms(property(), &a("bob"));
    assert_eq!(
        state
            .apply_block_unverified(3, vec![order(30, "bob", Side::Sell, "1.25", "100", 3)])
            .unwrap(),
        1
    );
    assert_eq!(state.reserve_atoms(tx(20)), 0);
    assert_eq!(
        state.ledger().available_atoms(property(), &a("alice")),
        alice_before + 3_125_000
    );
    assert_eq!(
        state.ledger().available_atoms(property(), &a("bob")),
        bob_before - 1_250_000_000 - 6_250_000
    );
    assert_eq!(state.margin_atoms(&a("alice")), 1_250_000_000);
    assert_eq!(state.margin_atoms(&a("bob")), 1_250_000_000);
    assert_eq!(state.receipts()[0].retained_fee_atoms, 3_125_000);
    assert_eq!(
        state
            .fees()
            .expected_custody_atoms(FeeRoute::Native(1).key(property()).unwrap())
            .unwrap(),
        3_125_000
    );
    assert_conserved(&state);
}

#[test]
fn deferred_profit_claim_is_paid_by_later_collected_loss_from_same_ledger() {
    let mut state = new();
    pair(&mut state, 2, 20, "alice", "bob", "1", "100");
    pair(&mut state, 3, 30, "carol", "alice", "0.5", "110");
    let key = IouKey::new(1, 7);
    assert_eq!(
        state.ious().claim_map(key).unwrap().claim(&a("alice")),
        Some(&d("5"))
    );
    assert_eq!(state.ious().get(key), d("-5"));
    assert_eq!(state.loss_pool_atoms(), 0);
    assert_eq!(state.receipts()[1].paid_claim_atoms, 0);
    let before = state.ledger().available_atoms(property(), &a("alice"));
    pair(&mut state, 4, 40, "bob", "dave", "0.5", "110");
    assert_eq!(state.receipts()[2].collected_loss_atoms, 5 * ATOMS);
    assert_eq!(state.receipts()[2].paid_claim_atoms, 5 * ATOMS);
    assert_eq!(
        state.ledger().available_atoms(property(), &a("alice")),
        before + 5 * ATOMS
    );
    assert!(state.ious().claim_map(key).unwrap().claims().is_empty());
    assert_eq!(state.ious().get(key), d("0"));
    assert_eq!(state.loss_pool_atoms(), 0);
    assert_eq!(
        state.positions().get(&a("alice")).unwrap().iou_claim,
        d("0")
    );
    state.apply_block_unverified(5, vec![]).unwrap();
    assert_eq!(
        state.ledger().available_atoms(property(), &a("alice")),
        before + 5 * ATOMS
    );
    assert_conserved(&state);
}

#[test]
fn unfunded_loss_never_creates_spendable_profit_or_reusable_pool() {
    let mut state = new();
    pair(&mut state, 2, 20, "alice", "bob", "1", "100");
    pair(&mut state, 3, 30, "carol", "alice", "0.5", "2100");
    let key = IouKey::new(1, 7);
    assert_eq!(
        state.ious().claim_map(key).unwrap().claim(&a("alice")),
        Some(&d("1000"))
    );
    pair(&mut state, 4, 40, "bob", "dave", "0.5", "4100");
    let receipt = &state.receipts()[2];
    assert_eq!(receipt.buyer_pnl, d("-2000"));
    assert!(receipt.collected_loss_atoms < 1000 * ATOMS);
    assert_eq!(
        receipt.collected_loss_atoms + receipt.unfunded_loss_atoms,
        2000 * ATOMS
    );
    assert_eq!(receipt.paid_claim_atoms, receipt.collected_loss_atoms);
    assert_eq!(state.loss_pool_atoms(), 0);
    assert!(
        state
            .ious()
            .claim_map(key)
            .unwrap()
            .claim(&a("alice"))
            .unwrap()
            > &d("0")
    );
    assert_eq!(state.margin_atoms(&a("bob")), 0);
    let alice_before = state.ledger().available_atoms(property(), &a("alice"));
    let claims_before = state.ious().clone();
    state.apply_block_unverified(5, vec![]).unwrap();
    assert_eq!(
        state.ledger().available_atoms(property(), &a("alice")),
        alice_before
    );
    assert_eq!(state.ious(), &claims_before);
    assert_conserved(&state);
}

#[test]
fn owner_cancellation_cannot_release_foreign_or_replayed_reserves() {
    let mut state = new();
    state
        .apply_block_unverified(2, vec![order(20, "alice", Side::Buy, "1.25", "100", 2)])
        .unwrap();
    let alice_before = state.ledger().available_atoms(property(), &a("alice"));
    let bob_before = state.ledger().available_atoms(property(), &a("bob"));
    state
        .apply_block_unverified(
            3,
            vec![EconomicInput::Cancel {
                txid: tx(30),
                owner: a("bob"),
                criteria: CancelCriteria::Txid(tx(20)),
            }],
        )
        .unwrap();
    assert_eq!(state.reserve_atoms(tx(20)), 1_250_000_000);
    assert_eq!(
        state.ledger().available_atoms(property(), &a("alice")),
        alice_before
    );
    assert_eq!(
        state.ledger().available_atoms(property(), &a("bob")),
        bob_before
    );
    state
        .apply_block_unverified(
            4,
            vec![EconomicInput::Cancel {
                txid: tx(40),
                owner: a("alice"),
                criteria: CancelCriteria::Txid(tx(20)),
            }],
        )
        .unwrap();
    assert!(state.book().resting().is_empty());
    assert_eq!(state.reserve_atoms(tx(20)), 0);
    assert_eq!(
        state.ledger().available_atoms(property(), &a("alice")),
        alice_before + 1_250_000_000
    );
    let before = state.clone();
    assert!(state
        .apply_block_unverified(5, vec![order(20, "alice", Side::Buy, "1.25", "100", 5)])
        .is_err());
    assert_eq!(state, before);
    assert_conserved(&state);
}

#[test]
fn last_fee_leg_failure_discards_prior_ordinary_send_match_and_replay_markers() {
    let mut state = new();
    state
        .apply_block_unverified(2, vec![order(20, "alice", Side::Buy, "1", "100", 2)])
        .unwrap();
    let before = state.clone();
    // poor can reserve the full ten-token margin, but has no spendable atoms
    // left for the taker fee. This fails after matching and margin transfers.
    assert!(state
        .apply_block_unverified(
            3,
            vec![
                send(30, "bob", "dave", 1),
                order(31, "poor", Side::Sell, "1", "100", 3)
            ]
        )
        .is_err());
    assert_eq!(state, before);
    assert!(!state.ledger().is_processed(tx(30)));
    assert!(!state.book().has_seen(tx(31)));
    assert!(state.receipts().is_empty());
    assert_eq!(state.reserve_atoms(tx(20)), 10 * ATOMS);
    // The same failed IDs can be retried with real funding in this staged block.
    assert_eq!(
        state
            .apply_block_unverified(
                3,
                vec![
                    send(30, "bob", "poor", 1),
                    order(31, "poor", Side::Sell, "1", "100", 3)
                ]
            )
            .unwrap(),
        1
    );
    assert_eq!(
        state.ledger().available_atoms(property(), &a("poor")),
        95_000_000
    );
    assert_eq!(state.margin_atoms(&a("poor")), 10 * ATOMS);
    assert_conserved(&state);
}

#[test]
fn one_replay_namespace_covers_ordinary_orders_and_cancellation_inputs() {
    let mut state = new();
    pair(&mut state, 2, 20, "alice", "bob", "1", "100");
    for input in [
        send(20, "alice", "bob", 1),
        EconomicInput::Cancel {
            txid: tx(21),
            owner: a("alice"),
            criteria: CancelCriteria::All,
        },
    ] {
        let before = state.clone();
        assert!(state.apply_block_unverified(3, vec![input]).is_err());
        assert_eq!(state, before);
    }
    state
        .apply_block_unverified(3, vec![send(30, "alice", "bob", 1)])
        .unwrap();
    let before = state.clone();
    assert!(state
        .apply_block_unverified(4, vec![order(30, "alice", Side::Buy, "1", "100", 4)])
        .is_err());
    assert_eq!(state, before);
    assert!(state.joint().snapshot_bytes().is_err());
    assert_conserved(&state);
}
