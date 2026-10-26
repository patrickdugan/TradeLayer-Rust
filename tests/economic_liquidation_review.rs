//! Independent funded-closeout regressions against the complete sole-Ledger owner.
use std::collections::BTreeSet;
use tradelayer_core::chain::{BlockHeader, BlockId};
use tradelayer_core::contract_orderbook::{Order, Side};
use tradelayer_core::contract_registry::ContractId;
use tradelayer_core::decimal::Decimal;
use tradelayer_core::economic::{
    EconomicConfig, EconomicInput, EconomicState, FundingConfig, FundingPolicy, LiquidationConfig,
    LiquidationEvent, LiquidationPolicy, LiquidationStatus, MarkConfig, MarkEventKind, MarkPolicy,
    SettlementPolicy,
};
use tradelayer_core::economic_chain::{
    EconomicChain, EconomicChainConfig, UnverifiedEconomicBlock,
};
use tradelayer_core::iou::IouKey;
use tradelayer_core::joint::{JointOperation, JointProtocolState, UnverifiedJointTx};
use tradelayer_core::ledger::{AccountId, Ledger, Operation, PropertyId, PropertyKind, TxId};
use tradelayer_core::network_profile::TradeLayerMainnet;
use tradelayer_core::registry::RegistryAction;
use tradelayer_core::tally::FeeRoute;

