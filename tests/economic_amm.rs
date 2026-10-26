//! Funded AMM custody, LP lifecycle and true pool-counterparty execution.
use std::collections::BTreeSet;
use tradelayer_core::chain::{BlockHeader, BlockId};
use tradelayer_core::contract_orderbook::{CancelCriteria, Order, Side};
use tradelayer_core::contract_registry::ContractId;
use tradelayer_core::decimal::Decimal;
use tradelayer_core::economic::{
    AmmCapitalInput, EconomicConfig, EconomicInput, EconomicState, FundedAmmConfig,
    FundedAmmPolicy, FundingConfig, FundingPolicy, LiquidationConfig, LiquidationEvent,
    LiquidationPolicy, LiquidationStatus, MarkConfig, MarkEventKind, MarkPolicy, SettlementPolicy,
};
use tradelayer_core::economic_chain::{
    EconomicChain, EconomicChainConfig, UnverifiedEconomicBlock,
};

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
        amm: Some(FundedAmmConfig {
            pool_id: 7,
            spread_bps: 40,
            max_quote_quantity: d("0.5"),
            max_abs_inventory: d("2"),
            max_collateral_atoms: 1000 * ATOMS,
            policy: FundedAmmPolicy::ConservingFundedContractAmmV1,
        }),
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
            types: vec![2, 6, 10, 13, 14, 15, 18],
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
    let report = tradelayer_core::economic_reporting::EconomicHistory::from_state(state).unwrap();
    for position in state.positions().positions() {
        let snapshots = report
            .history()
            .position_history(&position.address, c().get())
            .unwrap();
        let quantity = snapshots
            .last()
            .map_or_else(Decimal::zero, |snapshot| snapshot.effect.after.clone());
        assert_eq!(quantity, position.contracts);
    }
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
            + state.amm_state().cash_atoms
            + state
                .amm_state()
                .quotes
                .iter()
                .map(|quote| quote.reserve_atoms)
                .sum::<u128>()
            + reserves
            + fees
            + state.insurance_custody_atoms()
            + state.loss_pool_atoms(),
        4000 * ATOMS
    );
    assert_eq!(
        state.amm_state().cash_atoms,
        state
            .ledger()
            .available_atoms(p(), &state.amm_owner().unwrap())
    );
}
fn restored(state: &EconomicState) -> EconomicState {
    let bytes = state.snapshot_bytes().unwrap();
    assert_eq!(&bytes[..5], b"TLEC\x05");
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

fn deposit(number: u64, owner: &str, atoms: u128, minimum: u128) -> EconomicInput {
    EconomicInput::Amm(AmmCapitalInput::Deposit {
        txid: id(number),
        owner: a(owner),
        collateral_atoms: atoms,
        min_share_units: minimum,
    })
}
fn redeem(number: u64, owner: &str, units: u128, minimum: u128) -> EconomicInput {
    EconomicInput::Amm(AmmCapitalInput::Redeem {
        txid: id(number),
        owner: a(owner),
        share_units: units,
        min_collateral_atoms: minimum,
    })
}
fn pool_open(inverse: bool, makers: Vec<EconomicInput>) -> EconomicState {
    let mut state = new(inverse, None);
    let mut inputs = vec![
        create(11),
        publish(12, 1, 100),
        deposit(13, "dave", 100 * ATOMS, 100 * ATOMS),
    ];
    inputs.extend(pair_inputs(2, "alice", "bob", "1", "100"));
    inputs.extend(makers);
    state.apply_block_unverified(2, inputs).unwrap();
    assert_conserved(&state);
    state
}
fn unchanged_error(state: &mut EconomicState, inputs: Vec<EconomicInput>) {
    let before = state.clone();
    let bytes = state.snapshot_bytes().unwrap();
    assert!(state
        .apply_block_unverified(state.finalized_height().unwrap() + 1, inputs)
        .is_err());
    assert_eq!(*state, before);
    assert_eq!(state.snapshot_bytes().unwrap(), bytes);
}

#[test]
fn lp_deposits_redemptions_minimums_and_global_replay_move_actual_custody() {
    let mut state = new(false, None);
    state
        .apply_block_unverified(
            2,
            vec![
                deposit(20, "carol", 100 * ATOMS, 100 * ATOMS),
                deposit(21, "dave", 50 * ATOMS, 50 * ATOMS),
            ],
        )
        .unwrap();
    let pool = state.amm_owner().unwrap();
    assert_eq!(available(&state, "carol"), 900 * ATOMS);
    assert_eq!(available(&state, "dave"), 950 * ATOMS);
    assert_eq!(state.ledger().available_atoms(p(), &pool), 150 * ATOMS);
    assert_eq!(state.amm_state().cash_atoms, 150 * ATOMS);
    assert_eq!(state.amm_state().total_units, 150 * ATOMS);
    assert_eq!(state.amm_state().shares[&a("carol")], 100 * ATOMS);
    assert_eq!(
        state
            .amm_state()
            .frames
            .last()
            .unwrap()
            .capital_receipts
            .len(),
        2
    );
    let mut state = restored(&state);
    unchanged_error(&mut state, vec![deposit(20, "carol", ATOMS, 1)]);
    unchanged_error(&mut state, vec![deposit(30, "carol", ATOMS, ATOMS + 1)]);
    unchanged_error(&mut state, vec![redeem(31, "alice", 1, 0)]);
    unchanged_error(&mut state, vec![redeem(32, "dave", 50 * ATOMS + 1, 0)]);
    unchanged_error(
        &mut state,
        vec![redeem(33, "carol", 40 * ATOMS, 40 * ATOMS + 1)],
    );
    state
        .apply_block_unverified(3, vec![redeem(33, "carol", 40 * ATOMS, 40 * ATOMS)])
        .unwrap();
    assert_eq!(available(&state, "carol"), 940 * ATOMS);
    assert_eq!(state.amm_state().shares[&a("carol")], 60 * ATOMS);
    state
        .apply_block_unverified(
            4,
            vec![
                redeem(40, "carol", 60 * ATOMS, 0),
                redeem(41, "dave", 50 * ATOMS, 0),
            ],
        )
        .unwrap();
    assert_eq!(state.amm_state().total_units, 0);
    assert!(state.amm_state().shares.is_empty());
    assert_eq!(state.amm_state().cash_atoms, 0);
    assert_eq!(available(&state, "carol"), 1000 * ATOMS);
    assert_eq!(available(&state, "dave"), 1000 * ATOMS);
    restored(&state);
}

#[test]
fn capital_prefix_and_limits_fail_atomically_without_debiting_or_leaving_replay_ids() {
    let mut state = new(false, None);
    unchanged_error(
        &mut state,
        vec![
            deposit(20, "carol", 100 * ATOMS, 1),
            deposit(21, "dave", 0, 0),
        ],
    );
    assert_eq!(available(&state, "carol"), 1000 * ATOMS);
    assert!(state.amm_state().shares.is_empty());
    state
        .apply_block_unverified(2, vec![deposit(20, "carol", 100 * ATOMS, 1)])
        .unwrap();
    unchanged_error(&mut state, vec![deposit(30, "dave", 901 * ATOMS, 0)]);
    unchanged_error(&mut state, vec![redeem(31, "carol", 0, 0)]);
    state
        .apply_block_unverified(3, vec![deposit(30, "dave", ATOMS, ATOMS)])
        .unwrap();
    assert_eq!(state.amm_state().total_units, 101 * ATOMS);
    restored(&state);
}

#[test]
fn quotes_lock_real_half_budgets_then_next_frame_refunds_before_flat_redemption() {
    let mut state = new(false, None);
    state
        .apply_block_unverified(
            2,
            vec![
                deposit(20, "dave", 10 * ATOMS, 1),
                create(11),
                publish(12, 1, 100),
            ],
        )
        .unwrap();
    assert_eq!(state.amm_state().quotes.len(), 2);
    assert_eq!(state.amm_state().cash_atoms, 0);
    let epoch = state
        .amm_state()
        .frames
        .last()
        .unwrap()
        .quote_epoch
        .as_ref()
        .unwrap();
    assert_eq!(epoch.cash_before_atoms, 10 * ATOMS);
    assert_eq!(epoch.quotes[0].remaining, d("0.5"));
    assert_eq!(epoch.quotes[0].reserve_atoms, 5 * ATOMS);
    assert_eq!(epoch.quotes[1].reserve_atoms, 5 * ATOMS);
    assert_eq!(state.amm_state().total_units, 10 * ATOMS);
    let mut state = restored(&state);
    state
        .apply_block_unverified(3, vec![redeem(30, "dave", 10 * ATOMS, 10 * ATOMS)])
        .unwrap();
    assert_eq!(
        state
            .amm_state()
            .frames
            .last()
            .unwrap()
            .released_quotes
            .len(),
        2
    );
    assert!(state.amm_state().quotes.is_empty());
    assert_eq!(state.amm_state().cash_atoms, 0);
    assert_eq!(available(&state, "dave"), 1000 * ATOMS);
    restored(&state);
}

#[test]
fn actual_pool_fills_before_better_firm_book_on_both_sides_and_inverse_contracts() {
    for (inverse, short, price, better, quoted) in [
        (false, true, 120, "119", "120.48"),
        (false, false, 80, "81", "79.68"),
        (true, true, 200, "199", "200.8"),
        (true, false, 80, "81", "79.68"),
    ] {
        let owner = if short { "bob" } else { "alice" };
        let maker_side = if short { Side::Sell } else { Side::Buy };
        let mut state = pool_open(
            inverse,
            vec![order(220, "carol", maker_side, "1", better, 2)],
        );
        let amount = available(&state, owner);
        state
            .apply_block_unverified(
                3,
                vec![send(30, owner, "dave", amount), publish(31, 1, price)],
            )
            .unwrap();
        let event = closeout(&state, owner);
        assert_eq!(event.status, LiquidationStatus::Closed);
        assert_eq!(event.amm_fills.len(), 1);
        assert_eq!(event.fills.len(), 1);
        assert_eq!(event.amm_fills[0].quantity, d("0.5"));
        assert_eq!(event.amm_fills[0].price, d(quoted));
        assert_eq!(event.fills[0].quantity, d("0.5"));
        assert_eq!(event.fills[0].price, d(better));
        assert_eq!(event.amm_fills[0].charged_fee_atoms, 0);
        assert_eq!(event.amm_fills[0].maker_rebate_atoms, 0);
        let pool = state.amm_owner().unwrap();
        assert_eq!(
            state.positions().get(&pool).unwrap().contracts,
            d(if short { "-0.5" } else { "0.5" })
        );
        assert_eq!(state.margin_atoms(&pool), 5 * ATOMS);
        assert_eq!(state.amm_state().total_units, 100 * ATOMS);
        assert_eq!(state.amm_state().cash_atoms, 90 * ATOMS);
        assert_eq!(state.ledger().available_atoms(p(), &pool), 90 * ATOMS);
        assert_eq!(state.reserve_atoms(id(220)), 5 * ATOMS);
        assert_eq!(state.receipts().len(), 1);
        let actions: Vec<_> = state
            .mark_state()
            .events
            .iter()
            .filter(|entry| entry.height == 3)
            .map(|entry| &entry.kind)
            .collect();
        assert!(matches!(actions[0], MarkEventKind::Mark { .. }));
        assert!(matches!(actions[1], MarkEventKind::AmmForced { .. }));
        assert!(matches!(actions[2], MarkEventKind::Forced { .. }));
        let report =
            tradelayer_core::economic_reporting::EconomicHistory::from_state(&state).unwrap();
        let fills = report.history().executions().collect::<Vec<_>>();
        assert_eq!(fills.len(), 3);
        assert!(matches!(
            fills[0].origin,
            tradelayer_core::trade_history::ContractEventOrigin::Orders { .. }
        ));
        assert!(matches!(
            fills[1].origin,
            tradelayer_core::trade_history::ContractEventOrigin::ForcedAmm { pool_id: 7, .. }
        ));
        assert!(
            matches!(fills[2].origin,tradelayer_core::trade_history::ContractEventOrigin::ForcedBook{maker_txid,..} if maker_txid==id(220))
        );
        let counterparties = [&fills[1].buyer, &fills[1].seller];
        assert!(counterparties.contains(&&pool));
        assert!(report
            .history()
            .transaction_references(&pool, c().get())
            .unwrap()
            .iter()
            .all(|r| r.participant_order_txid.is_none()));
        let bytes = report.history().encode_checkpoint().unwrap();
        assert_eq!(&bytes[..5], b"TLTH\x02");
        assert_eq!(
            tradelayer_core::trade_history::TradeHistory::decode_checkpoint(&bytes).unwrap(),
            *report.history()
        );
        restored(&state);
    }
}

#[test]
fn unsafe_amm_quotes_remain_funded_but_fall_back_to_a_safe_real_maker() {
    let mut cfg = config(false, None);
    cfg.amm.as_mut().unwrap().spread_bps = 200;
    let joint = JointProtocolState::from_bootstrap_unverified(
        cfg.network,
        a("admin"),
        ledger(),
        BTreeSet::from([p()]),
    )
    .unwrap();
    let mut state = EconomicState::from_joint_unverified(cfg, joint).unwrap();
    state.apply_block_unverified(1, vec![activate()]).unwrap();
    let mut inputs = vec![
        create(11),
        publish(12, 1, 100),
        deposit(13, "dave", 100 * ATOMS, 0),
        order(220, "carol", Side::Sell, "1", "120.5", 2),
    ];
    inputs.extend(pair_inputs(2, "alice", "bob", "1", "100"));
    state.apply_block_unverified(2, inputs).unwrap();
    let bob = available(&state, "bob");
    state
        .apply_block_unverified(3, vec![send(30, "bob", "dave", bob), publish(31, 1, 120)])
        .unwrap();
    let event = closeout(&state, "bob");
    assert!(event.amm_fills.is_empty());
    assert_eq!(event.fills.len(), 1);
    assert_eq!(event.status, LiquidationStatus::Closed);
    assert_eq!(state.amm_state().quotes.len(), 2);
    assert_eq!(
        state
            .amm_state()
            .quotes
            .iter()
            .map(|q| q.reserve_atoms)
            .sum::<u128>(),
        10 * ATOMS
    );
    assert!(state.positions().get(&state.amm_owner().unwrap()).is_none());
    restored(&state);
}

#[test]
fn pool_without_actual_cash_has_an_empty_epoch_and_never_becomes_a_counterparty() {
    let mut state = opened(false);
    let bob = available(&state, "bob");
    state
        .apply_block_unverified(3, vec![send(30, "bob", "dave", bob), publish(31, 1, 120)])
        .unwrap();
    assert!(state
        .amm_state()
        .frames
        .last()
        .unwrap()
        .quote_epoch
        .as_ref()
        .unwrap()
        .quotes
        .is_empty());
    assert!(state.amm_state().quotes.is_empty());
    assert_eq!(
        closeout(&state, "bob").status,
        LiquidationStatus::NoLiquidityWithinGuard
    );
    assert!(closeout(&state, "bob").amm_fills.is_empty());
    assert_eq!(state.amm_state().cash_atoms, 0);
    restored(&state);
}

#[test]
fn live_pool_risk_blocks_lp_deposit_and_redemption_atomically() {
    let mut state = pool_open(false, vec![order(220, "carol", Side::Sell, "1", "119", 2)]);
    let bob = available(&state, "bob");
    state
        .apply_block_unverified(3, vec![send(30, "bob", "dave", bob), publish(31, 1, 120)])
        .unwrap();
    unchanged_error(&mut state, vec![redeem(40, "dave", 1, 0)]);
    unchanged_error(&mut state, vec![deposit(41, "carol", ATOMS, 0)]);
    assert_eq!(state.amm_state().total_units, 100 * ATOMS);
    restored(&state);
}

#[test]
fn pool_default_is_closed_only_against_real_book_and_does_not_unlock_lp_claims() {
    let mut state = pool_open(false, vec![order(220, "carol", Side::Sell, "1", "119", 2)]);
    let bob = available(&state, "bob");
    state
        .apply_block_unverified(3, vec![send(30, "bob", "dave", bob), publish(31, 1, 120)])
        .unwrap();
    let pool = state.amm_owner().unwrap();
    state
        .apply_block_unverified(
            4,
            vec![
                EconomicInput::Cancel {
                    txid: id(402),
                    owner: a("carol"),
                    criteria: CancelCriteria::Txid(id(220)),
                },
                order(400, "carol", Side::Sell, "0.5", "500", 4),
                publish(41, 1, 500),
            ],
        )
        .unwrap();
    let event = state
        .liquidation_state()
        .events
        .iter()
        .find(|event| event.height == 4 && event.owner == pool)
        .unwrap();
    assert_eq!(event.status, LiquidationStatus::Closed);
    assert!(event.amm_fills.is_empty());
    assert_eq!(event.fills.len(), 1);
    assert_eq!(state.positions().get(&pool).unwrap().contracts, d("0"));
    assert_eq!(state.margin_atoms(&pool), 0);
    assert_eq!(state.amm_state().cash_atoms, 0);
    assert!(state.amm_state().quotes.is_empty());
    assert!(state.mark_state().unpaid_losses[&pool] > 0);
    unchanged_error(&mut state, vec![redeem(50, "dave", 1, 0)]);
    restored(&state);
}

#[test]
fn private_pool_identity_cannot_be_used_as_public_sender_lp_or_book_owner() {
    let mut state = new(false, None);
    state
        .apply_block_unverified(2, vec![deposit(19, "carol", 100 * ATOMS, 0)])
        .unwrap();
    let pool = state.amm_owner().unwrap();
    // A final-balance check cannot authorize balanced access to private pool
    // custody. Each external leg must reject even when the two cancel exactly.
    unchanged_error(
        &mut state,
        vec![
            send(30, pool.as_str(), "alice", 1),
            send(31, "alice", pool.as_str(), 1),
        ],
    );
    unchanged_error(
        &mut state,
        vec![
            send(32, "alice", pool.as_str(), 1),
            send(33, pool.as_str(), "alice", 1),
        ],
    );
    let mut forged = order(20, "alice", Side::Buy, "1", "100", 3);
    if let EconomicInput::Order(order) = &mut forged {
        order.owner = pool.clone();
    }
    unchanged_error(&mut state, vec![forged]);
    unchanged_error(
        &mut state,
        vec![EconomicInput::Amm(AmmCapitalInput::Deposit {
            txid: id(21),
            owner: pool.clone(),
            collateral_atoms: ATOMS,
            min_share_units: 0,
        })],
    );
    unchanged_error(
        &mut state,
        vec![EconomicInput::Joint(UnverifiedJointTx {
            txid: id(22),
            actor: pool,
            operation: JointOperation::Send {
                id: p(),
                recipient: a("alice"),
                amount_atoms: 1,
            },
        })],
    );
    state
        .apply_block_unverified(3, vec![deposit(20, "carol", ATOMS, 0)])
        .unwrap();
    restored(&state);
}

#[test]
fn funded_pool_rebate_and_funding_share_one_height_without_nominal_credits() {
    let mut state = new(false, Some(1));
    let mut inputs = vec![
        create(11),
        publish(12, 1, 100),
        deposit(13, "dave", 100 * ATOMS, 0),
    ];
    inputs.extend(pair_inputs(2, "alice", "bob", "1", "100"));
    state.apply_block_unverified(2, inputs).unwrap();
    through(&mut state, 23);
    let bob = available(&state, "bob");
    state
        .apply_block_unverified(
            24,
            vec![send(240, "bob", "dave", bob), publish(241, 1, 106)],
        )
        .unwrap();
    let event = closeout(&state, "bob");
    assert_eq!(event.status, LiquidationStatus::Cured);
    assert!(event.fills.is_empty());
    assert_eq!(event.amm_fills.len(), 1);
    let fill = &event.amm_fills[0];
    assert_eq!(fill.price, d("106.424"));
    assert_eq!(fill.charged_fee_atoms, 2_660_600);
    assert_eq!(fill.maker_rebate_atoms, 1_330_300);
    assert_eq!(fill.retained_fee_atoms, 1_330_300);
    let credit = state.funding_state().epochs[0]
        .plan
        .as_ref()
        .unwrap()
        .credits
        .iter()
        .find(|entry| entry.owner == a("bob"))
        .unwrap()
        .atoms;
    assert_eq!(event.top_up_atoms, credit);
    assert_eq!(state.amm_state().cash_atoms, 90 * ATOMS + 1_330_300);
    assert_eq!(state.amm_state().total_units, 100 * ATOMS);
    restored(&state);
}

#[test]
fn ordinary_trade_after_amm_fill_preserves_pool_risk_and_journal_order() {
    let mut state = pool_open(false, vec![order(220, "carol", Side::Sell, "1", "119", 2)]);
    let bob = available(&state, "bob");
    state
        .apply_block_unverified(3, vec![send(30, "bob", "dave", bob), publish(31, 1, 120)])
        .unwrap();
    let mut state = restored(&state);
    let pool = state.amm_owner().unwrap();
    let basis = state.mark_state().bases[&pool].clone();
    state
        .apply_block_unverified(4, pair_inputs(4, "carol", "alice", "0.25", "121"))
        .unwrap();
    assert_eq!(state.receipts().len(), 2);
    assert_eq!(state.receipts()[1].buyer_pnl, d("-0.5"));
    assert_eq!(state.receipts()[1].seller_pnl, d("0.25"));
    assert_eq!(realized(&state, "alice"), d("20.25"));
    assert_eq!(state.mark_state().bases[&pool], basis);
    assert_eq!(state.amm_state().cash_atoms, 95 * ATOMS);
    assert!(state.amm_state().quotes.is_empty());
    restored(&state);
}

#[test]
fn reorg_restores_lp_cash_quotes_and_both_amm_and_real_book_fills() {
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
    let mut inputs = vec![
        create(11),
        publish(12, 1, 100),
        deposit(13, "dave", 100 * ATOMS, 0),
        order(220, "carol", Side::Sell, "1", "119", 2),
    ];
    inputs.extend(pair_inputs(2, "alice", "bob", "1", "100"));
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
    assert_eq!(closeout(chain.state(), "bob").amm_fills.len(), 1);
    assert_eq!(closeout(chain.state(), "bob").fills.len(), 1);
    chain.rollback_tip(chain.tip()).unwrap();
    assert_eq!(chain, before);
    assert_eq!(chain.state().amm_state().quotes.len(), 2);
    assert_eq!(chain.state().reserve_atoms(id(220)), 10 * ATOMS);
    chain
        .apply_unverified_block(&block(
            3,
            chain.tip().id,
            4,
            vec![send(30, "bob", "dave", bob), publish(31, 1, 106)],
        ))
        .unwrap();
    assert_eq!(closeout(chain.state(), "bob").amm_fills.len(), 1);
    assert!(closeout(chain.state(), "bob").fills.is_empty());
    assert_eq!(
        closeout(chain.state(), "bob").status,
        LiquidationStatus::Cured
    );
    restored(chain.state());
}

#[test]
fn full_source_sweep_fixture_characterizes_the_inverse_reversal_and_number_bridge() {
    let rows: Vec<Vec<&str>> = include_str!("../fixtures/amm_liquidation_vectors.tsv")
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| line.split('\t').collect())
        .collect();
    assert_eq!(rows.len(), 22);
    assert!(rows.iter().all(|row| row.len() == 10));
    let case = |name| rows.iter().find(|row| row[1] == name).unwrap();
    assert_eq!(
        case("inverse-buy-reversal")[8],
        "0.5|101|distressed|amm|equal;0.5|102|distressed|amm|above"
    );
    assert_eq!(
        case("inverse-sell-reversal")[8],
        "0.5|99|amm|distressed|equal;0.5|98|amm|distressed|below"
    );
    assert_eq!(
        case("negative-price-source-acceptance")[8],
        "1|-1|distressed|amm|negative-price"
    );
    assert_eq!(case("exact-take-number-bridge")[7], "0.3333333333333333");
    assert_eq!(
        case("large-number-price-bridge")[8],
        "1|9007199254740992|distressed|amm|rounded-price"
    );
}

