//! Genuine owner custody and source-rule reads, with no balance-map mock.
use std::collections::BTreeSet;
use std::path::PathBuf;
use tradelayer_core::chain::{BlockHeader, BlockId};
use tradelayer_core::contract_orderbook::{Order, Side};
use tradelayer_core::contract_registry::ContractId;
use tradelayer_core::decimal::Decimal;
use tradelayer_core::economic::{EconomicConfig, EconomicInput, SettlementPolicy};
use tradelayer_core::economic_chain::{
    DurableEconomicChain, EconomicChain, EconomicChainConfig, UnverifiedEconomicBlock,
};
use tradelayer_core::economic_validity::*;
use tradelayer_core::joint::{JointOperation, UnverifiedJointTx};
use tradelayer_core::ledger::{AccountId, Ledger, Operation, PropertyId, PropertyKind, TxId};
use tradelayer_core::network_profile::TradeLayerMainnet;
use tradelayer_core::registry::RegistryAction;
use tradelayer_core::tally::FeeRoute;
use tradelayer_core::validity_source::{
    self, DependencyCall, DependencyReturn, SourceBackend, SourceCall, SourceOutcome,
    SourceRuleError, SourceValue as V,
};

const ATOMS: u128 = 100_000_000;
fn a(s: &str) -> AccountId {
    s.try_into().unwrap()
}
fn p() -> PropertyId {
    7.try_into().unwrap()
}
fn id(n: u8) -> TxId {
    TxId::from_bytes([n; 32])
}
fn n(n: f64) -> V {
    V::number(n)
}
fn s(s: &str) -> V {
    V::String(s.into())
}
fn obj(fields: &[(&str, V)]) -> V {
    V::Object(
        fields
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
    )
}
fn config(network: TradeLayerMainnet) -> EconomicChainConfig {
    EconomicChainConfig {
        economics: EconomicConfig {
            network,
            contract: ContractId::try_from(1).unwrap(),
            collateral: p(),
            inverse: false,
            notional: Decimal::from(1u32),
            initial_margin_per_contract: Decimal::from(10u32),
            fee_route: FeeRoute::Native(1),
            policy: SettlementPolicy::ConservingV1,
            funding: None,
            marking: None,
            liquidation: None,
            amm: None,
            contract_binding: None,
            source_consensus: None,
        },
        admin: a("admin"),
        permitted_spot_properties: BTreeSet::from([p()]),
        rollback_limit: 4,
    }
}
fn chain(network: TradeLayerMainnet, alice_atoms: u128) -> EconomicChain {
    let mut ledger = Ledger::new();
    ledger
        .apply(
            id(1),
            Operation::CreateProperty {
                id: p(),
                issuer: a("alice"),
                kind: PropertyKind::Fixed,
                initial_atoms: alice_atoms + 100 * ATOMS,
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
                amount_atoms: 100 * ATOMS,
            },
        )
        .unwrap();
    EconomicChain::new_unverified(config(network), ledger).unwrap()
}
fn joint(n: u8, actor: &str, action: RegistryAction) -> EconomicInput {
    EconomicInput::Joint(UnverifiedJointTx {
        txid: id(n),
        actor: a(actor),
        operation: JointOperation::Registry(action),
    })
}
fn apply(chain: &mut EconomicChain, marker: u8, inputs: Vec<EconomicInput>) {
    let block = UnverifiedEconomicBlock {
        network: chain.config().economics.network,
        header: BlockHeader {
            height: chain.tip().height + 1,
            id: BlockId([marker; 32]),
            parent: chain.tip().id,
        },
        inputs,
    };
    chain.apply_unverified_block(&block).unwrap();
}
fn activation() -> EconomicInput {
    joint(
        10,
        "admin",
        RegistryAction::Activate {
            types: vec![2, 6, 13, 14, 15, 18],
            code_hash: [1; 32],
        },
    )
}
fn order(tx: u8, owner: &str, side: Side, height: u64) -> EconomicInput {
    EconomicInput::Order(Order {
        txid: id(tx),
        owner: a(owner),
        contract: ContractId::try_from(1).unwrap(),
        height,
        side,
        quantity: Decimal::from(1u32),
        price: Decimal::from(100u32),
        post: false,
        is_market: false,
    })
}
fn call(target: &str, arguments: Vec<V>) -> DependencyCall {
    DependencyCall {
        target: target.into(),
        arguments,
    }
}
fn read(chain: &EconomicChain, target: &str, arguments: Vec<V>) -> Result<V, SourceRuleError> {
    let mut backend = EconomicSourceBackend::from_chain_unverified(chain, chain.tip()).unwrap();
    match backend.invoke(&call(target, arguments))? {
        DependencyReturn::Immediate(v) | DependencyReturn::Pending(v) => Ok(v),
        _ => panic!("owner read unexpectedly threw"),
    }
}
fn tally(chain: &EconomicChain, owner: &str) -> V {
    read(chain, "Tally.getTally", vec![s(owner), n(7.0)]).unwrap()
}
fn oracle_chain() -> EconomicChain {
    let mut chain = chain(TradeLayerMainnet::Litecoin, 100 * ATOMS);
    apply(&mut chain, 11, vec![activation()]);
    apply(
        &mut chain,
        12,
        vec![joint(
            20,
            "alice",
            RegistryAction::CreateOracle {
                ticker: "REAL".into(),
                url: "https://oracle.example".into(),
                backup: a("backup"),
                lag: 1,
            },
        )],
    );
    chain
}