const ATOMS: u128 = 100_000_000;
fn d(value: &str) -> Decimal {
    value.parse().unwrap()
}
fn a(value: &str) -> AccountId {
    value.try_into().unwrap()
}
fn p() -> PropertyId {
    7.try_into().unwrap()
}
fn c() -> ContractId {
    1.try_into().unwrap()
}
fn id(number: u64) -> TxId {
    let mut bytes = [0; 32];
    bytes[24..].copy_from_slice(&number.to_be_bytes());
    TxId::from_bytes(bytes)
}
fn config(inverse: bool, funding_oracle: Option<u32>) -> EconomicConfig {
    EconomicConfig {
        source_consensus: None,
        contract_binding: None,
        network: TradeLayerMainnet::Litecoin,
        contract: c(),
        collateral: p(),
        inverse,
        notional: d(if inverse { "10000" } else { "1" }),
        initial_margin_per_contract: d("10"),
        fee_route: FeeRoute::Oracle(1),
        policy: SettlementPolicy::ConservingV1,
        funding: funding_oracle.map(|oracle_id| FundingConfig {
            oracle_id,
            max_staleness_blocks: 32,
            policy: FundingPolicy::ConservingFundingV1,
        }),
        marking: Some(MarkConfig {
            oracle_id: 1,
            policy: MarkPolicy::ConservingMarkBasisV1,
        }),
        liquidation: Some(LiquidationConfig {
            max_price_deviation_bps: 100,
            max_fills_per_owner: 64,
            policy: LiquidationPolicy::ConservingLiquidationV1,
        }),
        amm: None,
    }
}
fn ledger() -> Ledger {
    let mut ledger = Ledger::new();
    ledger
        .apply(
            id(1),
            Operation::CreateProperty {
                id: p(),
                issuer: a("alice"),
                kind: PropertyKind::Fixed,
                initial_atoms: 4000 * ATOMS,
            },
        )
        .unwrap();
    for (number, owner) in [(2, "bob"), (3, "carol"), (4, "dave")] {
        ledger
            .apply(
                id(number),
                Operation::Transfer {
                    id: p(),
                    from: a("alice"),
                    to: a(owner),
                    amount_atoms: 1000 * ATOMS,
                },
            )
            .unwrap();
    }
    ledger
}
fn joint(number: u64, actor: &str, operation: JointOperation) -> EconomicInput {
    EconomicInput::Joint(UnverifiedJointTx {
        txid: id(number),
        actor: a(actor),
        operation,
    })
}
fn activate() -> EconomicInput {
    joint(
        10,
        "admin",
        JointOperation::Registry(RegistryAction::Activate {
            types: vec![2, 6, 13, 14, 15, 18],
            code_hash: [0x76; 32],
        }),
    )
}
fn create(number: u64) -> EconomicInput {
    joint(
        number,
        "publisher",
        JointOperation::Registry(RegistryAction::CreateOracle {
            ticker: "MARK".into(),
            url: "https://example.invalid/mark".into(),
            backup: a("backup"),
            lag: 1,
        }),
    )
}
fn publish(number: u64, oracle_id: u32, price: u128) -> EconomicInput {
    joint(
        number,
        "publisher",
        JointOperation::Registry(RegistryAction::PublishOracle {
            oracle_id,
            price_1e4: price * 10_000,
            high_1e4: None,
            low_1e4: None,
            close_1e4: None,
        }),
    )
}
fn send(number: u64, from: &str, to: &str, atoms: u128) -> EconomicInput {
    joint(
        number,
        from,
        JointOperation::Send {
            id: p(),
            recipient: a(to),
            amount_atoms: atoms,
        },
    )
}
fn order(
    number: u64,
    owner: &str,
    side: Side,
    quantity: &str,
    price: &str,
    height: u64,
) -> EconomicInput {
    EconomicInput::Order(Order {
        txid: id(number),
        owner: a(owner),
        contract: c(),
        height,
        side,
        quantity: d(quantity),
        price: d(price),
        post: false,
        is_market: false,
    })
}
fn pair_inputs(
    height: u64,
    buyer: &str,
    seller: &str,
    quantity: &str,
    price: &str,
) -> Vec<EconomicInput> {
    vec![
        order(height * 100, buyer, Side::Buy, quantity, price, height),
        order(
            height * 100 + 1,
            seller,
            Side::Sell,
            quantity,
            price,
            height,
        ),
    ]
}
fn new(inverse: bool, funding_oracle: Option<u32>) -> EconomicState {
    let config = config(inverse, funding_oracle);
    let joint = JointProtocolState::from_bootstrap_unverified(
        config.network,
        a("admin"),
        ledger(),
        BTreeSet::from([p()]),
    )
    .unwrap();
    let mut state = EconomicState::from_joint_unverified(config, joint).unwrap();
    state.apply_block_unverified(1, vec![activate()]).unwrap();
    state
}
fn opened(inverse: bool) -> EconomicState {
    let mut state = new(inverse, None);
    let mut inputs = vec![create(11), publish(12, 1, 100)];
    inputs.extend(pair_inputs(2, "alice", "bob", "1", "100"));
    assert_eq!(state.apply_block_unverified(2, inputs).unwrap(), 1);
    assert_conserved(&state);
    state
}
fn available(state: &EconomicState, owner: &str) -> u128 {
    state.ledger().available_atoms(p(), &a(owner))
}
fn realized(state: &EconomicState, owner: &str) -> Decimal {
    state
        .positions()
        .get(&a(owner))
        .unwrap()
        .realized_pnl
        .clone()
}
fn assert_conserved(state: &EconomicState) {
    state.verify().unwrap();
    state.ledger().verify_conservation().unwrap();
    let available: u128 = ["alice", "bob", "carol", "dave"]
        .iter()
        .map(|owner| available(state, owner))
        .sum();
    let margin: u128 = state
        .positions()
        .positions()
        .map(|position| state.margin_atoms(&position.address))
        .sum();
    let reserves: u128 = state
        .book()
        .resting()
        .iter()
        .map(|order| state.reserve_atoms(order.order.txid))
        .sum();
    let fees = state
        .fees()
        .expected_custody_atoms(state.config().fee_route.key(p()).unwrap())
        .unwrap();
    assert_eq!(
        available
            + margin
            + reserves
            + fees
            + state.insurance_custody_atoms()
            + state.loss_pool_atoms(),
        4000 * ATOMS
    );
}
fn restored(state: &EconomicState) -> EconomicState {
    let bytes = state.snapshot_bytes().unwrap();
    assert_eq!(&bytes[..5], b"TLEC\x04");
    let restored = EconomicState::from_snapshot_bytes(
        &bytes,
        state.config(),
        &a("admin"),
        &BTreeSet::from([p()]),
    )
    .unwrap();
    assert_eq!(&restored, state);
    assert_eq!(restored.snapshot_bytes().unwrap(), bytes);
    assert_conserved(&restored);
    restored
}
fn through(state: &mut EconomicState, height: u64) {
    while state.finalized_height().unwrap() < height {
        state
            .apply_block_unverified(state.finalized_height().unwrap() + 1, vec![])
            .unwrap();
    }
}