#[test]
fn amm_inventory_and_shared_fill_limit_bound_this_and_the_next_quote_generation() {
    let mut cfg = config(false, None);
    cfg.amm.as_mut().unwrap().max_abs_inventory = d("0.25");
    cfg.liquidation.as_mut().unwrap().max_fills_per_owner = 1;
    let joint = JointProtocolState::from_bootstrap_unverified(
        cfg.network,
        a("admin"),
        ledger(),
        BTreeSet::from([p()]),
    )
    .unwrap();
    let mut state = EconomicState::from_joint_unverified(cfg, joint).unwrap();
    state.apply_block_unverified(1, vec![activate()]).unwrap();
    let mut inputs = vec![
        create(11),
        publish(12, 1, 100),
        deposit(13, "dave", 100 * ATOMS, 0),
        order(220, "carol", Side::Sell, "1", "119", 2),
    ];
    inputs.extend(pair_inputs(2, "alice", "bob", "1", "100"));
    state.apply_block_unverified(2, inputs).unwrap();
    assert!(state
        .amm_state()
        .quotes
        .iter()
        .all(|quote| quote.remaining == d("0.25")));
    let bob = available(&state, "bob");
    state
        .apply_block_unverified(3, vec![send(30, "bob", "dave", bob), publish(31, 1, 120)])
        .unwrap();
    let event = closeout(&state, "bob");
    assert_eq!(event.status, LiquidationStatus::FillLimit);
    assert_eq!(event.amm_fills.len(), 1);
    assert_eq!(event.amm_fills[0].quantity, d("0.25"));
    assert!(
        event.fills.is_empty(),
        "AMM fills consume the same one-fill budget as real book fills"
    );
    let pool = state.amm_owner().unwrap();
    assert_eq!(state.positions().get(&pool).unwrap().contracts, d("-0.25"));
    assert_eq!(state.margin_atoms(&pool), 5 * ATOMS / 2);
    assert_eq!(state.book().resting()[0].remaining, d("1"));
    assert_eq!(state.reserve_atoms(id(220)), 10 * ATOMS);
    let generation = state.amm_state().generation;
    let mut state = restored(&state);
    state
        .apply_block_unverified(4, vec![publish(40, 1, 120)])
        .unwrap();
    assert_eq!(state.amm_state().generation, generation + 1);
    let epoch = state
        .amm_state()
        .frames
        .last()
        .unwrap()
        .quote_epoch
        .as_ref()
        .unwrap();
    assert_eq!(epoch.position_before, d("-0.25"));
    assert_eq!(epoch.quotes.len(), 1);
    assert_eq!(epoch.quotes[0].side, Side::Buy);
    assert_eq!(epoch.quotes[0].remaining, d("0.5"));
    let event = closeout(&state, "bob");
    assert_eq!(event.height, 4);
    assert_eq!(event.status, LiquidationStatus::Closed);
    assert!(
        event.amm_fills.is_empty(),
        "short inventory cap leaves no further AMM ask"
    );
    assert_eq!(event.fills.len(), 1);
    assert_eq!(event.fills[0].quantity, d("0.75"));
    assert_eq!(state.positions().get(&pool).unwrap().contracts, d("-0.25"));
    assert_eq!(state.book().resting()[0].remaining, d("0.25"));
    restored(&state);
}

