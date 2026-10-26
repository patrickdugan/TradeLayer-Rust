//! Healthy selected-contract reads from actual funded owner positions/catalog.
use std::collections::BTreeSet;
use tradelayer_core::chain::{BlockId, Tip};
use tradelayer_core::consensus_source::{
    SourceBlockMode, SourceConsensusConfig, SourceConsensusPolicy, SourceTransaction,
};
use tradelayer_core::contract_orderbook::{Order, Side};
use tradelayer_core::contract_registry::{ContractAsset, ContractParameters};
use tradelayer_core::decimal::Decimal;
use tradelayer_core::economic::{
    contract_definition_commitment, ContractBindingConfig, ContractRiskPolicy, CreateContractInput,
    EconomicConfig, EconomicInput, EconomicState, MarkConfig, MarkPolicy, SettlementPolicy,
};
use tradelayer_core::economic_validity::EconomicSourceBackend;
use tradelayer_core::joint::{JointOperation, JointProtocolState, UnverifiedJointTx};
use tradelayer_core::ledger::{AccountId, Ledger, Operation, PropertyId, PropertyKind, TxId};
use tradelayer_core::network_profile::TradeLayerMainnet;
use tradelayer_core::registry::RegistryAction;
use tradelayer_core::tally::FeeRoute;
use tradelayer_core::types_source::{decode_payload_unverified, TypesOutcome};
use tradelayer_core::validity_source::{
    DependencyCall, DependencyReturn, SourceBackend, SourceRuleError, SourceValue as V,
};