fn closeout<'a>(state: &'a EconomicState, owner: &str) -> &'a LiquidationEvent {
    state
        .liquidation_state()
        .events
        .iter()
        .rev()
        .find(|event| event.owner == a(owner))
        .unwrap()
}
fn opened_with_makers(
    inverse: bool,
    makers: Vec<EconomicInput>,
    funding_oracle: Option<u32>,
    max_fills: u32,
) -> EconomicState {
    let mut config = config(inverse, funding_oracle);
    config.liquidation.as_mut().unwrap().max_fills_per_owner = max_fills;
    let joint = JointProtocolState::from_bootstrap_unverified(
        config.network,
        a("admin"),
        ledger(),
        BTreeSet::from([p()]),
    )
    .unwrap();
    let mut state = EconomicState::from_joint_unverified(config, joint).unwrap();
    state.apply_block_unverified(1, vec![activate()]).unwrap();
    let mut inputs = vec![create(11), publish(12, 1, 100)];
    inputs.extend(pair_inputs(2, "alice", "bob", "1", "100"));
    inputs.extend(makers);
    state.apply_block_unverified(2, inputs).unwrap();
    assert!(state.liquidation_state().events.is_empty());
    assert_conserved(&state);
    state
}

#[test]
fn complete_close_is_funded_reduce_only_with_zero_fee_at_zero_custody() {
    for (inverse, price, loss, default) in [(false, 120, "20", 10), (true, 200, "50", 40)] {
        let price_text = price.to_string();
        let maker = order(220, "carol", Side::Sell, "1", &price_text, 2);
        let mut state = opened_with_makers(inverse, vec![maker], None, 64);
        let bob = available(&state, "bob");
        let carol = available(&state, "carol");
        let receipts = state.receipts().to_vec();
        state
            .apply_block_unverified(3, vec![send(30, "bob", "dave", bob), publish(31, 1, price)])
            .unwrap();
        let event = closeout(&state, "bob");
        assert_eq!(event.status, LiquidationStatus::Closed);
        assert_eq!(event.quantity_before, d("-1"));
        assert_eq!(event.margin_before_atoms, 0);
        assert_eq!(event.top_up_atoms, 0);
        assert!(event.cancelled.is_empty());
        assert_eq!(event.fills.len(), 1);
        let fill = &event.fills[0];
        assert_eq!(fill.maker_before.order.txid, id(220));
        assert_eq!(fill.quantity, d("1"));
        assert_eq!(fill.price, d(&price_text));
        assert_eq!(fill.distressed_pnl, d("0"));
        assert_eq!(fill.maker_pnl, d("0"));
        assert!(fill.nominal_fee_atoms > 0);
        assert_eq!(fill.charged_fee_atoms, 0);
        assert_eq!(fill.maker_rebate_atoms, 0);
        assert_eq!(fill.retained_fee_atoms, 0);
        assert_eq!(state.receipts(), receipts);
        assert_eq!(state.positions().get(&a("bob")).unwrap().contracts, d("0"));
        assert_eq!(
            state.positions().get(&a("carol")).unwrap().contracts,
            d("-1")
        );
        assert_eq!(state.margin_atoms(&a("bob")), 0);
        assert_eq!(state.margin_atoms(&a("carol")), 10 * ATOMS);
        assert_eq!(available(&state, "carol"), carol);
        assert_eq!(realized(&state, "bob"), d(loss).negated());
        assert_eq!(state.unfunded_loss_atoms(), default * ATOMS);
        assert_eq!(state.mark_state().unpaid_losses[&a("bob")], default * ATOMS);
        assert_eq!(
            state
                .ious()
                .claim_map(IouKey::new(1, 7))
                .unwrap()
                .claim(&a("alice")),
            Some(&Decimal::from(default))
        );
        assert_eq!(state.reserve_atoms(id(220)), 0);
        assert!(!state.book().has_seen(event.id));
        assert!(
            matches!(&state.mark_state().events.last().unwrap().kind, MarkEventKind::Forced { epoch_id, maker_txid, distressed_owner, .. } if *epoch_id == event.id && *maker_txid == id(220) && *distressed_owner == a("bob"))
        );
        restored(&state);
    }
}

