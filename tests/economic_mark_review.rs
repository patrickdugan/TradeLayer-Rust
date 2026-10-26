//! Independent complete-owner regressions for the explicit conserving basis policy.
use std::collections::BTreeSet;
use tradelayer_core::chain::{BlockHeader, BlockId};
use tradelayer_core::contract_orderbook::{Order, Side};
use tradelayer_core::contract_registry::ContractId;
use tradelayer_core::decimal::Decimal;
use tradelayer_core::economic::{
    EconomicConfig, EconomicInput, EconomicState, FundingConfig, FundingPolicy, MarkConfig,
    MarkEvent, MarkEventKind, MarkPolicy, SettlementPolicy,
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
        native_entitlements: None,
        insurance: None,
        source_consensus: None,
        contract_binding: None,
        network: TradeLayerMainnet::Litecoin,
        contract: c(),
        collateral: p(),
        inverse,
        notional: d("1"),
        initial_margin_per_contract: d("10"),
        fee_route: FeeRoute::Oracle(1),
        policy: SettlementPolicy::ConservingV1,
        funding: funding_oracle.map(|oracle_id| FundingConfig {
            oracle_id,
            max_staleness_blocks: 32,
            policy: FundingPolicy::ConservingFundingV1,
        }),
        liquidation: None,
        amm: None,
        marking: Some(MarkConfig {
            oracle_id: 1,
            policy: MarkPolicy::ConservingMarkBasisV1,
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
fn mark(state: &EconomicState) -> &MarkEvent {
    let event = state.mark_state().events.last().unwrap();
    assert!(matches!(event.kind, MarkEventKind::Mark { .. }));
    event
}
fn assert_conserved(state: &EconomicState) {
    state.verify().unwrap();
    let report = tradelayer_core::economic_reporting::EconomicHistory::from_state(state).unwrap();
    for position in state.positions().positions() {
        let snapshots = report
            .history()
            .position_history(&position.address, c().get())
            .unwrap();
        assert_eq!(
            snapshots
                .last()
                .map_or_else(Decimal::zero, |snapshot| snapshot.effect.after.clone()),
            position.contracts
        );
        let mut booked = report
            .history()
            .pnl_summary(&position.address, c().get())
            .unwrap()
            .booked;
        for event in report.marks() {
            for entry in &event.entries {
                if entry.owner == position.address {
                    booked = booked.checked_add(&entry.atoms.to_decimal()).unwrap();
                }
            }
        }
        assert_eq!(booked, position.realized_pnl);
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
            + reserves
            + fees
            + state.insurance_custody_atoms()
            + state.loss_pool_atoms(),
        4000 * ATOMS
    );
}
fn restored(state: &EconomicState) -> EconomicState {
    let bytes = state.snapshot_bytes().unwrap();
    assert_eq!(&bytes[..5], b"TLEC\x03");
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

#[test]
fn linear_mark_then_close_realizes_only_the_unsettled_remainder() {
    let mut state = opened(false);
    state
        .apply_block_unverified(3, vec![publish(30, 1, 105)])
        .unwrap();
    assert_eq!(realized(&state, "alice"), d("5"));
    assert_eq!(mark(&state).collected_loss_atoms, 5 * ATOMS);
    assert_eq!(mark(&state).paid_claim_atoms, 5 * ATOMS);
    let mut state = restored(&state);
    state
        .apply_block_unverified(4, pair_inputs(4, "bob", "alice", "1", "110"))
        .unwrap();
    assert_eq!(state.receipts()[1].seller_pnl, d("5"));
    assert_eq!(state.receipts()[1].buyer_pnl, d("-5"));
    assert_eq!(realized(&state, "alice"), d("10"));
    assert_eq!(realized(&state, "bob"), d("-10"));
    assert_eq!(state.margin_atoms(&a("alice")), 0);
    assert_eq!(state.margin_atoms(&a("bob")), 0);
    assert!(state
        .mark_state()
        .bases
        .values()
        .all(|basis| basis.quantity.is_zero()));
    assert!(state.mark_state().maintenance.is_empty());
    assert_eq!(state.unfunded_loss_atoms(), 0);
    restored(&state);
}

#[test]
fn inverse_add_partial_close_and_flip_use_reciprocal_inventory_basis() {
    let mut state = opened(true);
    state
        .apply_block_unverified(3, pair_inputs(3, "alice", "bob", "1", "200"))
        .unwrap();
    assert_eq!(
        state.positions().get(&a("alice")).unwrap().avg_price,
        Some(d("150"))
    );
    state
        .apply_block_unverified(4, vec![publish(40, 1, 200)])
        .unwrap();
    // 1*(1/100 - 1/200) + 1*(1/200 - 1/200), not 2*(1/150 - 1/200).
    assert_eq!(realized(&state, "alice"), d("0.005"));
    assert_eq!(mark(&state).paid_claim_atoms, 500_000);
    state
        .apply_block_unverified(5, pair_inputs(5, "bob", "alice", "0.5", "100"))
        .unwrap();
    assert_eq!(state.receipts()[2].seller_pnl, d("-0.0025"));
    assert_eq!(realized(&state, "alice"), d("0.0025"));
    let mut state = restored(&state);
    state
        .apply_block_unverified(6, pair_inputs(6, "bob", "alice", "2", "100"))
        .unwrap();
    assert_eq!(state.receipts()[3].seller_pnl, d("-0.0075"));
    assert_eq!(realized(&state, "alice"), d("-0.005"));
    assert_eq!(state.mark_state().bases[&a("alice")].quantity, d("-0.5"));
    assert_eq!(
        state.positions().get(&a("alice")).unwrap().avg_price,
        Some(d("100"))
    );
    state
        .apply_block_unverified(7, vec![publish(70, 1, 200)])
        .unwrap();
    assert_eq!(realized(&state, "alice"), d("-0.0075"));
    assert_eq!(realized(&state, "bob"), d("0.0075"));
    let basis = &state.mark_state().bases[&a("alice")];
    assert_eq!(basis.entry_basis.numerator(), &d("1"));
    assert_eq!(basis.entry_basis.denominator(), &d("400"));
    assert!(basis.carry.is_zero());
    restored(&state);
}

#[test]
fn same_block_trade_precedes_mark_without_inheriting_older_exposure() {
    let mut state = opened(false);
    let mut inputs = vec![publish(30, 1, 105)];
    inputs.extend(pair_inputs(3, "carol", "dave", "1", "105"));
    state.apply_block_unverified(3, inputs).unwrap();
    assert!(matches!(
        state.mark_state().events[2].kind,
        MarkEventKind::Trade { .. }
    ));
    assert_eq!(state.receipts()[1].buyer_pnl, d("0"));
    assert_eq!(state.receipts()[1].seller_pnl, d("0"));
    assert_eq!(realized(&state, "alice"), d("5"));
    assert_eq!(realized(&state, "bob"), d("-5"));
    assert_eq!(realized(&state, "carol"), d("0"));
    assert_eq!(realized(&state, "dave"), d("0"));
    assert_eq!(mark(&state).entries.len(), 4);
    assert_eq!(mark(&state).collected_loss_atoms, 5 * ATOMS);
    restored(&state);
}

#[test]
fn unchanged_publications_final_same_height_quote_and_replay_are_explicit() {
    let mut state = opened(false);
    state.apply_block_unverified(3, vec![]).unwrap();
    assert_eq!(state.mark_state().events.len(), 2);
    state
        .apply_block_unverified(4, vec![publish(40, 1, 100)])
        .unwrap();
    assert!(mark(&state)
        .entries
        .iter()
        .all(|entry| entry.atoms.atoms == 0));
    assert_eq!(mark(&state).collected_loss_atoms, 0);
    assert_eq!(mark(&state).paid_claim_atoms, 0);
    state
        .apply_block_unverified(5, vec![publish(50, 1, 101), publish(51, 1, 102)])
        .unwrap();
    let MarkEventKind::Mark { observation, .. } = &mark(&state).kind else {
        unreachable!()
    };
    assert_eq!(observation.txid, id(51));
    assert_eq!(observation.price, d("102"));
    assert_eq!(realized(&state, "alice"), d("2"));
    assert!(state.joint().registry().is_processed(id(50)));
    assert!(state.joint().registry().is_processed(id(51)));
    let mut state = restored(&state);
    let bytes = state.snapshot_bytes().unwrap();
    assert!(state
        .apply_block_unverified(6, vec![publish(51, 1, 103)])
        .is_err());
    assert_eq!(state.snapshot_bytes().unwrap(), bytes);
    assert!(state.apply_block_unverified(5, vec![]).is_err());
    assert_eq!(state.snapshot_bytes().unwrap(), bytes);
}

#[test]
fn maintenance_uses_actual_margin_after_mark_and_survives_available_top_up() {
    let mut state = opened(false);
    let bob = available(&state, "bob");
    state
        .apply_block_unverified(3, vec![send(30, "bob", "dave", bob), publish(31, 1, 106)])
        .unwrap();
    assert_eq!(available(&state, "bob"), 0);
    assert_eq!(state.margin_atoms(&a("bob")), 4 * ATOMS);
    assert_eq!(state.mark_state().maintenance.len(), 1);
    let call = &state.mark_state().maintenance[0];
    assert_eq!(call.owner, a("bob"));
    assert_eq!(call.contracts, d("-1"));
    assert_eq!(call.required_atoms, 5 * ATOMS);
    assert_eq!(call.margin_atoms, 4 * ATOMS);
    let mut state = restored(&state);
    state
        .apply_block_unverified(4, vec![send(40, "alice", "bob", 20 * ATOMS)])
        .unwrap();
    assert_eq!(available(&state, "bob"), 20 * ATOMS);
    assert_eq!(state.mark_state().maintenance.len(), 1);
    assert_eq!(state.mark_state().maintenance[0].margin_atoms, 4 * ATOMS);
    state
        .apply_block_unverified(5, pair_inputs(5, "bob", "alice", "0.1", "106"))
        .unwrap();
    assert_eq!(state.margin_atoms(&a("bob")), 9 * ATOMS);
    assert_eq!(state.mark_state().bases[&a("bob")].quantity, d("-0.9"));
    assert!(state.mark_state().maintenance.is_empty());
    restored(&state);
}

#[test]
fn default_then_full_reversal_cancels_claims_and_unpaid_losses_without_wallet_debit() {
    let mut state = opened(false);
    let bob = available(&state, "bob");
    state
        .apply_block_unverified(3, vec![send(30, "bob", "dave", bob), publish(31, 1, 200)])
        .unwrap();
    assert_eq!(mark(&state).collected_loss_atoms, 10 * ATOMS);
    assert_eq!(mark(&state).paid_claim_atoms, 10 * ATOMS);
    assert_eq!(mark(&state).unfunded_loss_atoms, 90 * ATOMS);
    assert_eq!(state.unfunded_loss_atoms(), 90 * ATOMS);
    assert_eq!(state.mark_state().unpaid_losses[&a("bob")], 90 * ATOMS);
    let key = IouKey::new(1, 7);
    assert_eq!(
        state.ious().claim_map(key).unwrap().claim(&a("alice")),
        Some(&d("90"))
    );
    let mut state = restored(&state);
    // New available custody is not an authorization to recover an old default.
    state
        .apply_block_unverified(4, vec![send(40, "dave", "bob", ATOMS)])
        .unwrap();
    assert_eq!(available(&state, "bob"), ATOMS);
    assert_eq!(state.mark_state().unpaid_losses[&a("bob")], 90 * ATOMS);
    state
        .apply_block_unverified(5, vec![publish(50, 1, 100)])
        .unwrap();
    assert_eq!(mark(&state).collected_loss_atoms, 10 * ATOMS);
    assert_eq!(mark(&state).paid_claim_atoms, 10 * ATOMS);
    let alice_entry = mark(&state)
        .entries
        .iter()
        .find(|entry| entry.owner == a("alice"))
        .unwrap();
    let bob_entry = mark(&state)
        .entries
        .iter()
        .find(|entry| entry.owner == a("bob"))
        .unwrap();
    assert_eq!(alice_entry.claim_offset_atoms, 90 * ATOMS);
    assert_eq!(alice_entry.loss_offset_atoms, 0);
    assert_eq!(bob_entry.claim_offset_atoms, 0);
    assert_eq!(bob_entry.loss_offset_atoms, 90 * ATOMS);
    assert_eq!(available(&state, "bob"), 11 * ATOMS);
    assert_eq!(state.unfunded_loss_atoms(), 90 * ATOMS);
    assert_eq!(realized(&state, "alice"), d("0"));
    assert_eq!(realized(&state, "bob"), d("0"));
    assert!(state.ious().claim_map(key).unwrap().claims().is_empty());
    assert!(state.mark_state().unpaid_losses.is_empty());
    state
        .apply_block_unverified(6, pair_inputs(6, "bob", "alice", "1", "100"))
        .unwrap();
    assert_eq!(state.receipts()[1].buyer_pnl, d("0"));
    assert_eq!(state.receipts()[1].seller_pnl, d("0"));
    assert!(state.ious().claim_map(key).unwrap().claims().is_empty());
    assert!(state.mark_state().unpaid_losses.is_empty());
    restored(&state);
}

#[test]
fn failure_after_mark_in_independent_funding_reverts_quote_send_epoch_and_all_legs() {
    let mut state = new(false, Some(2));
    let mut inputs = vec![
        create(11),
        create(13),
        publish(12, 1, 100),
        publish(14, 2, 99),
    ];
    inputs.extend(pair_inputs(2, "alice", "bob", "1", "100"));
    state.apply_block_unverified(2, inputs).unwrap();
    through(&mut state, 22);
    state
        .apply_block_unverified(23, vec![publish(230, 2, 101)])
        .unwrap();
    let before = state.clone();
    let bytes = state.snapshot_bytes().unwrap();
    let alice = available(&state, "alice");
    // The zero mark and its system marker are staged before the funding payer
    // discovers its missing available custody; all earlier ordinary legs revert.
    assert!(state
        .apply_block_unverified(
            24,
            vec![publish(240, 1, 100), send(241, "alice", "dave", alice)]
        )
        .is_err());
    assert_eq!(state, before);
    assert_eq!(state.snapshot_bytes().unwrap(), bytes);
    assert!(!state.joint().registry().is_processed(id(240)));
    assert!(!state.ledger().is_processed(id(241)));
    assert!(state.funding_state().epochs.is_empty());
    state
        .apply_block_unverified(
            24,
            vec![
                publish(240, 1, 100),
                send(241, "alice", "dave", alice - 11_993_750),
            ],
        )
        .unwrap();
    assert_eq!(available(&state, "alice"), 0);
    assert_eq!(mark(&state).height, 24);
    assert_eq!(
        state.funding_state().epochs[0]
            .plan
            .as_ref()
            .unwrap()
            .total_atoms,
        11_993_750
    );
    restored(&state);
}

#[test]
fn mark_and_funding_same_height_have_distinct_reserved_system_ids_after_restore() {
    let mut state = new(false, Some(1));
    let mut inputs = vec![create(11), publish(12, 1, 99)];
    inputs.extend(pair_inputs(2, "alice", "bob", "1", "99"));
    state.apply_block_unverified(2, inputs).unwrap();
    through(&mut state, 23);
    state
        .apply_block_unverified(24, vec![publish(240, 1, 101)])
        .unwrap();
    let MarkEventKind::Mark { id: mark_id, .. } = mark(&state).kind else {
        unreachable!()
    };
    let funding_id = state.funding_state().epochs[0].id;
    assert_ne!(mark_id, funding_id);
    assert_eq!(mark(&state).height, 24);
    assert_eq!(mark(&state).paid_claim_atoms, 2 * ATOMS);
    assert_eq!(state.funding_state().epochs[0].height, 24);
    assert_eq!(
        state.funding_state().epochs[0]
            .plan
            .as_ref()
            .unwrap()
            .total_atoms,
        11_993_750
    );
    let state = restored(&state);
    assert!(state.joint().snapshot_bytes().is_err());
    for reserved in [mark_id, funding_id] {
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
fn complete_chain_rollback_restores_basis_claims_maintenance_and_epoch_admission() {
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
    let mut opening = vec![create(11), publish(12, 1, 100)];
    opening.extend(pair_inputs(2, "alice", "bob", "1", "100"));
    chain
        .apply_unverified_block(&block(2, chain.tip().id, 2, opening))
        .unwrap();
    let before = chain.clone();
    let bob = available(chain.state(), "bob");
    chain
        .apply_unverified_block(&block(
            3,
            chain.tip().id,
            3,
            vec![send(30, "bob", "dave", bob), publish(31, 1, 106)],
        ))
        .unwrap();
    assert_eq!(chain.state().mark_state().maintenance.len(), 1);
    chain.rollback_tip(chain.tip()).unwrap();
    assert_eq!(chain, before);
    assert!(chain.state().mark_state().maintenance.is_empty());
    assert_eq!(realized(chain.state(), "alice"), d("0"));
    chain
        .apply_unverified_block(&block(3, chain.tip().id, 4, vec![publish(31, 1, 110)]))
        .unwrap();
    assert_eq!(realized(chain.state(), "alice"), d("10"));
    assert_eq!(chain.state().unfunded_loss_atoms(), 0);
    assert_conserved(chain.state());
}