#[test]
fn actual_contract_reserve_margin_and_free_balance_remain_distinct_on_three_mainnets() {
    for network in [
        TradeLayerMainnet::Bitcoin,
        TradeLayerMainnet::Litecoin,
        TradeLayerMainnet::Dogecoin,
    ] {
        let mut chain = chain(network, 100 * ATOMS);
        apply(&mut chain, 11, vec![activation()]);
        apply(&mut chain, 12, vec![order(30, "alice", Side::Buy, 2)]);
        let before = chain.checkpoint_bytes().unwrap();
        let value = tally(&chain, "alice");
        assert_eq!(value.get("available"), n(90.0));
        assert_eq!(value.get("reserved"), n(10.0));
        assert_eq!(value.get("margin"), n(0.0));
        assert_eq!(value.get("amount"), n(100.0));
        let insufficient = read(
            &chain,
            "Tally.hasSufficientBalance",
            vec![s("alice"), n(7.0), n(95.0)],
        )
        .unwrap();
        assert_eq!(insufficient.get("hasSufficient"), V::Bool(false));
        assert_eq!(insufficient.get("shortfall"), n(5.0));
        assert_eq!(insufficient.get("available"), n(90.0));
        assert_eq!(chain.checkpoint_bytes().unwrap(), before);
        apply(&mut chain, 13, vec![order(31, "bob", Side::Sell, 3)]);
        let value = tally(&chain, "alice");
        assert_eq!(value.get("reserved"), n(0.0));
        assert_eq!(value.get("margin"), n(10.0));
        assert_eq!(chain.state().margin_atoms(&a("alice")), 10 * ATOMS);
        chain.state().verify().unwrap();
        let tip = chain.tip();
        chain.rollback_tip(tip).unwrap();
        assert_eq!(chain.checkpoint_bytes().unwrap(), before);
        assert_eq!(tally(&chain, "alice").get("reserved"), n(10.0));
    }
}

#[test]
fn original_available_source_number_cannot_round_an_atomic_balance_into_validity() {
    let precise = 9_007_199_254_740_993;
    let chain = chain(TradeLayerMainnet::Bitcoin, precise);
    assert_eq!(
        chain.state().ledger().available_atoms(p(), &a("alice")),
        precise
    );
    assert_eq!(
        read(&chain, "Tally.getTally", vec![s("alice"), n(7.0)]),
        Err(SourceRuleError::UnsupportedNumericDomain)
    );
    assert_eq!(
        read(&chain, "Activation.getAdmin", vec![]).unwrap(),
        s("admin")
    );
    assert_eq!(
        chain.state().ledger().available_atoms(p(), &a("alice")),
        precise
    );
}