#[test]
fn partial_firm_fill_cures_real_maintenance_and_preserves_reciprocal_carry() {
    for inverse in [false, true] {
        let mut state = opened_with_makers(
            inverse,
            vec![order(220, "carol", Side::Sell, "0.5", "106", 2)],
            None,
            64,
        );
        let bob = available(&state, "bob");
        state
            .apply_block_unverified(3, vec![send(30, "bob", "dave", bob), publish(31, 1, 106)])
            .unwrap();
        let event = closeout(&state, "bob");
        assert_eq!(event.status, LiquidationStatus::Cured);
        assert_eq!(event.fills.len(), 1);
        assert_eq!(event.fills[0].quantity, d("0.5"));
        assert_eq!(event.fills[0].distressed_pnl, d("0"));
        assert_eq!(
            event.fills[0].charged_fee_atoms,
            event.fills[0].nominal_fee_atoms
        );
        assert_eq!(
            event.fills[0].maker_rebate_atoms,
            event.fills[0].charged_fee_atoms / 2
        );
        assert_eq!(
            event.fills[0].retained_fee_atoms,
            event.fills[0].charged_fee_atoms - event.fills[0].maker_rebate_atoms
        );
        assert_eq!(state.mark_state().bases[&a("bob")].quantity, d("-0.5"));
        assert!(state.margin_atoms(&a("bob")) >= 5 * ATOMS / 2);
        assert!(state
            .mark_state()
            .maintenance
            .iter()
            .all(|call| call.owner != a("bob")));
        assert_eq!(state.receipts().len(), 1);
        if inverse {
            assert_eq!(realized(&state, "bob"), d("-5.66037735"));
            assert!(!state.mark_state().bases[&a("bob")].carry.is_zero());
        } else {
            assert_eq!(realized(&state, "bob"), d("-6"));
        }
        restored(&state);
    }
}

#[test]
fn ordinary_trading_after_a_forced_fill_keeps_fee_and_basis_journals_in_order() {
    let mut state = opened_with_makers(
        false,
        vec![order(220, "carol", Side::Sell, "0.5", "106", 2)],
        None,
        64,
    );
    let bob = available(&state, "bob");
    state
        .apply_block_unverified(3, vec![send(30, "bob", "dave", bob), publish(31, 1, 106)])
        .unwrap();
    assert_eq!(closeout(&state, "bob").status, LiquidationStatus::Cured);
    let forced = state.liquidation_state().events.clone();
    let mut state = restored(&state);
    state
        .apply_block_unverified(4, pair_inputs(4, "carol", "alice", "0.25", "107"))
        .unwrap();
    assert_eq!(state.receipts().len(), 2);
    assert_eq!(state.receipts()[1].buyer_pnl, d("-0.25"));
    assert_eq!(state.receipts()[1].seller_pnl, d("0.25"));
    assert_eq!(state.liquidation_state().events, forced);
    assert_eq!(state.mark_state().bases[&a("alice")].quantity, d("0.75"));
    assert_eq!(state.mark_state().bases[&a("carol")].quantity, d("-0.25"));
    assert_eq!(state.mark_state().bases[&a("bob")].quantity, d("-0.5"));
    restored(&state);
}