const ATOMS: u128 = 100_000_000;
const ADMIN: &str = "admin";
const OTHER: &str = "BC1QOTHER";
fn a(value: &str) -> AccountId {
    value.try_into().unwrap()
}
fn p() -> PropertyId {
    7.try_into().unwrap()
}
fn d(value: &str) -> Decimal {
    value.parse().unwrap()
}
fn n(value: f64) -> V {
    V::number(value)
}
fn s(value: &str) -> V {
    V::String(value.into())
}
fn id(value: u64) -> TxId {
    let mut bytes = [0; 32];
    bytes[24..].copy_from_slice(&value.to_be_bytes());
    TxId::from_bytes(bytes)
}
fn tx(number: u64, actor: &str, action: RegistryAction) -> EconomicInput {
    EconomicInput::Joint(UnverifiedJointTx {
        txid: id(number),
        actor: a(actor),
        operation: JointOperation::Registry(action),
    })
}
fn creation(inverse: bool, notional: &str) -> CreateContractInput {
    CreateContractInput {
        txid: id(30),
        issuer: a(ADMIN),
        parameters: ContractParameters {
            native: false,
            underlying_oracle_id: Some(1),
            on_chain_data: vec![],
            notional_property_id: ContractAsset::BaseChain,
            notional_value: d(notional),
            collateral_property_id: ContractAsset::Property(p()),
            leverage: 10.into(),
            expiry_period: 0,
            series: 1,
            inverse,
            fee: 0.into(),
            whitelist: Some(0),
            ticker: None,
        },
    }
}
fn pristine(
    inverse: bool,
    notional: &str,
    admin_tokens: u128,
    other_tokens: u128,
) -> EconomicState {
    let mut ledger = Ledger::new();
    ledger
        .apply(
            id(900),
            Operation::CreateProperty {
                id: p(),
                issuer: a(ADMIN),
                kind: PropertyKind::Fixed,
                initial_atoms: (admin_tokens + other_tokens) * ATOMS,
            },
        )
        .unwrap();
    ledger
        .apply(
            id(901),
            Operation::Transfer {
                id: p(),
                from: a(ADMIN),
                to: a(OTHER),
                amount_atoms: other_tokens * ATOMS,
            },
        )
        .unwrap();
    let joint = JointProtocolState::from_bootstrap_unverified(
        TradeLayerMainnet::Bitcoin,
        a(ADMIN),
        ledger,
        BTreeSet::from([p()]),
    )
    .unwrap();
    let input = creation(inverse, notional);
    let config = EconomicConfig {
        network: TradeLayerMainnet::Bitcoin,
        contract: 1.try_into().unwrap(),
        collateral: p(),
        inverse,
        notional: d(notional),
        initial_margin_per_contract: 0.into(),
        fee_route: FeeRoute::Oracle(1),
        policy: SettlementPolicy::ConservingV1,
        funding: None,
        marking: Some(MarkConfig {
            oracle_id: 1,
            policy: MarkPolicy::ConservingMarkBasisV1,
        }),
        liquidation: None,
        amm: None,
        contract_binding: Some(ContractBindingConfig {
            creation_txid: input.txid,
            definition_hash: contract_definition_commitment(&input).unwrap(),
            bootstrap_commitment: EconomicState::contract_bootstrap_commitment(&joint).unwrap(),
            policy: ContractRiskPolicy::SourceContextsConservingV1,
        }),
        source_consensus: None,
        native_entitlements: None,
        insurance: None,
    };
    EconomicState::from_joint_unverified(config, joint).unwrap()
}
fn published(number: u64, price_1e4: u128) -> EconomicInput {
    tx(
        number,
        "publisher",
        RegistryAction::PublishOracle {
            oracle_id: 1,
            price_1e4,
            high_1e4: None,
            low_1e4: None,
            close_1e4: None,
        },
    )
}
fn opened(
    inverse: bool,
    notional: &str,
    price: &str,
    admin_tokens: u128,
    other_tokens: u128,
) -> EconomicState {
    opened_quantity(inverse, notional, price, admin_tokens, other_tokens, "2")
}
fn opened_quantity(
    inverse: bool,
    notional: &str,
    price: &str,
    admin_tokens: u128,
    other_tokens: u128,
    quantity: &str,
) -> EconomicState {
    let mut state = pristine(inverse, notional, admin_tokens, other_tokens);
    state
        .apply_block_unverified(
            1,
            vec![tx(
                10,
                ADMIN,
                RegistryAction::Activate {
                    types: vec![13, 14, 16, 18],
                    code_hash: [0x19; 32],
                },
            )],
        )
        .unwrap();
    let price_1e4 = d(price)
        .checked_mul(&Decimal::from(10_000u32))
        .unwrap()
        .to_string()
        .parse()
        .unwrap();
    state
        .apply_block_unverified(
            2,
            vec![
                tx(
                    20,
                    "publisher",
                    RegistryAction::CreateOracle {
                        ticker: "READ".into(),
                        url: "https://oracle.example".into(),
                        backup: a("backup"),
                        lag: 1,
                    },
                ),
                EconomicInput::CreateContract(creation(inverse, notional)),
                published(21, price_1e4),
            ],
        )
        .unwrap();
    let order = |number, owner, side| {
        EconomicInput::Order(Order {
            txid: id(number),
            owner: a(owner),
            contract: 1.try_into().unwrap(),
            height: 3,
            side,
            quantity: d(quantity),
            price: d(price),
            post: false,
            is_market: false,
        })
    };
    state
        .apply_block_unverified(
            3,
            vec![order(31, ADMIN, Side::Sell), order(32, OTHER, Side::Buy)],
        )
        .unwrap();
    state
}
fn backend(state: &EconomicState) -> EconomicSourceBackend<'_> {
    EconomicSourceBackend::from_state_unverified(
        state,
        Tip {
            height: state.finalized_height().unwrap_or(0),
            id: if state.finalized_height().is_none() {
                state.config().network.genesis()
            } else {
                BlockId([0x19; 32])
            },
        },
    )
    .unwrap()
}
fn read(
    state: &EconomicState,
    target: &str,
    args: Vec<V>,
) -> Result<DependencyReturn, SourceRuleError> {
    backend(state).invoke(&DependencyCall {
        target: target.into(),
        arguments: args,
    })
}
fn pending(result: Result<DependencyReturn, SourceRuleError>) -> V {
    match result.unwrap() {
        DependencyReturn::Pending(value) => value,
        other => panic!("expected source Promise settlement: {other:?}"),
    }
}
fn empty() -> V {
    V::Object(vec![
        ("contracts".into(), n(0.0)),
        ("margin".into(), n(0.0)),
        ("unrealizedPNL".into(), n(0.0)),
    ])
}
fn base36(mut value: u128) -> String {
    let mut text = Vec::new();
    while value != 0 {
        text.push(b"0123456789abcdefghijklmnopqrstuvwxyz"[(value % 36) as usize] as char);
        value /= 36;
    }
    text.into_iter().rev().collect()
}
fn source_order(
    state: &EconomicState,
    amount: u32,
) -> tradelayer_core::types_source::TypesEvaluation {
    let raw = SourceTransaction {
        position: 0,
        txid: id(50 + u64::from(amount)),
        payload: format!(
            "i1,{},{},0,0,0,0,0",
            base36(100 * ATOMS),
            base36(u128::from(amount))
        ),
        marker: s("tl"),
        sender: s(ADMIN),
        reference: V::Null,
        sender_amount: n(1.0),
        reference_amount: n(0.0),
    };
    decode_payload_unverified(
        &raw.arguments(4, SourceBlockMode::IndexedReplay),
        &mut backend(state),
    )
    .unwrap()
}

