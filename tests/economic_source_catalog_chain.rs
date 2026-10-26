//! Actual catalog bootstrap in the existing checkpoint, reorg and durable owner.
use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use tradelayer_core::chain::{BlockHeader, BlockId};
use tradelayer_core::clearlist_source_owner::ClearlistSourceOwner;
use tradelayer_core::consensus_source::{
    SourceBlockMode, SourceConsensusConfig, SourceConsensusPolicy, SourceDisposition,
    SourceTransaction, UnverifiedSourceBlock,
};
use tradelayer_core::economic::{EconomicConfig, EconomicError, EconomicState, SettlementPolicy};
use tradelayer_core::economic_chain::{
    DurableEconomicChain, DurableEconomicError, EconomicChain, EconomicChainConfig,
};
use tradelayer_core::joint::JointProtocolState;
use tradelayer_core::ledger::{AccountId, Ledger, Operation, PropertyId, PropertyKind, TxId};
use tradelayer_core::network_profile::TradeLayerMainnet;
use tradelayer_core::property_catalog::PropertyCatalog;
use tradelayer_core::source_catalogs::{bootstrap_commitment, SourceCatalogOwners};
use tradelayer_core::storage::Recovery;
use tradelayer_core::tally::FeeRoute;
use tradelayer_core::validity_source::{
    DependencyCall, DependencyReturn, SourceBackend, SourceRuleError, SourceValue as V,
};

const ATOMS: u128 = 100_000_000;
fn a(value: &str) -> AccountId {
    value.try_into().unwrap()
}
fn p(id: u32) -> PropertyId {
    id.try_into().unwrap()
}
fn id(number: u64) -> TxId {
    let mut bytes = [0; 32];
    bytes[24..].copy_from_slice(&number.to_be_bytes());
    TxId::from_bytes(bytes)
}
fn block_id(number: u64) -> BlockId {
    let mut bytes = [0x68; 32];
    bytes[24..].copy_from_slice(&number.to_be_bytes());
    BlockId(bytes)
}
fn s(value: &str) -> V {
    V::String(value.into())
}
fn n(value: f64) -> V {
    V::number(value)
}
struct NoTally;
impl SourceBackend for NoTally {
    fn invoke(&mut self, _: &DependencyCall) -> Result<DependencyReturn, SourceRuleError> {
        Err(SourceRuleError::UnknownDependency)
    }
}
fn parts() -> (EconomicChainConfig, Ledger, SourceCatalogOwners) {
    let mut ledger = Ledger::new();
    ledger
        .apply(
            id(900),
            Operation::CreateProperty {
                id: p(7),
                issuer: a("alice"),
                kind: PropertyKind::Fixed,
                initial_atoms: 100 * ATOMS,
            },
        )
        .unwrap();
    let mut properties = PropertyCatalog::new();
    properties
        .execute(
            "addProperty",
            &[
                n(7.0),
                s("BASE"),
                n(100.0),
                s("Fixed"),
                n(0.0),
                s("alice"),
                s("backup"),
            ],
            &mut NoTally,
        )
        .unwrap();
    let owners = SourceCatalogOwners::new(properties, ClearlistSourceOwner::new());
    let joint = JointProtocolState::from_bootstrap_unverified(
        TradeLayerMainnet::Bitcoin,
        a("admin"),
        ledger.clone(),
        BTreeSet::from([p(7)]),
    )
    .unwrap();
    let config = EconomicChainConfig {
        admin: a("admin"),
        permitted_spot_properties: BTreeSet::from([p(7)]),
        rollback_limit: 4,
        economics: EconomicConfig {
            network: TradeLayerMainnet::Bitcoin,
            contract: 1.try_into().unwrap(),
            collateral: p(7),
            inverse: false,
            notional: "1".parse().unwrap(),
            initial_margin_per_contract: "10".parse().unwrap(),
            fee_route: FeeRoute::Native(1),
            policy: SettlementPolicy::ConservingV1,
            funding: None,
            marking: None,
            liquidation: None,
            amm: None,
            contract_binding: None,
            native_entitlements: None,
            insurance: None,
            source_consensus: Some(SourceConsensusConfig {
                bootstrap_commitment: EconomicState::source_bootstrap_commitment(&joint).unwrap(),
                policy: SourceConsensusPolicy::ConservingSequentialV1,
                catalog_bootstrap_commitment: Some(bootstrap_commitment(
                    &owners.checkpoint_bytes().unwrap(),
                )),
            }),
        },
    };
    (config, ledger, owners)
}
fn fixture() -> EconomicChain {
    let (config, ledger, owners) = parts();
    EconomicChain::new_with_source_catalogs_unverified(config, ledger, owners).unwrap()
}
fn tx(number: u64, payload: &str, sender: &str) -> SourceTransaction {
    SourceTransaction {
        position: 0,
        txid: id(number),
        payload: payload.into(),
        marker: s("tl"),
        sender: s(sender),
        reference: V::Null,
        sender_amount: n(1.0),
        reference_amount: n(0.0),
    }
}
fn block(
    chain: &EconomicChain,
    number: u64,
    transaction: SourceTransaction,
) -> UnverifiedSourceBlock {
    UnverifiedSourceBlock {
        network: chain.config().economics.network,
        header: BlockHeader {
            height: chain.tip().height + 1,
            id: block_id(number),
            parent: chain.tip().id,
        },
        mode: SourceBlockMode::IndexedReplay,
        transactions: vec![transaction],
    }
}
fn activation(chain: &EconomicChain) -> UnverifiedSourceBlock {
    block(chain, 1, tx(1, &format!("01,{}", "11".repeat(32)), "admin"))
}
fn issuance(chain: &EconomicChain, number: u64) -> UnverifiedSourceBlock {
    block(chain, number, tx(2, "1a,OWNED,,1,backup,0,0", "alice"))
}
fn property(chain: &EconomicChain, id: u32) -> V {
    chain
        .state()
        .source_catalog_owners()
        .unwrap()
        .properties
        .clone()
        .get_property_data(n(id as f64))
        .unwrap()
}
fn activate(chain: &mut EconomicChain) {
    let b = activation(chain);
    assert_eq!(chain.apply_source_unverified_block(&b).unwrap().accepted, 1);
}