#[test]
fn cancelling_real_owned_reserve_can_cure_without_a_forced_trade_or_self_fill() {
    let mut state = opened(false);
    state
        .apply_block_unverified(3, vec![order(300, "bob", Side::Sell, "1", "500", 3)])
        .unwrap();
    assert_eq!(state.reserve_atoms(id(300)), 10 * ATOMS);
    let bob = available(&state, "bob");
    state
        .apply_block_unverified(4, vec![send(40, "bob", "dave", bob), publish(41, 1, 106)])
        .unwrap();
    let event = closeout(&state, "bob");
    assert_eq!(event.status, LiquidationStatus::Cured);
    assert_eq!(event.cancelled.len(), 1);
    assert_eq!(event.cancelled[0].order.order.txid, id(300));
    assert_eq!(event.cancelled[0].reserve_atoms, 10 * ATOMS);
    assert_eq!(event.top_up_atoms, 6 * ATOMS);
    assert!(event.fills.is_empty());
    assert_eq!(state.reserve_atoms(id(300)), 0);
    assert_eq!(state.margin_atoms(&a("bob")), 10 * ATOMS);
    assert_eq!(available(&state, "bob"), 4 * ATOMS);
    assert_eq!(state.mark_state().bases[&a("bob")].quantity, d("-1"));
    assert!(state.book().resting().is_empty());
    assert_eq!(state.receipts().len(), 1);
    restored(&state);
}

#[test]
fn own_liquidity_is_cancelled_and_never_used_to_close_the_same_owner() {
    let mut state = opened_with_makers(
        false,
        vec![order(220, "bob", Side::Sell, "0.1", "120", 2)],
        None,
        64,
    );
    let bob = available(&state, "bob");
    state
        .apply_block_unverified(3, vec![send(30, "bob", "dave", bob), publish(31, 1, 120)])
        .unwrap();
    let event = closeout(&state, "bob");
    assert_eq!(event.status, LiquidationStatus::NoLiquidityWithinGuard);
    assert_eq!(event.cancelled.len(), 1);
    assert_eq!(event.cancelled[0].reserve_atoms, ATOMS);
    assert_eq!(event.top_up_atoms, ATOMS);
    assert!(event.fills.is_empty());
    assert_eq!(state.margin_atoms(&a("bob")), ATOMS);
    assert_eq!(state.mark_state().bases[&a("bob")].quantity, d("-1"));
    assert_eq!(state.receipts().len(), 1);
    restored(&state);
}

#[test]
fn guard_is_inclusive_and_an_outside_best_price_cannot_be_consumed() {
    for (price, status) in [
        ("121.2", LiquidationStatus::Closed),
        ("121.20000001", LiquidationStatus::NoLiquidityWithinGuard),
    ] {
        let mut state = opened_with_makers(
            false,
            vec![order(220, "carol", Side::Sell, "1", price, 2)],
            None,
            64,
        );
        let bob = available(&state, "bob");
        state
            .apply_block_unverified(3, vec![send(30, "bob", "dave", bob), publish(31, 1, 120)])
            .unwrap();
        let event = closeout(&state, "bob");
        assert_eq!(event.guard_price, d("121.2"));
        assert_eq!(event.status, status);
        assert_eq!(
            event.fills.len(),
            usize::from(status == LiquidationStatus::Closed)
        );
        if status == LiquidationStatus::NoLiquidityWithinGuard {
            assert_eq!(state.reserve_atoms(id(220)), 10 * ATOMS);
            assert_eq!(state.book().resting()[0].remaining, d("1"));
            assert_eq!(state.mark_state().bases[&a("bob")].quantity, d("-1"));
        }
        restored(&state);
    }
}

#[test]
fn price_priority_and_fill_limit_preserve_unconsumed_worse_liquidity() {
    let mut state = opened_with_makers(
        false,
        vec![
            order(220, "carol", Side::Sell, "0.1", "106", 2),
            order(221, "dave", Side::Sell, "0.1", "105", 2),
        ],
        None,
        1,
    );
    let bob = available(&state, "bob");
    state
        .apply_block_unverified(3, vec![send(30, "bob", "dave", bob), publish(31, 1, 106)])
        .unwrap();
    let event = closeout(&state, "bob");
    assert_eq!(event.status, LiquidationStatus::FillLimit);
    assert_eq!(event.fills.len(), 1);
    let fill = &event.fills[0];
    assert_eq!(fill.maker_before.order.txid, id(221));
    assert_eq!(fill.quantity, d("0.1"));
    assert_eq!(fill.price, d("105"));
    assert_eq!(fill.nominal_fee_atoms, 525_000);
    assert_eq!(fill.charged_fee_atoms, 525_000);
    assert_eq!(fill.maker_rebate_atoms, 262_500);
    assert_eq!(fill.retained_fee_atoms, 262_500);
    assert_eq!(state.reserve_atoms(id(220)), ATOMS);
    assert_eq!(state.reserve_atoms(id(221)), 0);
    assert_eq!(state.mark_state().bases[&a("bob")].quantity, d("-0.9"));
    assert!(state
        .mark_state()
        .maintenance
        .iter()
        .any(|call| call.owner == a("bob")));
    restored(&state);
}

