//! Real sole-owner read views and unchanged-source facade observations.
use std::collections::BTreeSet;
use tradelayer_core::chain::{BlockHeader, BlockId};
use tradelayer_core::client::RpcValue;
use tradelayer_core::contract_orderbook::{Order, Side};
use tradelayer_core::decimal::Decimal;
use tradelayer_core::economic::{
    AmmCapitalInput, EconomicConfig, EconomicInput, FundedAmmConfig, FundedAmmPolicy,
    FundingConfig, FundingPolicy, LiquidationConfig, LiquidationPolicy, MarkConfig, MarkPolicy,
    SettlementPolicy,
};
use tradelayer_core::economic_chain::{
    DurableEconomicChain, EconomicChain, EconomicChainConfig, UnverifiedEconomicBlock,
};
use tradelayer_core::joint::{JointOperation, UnverifiedJointTx};
use tradelayer_core::ledger::{AccountId, Ledger, Operation, PropertyId, PropertyKind, TxId};
use tradelayer_core::network_profile::TradeLayerMainnet;
use tradelayer_core::registry::RegistryAction;
use tradelayer_core::service::{ServiceBackend, ServiceCall, ServiceError};
use tradelayer_core::service_view::{
    audit_lines, EconomicReadBackend, InterfaceFacade, InterfaceQuery, MAX_AUDIT_ROWS,
};
use tradelayer_core::tally::FeeRoute;