#[test]
fn actual_signed_positions_default_shape_and_bech32_lookup_are_async_immutable_reads() {
    let state = opened(false, "1", "100", 1000, 1000);
    let before = state.snapshot_bytes().unwrap();
    let handle = pending(read(&state, "Margin.loadMarginMap", vec![n(1.0)]));
    assert_eq!(handle.get("seriesId"), n(1.0));
    let short = pending(read(
        &state,
        "Margin.getPositionForAddress",
        vec![s(ADMIN), n(1.0)],
    ));
    assert_eq!(short.get("contracts"), n(-2.0));
    assert_eq!(short.get("margin"), n(20.0));
    assert_eq!(short.get("address"), s(ADMIN));
    let long = pending(read(
        &state,
        "Margin.getPositionForAddress",
        vec![s("bc1qother"), n(1.0)],
    ));
    assert_eq!(long.get("contracts"), n(2.0));
    assert_eq!(long.get("address"), s(OTHER));
    for address in ["Bc1qother", "missing", ""] {
        assert_eq!(
            pending(read(
                &state,
                "Margin.getPositionForAddress",
                vec![s(address), n(1.0)]
            )),
            empty()
        );
    }
    assert_eq!(state.margin_atoms(&a(ADMIN)), 20 * ATOMS);
    assert_eq!(state.margin_atoms(&a(OTHER)), 20 * ATOMS);
    assert_eq!(state.snapshot_bytes().unwrap(), before);
    let restored = EconomicState::from_snapshot_bytes(
        &before,
        state.config(),
        &a(ADMIN),
        &BTreeSet::from([p()]),
    )
    .unwrap();
    assert_eq!(
        pending(read(
            &restored,
            "Margin.getPositionForAddress",
            vec![s(ADMIN), n(1.0)]
        )),
        short
    );
}

#[test]
fn accepted_definition_prices_drive_source_linear_half_up_and_inverse_ceiling() {
    let state = opened(false, "1", "100", 1000, 1000);
    assert_eq!(state.config().initial_margin_per_contract, Decimal::zero());
    assert_eq!(
        pending(read(
            &state,
            "Contract.getInitialMargin",
            vec![n(1.0), n(200.0)]
        )),
        n(20.0)
    );
    assert_eq!(
        pending(read(
            &state,
            "Contract.getInitialMargin",
            vec![n(1.0), n(10.00000005)]
        )),
        n(1.00000001)
    );
    let inverse = opened(true, "10000", "100", 1000, 1000);
    assert_eq!(
        pending(read(
            &inverse,
            "Contract.getInitialMargin",
            vec![n(1.0), n(3.0)]
        )),
        n(333.33333334)
    );
}

#[test]
fn raw_source_reduction_and_flip_reads_use_real_short_quantity_and_available_custody() {
    let state = opened(false, "1", "100", 25, 1000);
    let before = state.snapshot_bytes().unwrap();
    let reduce = source_order(&state, 1);
    let TypesOutcome::Returned(params) = reduce.outcome else {
        panic!()
    };
    assert_eq!(params.get("valid"), V::Bool(true));
    assert_eq!(params.get("amount"), n(1.0));
    let trace = &reduce.validity.unwrap().settlement.evaluation.dependencies;
    assert_eq!(
        trace
            .iter()
            .filter(|d| d.call.target == "Tally.hasSufficientBalance")
            .count(),
        1,
        "pure reduction skips opening-margin check but retains source's final check"
    );
    let flip = source_order(&state, 3);
    let TypesOutcome::Returned(params) = flip.outcome else {
        panic!()
    };
    assert_eq!(params.get("valid"), V::Bool(true));
    assert_eq!(
        params.get("amount"),
        n(2.0),
        "real shortfall reduces requested flip by source ceil arithmetic"
    );
    let trace = &flip.validity.unwrap().settlement.evaluation.dependencies;
    let balances = trace
        .iter()
        .filter(|d| d.call.target == "Tally.hasSufficientBalance")
        .collect::<Vec<_>>();
    assert_eq!(balances.len(), 2);
    assert!(
        balances.iter().all(|d| d.call.arguments[2] == n(10.0)),
        "source's second check retains the original excess-margin total"
    );
    assert_eq!(
        state.snapshot_bytes().unwrap(),
        before,
        "validation does not fund or settle an order"
    );
}