#[test]
fn unfunded_best_maker_stops_before_worse_prices_and_is_not_silently_skipped() {
    let mut state = opened(false);
    state
        .apply_block_unverified(3, pair_inputs(3, "alice", "carol", "1", "100"))
        .unwrap();
    state
        .apply_block_unverified(
            4,
            vec![
                order(400, "carol", Side::Sell, "1", "120", 4),
                order(401, "dave", Side::Sell, "1", "121", 4),
            ],
        )
        .unwrap();
    let bob = available(&state, "bob");
    let carol = available(&state, "carol");
    state
        .apply_block_unverified(
            5,
            vec![
                send(50, "bob", "dave", bob),
                send(51, "carol", "dave", carol),
                publish(52, 1, 120),
            ],
        )
        .unwrap();
    let event = closeout(&state, "bob");
    assert_eq!(event.status, LiquidationStatus::UnfundedBestMaker);
    assert!(event.fills.is_empty());
    let blocked = event.blocked_maker.as_ref().unwrap();
    assert_eq!(blocked.order.order.txid, id(400));
    assert_eq!(blocked.available_atoms, 0);
    assert_eq!(blocked.margin_atoms, 0);
    assert_eq!(state.reserve_atoms(id(401)), 10 * ATOMS);
    assert_eq!(state.mark_state().bases[&a("bob")].quantity, d("-1"));
    // Carol is visited later in canonical owner order and can cure from her
    // own refunded reserve. Bob's earlier stop is not retried retroactively.
    assert_eq!(closeout(&state, "carol").status, LiquidationStatus::Cured);
    assert_eq!(
        closeout(&state, "carol").cancelled[0].order.order.txid,
        id(400)
    );
    assert_eq!(
        state
            .liquidation_state()
            .events
            .iter()
            .map(|event| event.owner.clone())
            .collect::<Vec<_>>(),
        [a("bob"), a("carol")]
    );
    restored(&state);
}

#[test]
fn fee_is_capped_to_actual_atoms_and_rebate_uses_only_the_collected_amount() {
    for kept_atoms in [0, 1] {
        let mut state = opened_with_makers(
            false,
            vec![order(220, "carol", Side::Sell, "1", "109.9999", 2)],
            None,
            64,
        );
        let bob = available(&state, "bob");
        let carol = available(&state, "carol");
        let fractional_mark = joint(
            31,
            "publisher",
            JointOperation::Registry(RegistryAction::PublishOracle {
                oracle_id: 1,
                price_1e4: 1_099_999,
                high_1e4: None,
                low_1e4: None,
                close_1e4: None,
            }),
        );
        state
            .apply_block_unverified(
                3,
                vec![send(30, "bob", "dave", bob - kept_atoms), fractional_mark],
            )
            .unwrap();
        let event = closeout(&state, "bob");
        assert_eq!(event.status, LiquidationStatus::Closed);
        let fill = &event.fills[0];
        assert_eq!(fill.nominal_fee_atoms, 5_499_996);
        assert_eq!(fill.charged_fee_atoms, 10_000 + kept_atoms);
        assert_eq!(fill.maker_rebate_atoms, 5_000);
        assert_eq!(fill.retained_fee_atoms, 5_000 + kept_atoms);
        assert_eq!(available(&state, "bob"), 0);
        assert_eq!(state.margin_atoms(&a("bob")), 0);
        assert_eq!(available(&state, "carol"), carol + 5_000);
        assert_eq!(state.unfunded_loss_atoms(), 0);
        restored(&state);
    }
}