#[test]
fn both_catalog_and_joint_pins_are_required_by_the_new_chain_constructor() {
    let (config, ledger, owners) = parts();
    assert!(matches!(
        EconomicChain::new_unverified(config.clone(), ledger.clone()),
        Err(EconomicError::InvalidConfig)
    ));
    let chain = EconomicChain::new_with_source_catalogs_unverified(
        config.clone(),
        ledger.clone(),
        owners.clone(),
    )
    .unwrap();
    assert_eq!(chain.tip(), config.anchor());
    assert_eq!(chain.state().ledger(), &ledger);
    assert_eq!(chain.state().source_catalog_owners(), Some(&owners));
    let mut no_pin = config.clone();
    no_pin
        .economics
        .source_consensus
        .as_mut()
        .unwrap()
        .catalog_bootstrap_commitment = None;
    assert!(matches!(
        EconomicChain::new_with_source_catalogs_unverified(no_pin, ledger.clone(), owners.clone()),
        Err(EconomicError::InvalidConfig)
    ));
    let mut wrong_pin = config.clone();
    wrong_pin
        .economics
        .source_consensus
        .as_mut()
        .unwrap()
        .bootstrap_commitment[0] ^= 1;
    assert!(matches!(
        EconomicChain::new_with_source_catalogs_unverified(
            wrong_pin,
            ledger.clone(),
            owners.clone()
        ),
        Err(EconomicError::InvalidConfig)
    ));
    let mut changed_owners = owners;
    changed_owners.properties.replace_document(None).unwrap();
    assert!(matches!(
        EconomicChain::new_with_source_catalogs_unverified(config.clone(), ledger, changed_owners),
        Err(EconomicError::InvalidConfig)
    ));
    let bytes = chain.checkpoint_bytes().unwrap();
    assert_eq!(&bytes[..5], b"TLEH\x01");
    assert_eq!(
        EconomicChain::from_checkpoint_bytes(&bytes, config, chain.tip()).unwrap(),
        chain
    );
}