#[test]
fn real_earned_pnl_changes_flat_pool_nav_share_floor_and_all_redemption_dust() {
    let mut state = pool_open(false, vec![]);
    let bob = available(&state, "bob");
    state
        .apply_block_unverified(3, vec![send(30, "bob", "dave", bob), publish(31, 1, 106)])
        .unwrap();
    assert_eq!(closeout(&state, "bob").status, LiquidationStatus::Cured);
    let first = &closeout(&state, "bob").amm_fills[0];
    assert_eq!(first.price, d("106.424"));
    assert_eq!(first.maker_rebate_atoms, 1_330_300);
    let alice = available(&state, "alice");
    state
        .apply_block_unverified(
            4,
            vec![send(40, "alice", "dave", alice), publish(41, 1, 100)],
        )
        .unwrap();
    let second = &closeout(&state, "alice").amm_fills[0];
    assert_eq!(second.price, d("99.6"));
    assert_eq!(second.maker_pnl, d("0.2"));
    assert_eq!(second.maker_rebate_atoms, 1_245_000);
    let pool = state.amm_owner().unwrap();
    assert_eq!(state.positions().get(&pool).unwrap().contracts, d("0"));
    assert_eq!(
        state.positions().get(&pool).unwrap().realized_pnl,
        d("3.412")
    );
    assert_eq!(state.margin_atoms(&pool), 0);
    assert_eq!(state.amm_state().cash_atoms, 9_843_775_300);
    assert_eq!(
        state
            .amm_state()
            .quotes
            .iter()
            .map(|quote| quote.reserve_atoms)
            .sum::<u128>(),
        5 * ATOMS
    );
    assert_eq!(state.amm_state().total_units, 100 * ATOMS);
    let mut state = restored(&state);
    // The next frame refunds the last5 quote tokens. Integer division gives
    // floor(100e8 * 100e8 / 103.437753e8) = 9,667,650,069 new units.
    unchanged_error(&mut state, vec![deposit(50, "carol", 1, 0)]);
    unchanged_error(
        &mut state,
        vec![deposit(51, "carol", 100 * ATOMS, 9_667_650_070)],
    );
    state
        .apply_block_unverified(5, vec![deposit(51, "carol", 100 * ATOMS, 9_667_650_069)])
        .unwrap();
    assert_eq!(
        state.amm_state().frames.last().unwrap().capital_receipts[0].cash_before_atoms,
        10_343_775_300
    );
    assert_eq!(state.amm_state().shares[&a("dave")], 100 * ATOMS);
    assert_eq!(state.amm_state().shares[&a("carol")], 9_667_650_069);
    assert_eq!(state.amm_state().total_units, 19_667_650_069);
    assert_eq!(state.amm_state().cash_atoms, 20_343_775_300);
    let dave_before = available(&state, "dave");
    let carol_before = available(&state, "carol");
    let mut state = restored(&state);
    state
        .apply_block_unverified(
            6,
            vec![
                redeem(60, "carol", 1, 1),
                redeem(61, "dave", 40 * ATOMS, 4_137_510_120),
            ],
        )
        .unwrap();
    assert_eq!(available(&state, "carol"), carol_before + 1);
    assert_eq!(available(&state, "dave"), dave_before + 4_137_510_120);
    assert_eq!(state.amm_state().shares[&a("dave")], 60 * ATOMS);
    state
        .apply_block_unverified(7, vec![redeem(70, "dave", 60 * ATOMS, 6_206_265_180)])
        .unwrap();
    assert_eq!(
        available(&state, "dave"),
        dave_before + 10_343_775_300,
        "old LP keeps all prior funded NAV"
    );
    assert_eq!(state.amm_state().cash_atoms, 9_999_999_999);
    let final_units = state.amm_state().shares[&a("carol")];
    assert_eq!(final_units, 9_667_650_068);
    state
        .apply_block_unverified(8, vec![redeem(80, "carol", final_units, 9_999_999_999)])
        .unwrap();
    assert_eq!(
        available(&state, "carol"),
        carol_before + 100 * ATOMS,
        "new LP does not capture old earnings or lose final dust"
    );
    assert_eq!(state.amm_state().cash_atoms, 0);
    assert_eq!(state.amm_state().total_units, 0);
    assert!(state.amm_state().shares.is_empty());
    assert!(state.amm_state().quotes.is_empty());
    restored(&state);
}
