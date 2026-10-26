//! Both opt-ins over the actual sole Ledger. Every amount below is a test fixture.
use std::collections::BTreeSet;
use tradelayer_core::chain::{BlockHeader, BlockId, Tip};
use tradelayer_core::contract_orderbook::{Order, Side};
use tradelayer_core::contract_registry::{ContractAsset, ContractParameters};
use tradelayer_core::decimal::Decimal;
use tradelayer_core::economic::*;
use tradelayer_core::economic_chain::{
    DurableEconomicChain, EconomicChain, EconomicChainConfig, UnverifiedEconomicBlock,
};
use tradelayer_core::joint::{JointOperation, JointProtocolState, UnverifiedJointTx};
use tradelayer_core::ledger::{AccountId, Ledger, Operation, PropertyId, PropertyKind, TxId};
use tradelayer_core::native_entitlement::*;
use tradelayer_core::network_profile::TradeLayerMainnet;
use tradelayer_core::registry::RegistryAction;
use tradelayer_core::storage::Recovery;
use tradelayer_core::tally::FeeRoute;
const U: u128 = 100_000_000;
fn a(s: &str) -> AccountId {
    s.try_into().unwrap()
}
fn p(n: u32) -> PropertyId {
    n.try_into().unwrap()
}
fn d(s: &str) -> Decimal {
    s.parse().unwrap()
}
fn id(n: u64) -> TxId {
    let mut b = [0; 32];
    b[24..].copy_from_slice(&n.to_be_bytes());
    TxId::from_bytes(b)
}
fn creation() -> CreateContractInput {
    CreateContractInput {
        txid: id(200),
        issuer: a("issuer"),
        parameters: ContractParameters {
            native: false,
            underlying_oracle_id: Some(1),
            on_chain_data: vec![],
            notional_property_id: ContractAsset::BaseChain,
            notional_value: d("1"),
            collateral_property_id: ContractAsset::Property(p(1)),
            leverage: d("10"),
            expiry_period: 0,
            series: 1,
            inverse: false,
            fee: d("0"),
            whitelist: Some(0),
            ticker: None,
        },
    }
}
fn fixture() -> (Ledger, NativeEntitlementPolicy, EconomicChainConfig) {
    let network = TradeLayerMainnet::Bitcoin;
    let mut ledger = Ledger::new();
    for n in 1..=4 {
        ledger
            .apply(
                id(n as u64),
                Operation::CreateProperty {
                    id: p(n),
                    issuer: a("funder"),
                    kind: PropertyKind::Managed,
                    initial_atoms: if n == 1 { 20_000 * U } else { 20 * U },
                },
            )
            .unwrap();
    }
    for (n, property, to, atoms) in [
        (10, 2, "alice", 10 * U),
        (11, 2, "bob", 10 * U),
        (12, 3, "alice", 10 * U),
        (13, 3, "bob", 10 * U),
        (20, 1, "alice", 10_000 * U),
        (21, 1, "bob", 12 * U),
        (22, 1, "carol", 1_000 * U),
        (23, 1, "sponsor", 1_000 * U),
    ] {
        ledger
            .apply(
                id(n),
                Operation::Transfer {
                    id: p(property),
                    from: a("funder"),
                    to: a(to),
                    amount_atoms: atoms,
                },
            )
            .unwrap();
    }
    let roles = [
        NativeRole::Base,
        NativeRole::VestingBase,
        NativeRole::VestingIncome,
        NativeRole::Income,
    ];
    let classes = [
        SourceCatalogClass::Fixed,
        SourceCatalogClass::Vesting,
        SourceCatalogClass::Vesting,
        SourceCatalogClass::Native,
    ];
    let policy = NativeEntitlementPolicy {
        network,
        bootstrap: Tip {
            height: 0,
            id: network.genesis(),
        },
        bootstrap_ledger_commitment: bootstrap_commitment(&ledger.snapshot_bytes().unwrap()),
        target_operator: a("native-operator"),
        assets: (1..=4)
            .map(|n| NativeAssetSpec {
                role: roles[n - 1],
                property: p(n as u32),
                issuer: a("funder"),
                ledger_kind: PropertyKind::Managed,
                catalog_class: classes[n - 1],
                declared_figure_atoms: 20 * U,
                issuance_ceiling_atoms: None,
                expected_initial_issued_atoms: if n == 1 { 20_000 * U } else { 20 * U },
            })
            .collect(),
        bindings: [(2, 1), (3, 4)]
            .into_iter()
            .map(|(coupon, target)| NativeBindingSpec {
                coupon: p(coupon),
                target: p(target),
                curve_scale_atoms: 20 * U,
                initial_markers: ["alice", "bob"]
                    .into_iter()
                    .map(|owner| NativeMarkerRow {
                        owner: a(owner),
                        atoms: 10 * U,
                    })
                    .collect(),
                funding: PrefundedTargetSpec {
                    funding_owner: a("funder"),
                    target_atoms: 20 * U,
                },
            })
            .collect(),
        quote_policy: NativeQuotePolicy::SourceQuoteExactOrRejectV1,
        driver: NativeDriver::MonotonicDeltaV1,
        payout_order: NativePayoutOrder::AsciiAccountV1,
        coverage: NativeCoverage::CoverInitialMarkersV1,
    };
    let mut adopted = ledger.clone();
    adopted
        .adopt_native_entitlements_unverified(&policy)
        .unwrap();
    let joint = JointProtocolState::from_native_bootstrap_unverified(
        network,
        a("admin"),
        adopted,
        BTreeSet::new(),
        &policy,
    )
    .unwrap();
    let definition_hash = contract_definition_commitment(&creation()).unwrap();
    let cfg = EconomicChainConfig {
        economics: EconomicConfig {
            network,
            contract: 1.try_into().unwrap(),
            collateral: p(1),
            inverse: false,
            notional: d("1"),
            initial_margin_per_contract: d("0"),
            fee_route: FeeRoute::Oracle(1),
            policy: SettlementPolicy::ConservingV1,
            funding: None,
            marking: Some(MarkConfig {
                oracle_id: 1,
                policy: MarkPolicy::ConservingMarkBasisV1,
            }),
            liquidation: Some(LiquidationConfig {
                max_price_deviation_bps: 100,
                max_fills_per_owner: 8,
                policy: LiquidationPolicy::ConservingLiquidationV1,
            }),
            amm: None,
            contract_binding: Some(ContractBindingConfig {
                creation_txid: id(200),
                definition_hash,
                bootstrap_commitment: EconomicState::contract_bootstrap_commitment(&joint).unwrap(),
                policy: ContractRiskPolicy::SourceContextsConservingV1,
            }),
            source_consensus: None,
            native_entitlements: Some(policy.clone()),
            insurance: Some(InsuranceConfig {
                fund: InsuranceFundIdentity {
                    network,
                    contract: 1.try_into().unwrap(),
                    collateral: p(1),
                    creation_txid: id(200),
                    definition_hash,
                    oracle_id: 1,
                },
                sponsor: a("sponsor"),
                operator: a("insurance-operator"),
                policy: InsurancePolicy::ConservingOracleBackstopV1,
            }),
        },
        admin: a("admin"),
        permitted_spot_properties: BTreeSet::new(),
        rollback_limit: 8,
    };
    (ledger, policy, cfg)
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
        "admin",
        JointOperation::Registry(RegistryAction::PublishOracle {
            oracle_id: 1,
            price_1e4: price * 10_000,
            high_1e4: None,
            low_1e4: None,
            close_1e4: None,
        }),
    )
}
fn target(n: u64, atoms: u128) -> EconomicInput {
    EconomicInput::NativeTarget(UnverifiedNativeTarget {
        txid: id(n),
        claimed_actor: a("native-operator"),
        coupon: p(2),
        requested_target_atoms: atoms,
    })
}
fn insurance(n: u64, actor: &str, action: InsuranceAction) -> EconomicInput {
    EconomicInput::Insurance(UnverifiedInsuranceInput {
        txid: id(n),
        actor: a(actor),
        action,
    })
}
fn order(n: u64, owner: &str, side: Side, price: &str, height: u64) -> EconomicInput {
    EconomicInput::Order(Order {
        txid: id(n),
        owner: a(owner),
        contract: 1.try_into().unwrap(),
        height,
        side,
        quantity: d("1"),
        price: d(price),
        post: false,
        is_market: false,
    })
}
fn block(
    chain: &EconomicChain,
    number: u64,
    inputs: Vec<EconomicInput>,
) -> UnverifiedEconomicBlock {
    UnverifiedEconomicBlock {
        network: chain.config().economics.network,
        header: BlockHeader {
            height: chain.tip().height + 1,
            parent: chain.tip().id,
            id: BlockId(id(number).as_bytes()),
        },
        inputs,
    }
}
fn apply(chain: &mut EconomicChain, n: u64, inputs: Vec<EconomicInput>) {
    chain
        .apply_unverified_block(&block(chain, n, inputs))
        .unwrap();
}
#[test]
fn combined_native_collateral_insurance_v9_replay_restart_wrong_policy_and_rollback() {
    let (ledger, policy, cfg) = fixture();
    let issued = ledger.property(p(1)).unwrap().total_supply_atoms;
    let mut chain = EconomicChain::new_unverified(cfg.clone(), ledger).unwrap();
    apply(
        &mut chain,
        100,
        vec![joint(
            100,
            "admin",
            JointOperation::Registry(RegistryAction::Activate {
                types: vec![2, 6, 13, 14, 16, 18],
                code_hash: [1; 32],
            }),
        )],
    );
    let second = vec![
        joint(
            101,
            "admin",
            JointOperation::Registry(RegistryAction::CreateOracle {
                ticker: "INDEX".into(),
                url: "https://example.invalid".into(),
                backup: a("backup"),
                lag: 1,
            }),
        ),
        publish(102, 100),
        EconomicInput::CreateContract(creation()),
        target(201, 4 * U),
        insurance(202, "sponsor", InsuranceAction::Deposit { atoms: 100 * U }),
    ];
    apply(&mut chain, 101, second.clone());
    assert_eq!(chain.state().insurance_state().reserve_atoms, 100 * U);
    assert_eq!(
        chain
            .state()
            .ledger()
            .native_remaining_atoms(p(2), &a("alice")),
        8 * U
    );
    apply(
        &mut chain,
        102,
        vec![
            joint(
                302,
                "alice",
                JointOperation::Send {
                    id: p(2),
                    recipient: a("bob"),
                    amount_atoms: 110_000_000,
                },
            ),
            order(300, "alice", Side::Buy, "100", 3),
            order(301, "bob", Side::Sell, "100", 3),
        ],
    );
    let before_closeout = chain.tip();
    assert_eq!(chain.state().receipts().len(), 1);
    assert_eq!(
        chain.state().ledger().available_atoms(p(2), &a("alice")),
        890_000_000
    );
    let fourth = vec![
        target(403, 6 * U),
        insurance(
            400,
            "insurance-operator",
            InsuranceAction::Backstop { max_atoms: 30 * U },
        ),
        publish(401, 200),
        order(402, "carol", Side::Sell, "200", 4),
    ];
    apply(&mut chain, 103, fourth.clone());
    let state = chain.state();
    let last = state.insurance_state().receipts.last().unwrap();
    assert_eq!(last.plan.as_ref().unwrap().cover_atoms, 30 * U);
    assert_eq!(state.insurance_state().reserve_atoms, 70 * U);
    assert!(state
        .liquidation_state()
        .events
        .iter()
        .any(|e| e.height == 4 && !e.fills.is_empty()));
    assert_eq!(state.positions().get(&a("bob")).unwrap().contracts, d("0"));
    assert_eq!(
        state.ledger().native_remaining_atoms(p(2), &a("alice")),
        711_000_000
    );
    assert_eq!(
        state.ledger().native_remaining_atoms(p(2), &a("bob")),
        689_000_000
    );
    assert_eq!(
        state.native_entitlement_state().frames(),
        state.contract_binding_state().frames
    );
    assert_eq!(state.native_entitlement_state().frames()[1].inputs, second);
    assert_eq!(state.native_entitlement_state().frames()[3].inputs, fourth);
    assert_eq!(
        state.ledger().property(p(1)).unwrap().total_supply_atoms,
        issued
    );
    assert_eq!(
        state
            .ledger()
            .available_atoms(p(1), &escrow(&policy, p(2)).unwrap()),
        14 * U
    );
    let bytes = state.snapshot_bytes().unwrap();
    assert_eq!(&bytes[..5], b"TLEC\x09");
    assert_eq!(
        EconomicState::from_snapshot_bytes(&bytes, &cfg.economics, &a("admin"), &BTreeSet::new())
            .unwrap(),
        *state
    );
    let mut wrong = cfg.clone();
    wrong
        .economics
        .native_entitlements
        .as_mut()
        .unwrap()
        .target_operator = a("different-native-operator");
    assert!(EconomicState::from_snapshot_bytes(
        &bytes,
        &wrong.economics,
        &a("admin"),
        &BTreeSet::new()
    )
    .is_err());
    let mut absent = cfg.clone();
    absent.economics.native_entitlements = None;
    assert!(EconomicState::from_snapshot_bytes(
        &bytes,
        &absent.economics,
        &a("admin"),
        &BTreeSet::new()
    )
    .is_err());
    let checkpoint = chain.checkpoint_bytes().unwrap();
    assert!(EconomicChain::from_checkpoint_bytes(&checkpoint, wrong, chain.tip()).is_err());
    let mut reopened =
        EconomicChain::from_checkpoint_bytes(&checkpoint, cfg.clone(), chain.tip()).unwrap();
    reopened.rollback_tip(reopened.tip()).unwrap();
    assert_eq!(reopened.tip(), before_closeout);
    assert_eq!(reopened.state().insurance_state().reserve_atoms, 100 * U);
    assert_eq!(
        reopened
            .state()
            .ledger()
            .native_remaining_atoms(p(2), &a("alice")),
        8 * U
    );
    assert!(!reopened.state().ledger().is_processed(id(403)));
    apply(&mut reopened, 104, vec![]);
    assert_eq!(reopened.state().insurance_state().receipts.len(), 1);
    assert_eq!(
        reopened.state().native_entitlement_state().frames()[3]
            .inputs
            .len(),
        0
    );
    let root = std::env::temp_dir().join(format!(
        "tl-native-insurance-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&root).unwrap();
    let tip = chain.tip();
    let owner = DurableEconomicChain::initialize(&root, chain, tip).unwrap();
    let head = owner.head().unwrap();
    drop(owner);
    let (mut owner, _) =
        DurableEconomicChain::load(&root, cfg.clone(), head, Recovery::RejectIncompleteTail)
            .unwrap();
    assert_eq!(
        owner
            .chain()
            .unwrap()
            .state()
            .insurance_state()
            .reserve_atoms,
        70 * U
    );
    let after = owner.rollback_to(head, before_closeout).unwrap();
    assert!(after.sequence > head.sequence);
    drop(owner);
    let (owner, _) =
        DurableEconomicChain::load(&root, cfg, after, Recovery::RejectIncompleteTail).unwrap();
    assert_eq!(owner.chain().unwrap().tip(), before_closeout);
    assert_eq!(
        owner
            .chain()
            .unwrap()
            .state()
            .native_entitlement_state()
            .frames()
            .len(),
        3
    );
    drop(owner);
    let resolved = std::fs::canonicalize(&root).unwrap();
    let temp = std::fs::canonicalize(std::env::temp_dir()).unwrap();
    assert!(resolved.is_absolute() && resolved.starts_with(&temp));
    assert!(resolved
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .starts_with("tl-native-insurance-"));
    std::fs::remove_dir_all(&resolved).unwrap();
}
