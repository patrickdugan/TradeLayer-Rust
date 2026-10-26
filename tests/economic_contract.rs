//! Admitted catalog identity and source price-context risk in the sole Ledger.
//! Typed actors and blocks remain unverified; source admission defects are
//! characterized separately from SourceContextsConservingV1 execution.
use std::collections::BTreeSet;
use tradelayer_core::chain::{BlockHeader, BlockId};
use tradelayer_core::contract_orderbook::{Order, Side};
use tradelayer_core::contract_registry::{
    ContractAsset, ContractId, ContractParameters, ContractRegistry, TickerContext,
};
use tradelayer_core::decimal::Decimal;
use tradelayer_core::economic::{
    contract_definition_commitment, AmmCapitalInput, ContractBindingConfig, ContractRiskPolicy,
    CreateContractInput, EconomicConfig, EconomicError, EconomicInput, EconomicState,
    FundedAmmConfig, FundedAmmPolicy, FundingConfig, FundingPolicy, FundingStatus,
    LiquidationConfig, LiquidationPolicy, LiquidationStatus, MarkConfig, MarkPolicy,
    SettlementPolicy,
};
use tradelayer_core::economic_chain::{
    EconomicChain, EconomicChainConfig, UnverifiedEconomicBlock,
};
use tradelayer_core::joint::{JointOperation, JointProtocolState, UnverifiedJointTx};
use tradelayer_core::ledger::{AccountId, Ledger, Operation, PropertyId, PropertyKind, TxId};
use tradelayer_core::margin_map::{ContractContext, MarginMap, Position, PositionTrade};
use tradelayer_core::network_profile::TradeLayerMainnet;
use tradelayer_core::registry::RegistryAction;
use tradelayer_core::tally::{
    calculate_match_fees, FeeRoute, MakerFlag, MatchFeeInput, SignedAtoms,
};