#[test]
fn source_sufficiency_preserves_missing_channel_shape_and_half_up_shortfall() {
    let chain = chain(TradeLayerMainnet::Bitcoin, ATOMS / 10);
    let absent = read(&chain, "Tally.getTally", vec![s("nobody"), n(7.0)]).unwrap();
    assert_eq!(absent, n(0.0));
    let missing = read(
        &chain,
        "Tally.hasSufficientChannel",
        vec![s("nobody"), n(7.0), n(1.0)],
    )
    .unwrap();
    assert_eq!(missing.get("reason"), s("undefined"));
    assert_eq!(missing.get("shortfall"), V::Undefined);
    for required in [s("1.0"), V::BigNumber("1".into())] {
        let result = read(
            &chain,
            "Tally.hasSufficientBalance",
            vec![s("nobody"), n(7.0), required.clone()],
        )
        .unwrap();
        assert_eq!(result.get("shortfall"), required);
    }
    let shortfall = read(
        &chain,
        "Tally.hasSufficientBalance",
        vec![s("alice"), n(7.0), V::BigNumber("0.100000005".into())],
    )
    .unwrap();
    assert_eq!(shortfall.get("shortfall"), n(0.00000001));
    assert_eq!(shortfall.get("hasSufficient"), V::Bool(false));
    let exact = read(
        &chain,
        "Tally.hasSufficientBalance",
        vec![s("alice"), s("7"), n(0.1)],
    )
    .unwrap();
    assert_eq!(exact.get("hasSufficient"), V::Bool(true));
    for required in [
        V::BigNumber("0.100000000000000001".into()),
        s("0.100000000000000001"),
    ] {
        assert_eq!(
            read(
                &chain,
                "Tally.hasSufficientBalance",
                vec![s("alice"), n(7.0), required]
            ),
            Err(SourceRuleError::UnsupportedNumericDomain)
        );
    }
}

#[test]
fn oracle_reads_use_real_published_state_and_never_manufacture_default_price() {
    let mut chain = oracle_chain();
    assert_eq!(
        read(&chain, "Oracle.getOraclePrice", vec![n(1.0)]),
        Err(SourceRuleError::Backend)
    );
    assert_eq!(
        read(&chain, "Oracle.getOracleInfo", vec![n(2.0)]).unwrap(),
        V::Null
    );
    assert_eq!(
        read(&chain, "Oracle.isAdmin", vec![s("backup"), n(1.0)]).unwrap(),
        V::Bool(true)
    );
    assert_eq!(
        read(&chain, "Oracle.isAdmin", vec![s("outsider"), n(1.0)]).unwrap(),
        V::Bool(false)
    );
    apply(
        &mut chain,
        13,
        vec![joint(
            21,
            "alice",
            RegistryAction::PublishOracle {
                oracle_id: 1,
                price_1e4: 1_234_567,
                high_1e4: None,
                low_1e4: None,
                close_1e4: None,
            },
        )],
    );
    let before = chain.checkpoint_bytes().unwrap();
    assert_eq!(
        read(&chain, "Oracle.getOraclePrice", vec![n(1.0)]).unwrap(),
        n(123.4567)
    );
    let info = read(&chain, "Oracle.getOracleInfo", vec![n(1.0)]).unwrap();
    assert_eq!(info.get("ticker"), s("REAL"));
    assert_eq!(info.get("adminAddress"), s("alice"));
    assert_eq!(info.get("data").get("price"), n(123.4567));
    assert_eq!(chain.checkpoint_bytes().unwrap(), before);
}

#[test]
fn real_validity_executes_against_owner_facts_without_granting_admission() {
    let chain = oracle_chain();
    let before = chain.checkpoint_bytes().unwrap();
    let mut backend = EconomicSourceBackend::from_chain_unverified(&chain, chain.tip()).unwrap();
    let publish = |actor: &str| SourceCall {
        method: "validatePublishOracleData".into(),
        arguments: vec![
            s(actor),
            obj(&[("oracleId", n(1.0)), ("price", n(1.25))]),
            s("txid"),
        ],
    };
    let valid = validity_source::evaluate_unverified(&publish("alice"), &mut backend).unwrap();
    let SourceOutcome::Returned(result) = &valid.outcome else {
        panic!("genuine validator unexpectedly threw")
    };
    assert_eq!(result.get("valid"), V::Bool(true));
    assert_eq!(
        valid.require_authenticated_admission(),
        Err(SourceRuleError::UnauthenticatedInputAndState)
    );
    assert_eq!(
        backend.require_authenticated_admission(),
        Err(SourceRuleError::UnauthenticatedInputAndState)
    );
    let invalid = validity_source::evaluate_unverified(&publish("outsider"), &mut backend).unwrap();
    let SourceOutcome::Returned(result) = invalid.outcome else {
        panic!("genuine validator unexpectedly threw")
    };
    assert_eq!(result.get("valid"), V::Bool(false));
    let close = SourceCall {
        method: "validateCloseOracle".into(),
        arguments: vec![s("outsider"), obj(&[("oracleId", n(1.0))]), s("txid")],
    };
    let observed = validity_source::evaluate_unverified(&close, &mut backend).unwrap();
    let SourceOutcome::Returned(result) = observed.outcome else {
        panic!("genuine source close unexpectedly threw")
    };
    // The source omitted await on Oracle.isAdmin. The diagnostic keeps the
    // Promise value; nobody turns its truthiness into owner authority.
    assert_eq!(result.get("valid"), V::Promise);
    assert_eq!(chain.checkpoint_bytes().unwrap(), before);
}