const ATOMS: u128 = 100_000_000;
fn a(s: &str) -> AccountId {
    s.try_into().unwrap()
}
fn p() -> PropertyId {
    7.try_into().unwrap()
}
fn d(s: &str) -> Decimal {
    s.parse().unwrap()
}
fn id(n: u64) -> TxId {
    let mut b = [0; 32];
    b[24..].copy_from_slice(&n.to_be_bytes());
    TxId::from_bytes(b)
}
fn text(s: &str) -> RpcValue {
    RpcValue::string(s).unwrap()
}
fn n(n: u64) -> RpcValue {
    RpcValue::unsigned(n)
}
fn field(v: &RpcValue, key: &str) -> RpcValue {
    v.field(key).unwrap().unwrap()
}
fn call(b: &mut impl ServiceBackend, target: &'static str, arguments: Vec<RpcValue>) -> RpcValue {
    b.call(ServiceCall { target, arguments }).unwrap()
}
fn joint(n: u64, actor: &str, operation: JointOperation) -> EconomicInput {
    EconomicInput::Joint(UnverifiedJointTx {
        txid: id(n),
        actor: a(actor),
        operation,
    })
}
fn publish(n: u64, price: u128) -> EconomicInput {
    joint(
        n,
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
fn order(
    n: u64,
    owner: &str,
    side: Side,
    quantity: &str,
    price: &str,
    height: u64,
) -> EconomicInput {
    EconomicInput::Order(Order {
        txid: id(n),
        owner: a(owner),
        contract: 1.try_into().unwrap(),
        height,
        side,
        quantity: d(quantity),
        price: d(price),
        post: false,
        is_market: false,
    })
}
fn config(network: TradeLayerMainnet, amm: bool) -> EconomicConfig {
    EconomicConfig {
        native_entitlements: None,
        insurance: None,
        source_consensus: None,
        network,
        contract: 1.try_into().unwrap(),
        collateral: p(),
        inverse: false,
        notional: d("1"),
        initial_margin_per_contract: d("10"),
        fee_route: FeeRoute::Oracle(1),
        policy: SettlementPolicy::ConservingV1,
        funding: Some(FundingConfig {
            oracle_id: 1,
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
        amm: amm.then_some(FundedAmmConfig {
            pool_id: 7,
            spread_bps: 40,
            max_quote_quantity: d("0.5"),
            max_abs_inventory: d("2"),
            max_collateral_atoms: 1000 * ATOMS,
            policy: FundedAmmPolicy::ConservingFundedContractAmmV1,
        }),
        contract_binding: None,
    }
}
fn bootstrap_ledger() -> Ledger {
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
    for (n, owner) in [(2, "bob"), (3, "carol"), (4, "dave")] {
        ledger
            .apply(
                id(n),
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
fn apply(chain: &mut EconomicChain, inputs: Vec<EconomicInput>) {
    let height = chain.tip().height + 1;
    chain
        .apply_unverified_block(&UnverifiedEconomicBlock {
            network: chain.config().economics.network,
            header: BlockHeader {
                height,
                parent: chain.tip().id,
                id: BlockId([height as u8; 32]),
            },
            inputs,
        })
        .unwrap();
}
fn opened(network: TradeLayerMainnet, amm: bool) -> EconomicChain {
    let mut chain = EconomicChain::new_unverified(
        EconomicChainConfig {
            economics: config(network, amm),
            admin: a("admin"),
            permitted_spot_properties: BTreeSet::from([p()]),
            rollback_limit: 4,
        },
        bootstrap_ledger(),
    )
    .unwrap();
    apply(
        &mut chain,
        vec![joint(
            10,
            "admin",
            JointOperation::Registry(RegistryAction::Activate {
                types: vec![2, 6, 10, 13, 14, 15, 18],
                code_hash: [0x76; 32],
            }),
        )],
    );
    let mut inputs = vec![
        joint(
            11,
            "publisher",
            JointOperation::Registry(RegistryAction::CreateOracle {
                ticker: "MARK".into(),
                url: "https://example.invalid/mark".into(),
                backup: a("backup"),
                lag: 1,
            }),
        ),
        publish(12, 100),
    ];
    if amm {
        inputs.push(EconomicInput::Amm(AmmCapitalInput::Deposit {
            txid: id(13),
            owner: a("dave"),
            collateral_atoms: 100 * ATOMS,
            min_share_units: 0,
        }));
        inputs.push(order(220, "carol", Side::Sell, "1", "119", 2));
    }
    inputs.extend([
        order(200, "alice", Side::Buy, "1", "100", 2),
        order(201, "bob", Side::Sell, "1", "100", 2),
    ]);
    apply(&mut chain, inputs);
    chain
}

#[test]
fn actual_fills_balances_positions_and_routes_are_exact_and_read_only() {
    for network in [
        TradeLayerMainnet::Bitcoin,
        TradeLayerMainnet::Litecoin,
        TradeLayerMainnet::Dogecoin,
    ] {
        let chain = opened(network, false);
        let before = chain.checkpoint_bytes().unwrap();
        let mut backend = EconomicReadBackend::from_chain(&chain).unwrap();
        let balances = call(&mut backend, "ledger.balances", vec![text("alice")])
            .as_array()
            .unwrap();
        assert_eq!(
            field(&balances[0], "availableAtoms").as_string().unwrap(),
            chain
                .state()
                .ledger()
                .available_atoms(p(), &a("alice"))
                .to_string()
        );
        assert_eq!(
            field(&balances[0], "contractMarginAtoms")
                .as_string()
                .unwrap(),
            "1000000000"
        );
        let position = call(&mut backend, "wallet.position", vec![text("alice"), n(1)]);
        assert_eq!(field(&position, "contracts").as_string().unwrap(), "1");
        assert_eq!(field(&position, "averagePrice").as_string().unwrap(), "100");
        assert_eq!(
            call(&mut backend, "wallet.positions", vec![text("absent")]).as_json(),
            "[]"
        );
        let trades = call(&mut backend, "history.contract", vec![n(1)])
            .as_array()
            .unwrap();
        assert_eq!(trades.len(), 1);
        assert_eq!(field(&trades[0], "buyer").as_string().unwrap(), "alice");
        assert_eq!(field(&trades[0], "price").as_string().unwrap(), "100");
        assert_eq!(
            field(&field(&trades[0], "origin"), "kind")
                .as_string()
                .unwrap(),
            "orders"
        );
        assert_eq!(
            call(
                &mut backend,
                "history.contract_for_address",
                vec![n(1), text("dave")]
            )
            .as_json(),
            "[]"
        );
        let addresses = call(&mut backend, "ledger.addresses_with_property", vec![n(7)])
            .as_array()
            .unwrap();
        assert_eq!(addresses.len(), 4);
        for row in &addresses {
            assert!(!field(row, "address").as_string().unwrap().starts_with('~'));
        }
        assert!(!call(&mut backend, "ledger.custody", vec![])
            .field("privateAvailable")
            .unwrap()
            .unwrap()
            .as_array()
            .unwrap()
            .is_empty());
        let heights = call(&mut backend, "runtime.heights", vec![]);
        assert_eq!(field(&heights, "maxProcessedHeight").as_u64().unwrap(), 2);
        assert_eq!(field(&heights, "maxParsedHeight").as_json(), "null");
        assert_eq!(field(&heights, "trackHeight").as_json(), "null");
        assert_eq!(
            call(&mut backend, "registry.oracles", vec![])
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            call(&mut backend, "registry.activations", vec![])
                .as_array()
                .unwrap()
                .len(),
            7
        );
        assert_eq!(
            field(
                &call(&mut backend, "ledger.property", vec![n(7)]),
                "totalSupplyAtoms"
            )
            .as_string()
            .unwrap(),
            "400000000000"
        );
        let mut facade = InterfaceFacade::new(&mut backend);
        assert_eq!(
            facade
                .query(InterfaceQuery::IsActive(18))
                .unwrap()
                .as_json(),
            "true"
        );
        assert_eq!(
            field(
                &facade.query(InterfaceQuery::FeatureStatus(999)).unwrap(),
                "status"
            )
            .as_string()
            .unwrap(),
            "inactive"
        );
        assert!(!facade.tally_audit_bytes().unwrap().is_empty());
        assert_eq!(chain.checkpoint_bytes().unwrap(), before);
    }
}

#[test]
fn real_forced_amm_and_book_origins_marks_and_iou_are_retained() {
    let mut chain = opened(TradeLayerMainnet::Litecoin, true);
    let available = chain.state().ledger().available_atoms(p(), &a("bob"));
    apply(
        &mut chain,
        vec![
            joint(
                30,
                "bob",
                JointOperation::Send {
                    id: p(),
                    recipient: a("dave"),
                    amount_atoms: available,
                },
            ),
            publish(31, 120),
        ],
    );
    let before = chain.checkpoint_bytes().unwrap();
    let mut backend = EconomicReadBackend::from_chain(&chain).unwrap();
    let rows = call(&mut backend, "history.contract", vec![n(1)])
        .as_array()
        .unwrap();
    assert_eq!(rows.len(), 3);
    let kinds: Vec<_> = rows
        .iter()
        .map(|r| field(&field(r, "origin"), "kind").as_string().unwrap())
        .collect();
    assert_eq!(kinds, ["orders", "forced_amm", "forced_book"]);
    assert_eq!(
        field(&field(&rows[1], "origin"), "poolId")
            .as_u64()
            .unwrap(),
        7
    );
    let pool = chain.state().amm_owner().unwrap();
    assert!(rows[1].as_json().contains(pool.as_str()));
    assert_eq!(field(&rows[1], "quantity").as_string().unwrap(), "0.5");
    let marks = call(&mut backend, "history.clearing", vec![n(1)])
        .as_array()
        .unwrap();
    assert_eq!(marks.len(), 5); // opening trade+mark, then mark+AMM+book.
    let at3 = call(&mut backend, "interface.audit_data", vec![n(3), n(1)])
        .as_array()
        .unwrap();
    assert_eq!(at3.len(), 3);
    assert_eq!(
        field(&field(&at3[1], "event"), "kind").as_string().unwrap(),
        "forced_amm"
    );
    let claims = call(&mut backend, "iou.claims", vec![]).as_array().unwrap();
    assert_eq!(claims.len(), chain.state().ious().claim_count());
    let fees = call(&mut backend, "fees.all", vec![]).as_array().unwrap();
    assert!(!fees.is_empty());
    assert!(matches!(
        backend.call(ServiceCall {
            target: "ledger.balances",
            arguments: vec![text(pool.as_str())]
        }),
        Err(ServiceError::Unavailable("private_custody_account"))
    ));
    assert_eq!(chain.checkpoint_bytes().unwrap(), before);
}

#[test]
fn funding_history_uses_actual_paid_epoch_and_retained_oracle_window() {
    let mut chain = opened(TradeLayerMainnet::Bitcoin, false);
    while chain.tip().height < 22 {
        apply(&mut chain, vec![]);
    }
    apply(&mut chain, vec![publish(230, 101)]);
    apply(&mut chain, vec![]);
    let before = chain.checkpoint_bytes().unwrap();
    let epoch = &chain.state().funding_state().epochs[0];
    let mut backend = EconomicReadBackend::from_chain(&chain).unwrap();
    let rows = call(
        &mut backend,
        "history.funding",
        vec![n(1), n(24), RpcValue::null()],
    )
    .as_array()
    .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(field(&rows[0], "status").as_string().unwrap(), "Paid");
    assert_eq!(
        field(&field(&rows[0], "plan"), "totalAtoms")
            .as_string()
            .unwrap(),
        epoch.plan.as_ref().unwrap().total_atoms.to_string()
    );
    let obs = call(&mut backend, "history.oracle", vec![n(1)])
        .as_array()
        .unwrap();
    assert_eq!(obs.len(), 2);
    assert_eq!(field(&obs[1], "price").as_string().unwrap(), "101");
    assert_eq!(chain.checkpoint_bytes().unwrap(), before);
}

#[test]
fn unavailable_capabilities_and_invalid_queries_never_become_empty_success() {
    let chain = opened(TradeLayerMainnet::Litecoin, false);
    let mut b = EconomicReadBackend::from_chain(&chain).unwrap();
    for target in [
        "transactions.get",
        "transactions.block",
        "channel.predict_column",
        "issuance.intents",
        "procedural.bitvm_status",
        "synthetic.max",
        "wallet.load",
        "runtime.initialize",
        "interface.consensus_hash",
        "interface.insurance_payouts",
        "history.spot_for_address",
        "contract.list",
    ] {
        assert!(
            matches!(
                b.call(ServiceCall {
                    target,
                    arguments: vec![]
                }),
                Err(ServiceError::Unavailable(_))
            ),
            "{target}"
        );
    }
    // The catalog target validates arguments before reporting capability absence.
    for (target, arguments) in [
        ("ledger.balances", vec![]),
        ("ledger.property", vec![n(0)]),
        ("wallet.positions", vec![text("not an AccountId")]),
        ("contract.initial_margin", vec![n(1), text("NaN")]),
        ("history.funding", vec![n(1), n(30), n(20)]),
    ] {
        assert_eq!(
            b.call(ServiceCall { target, arguments }),
            Err(ServiceError::InvalidRequest),
            "{target}"
        );
    }
    assert_eq!(
        b.call(ServiceCall {
            target: "ledger.property",
            arguments: vec![n(99)]
        }),
        Err(ServiceError::NotFound)
    );
    assert_eq!(
        b.call(ServiceCall {
            target: "contract.book",
            arguments: vec![n(99)]
        }),
        Err(ServiceError::Unavailable("contract_runtime"))
    );
    assert_eq!(
        b.call(ServiceCall {
            target: "invented",
            arguments: vec![]
        }),
        Err(ServiceError::NotFound)
    );
    assert_eq!(
        InterfaceQuery::BlockRange(3, 2).service_call(),
        Err(ServiceError::InvalidRequest)
    );
}

#[test]
fn accepted_definition_and_source_dynamic_initial_margin_are_real_catalog_data() {
    use tradelayer_core::contract_registry::{ContractAsset, ContractParameters};
    use tradelayer_core::economic::{
        contract_definition_commitment, ContractBindingConfig, ContractRiskPolicy,
        CreateContractInput, EconomicState,
    };
    use tradelayer_core::joint::JointProtocolState;
    let creation = CreateContractInput {
        txid: id(30),
        issuer: a("issuer"),
        parameters: ContractParameters {
            native: false,
            underlying_oracle_id: Some(1),
            on_chain_data: vec![],
            notional_property_id: ContractAsset::Property(8.try_into().unwrap()),
            notional_value: d("1"),
            collateral_property_id: ContractAsset::Property(p()),
            leverage: d("10"),
            expiry_period: 0,
            series: 1,
            inverse: false,
            fee: Decimal::zero(),
            whitelist: Some(0),
            ticker: None,
        },
    };
    let bootstrap = JointProtocolState::from_bootstrap_unverified(
        TradeLayerMainnet::Litecoin,
        a("admin"),
        bootstrap_ledger(),
        BTreeSet::from([p()]),
    )
    .unwrap();
    let mut cfg = config(TradeLayerMainnet::Litecoin, false);
    cfg.initial_margin_per_contract = Decimal::zero();
    cfg.funding = None;
    cfg.liquidation = None;
    cfg.contract_binding = Some(ContractBindingConfig {
        creation_txid: creation.txid,
        definition_hash: contract_definition_commitment(&creation).unwrap(),
        bootstrap_commitment: EconomicState::contract_bootstrap_commitment(&bootstrap).unwrap(),
        policy: ContractRiskPolicy::SourceContextsConservingV1,
    });
    let mut chain = EconomicChain::new_unverified(
        EconomicChainConfig {
            economics: cfg,
            admin: a("admin"),
            permitted_spot_properties: BTreeSet::from([p()]),
            rollback_limit: 4,
        },
        bootstrap_ledger(),
    )
    .unwrap();
    apply(
        &mut chain,
        vec![joint(
            10,
            "admin",
            JointOperation::Registry(RegistryAction::Activate {
                types: vec![13, 14, 16, 18],
                code_hash: [2; 32],
            }),
        )],
    );
    apply(
        &mut chain,
        vec![
            joint(
                11,
                "publisher",
                JointOperation::Registry(RegistryAction::CreateOracle {
                    ticker: "MARK".into(),
                    url: "".into(),
                    backup: a("backup"),
                    lag: 1,
                }),
            ),
            publish(12, 100),
            EconomicInput::CreateContract(creation),
        ],
    );
    let mut backend = EconomicReadBackend::from_chain(&chain).unwrap();
    let contracts = call(&mut backend, "contract.list", vec![])
        .as_array()
        .unwrap();
    assert_eq!(contracts.len(), 1);
    assert_eq!(field(&contracts[0], "leverage").as_string().unwrap(), "10");
    let im = call(
        &mut backend,
        "contract.initial_margin",
        vec![n(1), text("200")],
    );
    assert_eq!(im.as_string().unwrap(), "20");
    assert_eq!(
        call(&mut backend, "contract.info", vec![n(1)]),
        contracts[0]
    );
    assert_eq!(
        backend.call(ServiceCall {
            target: "contract.info",
            arguments: vec![n(99)]
        }),
        Err(ServiceError::NotFound)
    );
}

#[test]
fn durable_view_borrows_health_checked_owner_without_mutation() {
    let chain = opened(TradeLayerMainnet::Litecoin, false);
    let tip = chain.tip();
    let temp_root = std::fs::canonicalize(std::env::temp_dir()).unwrap();
    let filename = format!(
        "tl-service-view-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let root = temp_root.join(&filename);
    std::fs::create_dir(&root).unwrap();
    let owner = DurableEconomicChain::initialize(&root, chain, tip).unwrap();
    let before = owner.head().unwrap();
    {
        let mut b = EconomicReadBackend::from_durable(&owner).unwrap();
        assert_eq!(
            field(&call(&mut b, "runtime.status", vec![]), "height")
                .as_u64()
                .unwrap(),
            2
        );
        assert_eq!(owner.head().unwrap(), before);
    }
    drop(owner);
    let resolved_root = std::fs::canonicalize(&root).unwrap();
    assert!(resolved_root.is_absolute());
    assert!(resolved_root.starts_with(&temp_root));
    assert_eq!(resolved_root.parent(), Some(temp_root.as_path()));
    assert!(filename.starts_with("tl-service-view-"));
    assert_eq!(
        resolved_root.file_name(),
        Some(std::ffi::OsStr::new(&filename))
    );
    std::fs::remove_dir_all(resolved_root).unwrap();
}

#[test]
fn exact_large_supply_and_zero_available_spot_owner_keep_owned_custody_separate() {
    use tradelayer_core::spot::SpotIntent;
    let mut ledger = bootstrap_ledger();
    let huge = u128::MAX;
    let huge_id: PropertyId = 9.try_into().unwrap();
    let other: PropertyId = 8.try_into().unwrap();
    ledger
        .apply(
            id(90),
            Operation::CreateProperty {
                id: huge_id,
                issuer: a("quo\"te\\owner"),
                kind: PropertyKind::Fixed,
                initial_atoms: huge,
            },
        )
        .unwrap();
    ledger
        .apply(
            id(91),
            Operation::CreateProperty {
                id: other,
                issuer: a("alice"),
                kind: PropertyKind::Fixed,
                initial_atoms: 100 * ATOMS,
            },
        )
        .unwrap();
    let mut chain = EconomicChain::new_unverified(
        EconomicChainConfig {
            economics: config(TradeLayerMainnet::Litecoin, false),
            admin: a("admin"),
            permitted_spot_properties: BTreeSet::from([p(), other]),
            rollback_limit: 4,
        },
        ledger,
    )
    .unwrap();
    apply(
        &mut chain,
        vec![joint(
            10,
            "admin",
            JointOperation::Registry(RegistryAction::Activate {
                types: vec![5],
                code_hash: [2; 32],
            }),
        )],
    );
    apply(
        &mut chain,
        vec![joint(
            20,
            "carol",
            JointOperation::SpotPlace(SpotIntent {
                offered: p(),
                desired: other,
                offered_atoms: (1000 * ATOMS) as u64,
                expected_atoms: (1000 * ATOMS) as u64,
                post: false,
                stop: false,
            }),
        )],
    );
    {
        let before = chain.checkpoint_bytes().unwrap();
        let mut b = EconomicReadBackend::from_chain(&chain).unwrap();
        let properties = call(&mut b, "ledger.property", vec![n(9)]);
        assert_eq!(
            field(&properties, "totalSupplyAtoms").as_string().unwrap(),
            huge.to_string()
        );
        assert_eq!(
            field(&properties, "issuer").as_string().unwrap(),
            "quo\"te\\owner"
        );
        let balances = call(&mut b, "ledger.balances", vec![text("carol")])
            .as_array()
            .unwrap();
        let row = balances
            .iter()
            .find(|r| field(r, "propertyId").as_u64().unwrap() == 7)
            .unwrap();
        assert_eq!(field(row, "availableAtoms").as_string().unwrap(), "0");
        assert_eq!(
            field(row, "spotReserveAtoms").as_string().unwrap(),
            "100000000000"
        );
        assert_eq!(
            field(row, "ownedAtoms").as_string().unwrap(),
            "100000000000"
        );
        let owners = call(&mut b, "ledger.addresses_with_property", vec![n(7)])
            .as_array()
            .unwrap();
        assert!(owners
            .iter()
            .any(|r| field(r, "address").as_string().unwrap() == "carol"));
        assert_eq!(
            call(&mut b, "spot.book", vec![n(7), n(8)])
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(chain.checkpoint_bytes().unwrap(), before);
    }
    apply(
        &mut chain,
        vec![joint(
            21,
            "alice",
            JointOperation::SpotPlace(SpotIntent {
                offered: other,
                desired: p(),
                offered_atoms: (100 * ATOMS) as u64,
                expected_atoms: (100 * ATOMS) as u64,
                post: false,
                stop: false,
            }),
        )],
    );
    let mut b = EconomicReadBackend::from_chain(&chain).unwrap();
    assert_eq!(
        call(&mut b, "history.spot", vec![n(7), n(8)])
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        b.call(ServiceCall {
            target: "history.address",
            arguments: vec![text("alice")]
        }),
        Err(ServiceError::Unavailable("spot_fill_owner_history"))
    );
}

fn unhex(s: &str) -> Vec<u8> {
    if s == "-" {
        return vec![];
    }
    s.as_bytes()
        .chunks_exact(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect()
}
fn json_hex(s: &str) -> RpcValue {
    RpcValue::parse(std::str::from_utf8(&unhex(s)).unwrap()).unwrap()
}
#[test]
fn complete_source_inventory_failures_and_exact_stream_bytes_are_checked() {
    let mut methods = 0;
    let mut streams = 0;
    let mut inventory = 0;
    for line in include_str!("../fixtures/service_view_vectors.tsv")
        .lines()
        .filter(|s| !s.starts_with('#') && !s.is_empty())
    {
        let f: Vec<_> = line.split('\t').collect();
        match f[0] {
            "I" => {
                let names = json_hex(f[1]).as_array().unwrap();
                assert_eq!(names.len(), 27);
                assert_eq!(f[2], "28");
                assert_eq!(
                    names
                        .iter()
                        .filter(|n| n.as_string().unwrap() == "getBalancesAcrossAllWallets")
                        .count(),
                    1
                );
                inventory += 1;
            }
            "S" => {
                assert_eq!(f.len(), 4);
                assert_eq!(
                    audit_lines(&json_hex(f[2]).as_array().unwrap()).unwrap(),
                    unhex(f[3]),
                    "{}",
                    f[1]
                );
                streams += 1;
            }
            "M" => {
                assert_eq!(f.len(), 10);
                let result = json_hex(f[6]);
                let trace = json_hex(f[7]).as_array().unwrap();
                match f[1] {
                    "missing-global-getBalancesAcrossAllWallets" => {
                        assert_eq!(f[5], "throw");
                        assert_eq!(
                            field(&result, "name").as_string().unwrap(),
                            "ReferenceError"
                        );
                        assert!(trace.is_empty());
                    }
                    "injected-getBalancesAcrossAllWallets" => assert_eq!(
                        field(&trace[0], "target").as_string().unwrap(),
                        "WalletCache.getAllBalances"
                    ),
                    "audit-write-failure-swallowed" => {
                        assert_eq!(f[5], "ok");
                        assert!(!json_hex(f[8]).as_array().unwrap().is_empty());
                    }
                    "audit-load-failure-escapes" | "audit-not-iterable-before-try" => {
                        assert_eq!(f[5], "throw");
                        assert!(json_hex(f[9]).as_array().unwrap().is_empty());
                    }
                    "audit-success" => {
                        let files = json_hex(f[9]).as_array().unwrap();
                        assert_eq!(
                            field(&files[0], "filename").as_string().unwrap(),
                            "tallyMapAudit.json"
                        );
                        assert!(!field(&files[0], "hex").as_string().unwrap().is_empty());
                    }
                    _ => {}
                }
                methods += 1;
            }
            _ => panic!("unknown fixture row"),
        }
    }
    assert_eq!((inventory, methods, streams), (1, 90, 5));
    assert_eq!(
        audit_lines(&[RpcValue::parse(" { \"a\" : [ ], \"é\" : \"雪\" } ").unwrap()]).unwrap(),
        "{\n    \"a\": [],\n    \"é\": \"雪\"\n}\n".as_bytes()
    );
    assert_eq!(
        audit_lines(&vec![RpcValue::null(); MAX_AUDIT_ROWS + 1]),
        Err(ServiceError::Capacity)
    );
}

#[derive(Default)]
struct Recorder {
    calls: Vec<ServiceCall>,
}
impl ServiceBackend for Recorder {
    fn call(&mut self, call: ServiceCall) -> Result<RpcValue, ServiceError> {
        self.calls.push(call);
        Ok(RpcValue::null())
    }
}
#[test]
fn generic_facade_exposes_all_query_capabilities_with_exact_arguments() {
    use InterfaceQuery::*;
    let queries = [
        AuditTally,
        ConsensusHash(2),
        FeatureStatus(18),
        Balances(a("alice")),
        TotalTokens(p()),
        AllBalances,
        IsActive(18),
        ActiveTypes,
        Addresses(p()),
        Transaction("ab".into()),
        Property(p()),
        Properties,
        Grants(p()),
        PayToToken(p()),
        BlockTransactions(2),
        BlockRange(2, 3),
        Pending("".into()),
        UpdateWalletCache,
        Oracles,
        Whitelists,
        Vaults(p()),
        OptionsChain(1),
        InsuranceBalance(1),
        InsurancePayouts {
            contract: 1,
            start: 2,
            end: None,
        },
        AuditData {
            height: 2,
            contract: 1,
        },
    ];
    let mut recorder = Recorder::default();
    {
        let mut facade = InterfaceFacade::new(&mut recorder);
        for query in queries {
            facade.query(query).unwrap();
        }
    }
    assert_eq!(recorder.calls.len(), 25);
    assert_eq!(recorder.calls[5].target, "ledger.all_balances");
    assert_eq!(recorder.calls[23].arguments[2].as_json(), "null");
    assert_eq!(recorder.calls[24].target, "interface.audit_data");
}
