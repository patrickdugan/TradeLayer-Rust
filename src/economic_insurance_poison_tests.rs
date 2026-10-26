// Actual accepted-contract owner, real custody and complete insurance replay.
use crate::chain::{BlockHeader, BlockId};
use crate::contract_orderbook::{Order, Side};
use crate::contract_registry::{ContractAsset, ContractParameters};
use crate::economic::*;
use crate::economic_chain::{
    DurableEconomicChain, EconomicChain, EconomicChainConfig, UnverifiedEconomicBlock,
};
use crate::joint::{JointOperation, JointProtocolState, UnverifiedJointTx};
use crate::ledger::{AccountId, Ledger, Operation, PropertyId, PropertyKind, TxId};
use crate::network_profile::TradeLayerMainnet;
use crate::registry::RegistryAction;
use crate::storage::Recovery;
use crate::tally::FeeRoute;
use std::collections::BTreeSet;

const A: u128 = 100_000_000;
fn d(s: &str) -> crate::decimal::Decimal {
    s.parse().unwrap()
}
fn a(s: &str) -> AccountId {
    s.try_into().unwrap()
}
fn p() -> PropertyId {
    7.try_into().unwrap()
}
fn id(n: u8) -> TxId {
    TxId::from_bytes([n; 32])
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
                initial_atoms: 20_000_000_000_000_013,
            },
        )
        .unwrap();
    for (n, owner, atoms) in [
        (2, "bob", 12 * A),
        (3, "carol", 1000 * A),
        (4, "sponsor", 10_000_000_000_000_001),
    ] {
        ledger
            .apply(
                id(n),
                Operation::Transfer {
                    id: p(),
                    from: a("alice"),
                    to: a(owner),
                    amount_atoms: atoms,
                },
            )
            .unwrap();
    }
    ledger
}
fn creation() -> CreateContractInput {
    CreateContractInput {
        txid: id(40),
        issuer: a("alice"),
        parameters: ContractParameters {
            native: false,
            underlying_oracle_id: Some(1),
            on_chain_data: vec![],
            notional_property_id: ContractAsset::BaseChain,
            notional_value: d("1"),
            collateral_property_id: ContractAsset::Property(p()),
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
fn config(network: TradeLayerMainnet) -> EconomicChainConfig {
    let bootstrap = JointProtocolState::from_bootstrap_unverified(
        network,
        a("admin"),
        ledger(),
        BTreeSet::from([p()]),
    )
    .unwrap();
    let definition_hash = contract_definition_commitment(&creation()).unwrap();
    EconomicChainConfig {
        economics: EconomicConfig {
            native_entitlements: None,
            network,
            contract: 1.try_into().unwrap(),
            collateral: p(),
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
            source_consensus: None,
            contract_binding: Some(ContractBindingConfig {
                creation_txid: id(40),
                definition_hash,
                bootstrap_commitment: EconomicState::contract_bootstrap_commitment(&bootstrap)
                    .unwrap(),
                policy: ContractRiskPolicy::SourceContextsConservingV1,
            }),
            insurance: Some(InsuranceConfig {
                fund: InsuranceFundIdentity {
                    network,
                    contract: 1.try_into().unwrap(),
                    collateral: p(),
                    creation_txid: id(40),
                    definition_hash,
                    oracle_id: 1,
                },
                sponsor: a("sponsor"),
                operator: a("operator"),
                policy: InsurancePolicy::ConservingOracleBackstopV1,
            }),
        },
        admin: a("admin"),
        permitted_spot_properties: BTreeSet::from([p()]),
        rollback_limit: 16,
    }
}
fn joint(n: u8, action: RegistryAction) -> EconomicInput {
    EconomicInput::Joint(UnverifiedJointTx {
        txid: id(n),
        actor: a("admin"),
        operation: JointOperation::Registry(action),
    })
}
fn publication(n: u8, price: u128) -> EconomicInput {
    joint(
        n,
        RegistryAction::PublishOracle {
            oracle_id: 1,
            price_1e4: price * 10_000,
            high_1e4: None,
            low_1e4: None,
            close_1e4: None,
        },
    )
}
fn order(n: u8, owner: &str, side: Side, price: &str, height: u64) -> EconomicInput {
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
fn insurance(n: u8, actor: &str, action: InsuranceAction) -> EconomicInput {
    EconomicInput::Insurance(UnverifiedInsuranceInput {
        txid: id(n),
        actor: a(actor),
        action,
    })
}
fn block(chain: &EconomicChain, inputs: Vec<EconomicInput>) -> UnverifiedEconomicBlock {
    let h = chain.tip().height + 1;
    let mut bytes = [0; 32];
    bytes[24..].copy_from_slice(&h.to_be_bytes());
    UnverifiedEconomicBlock {
        network: chain.config().economics.network,
        header: BlockHeader {
            height: h,
            id: BlockId(bytes),
            parent: chain.tip().id,
        },
        inputs,
    }
}
fn apply(chain: &mut EconomicChain, inputs: Vec<EconomicInput>) {
    chain.apply_unverified_block(&block(chain, inputs)).unwrap();
}
fn ready(network: TradeLayerMainnet) -> EconomicChain {
    let mut chain = EconomicChain::new_unverified(config(network), ledger()).unwrap();
    apply(
        &mut chain,
        vec![joint(
            10,
            RegistryAction::Activate {
                types: vec![2, 6, 13, 14, 15, 16, 18],
                code_hash: [0x51; 32],
            },
        )],
    );
    apply(
        &mut chain,
        vec![
            joint(
                11,
                RegistryAction::CreateOracle {
                    ticker: "TEST".into(),
                    url: "https://example.invalid".into(),
                    backup: a("backup"),
                    lag: 1,
                },
            ),
            publication(12, 100),
            EconomicInput::CreateContract(creation()),
            insurance(
                41,
                "sponsor",
                InsuranceAction::Deposit { atoms: 100 * A + 1 },
            ),
        ],
    );
    apply(
        &mut chain,
        vec![
            order(20, "alice", Side::Buy, "100", 3),
            order(21, "bob", Side::Sell, "100", 3),
        ],
    );
    chain
}
fn closeout_inputs(requests: Vec<EconomicInput>) -> Vec<EconomicInput> {
    let mut inputs = requests;
    inputs.push(publication(30, 200));
    inputs.push(order(31, "carol", Side::Sell, "200", 4));
    inputs
}

#[test]
fn synced_insurance_payout_lost_ack_poison_denies_cached_owner_and_reopens_exactly_once() {
    let chain = ready(TradeLayerMainnet::Bitcoin);
    let config = chain.config().clone();
    let tip = chain.tip();
    let inputs = closeout_inputs(vec![insurance(
        50,
        "operator",
        InsuranceAction::Backstop { max_atoms: 30 * A },
    )]);
    let proposed = block(&chain, inputs);
    let mut committed = chain.clone();
    committed.apply_unverified_block(&proposed).unwrap();
    let committed_bytes = committed.checkpoint_bytes().unwrap();
    let path = std::env::temp_dir().join(format!(
        "tl-insurance-poison-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&path).unwrap();
    let mut owner = DurableEconomicChain::initialize(&path, chain, tip).unwrap();
    let previous = owner.head().unwrap();
    let expected = crate::economic_chain::EconomicHead {
        sequence: previous.sequence + 1,
        tip: committed.tip(),
        commitment: crate::hash256::double_sha256_parts(&[
            b"TradeLayer/DurableEconomicChain/v1/",
            &committed_bytes,
        ]),
    };
    owner.test_fail_next_commit_after_sync();
    assert!(owner.apply_unverified_block(previous, &proposed).is_err());
    assert!(owner.chain().is_err());
    assert!(owner.head().is_err());
    assert!(owner.apply_unverified_block(previous, &proposed).is_err());
    drop(owner);
    assert!(DurableEconomicChain::load(
        &path,
        config.clone(),
        previous,
        Recovery::RejectIncompleteTail
    )
    .is_err());
    let (mut owner, _) =
        DurableEconomicChain::load(&path, config, expected, Recovery::RejectIncompleteTail)
            .unwrap();
    assert_eq!(
        owner.chain().unwrap().checkpoint_bytes().unwrap(),
        committed_bytes
    );
    assert_eq!(
        owner
            .chain()
            .unwrap()
            .state()
            .insurance_state()
            .covered_atoms,
        30 * A
    );
    // The old header and actual external input id cannot repeat the payout.
    assert!(owner.apply_unverified_block(expected, &proposed).is_err());
    let replay = block(
        owner.chain().unwrap(),
        vec![insurance(
            50,
            "operator",
            InsuranceAction::Backstop { max_atoms: 1 },
        )],
    );
    assert!(owner.apply_unverified_block(expected, &replay).is_err());
    assert_eq!(owner.head().unwrap(), expected);
    drop(owner);
    assert!(
        path.is_absolute()
            && path.starts_with(std::env::temp_dir())
            && path
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("tl-insurance-poison-")
    );
    std::fs::remove_dir_all(path).unwrap();
}