#[test]
fn missing_metadata_evidence_and_noncanonical_query_domains_are_explicit_errors() {
    let chain = chain(TradeLayerMainnet::Litecoin, ATOMS);
    for target in [
        "Property.getPropertyData",
        "ClearList.getBanlist",
        "TxUtils.getTransactionOutputs",
        "Zk.verifyProof",
        "Volume.getLastPrice",
    ] {
        assert_eq!(
            read(&chain, target, vec![]),
            Err(SourceRuleError::UnknownDependency)
        );
    }
    for bad in [s("07"), s("7junk"), n(7.5), n(f64::INFINITY), V::Null] {
        assert!(read(&chain, "Tally.getTally", vec![s("alice"), bad]).is_err());
    }
    assert_eq!(
        read(&chain, "Tally.getTally", vec![s("~fees:hidden"), n(7.0)]),
        Err(SourceRuleError::InvalidInput)
    );
    assert_eq!(
        read(&chain, "Activation.getAdmin", vec![s("extra")]),
        Err(SourceRuleError::InvalidInput)
    );
    assert_eq!(
        read(&chain, "Contract.getContractInfo", vec![n(1.0)]).unwrap(),
        V::Null
    );
}

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "tl-economic-validity-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        assert!(
            self.0.is_absolute()
                && self.0.starts_with(std::env::temp_dir())
                && self
                    .0
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .starts_with("tl-economic-validity-")
        );
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn durable_context_requires_exact_head_after_restart_rollback_and_aba() {
    let directory = Directory::new();
    let chain = chain(TradeLayerMainnet::Dogecoin, 100 * ATOMS);
    let config = chain.config().clone();
    let tip = chain.tip();
    let mut owner = DurableEconomicChain::initialize(&directory.0, chain, tip).unwrap();
    let initial = owner.head().unwrap();
    let backend = EconomicSourceBackend::from_durable(&owner, initial).unwrap();
    assert_eq!(
        backend.context(),
        ValidationContext {
            network: TradeLayerMainnet::Dogecoin,
            height: 1,
            parent: tip
        }
    );
    assert_eq!(backend.durable_head(), Some(initial));
    let block = UnverifiedEconomicBlock {
        network: TradeLayerMainnet::Dogecoin,
        header: BlockHeader {
            height: 1,
            id: BlockId([3; 32]),
            parent: tip.id,
        },
        inputs: vec![activation()],
    };
    owner.apply_unverified_block(initial, &block).unwrap();
    assert!(matches!(
        EconomicSourceBackend::from_durable(&owner, initial),
        Err(EconomicValidityError::WrongHead)
    ));
    let head = owner.head().unwrap();
    owner.rollback_to(head, tip).unwrap();
    let fresh = owner.head().unwrap();
    assert_eq!(fresh.tip, initial.tip);
    assert!(fresh.sequence > initial.sequence);
    assert!(matches!(
        EconomicSourceBackend::from_durable(&owner, initial),
        Err(EconomicValidityError::WrongHead)
    ));
    drop(owner);
    let (owner, _) = DurableEconomicChain::load(
        &directory.0,
        config,
        fresh,
        tradelayer_core::storage::Recovery::RejectIncompleteTail,
    )
    .unwrap();
    assert_eq!(owner.head().unwrap(), fresh);
    let mut backend = EconomicSourceBackend::from_durable(&owner, fresh).unwrap();
    assert_eq!(
        backend
            .invoke(&call("Activation.getAdmin", vec![]))
            .unwrap(),
        DependencyReturn::Immediate(s("admin"))
    );
}