#[test]
fn full_catalog_chain_checkpoint_reorg_and_resume_restore_the_same_sole_ledger() {
    let mut chain = fixture();
    activate(&mut chain);
    let activated = chain.clone();
    let activated_tip = chain.tip();
    let b = issuance(&chain, 2);
    let report = chain.apply_source_unverified_block(&b).unwrap();
    assert_eq!(
        report.receipts[0].disposition,
        SourceDisposition::Applied,
        "{:?}",
        report.receipts
    );
    assert_eq!(property(&chain, 8).get("ticker"), s("OWNED"));
    assert_eq!(property(&chain, 8).get("totalInCirculation"), n(10.0));
    assert_eq!(
        chain.state().ledger().available_atoms(p(8), &a("alice")),
        10 * ATOMS
    );
    assert_eq!(
        chain.state().ledger().available_atoms(p(7), &a("alice")),
        100 * ATOMS
    );
    assert!(chain.state().ledger().is_processed(id(2)));
    let checkpoint = chain.checkpoint_bytes().unwrap();
    let mut restored =
        EconomicChain::from_checkpoint_bytes(&checkpoint, chain.config().clone(), chain.tip())
            .unwrap();
    assert_eq!(restored, chain);
    assert_eq!(restored.checkpoint_bytes().unwrap(), checkpoint);
    assert_eq!(
        restored.rollback_tip(restored.tip()).unwrap(),
        activated_tip
    );
    assert_eq!(restored, activated);
    assert_eq!(property(&restored, 8), V::Null);
    assert!(restored.state().ledger().property(p(8)).is_none());
    assert!(!restored.state().ledger().is_processed(id(2)));
    let replacement = issuance(&restored, 3);
    assert_eq!(
        restored
            .apply_source_unverified_block(&replacement)
            .unwrap()
            .accepted,
        1
    );
    assert_eq!(property(&restored, 8).get("ticker"), s("OWNED"));
    assert_eq!(
        restored.state().ledger().available_atoms(p(8), &a("alice")),
        10 * ATOMS
    );
    assert!(restored.state().ledger().property(p(9)).is_none());
    let complete = restored.checkpoint_bytes().unwrap();
    assert_eq!(
        EconomicChain::from_checkpoint_bytes(&complete, restored.config().clone(), restored.tip())
            .unwrap(),
        restored
    );
}

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "tl-source-catalog-chain-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
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
                    .to_string_lossy()
                    .starts_with("tl-source-catalog-chain-")
        );
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn durable_catalog_cas_restart_rollback_and_reexecution_keep_both_owners_together() {
    let directory = Directory::new();
    let mut chain = fixture();
    activate(&mut chain);
    let config = chain.config().clone();
    let activated_tip = chain.tip();
    let mut durable = DurableEconomicChain::initialize(&directory.0, chain, activated_tip).unwrap();
    let first = durable.head().unwrap();
    let b = issuance(durable.chain().unwrap(), 2);
    let (second, report) = durable.apply_source_unverified_block(first, &b).unwrap();
    assert_eq!(report.accepted, 1);
    assert!(second.sequence > first.sequence);
    let checkpoint = durable.chain().unwrap().checkpoint_bytes().unwrap();
    assert!(matches!(
        durable.apply_source_unverified_block(first, &b),
        Err(DurableEconomicError::StaleHead)
    ));
    assert_eq!(durable.head().unwrap(), second);
    assert_eq!(
        durable.chain().unwrap().checkpoint_bytes().unwrap(),
        checkpoint
    );
    drop(durable);
    let (mut durable, _) = DurableEconomicChain::load(
        &directory.0,
        config.clone(),
        second,
        Recovery::RejectIncompleteTail,
    )
    .unwrap();
    assert_eq!(
        durable.chain().unwrap().checkpoint_bytes().unwrap(),
        checkpoint
    );
    assert_eq!(
        property(durable.chain().unwrap(), 8).get("totalInCirculation"),
        n(10.0)
    );
    let rolled_back = durable.rollback_to(second, activated_tip).unwrap();
    assert!(rolled_back.sequence > second.sequence);
    assert_eq!(property(durable.chain().unwrap(), 8), V::Null);
    assert!(durable
        .chain()
        .unwrap()
        .state()
        .ledger()
        .property(p(8))
        .is_none());
    drop(durable);
    assert!(matches!(
        DurableEconomicChain::load(
            &directory.0,
            config.clone(),
            second,
            Recovery::RejectIncompleteTail
        ),
        Err(DurableEconomicError::StaleHead)
    ));
    let (mut durable, _) = DurableEconomicChain::load(
        &directory.0,
        config,
        rolled_back,
        Recovery::RejectIncompleteTail,
    )
    .unwrap();
    let replacement = issuance(durable.chain().unwrap(), 3);
    let (resumed, report) = durable
        .apply_source_unverified_block(rolled_back, &replacement)
        .unwrap();
    assert_eq!(report.accepted, 1);
    assert!(resumed.sequence > rolled_back.sequence);
    assert_eq!(
        property(durable.chain().unwrap(), 8).get("ticker"),
        s("OWNED")
    );
    assert_eq!(
        durable
            .chain()
            .unwrap()
            .state()
            .ledger()
            .available_atoms(p(8), &a("alice")),
        10 * ATOMS
    );
    assert!(durable
        .chain()
        .unwrap()
        .state()
        .ledger()
        .property(p(9))
        .is_none());
    let expected = durable.chain().unwrap().clone();
    let config = expected.config().clone();
    drop(durable);
    let (durable, _) = DurableEconomicChain::load(
        &directory.0,
        config,
        resumed,
        Recovery::RejectIncompleteTail,
    )
    .unwrap();
    assert_eq!(durable.chain().unwrap(), &expected);
}