const ATOMS: u128 = 100_000_000;
fn d(text: &str) -> Decimal {
    text.parse().unwrap()
}
fn a(text: &str) -> AccountId {
    text.try_into().unwrap()
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
fn parameters(inverse: bool, notional: &str) -> ContractParameters {
    ContractParameters {
        native: false,
        underlying_oracle_id: Some(1),
        on_chain_data: vec![],
        notional_property_id: ContractAsset::Property(8.try_into().unwrap()),
        notional_value: d(notional),
        collateral_property_id: ContractAsset::Property(p()),
        leverage: d("10"),
        expiry_period: 0,
        series: 1,
        inverse,
        fee: Decimal::zero(),
        whitelist: Some(0),
        ticker: None,
    }
}
fn creation(inverse: bool, notional: &str) -> CreateContractInput {
    CreateContractInput {
        txid: id(30),
        issuer: a("issuer"),
        parameters: parameters(inverse, notional),
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
fn bootstrap() -> JointProtocolState {
    JointProtocolState::from_bootstrap_unverified(
        TradeLayerMainnet::Litecoin,
        a("admin"),
        ledger(),
        BTreeSet::from([p()]),
    )
    .unwrap()
}
fn config(input: &CreateContractInput) -> EconomicConfig {
    EconomicConfig {
        native_entitlements: None,
        insurance: None,
        source_consensus: None,
        network: TradeLayerMainnet::Litecoin,
        contract: c(),
        collateral: p(),
        inverse: input.parameters.inverse,
        notional: input.parameters.notional_value.clone(),
        initial_margin_per_contract: Decimal::zero(),
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
            definition_hash: contract_definition_commitment(input).unwrap(),
            bootstrap_commitment: EconomicState::contract_bootstrap_commitment(&bootstrap())
                .unwrap(),
            policy: ContractRiskPolicy::SourceContextsConservingV1,
        }),
    }
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
            types: vec![2, 6, 10, 13, 14, 15, 16, 18],
            code_hash: [0x79; 32],
        }),
    )
}
fn oracle(number: u64) -> EconomicInput {
    joint(
        number,
        "publisher",
        JointOperation::Registry(RegistryAction::CreateOracle {
            ticker: "INDEX".into(),
            url: "https://example.invalid/index".into(),
            backup: a("backup"),
            lag: 1,
        }),
    )
}
fn publish(number: u64, price: u128) -> EconomicInput {
    joint(
        number,
        "publisher",
        JointOperation::Registry(RegistryAction::PublishOracle {
            oracle_id: 1,
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
    amount: &str,
    price: &str,
    height: u64,
) -> EconomicInput {
    EconomicInput::Order(Order {
        txid: id(number),
        owner: a(owner),
        contract: c(),
        height,
        side,
        quantity: d(amount),
        price: d(price),
        post: false,
        is_market: false,
    })
}
fn pair(height: u64, buyer: &str, seller: &str, amount: &str, price: &str) -> Vec<EconomicInput> {
    vec![
        publish(height * 100 + 90, price.parse().unwrap()),
        order(height * 100, buyer, Side::Buy, amount, price, height),
        order(height * 100 + 1, seller, Side::Sell, amount, price, height),
    ]
}
fn before_creation(config: EconomicConfig) -> EconomicState {
    let mut state = EconomicState::from_joint_unverified(config, bootstrap()).unwrap();
    state.apply_block_unverified(1, vec![activate()]).unwrap();
    state
}
fn new(inverse: bool) -> EconomicState {
    let input = creation(inverse, if inverse { "10000" } else { "1" });
    let mut state = before_creation(config(&input));
    state
        .apply_block_unverified(
            2,
            vec![
                oracle(20),
                EconomicInput::CreateContract(input),
                publish(21, 100),
            ],
        )
        .unwrap();
    state
}
fn opened(inverse: bool) -> EconomicState {
    let mut state = new(inverse);
    state
        .apply_block_unverified(3, pair(3, "alice", "bob", "1", "100"))
        .unwrap();
    state
}
fn available(state: &EconomicState, owner: &str) -> u128 {
    state.ledger().available_atoms(p(), &a(owner))
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
        .map(|resting| state.reserve_atoms(resting.order.txid))
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
            + state.loss_pool_atoms()
            + state.insurance_custody_atoms()
            + state.amm_state().cash_atoms
            + state
                .amm_state()
                .quotes
                .iter()
                .map(|quote| quote.reserve_atoms)
                .sum::<u128>(),
        4000 * ATOMS
    );
}
fn restored(state: &EconomicState) -> EconomicState {
    let bytes = state.snapshot_bytes().unwrap();
    assert_eq!(&bytes[..5], b"TLEC\x06");
    let out = EconomicState::from_snapshot_bytes(
        &bytes,
        state.config(),
        &a("admin"),
        &BTreeSet::from([p()]),
    )
    .unwrap();
    assert_eq!(&out, state);
    assert_conserved(&out);
    out
}
fn signed_tokens(atoms: SignedAtoms) -> Decimal {
    let (negative, amount) = match atoms {
        SignedAtoms::Debit(amount) => (false, amount),
        SignedAtoms::Credit(amount) => (true, amount),
    };
    let value = Decimal::from(amount)
        .checked_div(
            &Decimal::from(ATOMS),
            8,
            tradelayer_core::decimal::Rounding::Down,
        )
        .unwrap();
    if negative {
        value.negated()
    } else {
        value
    }
}

#[test]
fn complete_source_context_vectors_compose_registry_fees_and_maintenance() {
    let mut contexts = 0;
    let mut positions = 0;
    let mut map = MarginMap::new(c());
    let mut last_inverse = None;
    for line in include_str!("../fixtures/contract_binding_vectors.tsv")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let fields: Vec<_> = line.split('\t').collect();
        if fields[0] == "CONTEXT" {
            let native = fields[1] == "1";
            let inverse = fields[2] == "1";
            let mut parameters = parameters(inverse, fields[3]);
            parameters.native = native;
            parameters.underlying_oracle_id = Some(if native { 0 } else { 1 });
            parameters.leverage = d(fields[4]);
            let mut registry = ContractRegistry::new();
            registry
                .create_contract_series(
                    a("issuer"),
                    parameters,
                    TickerContext {
                        notional_ticker: Some("ASSET"),
                        collateral_ticker: Some("USD"),
                        oracle_ticker: Some("INDEX"),
                    },
                    2,
                )
                .unwrap();
            let price = d(fields[5]);
            let quantity = d(fields[6]);
            let im = registry.initial_margin(c(), &price).unwrap();
            assert_eq!(im, d(fields[8]), "{line}");
            let valued = registry
                .notional_value(c(), Some(&price), None)
                .unwrap()
                .notional_value;
            assert_eq!(valued, d(fields[9]), "{line}");
            let fee = calculate_match_fees(&MatchFeeInput {
                amount_buy: quantity.clone(),
                amount_sell: quantity.clone(),
                buy_maker: if fields[7] == "1" {
                    MakerFlag::True
                } else {
                    MakerFlag::False
                },
                sell_maker: MakerFlag::False,
                inverse,
                last_mark: price,
                notional_value: valued,
                channel: false,
            })
            .unwrap();
            assert_eq!(signed_tokens(fee.buyer), d(fields[10]), "{line}");
            assert_eq!(signed_tokens(fee.seller), d(fields[11]), "{line}");
            let mut margin = MarginMap::new(c());
            let mut position = Position::new(a("alice"));
            position.contracts = quantity;
            margin.write_position(position).unwrap();
            assert_eq!(
                margin
                    .compute_maintenance_margin_requirement(&a("alice"), &im)
                    .unwrap(),
                d(fields[12]),
                "{line}"
            );
            contexts += 1;
        } else if fields[0] == "POSITION" {
            let inverse = fields[1] == "1";
            if last_inverse != Some(inverse) {
                map = MarginMap::new(c());
                let mut position = Position::new(a("alice"));
                position.margin = d("100");
                map.write_position(position).unwrap();
                last_inverse = Some(inverse);
            }
            let position = map
                .update_contract_balances(
                    &PositionTrade {
                        address: a("alice"),
                        amount: d(fields[2]),
                        price: d(fields[3]),
                        sell: fields[4] == "1",
                        close: d(fields[5]),
                        flip: d(fields[6]),
                        in_clearing: false,
                        initial: false,
                        block: 3,
                    },
                    &ContractContext {
                        inverse,
                        notional: d(if inverse { "10000" } else { "1" }),
                        available: d("1000"),
                    },
                )
                .unwrap();
            assert_eq!(position.contracts, d(fields[7]), "{line}");
            assert_eq!(
                position.avg_price,
                if fields[8] == "null" {
                    None
                } else {
                    Some(d(fields[8]))
                },
                "{line}"
            );
            assert_eq!(
                map.compute_maintenance_margin_requirement(&a("alice"), &d(fields[9]))
                    .unwrap(),
                d(fields[10]),
                "{line}"
            );
            positions += 1;
        }
    }
    assert_eq!((contexts, positions), (48, 14));
}

#[test]
fn admitted_definition_and_dependencies_are_retained_in_actual_input_order() {
    let input = creation(false, "1");
    let mut state = before_creation(config(&input));
    let before = state.clone();
    assert!(state
        .apply_block_unverified(
            2,
            vec![EconomicInput::CreateContract(input.clone()), oracle(20)]
        )
        .is_err());
    assert_eq!(state, before);
    state
        .apply_block_unverified(
            2,
            vec![
                oracle(20),
                EconomicInput::CreateContract(input.clone()),
                publish(21, 100),
            ],
        )
        .unwrap();
    let accepted = state.contract(c()).unwrap();
    assert_eq!(accepted.height, 2);
    assert_eq!(accepted.creation_txid, input.txid);
    assert_eq!(
        accepted.definition_hash,
        contract_definition_commitment(&input).unwrap()
    );
    assert_eq!(accepted.definition.parameters, {
        let mut parameters = input.parameters.clone();
        parameters.ticker = Some(accepted.definition.ticker.clone());
        parameters
    });
    assert_eq!(state.contract_binding_state().frames.len(), 2);
    assert_eq!(
        state.contract_binding_state().frames[1].inputs[1],
        EconomicInput::CreateContract(input)
    );
    restored(&state);
}

#[test]
fn oversized_registry_text_and_nested_contract_pairs_reject_before_consuming_ids() {
    for oversized_registry in [true, false] {
        let input = creation(false, "1");
        let mut state = before_creation(config(&input));
        let before = state.clone();
        let malformed = if oversized_registry {
            vec![
                joint(
                    20,
                    "publisher",
                    JointOperation::Registry(RegistryAction::CreateOracle {
                        ticker: "x".repeat(tradelayer_core::registry::MAX_NAME_BYTES + 1),
                        url: "https://example.invalid/index".into(),
                        backup: a("backup"),
                        lag: 1,
                    }),
                ),
                EconomicInput::CreateContract(input.clone()),
            ]
        } else {
            let mut oversized = input.clone();
            oversized.parameters.on_chain_data =
                vec![
                    (ContractAsset::BaseChain, ContractAsset::Property(p()));
                    tradelayer_core::contract_registry::MAX_CONTRACT_PAIRS + 1
                ];
            vec![oracle(20), EconomicInput::CreateContract(oversized)]
        };
        assert_eq!(
            state.apply_block_unverified(2, malformed),
            Err(EconomicError::Capacity)
        );
        assert_eq!(state, before);
        assert!(state.contract(c()).is_none());
        assert!(state.joint().registry().oracle(1).is_none());
        assert_eq!(state.contract_binding_state().frames.len(), 1);

        // Reusing every external ID and allocated oracle/contract ID proves
        // the failed preflight did not publish partial dependency or replay state.
        state
            .apply_block_unverified(
                2,
                vec![
                    oracle(20),
                    EconomicInput::CreateContract(input),
                    publish(21, 100),
                ],
            )
            .unwrap();
        assert_eq!(state.contract_binding_state().contracts.len(), 1);
        restored(&state);
    }
}

#[test]
fn selected_definition_commitment_and_bootstrap_cannot_be_substituted() {
    let input = creation(false, "1");
    let cfg = config(&input);
    let mut wrong_bootstrap = cfg.clone();
    wrong_bootstrap
        .contract_binding
        .as_mut()
        .unwrap()
        .bootstrap_commitment[0] ^= 1;
    assert!(EconomicState::from_joint_unverified(wrong_bootstrap, bootstrap()).is_err());
    for field in 0..6 {
        let mut changed = input.clone();
        match field {
            0 => changed.issuer = a("another"),
            1 => changed.parameters.notional_value = d("2"),
            2 => changed.parameters.inverse = true,
            3 => changed.parameters.leverage = d("20"),
            4 => changed.txid = id(31),
            _ => changed.parameters.fee = d("1"),
        }
        let mut state = before_creation(cfg.clone());
        let before = state.clone();
        assert!(
            state
                .apply_block_unverified(2, vec![oracle(20), EconomicInput::CreateContract(changed)])
                .is_err(),
            "field {field}"
        );
        assert_eq!(state, before);
    }
}

#[test]
fn selected_runtime_assertions_must_match_the_admitted_row() {
    let input = creation(false, "1");
    for field in 0..5 {
        let mut cfg = config(&input);
        match field {
            0 => cfg.inverse = true,
            1 => cfg.notional = d("2"),
            2 => cfg.fee_route = FeeRoute::Native(1),
            3 => cfg.marking.as_mut().unwrap().oracle_id = 2,
            _ => {
                cfg.funding = Some(FundingConfig {
                    oracle_id: 2,
                    max_staleness_blocks: 32,
                    policy: FundingPolicy::ConservingFundingV1,
                })
            }
        }
        if let Ok(mut state) = EconomicState::from_joint_unverified(cfg, bootstrap()) {
            state.apply_block_unverified(1, vec![activate()]).unwrap();
            let before = state.clone();
            assert!(
                state
                    .apply_block_unverified(
                        2,
                        vec![oracle(20), EconomicInput::CreateContract(input.clone())]
                    )
                    .is_err(),
                "field {field}"
            );
            assert_eq!(state, before);
        }
    }
    let mut cfg = config(&input);
    cfg.initial_margin_per_contract = d("10");
    assert!(cfg.validate().is_err());
}

#[test]
fn native_dated_whitelisted_and_invalid_selected_definitions_fail_atomically() {
    for field in 0..8 {
        let mut input = creation(false, "1");
        match field {
            0 => input.parameters.native = true,
            1 => input.parameters.expiry_period = 144,
            2 => input.parameters.whitelist = Some(7),
            3 => input.parameters.series = 2,
            4 => input.parameters.underlying_oracle_id = Some(0),
            5 => input.parameters.fee = d("0.01"),
            6 => input.parameters.leverage = d("0"),
            _ => input.parameters.collateral_property_id = ContractAsset::BaseChain,
        }
        let cfg = config(&input);
        if let Ok(mut state) = EconomicState::from_joint_unverified(cfg, bootstrap()) {
            state.apply_block_unverified(1, vec![activate()]).unwrap();
            let before = state.clone();
            assert!(
                state
                    .apply_block_unverified(
                        2,
                        vec![oracle(20), EconomicInput::CreateContract(input)]
                    )
                    .is_err(),
                "field {field}"
            );
            assert_eq!(state, before);
        }
    }
}

#[test]
fn order_price_reserve_and_fill_price_margin_are_distinct_with_residual_custody() {
    for inverse in [false, true] {
        let mut state = new(inverse);
        state
            .apply_block_unverified(3, vec![order(300, "alice", Side::Buy, "2", "104", 3)])
            .unwrap();
        assert_eq!(
            state.reserve_atoms(id(300)),
            if inverse {
                1_923_076_924
            } else {
                2_080_000_000
            }
        );
        state
            .apply_block_unverified(4, vec![order(400, "bob", Side::Sell, "0.5", "100", 4)])
            .unwrap();
        assert_eq!(state.receipts()[0].price, d("104"));
        assert_eq!(
            state.margin_atoms(&a("alice")),
            if inverse { 480_769_231 } else { 520_000_000 }
        );
        assert_eq!(
            state.margin_atoms(&a("bob")),
            if inverse { 480_769_231 } else { 520_000_000 }
        );
        assert_eq!(
            state.reserve_atoms(id(300)),
            if inverse {
                1_442_307_693
            } else {
                1_560_000_000
            }
        );
        assert_eq!(state.reserve_atoms(id(400)), 0);
        if !inverse {
            assert_eq!(state.receipts()[0].retained_fee_atoms, 12_500);
        }
        restored(&state);
    }
}

#[test]
fn order_admission_requires_an_earlier_reference_and_strict_source_leverage_band() {
    let input = creation(false, "1");
    let mut missing = before_creation(config(&input));
    missing
        .apply_block_unverified(2, vec![oracle(20), EconomicInput::CreateContract(input)])
        .unwrap();
    let before = missing.clone();
    assert!(missing
        .apply_block_unverified(
            3,
            vec![
                order(300, "alice", Side::Buy, "1", "100", 3),
                publish(390, 100)
            ]
        )
        .is_err());
    assert_eq!(missing, before);
    missing
        .apply_block_unverified(
            3,
            vec![
                publish(390, 100),
                order(300, "alice", Side::Buy, "1", "100", 3),
            ],
        )
        .unwrap();
    assert_eq!(missing.reserve_atoms(id(300)), 10 * ATOMS);
    for price in ["95.65", "104.35", "90", "110"] {
        let mut state = new(false);
        let before = state.clone();
        assert!(
            state
                .apply_block_unverified(3, vec![order(300, "alice", Side::Buy, "1", price, 3)])
                .is_err(),
            "price {price}"
        );
        assert_eq!(state, before);
    }
    for price in ["95.65000001", "104.34999999"] {
        let mut state = new(false);
        state
            .apply_block_unverified(3, vec![order(300, "alice", Side::Buy, "1", price, 3)])
            .unwrap();
        assert_eq!(state.book().resting().len(), 1);
        restored(&state);
    }
    restored(&missing);
}

#[test]
fn numeric_price_cap_rejects_an_order_even_at_its_accepted_reference() {
    let mut state = new(false);
    let huge = 10_000_000_000_000_000_u128;
    state
        .apply_block_unverified(3, vec![publish(390, huge)])
        .unwrap();
    assert_eq!(
        state
            .joint()
            .registry()
            .oracle(1)
            .unwrap()
            .latest
            .as_ref()
            .unwrap()
            .price_1e4,
        huge * 10_000
    );
    let before = state.clone();
    // This tiny quantity would need only 100 tokens of funded margin, so
    // rejection cannot be attributed to wallet insufficiency or deviation.
    assert!(state
        .apply_block_unverified(
            4,
            vec![order(
                400,
                "alice",
                Side::Buy,
                "0.0000000000001",
                "10000000000000000",
                4
            )]
        )
        .is_err());
    assert_eq!(state, before);

    // The inclusive source maximum itself remains admitted with zero deviation.
    let mut at_maximum = new(false);
    at_maximum
        .apply_block_unverified(3, vec![publish(390, 9_007_199_254_740_991)])
        .unwrap();
    at_maximum
        .apply_block_unverified(
            4,
            vec![order(
                400,
                "alice",
                Side::Buy,
                "0.0000000000001",
                "9007199254740991",
                4,
            )],
        )
        .unwrap();
    assert!(at_maximum.reserve_atoms(id(400)) > 0);
    assert!(at_maximum.reserve_atoms(id(400)) < 1000 * ATOMS);
    restored(&at_maximum);
}

#[test]
fn fill_context_add_reduce_flip_and_close_preserve_both_risk_and_raw_pnl() {
    for inverse in [false, true] {
        let mut state = opened(inverse);
        assert_eq!(state.margin_atoms(&a("alice")), 10 * ATOMS);
        state
            .apply_block_unverified(4, pair(4, "alice", "bob", "1", "200"))
            .unwrap();
        assert_eq!(
            state.positions().get(&a("alice")).unwrap().avg_price,
            Some(d("150"))
        );
        assert_eq!(
            state.margin_atoms(&a("alice")),
            if inverse { 15 * ATOMS } else { 30 * ATOMS }
        );
        state
            .apply_block_unverified(5, pair(5, "bob", "alice", "0.5", "120"))
            .unwrap();
        assert_eq!(
            state.positions().get(&a("alice")).unwrap().avg_price,
            Some(d("150"))
        );
        assert_eq!(
            state.margin_atoms(&a("alice")),
            if inverse { 1_250_000_001 } else { 18 * ATOMS }
        );
        state
            .apply_block_unverified(6, pair(6, "bob", "alice", "2", "90"))
            .unwrap();
        let position = state.positions().get(&a("alice")).unwrap();
        assert_eq!(position.contracts, d("-0.5"));
        assert_eq!(position.avg_price, Some(d("90")));
        assert_eq!(
            state.margin_atoms(&a("alice")),
            if inverse { 555_555_556 } else { 450_000_000 }
        );
        state
            .apply_block_unverified(7, pair(7, "alice", "bob", "0.5", "100"))
            .unwrap();
        assert!(state
            .positions()
            .get(&a("alice"))
            .unwrap()
            .contracts
            .is_zero());
        assert_eq!(state.margin_atoms(&a("alice")), 0);
        assert_eq!(state.margin_atoms(&a("bob")), 0);
        if !inverse {
            assert_eq!(
                state.positions().get(&a("alice")).unwrap().realized_pnl,
                d("-110")
            );
        }
        assert_eq!(state.unfunded_loss_atoms(), 0);
        restored(&state);
    }
}

#[test]
fn maintenance_uses_source_trade_average_rather_than_fresh_mark() {
    for inverse in [false, true] {
        let mut state = opened(inverse);
        state
            .apply_block_unverified(4, pair(4, "alice", "bob", "1", "200"))
            .unwrap();
        let owner = if inverse { "alice" } else { "bob" };
        let balance = available(&state, owner);
        state
            .apply_block_unverified(
                5,
                vec![
                    send(500, owner, "dave", balance),
                    publish(501, if inverse { 50 } else { 280 }),
                ],
            )
            .unwrap();
        let call = state
            .mark_state()
            .maintenance
            .iter()
            .find(|call| call.owner == a(owner))
            .unwrap();
        assert_eq!(
            call.required_atoms,
            if inverse { 666_666_667 } else { 15 * ATOMS }
        );
        assert_eq!(call.margin_atoms, 0);
        assert_eq!(
            state.positions().get(&a(owner)).unwrap().avg_price,
            Some(d("150"))
        );
        restored(&state);
    }
}

#[test]
fn fees_use_index_valued_notional_but_mark_uses_raw_contract_notional() {
    for (inverse, raw, next_mark, expected_pnl, retained) in [
        (false, "2", 105, "10", 25_000),
        (true, "10000", 200, "50", 50_000),
    ] {
        let input = creation(inverse, raw);
        let mut state = before_creation(config(&input));
        let mut inputs = vec![
            oracle(20),
            EconomicInput::CreateContract(input),
            publish(21, 100),
        ];
        inputs.extend(pair(2, "alice", "bob", "1", "100"));
        state.apply_block_unverified(2, inputs).unwrap();
        assert_eq!(state.receipts()[0].retained_fee_atoms, retained);
        state
            .apply_block_unverified(3, vec![publish(300, next_mark)])
            .unwrap();
        assert_eq!(
            state.positions().get(&a("alice")).unwrap().realized_pnl,
            d(expected_pnl)
        );
        restored(&state);
    }
}

fn with_liquidation(mut cfg: EconomicConfig, amm: bool) -> EconomicConfig {
    cfg.liquidation = Some(LiquidationConfig {
        max_price_deviation_bps: 100,
        max_fills_per_owner: 64,
        policy: LiquidationPolicy::ConservingLiquidationV1,
    });
    if amm {
        cfg.amm = Some(FundedAmmConfig {
            pool_id: 7,
            spread_bps: 40,
            max_quote_quantity: d("0.5"),
            max_abs_inventory: d("2"),
            max_collateral_atoms: 1000 * ATOMS,
            policy: FundedAmmPolicy::ConservingFundedContractAmmV1,
        });
    }
    cfg
}
#[test]
fn amm_bid_and_ask_reserves_each_use_their_own_actual_quote_price() {
    let input = creation(false, "1");
    let mut state = before_creation(with_liquidation(config(&input), true));
    state
        .apply_block_unverified(
            2,
            vec![
                oracle(20),
                EconomicInput::CreateContract(input),
                EconomicInput::Amm(AmmCapitalInput::Deposit {
                    txid: id(40),
                    owner: a("dave"),
                    collateral_atoms: 100 * ATOMS,
                    min_share_units: 100 * ATOMS,
                }),
                publish(21, 100),
            ],
        )
        .unwrap();
    assert_eq!(state.amm_state().quotes.len(), 2);
    let epoch = state
        .amm_state()
        .frames
        .last()
        .unwrap()
        .quote_epoch
        .as_ref()
        .unwrap();
    assert!(epoch.position_before.is_zero());
    assert_eq!(epoch.average_price_before, None);
    for quote in &state.amm_state().quotes {
        assert_eq!(quote.remaining, d("0.5"));
        assert_eq!(
            (quote.price.clone(), quote.reserve_atoms),
            match quote.side {
                Side::Buy => (d("99.6"), 498_000_000),
                Side::Sell => (d("100.4"), 502_000_000),
            }
        );
    }
    assert_eq!(state.amm_state().cash_atoms, 90 * ATOMS);
    state
        .apply_block_unverified(3, vec![publish(300, 120)])
        .unwrap();
    assert_eq!(state.amm_state().cash_atoms, 88 * ATOMS);
    for quote in &state.amm_state().quotes {
        assert_eq!(
            quote.reserve_atoms,
            if quote.side == Side::Buy {
                597_600_000
            } else {
                602_400_000
            }
        );
    }
    restored(&state);
}

#[test]
fn closed_oracle_allows_quiescent_lp_exit_but_rejects_new_pool_risk() {
    let input = creation(false, "1");
    let mut state = before_creation(with_liquidation(config(&input), true));
    state
        .apply_block_unverified(
            2,
            vec![
                oracle(20),
                EconomicInput::CreateContract(input),
                EconomicInput::Amm(AmmCapitalInput::Deposit {
                    txid: id(40),
                    owner: a("dave"),
                    collateral_atoms: 100 * ATOMS,
                    min_share_units: 100 * ATOMS,
                }),
                publish(21, 100),
            ],
        )
        .unwrap();
    let before = state.clone();
    let close = joint(
        300,
        "publisher",
        JointOperation::Registry(RegistryAction::CloseOracle { oracle_id: 1 }),
    );
    let deposit = EconomicInput::Amm(AmmCapitalInput::Deposit {
        txid: id(301),
        owner: a("carol"),
        collateral_atoms: 100 * ATOMS,
        min_share_units: 100 * ATOMS,
    });
    assert!(state
        .apply_block_unverified(3, vec![close.clone(), deposit.clone()])
        .is_err());
    assert_eq!(state, before);
    state.apply_block_unverified(3, vec![close]).unwrap();
    assert!(state.amm_state().quotes.is_empty());
    assert_eq!(state.amm_state().cash_atoms, 100 * ATOMS);
    let closed = state.clone();
    assert!(state.apply_block_unverified(4, vec![deposit]).is_err());
    assert_eq!(state, closed);
    assert!(state
        .apply_block_unverified(4, vec![order(400, "alice", Side::Buy, "1", "100", 4)])
        .is_err());
    assert_eq!(state, closed);
    state
        .apply_block_unverified(
            4,
            vec![EconomicInput::Amm(AmmCapitalInput::Redeem {
                txid: id(401),
                owner: a("dave"),
                share_units: 100 * ATOMS,
                min_collateral_atoms: 100 * ATOMS,
            })],
        )
        .unwrap();
    assert_eq!(available(&state, "dave"), 1000 * ATOMS);
    assert_eq!(state.amm_state().cash_atoms, 0);
    assert_eq!(state.amm_state().total_units, 0);
    restored(&state);
}

#[test]
fn liquidation_counterparty_uses_fill_context_and_cannot_charge_an_unfunded_fee() {
    let input = creation(false, "1");
    let mut state = before_creation(with_liquidation(config(&input), false));
    state
        .apply_block_unverified(
            2,
            vec![
                oracle(20),
                EconomicInput::CreateContract(input),
                publish(21, 100),
            ],
        )
        .unwrap();
    state
        .apply_block_unverified(3, pair(3, "alice", "bob", "1", "100"))
        .unwrap();
    let balance = available(&state, "bob");
    state
        .apply_block_unverified(
            4,
            vec![
                send(400, "bob", "dave", balance),
                publish(402, 120),
                order(401, "carol", Side::Sell, "1", "120", 4),
            ],
        )
        .unwrap();
    let event = state
        .liquidation_state()
        .events
        .iter()
        .find(|event| event.owner == a("bob"))
        .unwrap();
    assert_eq!(event.average_price_before, Some(d("100")));
    assert_eq!(event.required_before_atoms, 5 * ATOMS);
    assert_eq!(event.status, LiquidationStatus::Closed);
    assert_eq!(event.fills.len(), 1);
    assert!(event.amm_fills.is_empty());
    assert_eq!(event.fills[0].price, d("120"));
    assert_eq!(event.fills[0].nominal_fee_atoms, 50_000);
    assert_eq!(event.fills[0].charged_fee_atoms, 0);
    assert_eq!(state.margin_atoms(&a("carol")), 12 * ATOMS);
    assert_eq!(state.reserve_atoms(id(401)), 0);
    assert_eq!(state.receipts().len(), 1);
    restored(&state);
}

#[test]
fn amm_forced_counterparty_posts_real_margin_at_execution_price() {
    let input = creation(false, "1");
    let mut state = before_creation(with_liquidation(config(&input), true));
    state
        .apply_block_unverified(
            2,
            vec![
                oracle(20),
                EconomicInput::CreateContract(input),
                publish(21, 100),
            ],
        )
        .unwrap();
    state
        .apply_block_unverified(3, pair(3, "alice", "bob", "1", "100"))
        .unwrap();
    let balance = available(&state, "bob");
    state
        .apply_block_unverified(
            4,
            vec![
                send(400, "bob", "dave", balance),
                EconomicInput::Amm(AmmCapitalInput::Deposit {
                    txid: id(401),
                    owner: a("dave"),
                    collateral_atoms: 100 * ATOMS,
                    min_share_units: 100 * ATOMS,
                }),
                publish(402, 110),
            ],
        )
        .unwrap();
    let pool = state.amm_owner().unwrap();
    let event = state
        .liquidation_state()
        .events
        .iter()
        .find(|event| event.owner == a("bob"))
        .unwrap();
    assert_eq!(event.amm_fills.len(), 1);
    assert_eq!(event.amm_fills[0].price, d("110.44"));
    assert_eq!(event.amm_fills[0].quantity, d("0.5"));
    assert_eq!(event.amm_fills[0].charged_fee_atoms, 0);
    assert_eq!(state.positions().get(&pool).unwrap().contracts, d("-0.5"));
    assert_eq!(state.margin_atoms(&pool), 552_200_000);
    assert_eq!(state.receipts().len(), 1);
    restored(&state);
}

#[test]
fn due_funding_uses_the_bound_oracle_and_index_valued_raw_notional() {
    let input = creation(false, "2");
    let mut cfg = config(&input);
    cfg.funding = Some(FundingConfig {
        oracle_id: 1,
        max_staleness_blocks: 32,
        policy: FundingPolicy::ConservingFundingV1,
    });
    let mut state = before_creation(cfg);
    state
        .apply_block_unverified(
            2,
            vec![
                oracle(20),
                EconomicInput::CreateContract(input),
                publish(21, 100),
            ],
        )
        .unwrap();
    state
        .apply_block_unverified(3, pair(3, "alice", "bob", "0.5", "100"))
        .unwrap();
    for height in 4..24 {
        state.apply_block_unverified(height, vec![]).unwrap();
    }
    state
        .apply_block_unverified(24, vec![publish(2400, 110)])
        .unwrap();
    let epoch = &state.funding_state().epochs[0];
    assert_eq!(epoch.status, FundingStatus::Paid);
    let plan = epoch.plan.as_ref().unwrap();
    assert_eq!(plan.collateral_notional, d("220"));
    assert_eq!(plan.rate_bps, d("12.5"));
    assert_eq!(plan.total_atoms, 13_750_000);
    assert_eq!(plan.debits[0].owner, a("alice"));
    assert_eq!(plan.credits[0].owner, a("bob"));
    assert_eq!(state.mark_state().events.last().unwrap().height, 24);
    restored(&state);
}

#[test]
fn late_failure_rolls_back_catalog_dependencies_book_and_funded_margin_together() {
    let input = creation(false, "1");
    let mut state = before_creation(config(&input));
    let before = state.clone();
    let mut inputs = vec![
        oracle(20),
        EconomicInput::CreateContract(input.clone()),
        publish(21, 100),
    ];
    inputs.extend(pair(2, "alice", "bob", "1", "100"));
    // All inputs are staged before queued matching; this consumes Alice's
    // wallet after order capture and forces a late physical funding failure.
    inputs.push(send(299, "alice", "dave", 1000 * ATOMS));
    assert!(state.apply_block_unverified(2, inputs).is_err());
    assert_eq!(state, before);
    let mut retry = vec![
        oracle(20),
        EconomicInput::CreateContract(input),
        publish(21, 100),
    ];
    retry.extend(pair(2, "alice", "bob", "1", "100"));
    state.apply_block_unverified(2, retry).unwrap();
    assert_eq!(state.receipts().len(), 1);
    assert_eq!(state.contract_binding_state().contracts.len(), 1);
    restored(&state);
}

#[test]
fn full_chain_rollback_removes_creation_and_replays_its_same_ids_once() {
    let input = creation(false, "1");
    let mut chain = EconomicChain::new_unverified(
        EconomicChainConfig {
            economics: config(&input),
            admin: a("admin"),
            permitted_spot_properties: BTreeSet::from([p()]),
            rollback_limit: 8,
        },
        ledger(),
    )
    .unwrap();
    let block = |height, parent, number, inputs| UnverifiedEconomicBlock {
        network: TradeLayerMainnet::Litecoin,
        header: BlockHeader {
            height,
            parent,
            id: BlockId([number; 32]),
        },
        inputs,
    };
    chain
        .apply_unverified_block(&block(1, chain.tip().id, 101, vec![activate()]))
        .unwrap();
    let before = chain.state().clone();
    let inputs = vec![
        oracle(20),
        EconomicInput::CreateContract(input),
        publish(21, 100),
    ];
    chain
        .apply_unverified_block(&block(2, chain.tip().id, 102, inputs.clone()))
        .unwrap();
    let admitted = chain.state().clone();
    chain.rollback_tip(chain.tip()).unwrap();
    assert_eq!(chain.state(), &before);
    assert!(chain.state().contract(c()).is_none());
    chain
        .apply_unverified_block(&block(2, chain.tip().id, 103, inputs))
        .unwrap();
    assert_eq!(chain.state(), &admitted);
    assert_conserved(chain.state());
}

#[test]
fn accepted_oracle_catalog_retains_finite_and_empty_maturity_metadata() {
    let mut state = new(false);
    let mut finite = creation(false, "2");
    finite.txid = id(40);
    finite.parameters.expiry_period = 10;
    finite.parameters.series = 3;
    finite.parameters.whitelist = Some(91);
    let mut empty = creation(false, "3");
    empty.txid = id(41);
    empty.parameters.expiry_period = 20;
    empty.parameters.series = 0;
    state
        .apply_block_unverified(
            3,
            vec![
                EconomicInput::CreateContract(finite.clone()),
                EconomicInput::CreateContract(empty),
            ],
        )
        .unwrap();
    let second = ContractId::try_from(2).unwrap();
    let row = state.contract(second).unwrap();
    assert_eq!(row.creation_txid, finite.txid);
    assert_eq!(
        row.definition_hash,
        contract_definition_commitment(&finite).unwrap()
    );
    assert_eq!(row.definition.parameters.whitelist, Some(91));
    assert_eq!(
        row.definition
            .unexpired
            .iter()
            .map(|c| c.expiration_block)
            .collect::<Vec<_>>(),
        vec![13, 23, 33]
    );
    assert!(state
        .contract(3.try_into().unwrap())
        .unwrap()
        .definition
        .unexpired
        .is_empty());
    let mut detached = state.accepted_contract_catalog().unwrap();
    assert_eq!(detached.all_contracts().count(), 3);
    // The source ticker contains PERP even for finite definitions; retain this
    // query quirk without using it to grant trading permission.
    assert!(detached.all_perpetual_contracts().contains(&second));
    detached
        .create_contract_series(
            a("other"),
            parameters(false, "5"),
            TickerContext {
                oracle_ticker: Some("OTHER"),
                ..TickerContext::default()
            },
            3,
        )
        .unwrap();
    assert!(state.contract(4.try_into().unwrap()).is_none());
    let before = state.clone();
    let EconomicInput::Order(mut forbidden) = order(42, "alice", Side::Buy, "1", "100", 4) else {
        unreachable!()
    };
    forbidden.contract = second;
    assert!(state
        .apply_block_unverified(4, vec![EconomicInput::Order(forbidden)])
        .is_err());
    assert_eq!(state, before);
    restored(&state);
}

#[test]
fn finite_metadata_generation_failure_preserves_ids_and_prior_catalog() {
    let mut state = new(false);
    let before = state.clone();
    let mut invalid = creation(false, "1");
    invalid.txid = id(45);
    invalid.parameters.expiry_period = tradelayer_core::contract_registry::MAX_JS_SAFE_HEIGHT;
    invalid.parameters.series = 2;
    assert!(state
        .apply_block_unverified(3, vec![EconomicInput::CreateContract(invalid.clone())])
        .is_err());
    assert_eq!(state, before);
    invalid.parameters.expiry_period = 7;
    state
        .apply_block_unverified(3, vec![EconomicInput::CreateContract(invalid)])
        .unwrap();
    assert_eq!(
        state
            .contract(2.try_into().unwrap())
            .unwrap()
            .definition
            .unexpired
            .iter()
            .map(|c| c.expiration_block)
            .collect::<Vec<_>>(),
        vec![10, 17]
    );
    restored(&state);
}