#[test]
fn forced_fee_debits_released_available_atoms_before_remaining_margin() {
    let mut state = opened_with_makers(
        false,
        vec![order(220, "carol", Side::Sell, "0.603", "106", 2)],
        None,
        64,
    );
    let bob = available(&state, "bob");
    state
        .apply_block_unverified(3, vec![send(30, "bob", "dave", bob), publish(31, 1, 106)])
        .unwrap();
    let event = closeout(&state, "bob");
    assert_eq!(event.status, LiquidationStatus::Cured);
    let fill = &event.fills[0];
    assert_eq!(fill.quantity, d("0.603"));
    assert_eq!(fill.distressed_available_before_fee_atoms, 3_000_000);
    assert_eq!(fill.distressed_margin_before_fee_atoms, 397_000_000);
    assert_eq!(fill.charged_fee_atoms, 3_195_900);
    assert_eq!(available(&state, "bob"), 0);
    assert_eq!(state.margin_atoms(&a("bob")), 396_804_100);
    assert_eq!(state.mark_state().bases[&a("bob")].quantity, d("-0.397"));
    restored(&state);
}

#[test]
fn no_liquidity_waits_for_a_fresh_mark_and_a_closed_oracle_does_not_repeat_it() {
    let mut state = opened(false);
    let bob = available(&state, "bob");
    state
        .apply_block_unverified(3, vec![send(30, "bob", "dave", bob), publish(31, 1, 106)])
        .unwrap();
    assert_eq!(
        closeout(&state, "bob").status,
        LiquidationStatus::NoLiquidityWithinGuard
    );
    let first = state.liquidation_state().events.len();
    state.apply_block_unverified(4, vec![]).unwrap();
    assert_eq!(state.liquidation_state().events.len(), first);
    state
        .apply_block_unverified(5, vec![publish(50, 1, 106)])
        .unwrap();
    assert_eq!(state.liquidation_state().events.len(), first + 1);
    state
        .apply_block_unverified(
            6,
            vec![
                publish(60, 1, 106),
                joint(
                    61,
                    "publisher",
                    JointOperation::Registry(RegistryAction::CloseOracle { oracle_id: 1 }),
                ),
            ],
        )
        .unwrap();
    assert_eq!(state.liquidation_state().events.len(), first + 1);
    restored(&state);
}

#[test]
fn mark_funding_and_closeout_share_one_height_without_id_or_custody_aliases() {
    let mut state = opened_with_makers(
        false,
        vec![order(220, "carol", Side::Sell, "0.5", "106", 2)],
        Some(1),
        64,
    );
    through(&mut state, 23);
    let bob = available(&state, "bob");
    state
        .apply_block_unverified(
            24,
            vec![send(240, "bob", "dave", bob), publish(241, 1, 106)],
        )
        .unwrap();
    let event = closeout(&state, "bob");
    let funding = &state.funding_state().epochs[0];
    let credit = funding
        .plan
        .as_ref()
        .unwrap()
        .credits
        .iter()
        .find(|payment| payment.owner == a("bob"))
        .unwrap()
        .atoms;
    assert_eq!(event.height, 24);
    assert_eq!(funding.height, 24);
    assert_eq!(event.available_before_atoms, credit);
    assert_eq!(event.top_up_atoms, credit);
    assert_eq!(event.status, LiquidationStatus::Cured);
    assert_ne!(event.id, event.mark_id);
    assert_ne!(event.id, funding.id);
    assert_ne!(event.mark_id, funding.id);
    let reserved_ids = [event.id, event.mark_id, funding.id];
    let state = restored(&state);
    assert_eq!(state.receipts().len(), 1);
    for reserved in reserved_ids {
        let mut clone = state.clone();
        let bytes = clone.snapshot_bytes().unwrap();
        assert!(clone
            .apply_block_unverified(
                25,
                vec![EconomicInput::Joint(UnverifiedJointTx {
                    txid: reserved,
                    actor: a("alice"),
                    operation: JointOperation::Send {
                        id: p(),
                        recipient: a("dave"),
                        amount_atoms: 1
                    },
                })]
            )
            .is_err());
        assert_eq!(clone.snapshot_bytes().unwrap(), bytes);
    }
}