#[test]
fn missing_definitions_and_non_string_addresses_keep_source_async_exceptions() {
    let state = pristine(false, "1", 1000, 1000);
    let DependencyReturn::PendingThrow(error) =
        read(&state, "Contract.getInitialMargin", vec![n(1.0), n(100.0)]).unwrap()
    else {
        panic!()
    };
    assert_eq!(error.name, "Error");
    assert_eq!(error.message, "Contract info not found for contract ID: 1");
    assert_eq!(
        pending(read(
            &state,
            "Margin.getPositionForAddress",
            vec![s("missing"), n(1.0)]
        )),
        empty()
    );
    for address in [V::Null, V::Undefined, n(3.0)] {
        let DependencyReturn::PendingThrow(error) = read(
            &state,
            "Margin.getPositionForAddress",
            vec![address, n(1.0)],
        )
        .unwrap() else {
            panic!()
        };
        assert_eq!(error.name, "TypeError");
        assert!(error.message.contains("startsWith"));
    }
    for target in ["Margin.loadMarginMap", "Contract.getInitialMargin"] {
        let mut args = vec![s("1")];
        if target == "Contract.getInitialMargin" {
            args.push(n(100.0));
        }
        assert_eq!(
            read(&state, target, args),
            Err(SourceRuleError::InvalidInput)
        );
    }
    assert_eq!(
        read(&state, "Margin.loadMarginMap", vec![n(2.0)]),
        Err(SourceRuleError::UnknownDependency)
    );
    assert_eq!(
        read(&state, "Margin.getInstance", vec![n(1.0)]),
        Err(SourceRuleError::UnknownDependency)
    );
    assert_eq!(
        read(&state, "Margin.margins.get", vec![s(ADMIN)]),
        Err(SourceRuleError::UnknownDependency)
    );
}

#[test]
fn real_funded_positions_outside_lossless_signed_number_domain_are_explicitly_unsupported() {
    let mut state = opened_quantity(
        false,
        "0.00000001",
        "100",
        1_000_000_000_000_000_000_000_000,
        1_000_000_000_000_000_000_000_000,
        "9007199254740992",
    );
    let order = |number, owner, side| {
        EconomicInput::Order(Order {
            txid: id(number),
            owner: a(owner),
            contract: 1.try_into().unwrap(),
            height: 4,
            side,
            quantity: 1.into(),
            price: d("100"),
            post: false,
            is_market: false,
        })
    };
    state
        .apply_block_unverified(
            4,
            vec![order(41, ADMIN, Side::Sell), order(42, OTHER, Side::Buy)],
        )
        .unwrap();
    state.verify().unwrap();
    assert_eq!(
        state.positions().get(&a(ADMIN)).unwrap().contracts,
        d("-9007199254740993")
    );
    assert!(state.margin_atoms(&a(ADMIN)) > 0);
    let before = state.snapshot_bytes().unwrap();
    assert_eq!(
        read(
            &state,
            "Margin.getPositionForAddress",
            vec![s(ADMIN), n(1.0)]
        ),
        Err(SourceRuleError::UnsupportedNumericDomain)
    );
    assert_eq!(
        state.snapshot_bytes().unwrap(),
        before,
        "unsupported projection never rounds or alters actual custody"
    );
}

#[test]
fn original_source_policy_keeps_the_three_dependencies_unavailable() {
    let pristine = pristine(false, "1", 1000, 1000);
    let mut config = pristine.config().clone();
    config.source_consensus = Some(SourceConsensusConfig {
        bootstrap_commitment: EconomicState::source_bootstrap_commitment(pristine.joint()).unwrap(),
        policy: SourceConsensusPolicy::ConservingSequentialV1,
        catalog_bootstrap_commitment: None,
    });
    let state = EconomicState::from_joint_unverified(config, pristine.joint().clone()).unwrap();
    for (target, args) in [
        ("Margin.loadMarginMap", vec![n(1.0)]),
        ("Margin.getPositionForAddress", vec![s(ADMIN), n(1.0)]),
        ("Contract.getInitialMargin", vec![n(1.0), n(100.0)]),
    ] {
        assert_eq!(
            read(&state, target, args),
            Err(SourceRuleError::UnknownDependency)
        );
    }
}