#[test]
fn late_forced_loss_overflow_reverts_real_maker_consumption_mark_and_ordinary_send() {
    let mark_price = 3_400_000_000_000_000_000_000_000_000_000_u128;
    let maker_price = "3434000000000000000000000000000";
    let build = |liquidating: bool| {
        let mut config = config(false, None);
        if !liquidating {
            config.liquidation = None;
        }
        let joint = JointProtocolState::from_bootstrap_unverified(
            config.network,
            a("admin"),
            ledger(),
            BTreeSet::from([p()]),
        )
        .unwrap();
        let mut state = EconomicState::from_joint_unverified(config, joint).unwrap();
        state.apply_block_unverified(1, vec![activate()]).unwrap();
        let mut inputs = vec![create(11), publish(12, 1, 100)];
        inputs.extend(pair_inputs(2, "alice", "bob", "1", "100"));
        inputs.push(order(220, "carol", Side::Sell, "1", maker_price, 2));
        state.apply_block_unverified(2, inputs).unwrap();
        state
    };
    let mut control = build(false);
    let mut liquidating = build(true);
    let bob = available(&liquidating, "bob");
    let inputs = vec![send(30, "bob", "dave", bob), publish(31, 1, mark_price)];
    // The identical mark, nominal default and ordinary send fit by themselves.
    control.apply_block_unverified(3, inputs.clone()).unwrap();
    control.verify().unwrap();
    let before = liquidating.clone();
    let bytes = liquidating.snapshot_bytes().unwrap();
    // Only the guarded forced fill's additional slippage default overflows the
    // cumulative u128 counter, after maker reserve/basis/fee work was staged.
    assert!(liquidating.apply_block_unverified(3, inputs).is_err());
    assert_eq!(liquidating, before);
    assert_eq!(liquidating.snapshot_bytes().unwrap(), bytes);
    assert_eq!(liquidating.reserve_atoms(id(220)), 10 * ATOMS);
    assert!(!liquidating.ledger().is_processed(id(30)));
    assert!(!liquidating.joint().registry().is_processed(id(31)));
    assert!(liquidating.liquidation_state().events.is_empty());
}

#[test]
fn complete_chain_rollback_removes_closeout_and_allows_an_alternate_fork() {
    let economics = config(false, None);
    let chain_config = EconomicChainConfig {
        economics: economics.clone(),
        admin: a("admin"),
        permitted_spot_properties: BTreeSet::from([p()]),
        rollback_limit: 4,
    };
    let mut chain = EconomicChain::new_unverified(chain_config, ledger()).unwrap();
    let block = |height, parent, number: u8, inputs| UnverifiedEconomicBlock {
        network: economics.network,
        header: BlockHeader {
            height,
            parent,
            id: BlockId([number; 32]),
        },
        inputs,
    };
    chain
        .apply_unverified_block(&block(1, chain.tip().id, 1, vec![activate()]))
        .unwrap();
    let mut inputs = vec![create(11), publish(12, 1, 100)];
    inputs.extend(pair_inputs(2, "alice", "bob", "1", "100"));
    inputs.push(order(220, "carol", Side::Sell, "1", "120", 2));
    chain
        .apply_unverified_block(&block(2, chain.tip().id, 2, inputs))
        .unwrap();
    let before = chain.clone();
    let bob = available(chain.state(), "bob");
    chain
        .apply_unverified_block(&block(
            3,
            chain.tip().id,
            3,
            vec![send(30, "bob", "dave", bob), publish(31, 1, 120)],
        ))
        .unwrap();
    assert_eq!(
        closeout(chain.state(), "bob").status,
        LiquidationStatus::Closed
    );
    chain.rollback_tip(chain.tip()).unwrap();
    assert_eq!(chain, before);
    assert!(chain.state().liquidation_state().events.is_empty());
    assert_eq!(chain.state().reserve_atoms(id(220)), 10 * ATOMS);
    chain
        .apply_unverified_block(&block(
            3,
            chain.tip().id,
            4,
            vec![send(30, "bob", "dave", bob), publish(31, 1, 106)],
        ))
        .unwrap();
    assert_eq!(
        closeout(chain.state(), "bob").status,
        LiquidationStatus::NoLiquidityWithinGuard
    );
    assert_eq!(
        chain.state().mark_state().bases[&a("bob")].quantity,
        d("-1")
    );
    restored(chain.state());
}
