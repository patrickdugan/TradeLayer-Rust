use std::collections::BTreeSet;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use tradelayer_core::bootstrap::*;
use tradelayer_core::bootstrap_storage::*;
use tradelayer_core::chain::{BlockHeader, BlockId, Tip};
use tradelayer_core::durable_iou::{
    DurableFundedIou, DurableIouError, DurableIouHead, IouOperation, IouOperationResult,
};
use tradelayer_core::iou::{IouKey, IouStore};
use tradelayer_core::iou_ledger::{
    FundedIouError, FundedIouLedger, IouInput, MatchProfit, PayoutAtomPolicy,
};
use tradelayer_core::joint::{JointOperation, UnverifiedJointTx};
use tradelayer_core::joint_chain::{JointChainState, UnverifiedJointBlock};
use tradelayer_core::ledger::{AccountId, Ledger, Operation, PropertyId, PropertyKind, TxId};
use tradelayer_core::network_profile::TradeLayerMainnet;
use tradelayer_core::persistence::{PersistenceConfig, PersistentJointChain};
use tradelayer_core::registry::RegistryAction;
use tradelayer_core::storage::{Journal, Mutation, Namespace, Recovery, StorageError};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "tl-bootstrap-storage-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn config(&self, category: &str) -> BootstrapConfig {
        BootstrapConfig::new(TradeLayerMainnet::Bitcoin, self.0.clone(), category).unwrap()
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        assert!(self.0.is_absolute() && self.0.starts_with(std::env::temp_dir()));
        assert!(self
            .0
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("tl-bootstrap-storage-"));
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn a(name: &str) -> AccountId {
    name.try_into().unwrap()
}
fn p(id: u32) -> PropertyId {
    id.try_into().unwrap()
}
fn txid(value: u8) -> TxId {
    TxId::from_bytes([value; 32])
}
struct Clients;
struct Client(TradeLayerMainnet);
impl BootstrapClientFactory for Clients {
    type Client = Client;
    fn acquire(
        &mut self,
        config: &BootstrapConfig,
        control: &BootstrapControl,
    ) -> Result<Client, String> {
        control.check().map_err(|e| format!("{e:?}"))?;
        Ok(Client(config.network()))
    }
}
impl BootstrapClient for Client {
    fn identity(&mut self, control: &BootstrapControl) -> Result<ClaimedNodeIdentity, String> {
        control.check().map_err(|e| format!("{e:?}"))?;
        Ok(ClaimedNodeIdentity {
            network: self.0,
            test: false,
            genesis: self.0.genesis(),
        })
    }
}

fn create_iou(root: &Temp) -> DurableIouHead {
    let mut ledger = Ledger::new();
    ledger
        .apply(
            txid(255),
            Operation::CreateProperty {
                id: p(1),
                issuer: a("payer"),
                kind: PropertyKind::Fixed,
                initial_atoms: 1_000_000_000,
            },
        )
        .unwrap();
    let state = FundedIouLedger::from_ledger_unverified(
        TradeLayerMainnet::Bitcoin,
        PayoutAtomPolicy::Exact,
        ledger,
        IouStore::new(),
    )
    .unwrap();
    let owner = DurableFundedIou::initialize_unverified(&root.0, state).unwrap();
    owner.head().unwrap()
}
fn iou_factory(head: DurableIouHead) -> FundedIouStoreFactory {
    FundedIouStoreFactory::new(FundedIouRestoreConfig {
        network: TradeLayerMainnet::Bitcoin,
        atom_policy: PayoutAtomPolicy::Exact,
        expected_head: head,
        recovery: Recovery::RejectIncompleteTail,
    })
}
fn iou_input(value: u8) -> IouInput {
    IouInput {
        network: TradeLayerMainnet::Bitcoin,
        txid: txid(value),
        height: 10,
    }
}
fn loss(value: u8, amount_atoms: u128) -> IouOperation {
    IouOperation::CollectLoss {
        input: iou_input(value),
        key: IouKey::new(42, 1),
        payer: a("payer"),
        amount_atoms,
    }
}

#[test]
fn funded_iou_bootstrap_payout_restart_and_replay_keep_one_durable_owner() {
    let root = Temp::new();
    let original = create_iou(&root);
    let cfg = root.config("iou");
    let mut ready = BootstrapCoordinator::default()
        .start(
            cfg.clone(),
            &mut Clients,
            &mut iou_factory(original),
            &BootstrapControl::default(),
        )
        .unwrap();
    assert_eq!(
        ready.store_mut().recovery_report().unwrap().discarded_bytes,
        0
    );
    let operations = [
        IouOperation::AccrueMatchProfit {
            input: iou_input(1),
            profit: MatchProfit {
                key: IouKey::new(42, 1),
                buyer: a("buyer"),
                seller: a("seller"),
                buyer_pnl: "6".parse().unwrap(),
                seller_pnl: "-2".parse().unwrap(),
            },
        },
        loss(2, 400_000_000),
        IouOperation::Pay {
            input: iou_input(3),
            key: IouKey::new(42, 1),
            budget_atoms: 400_000_000,
        },
    ];
    let commit = ready
        .state_mut()
        .apply_batch_unverified(original, &operations)
        .unwrap();
    assert!(
        matches!(&commit.results[2], IouOperationResult::Paid(receipt) if receipt.credited_atoms == 400_000_000)
    );
    assert_eq!(
        ready
            .state()
            .state()
            .unwrap()
            .ledger()
            .available_atoms(p(1), &a("buyer")),
        400_000_000
    );
    assert!(matches!(
        Journal::open_existing(
            cfg.root(),
            cfg.namespace().clone(),
            Recovery::RejectIncompleteTail
        ),
        Err(StorageError::Locked)
    ));
    drop(ready);
    let mut ready = BootstrapCoordinator::default()
        .start(
            cfg,
            &mut Clients,
            &mut iou_factory(commit.head),
            &BootstrapControl::default(),
        )
        .unwrap();
    assert_eq!(ready.state().head().unwrap(), commit.head);
    assert_eq!(
        ready
            .state()
            .state()
            .unwrap()
            .ledger()
            .available_atoms(p(1), &a("payer")),
        600_000_000
    );
    for value in 1..=3 {
        assert!(ready.state().is_processed_input(txid(value)).unwrap());
    }
    assert!(matches!(
        ready
            .state_mut()
            .apply_batch_unverified(commit.head, &[loss(2, 1)]),
        Err(DurableIouError::Operation(FundedIouError::Replay(_)))
    ));
    ready
        .state_mut()
        .apply_batch_unverified(commit.head, &[loss(4, 1)])
        .unwrap();
    ready.state().state().unwrap().verify().unwrap();
}

fn persistence_config() -> PersistenceConfig {
    let network = TradeLayerMainnet::Bitcoin;
    PersistenceConfig {
        network,
        anchor: Tip {
            height: 0,
            id: network.genesis(),
        },
        admin: a("admin"),
        permitted_spot_properties: BTreeSet::from([p(7)]),
        rollback_limit: 4,
        header_retention: 8,
    }
}
fn create_joint(root: &Temp, config: &PersistenceConfig) {
    let mut ledger = Ledger::new();
    ledger
        .apply(
            txid(255),
            Operation::CreateProperty {
                id: p(7),
                issuer: a("seller"),
                kind: PropertyKind::Fixed,
                initial_atoms: 1000,
            },
        )
        .unwrap();
    let initial = JointChainState::new_unverified(
        config.network,
        config.anchor,
        config.admin.clone(),
        ledger,
        config.permitted_spot_properties.clone(),
        config.rollback_limit,
    )
    .unwrap();
    PersistentJointChain::initialize(&root.0, config.clone(), initial, config.anchor).unwrap();
}
fn joint_factory(config: &PersistenceConfig, expected_tip: Tip) -> JointStoreFactory {
    JointStoreFactory::new(JointRestoreConfig {
        persistence: config.clone(),
        expected_tip,
        recovery: Recovery::RejectIncompleteTail,
    })
}
fn block(
    owner: &PersistentJointChain,
    marker: u8,
    transactions: Vec<UnverifiedJointTx>,
) -> UnverifiedJointBlock {
    let tip = owner.tip().unwrap();
    UnverifiedJointBlock {
        network: owner.config().unwrap().network,
        header: BlockHeader {
            height: tip.height + 1,
            id: BlockId([marker; 32]),
            parent: tip.id,
        },
        transactions,
    }
}
fn send(amount: u128) -> UnverifiedJointTx {
    UnverifiedJointTx {
        txid: txid(11),
        actor: a("seller"),
        operation: JointOperation::Send {
            id: p(7),
            recipient: a("alice"),
            amount_atoms: amount,
        },
    }
}

#[test]
fn joint_bootstrap_mutation_restart_and_rollback_preserve_replay_state() {
    let root = Temp::new();
    let pcfg = persistence_config();
    create_joint(&root, &pcfg);
    let cfg = root.config("persistence");
    let mut ready = BootstrapCoordinator::default()
        .start(
            cfg.clone(),
            &mut Clients,
            &mut joint_factory(&pcfg, pcfg.anchor),
            &BootstrapControl::default(),
        )
        .unwrap();
    let activation = UnverifiedJointTx {
        txid: txid(10),
        actor: a("admin"),
        operation: JointOperation::Registry(RegistryAction::Activate {
            types: vec![2],
            code_hash: [0x29; 32],
        }),
    };
    let b1 = block(ready.state(), 20, vec![activation]);
    let first = ready.state_mut().apply_unverified_block(&b1).unwrap();
    let b2 = block(ready.state(), 21, vec![send(100)]);
    let latest = ready.state_mut().apply_unverified_block(&b2).unwrap();
    assert_eq!(
        ready
            .state()
            .state()
            .unwrap()
            .state()
            .ledger()
            .available_atoms(p(7), &a("alice")),
        100
    );
    assert_eq!(
        ready.store_mut().recovery_report().unwrap().discarded_bytes,
        0
    );
    drop(ready);
    let mut ready = BootstrapCoordinator::default()
        .start(
            cfg.clone(),
            &mut Clients,
            &mut joint_factory(&pcfg, latest),
            &BootstrapControl::default(),
        )
        .unwrap();
    let replay = block(ready.state(), 22, vec![send(1)]);
    assert!(ready.state_mut().apply_unverified_block(&replay).is_err());
    assert_eq!(ready.state().tip().unwrap(), latest);
    ready.state_mut().rollback_to(latest, first).unwrap();
    assert_eq!(
        ready
            .state()
            .state()
            .unwrap()
            .state()
            .ledger()
            .available_atoms(p(7), &a("alice")),
        0
    );
    let alternate = block(ready.state(), 23, vec![send(7)]);
    let alternate_tip = ready
        .state_mut()
        .apply_unverified_block(&alternate)
        .unwrap();
    drop(ready);
    let ready = BootstrapCoordinator::default()
        .start(
            cfg,
            &mut Clients,
            &mut joint_factory(&pcfg, alternate_tip),
            &BootstrapControl::default(),
        )
        .unwrap();
    assert_eq!(ready.state().tip().unwrap(), alternate_tip);
    assert_eq!(
        ready
            .state()
            .state()
            .unwrap()
            .state()
            .ledger()
            .available_atoms(p(7), &a("alice")),
        7
    );
    assert_eq!(
        ready.state().get_block_header(2).unwrap().unwrap().id,
        BlockId([23; 32])
    );
}

#[test]
fn factory_scope_and_bound_root_reject_before_opening_any_files() {
    let root = Temp::new();
    let other = Temp::new();
    let factory = iou_factory(DurableIouHead {
        sequence: 1,
        fingerprint: [0; 32],
    });
    let control = BootstrapControl::default();
    assert!(matches!(
        factory.open_checked(&root.config("persistence"), &control),
        Err(RestoreAdapterError::ScopeMismatch)
    ));
    let wrong_network =
        BootstrapConfig::new(TradeLayerMainnet::Litecoin, root.0.clone(), "iou").unwrap();
    assert!(matches!(
        factory.open_checked(&wrong_network, &control),
        Err(RestoreAdapterError::ScopeMismatch)
    ));
    let mut pending = factory.open_checked(&root.config("iou"), &control).unwrap();
    assert!(matches!(
        pending.restore_checked(&other.config("iou"), &control),
        Err(RestoreAdapterError::ScopeMismatch)
    ));
    let joint = joint_factory(&persistence_config(), persistence_config().anchor);
    assert!(matches!(
        joint.open_checked(&root.config("iou"), &control),
        Err(RestoreAdapterError::ScopeMismatch)
    ));
    let wrong_network =
        BootstrapConfig::new(TradeLayerMainnet::Litecoin, root.0.clone(), "persistence").unwrap();
    assert!(matches!(
        joint.open_checked(&wrong_network, &control),
        Err(RestoreAdapterError::ScopeMismatch)
    ));
    let mut pending = joint
        .open_checked(&root.config("persistence"), &control)
        .unwrap();
    assert!(matches!(
        pending.restore_checked(&other.config("persistence"), &control),
        Err(RestoreAdapterError::ScopeMismatch)
    ));
    assert_eq!(std::fs::read_dir(&root.0).unwrap().count(), 0);
    assert_eq!(std::fs::read_dir(&other.0).unwrap().count(), 0);
}

fn add_short_tail(config: &BootstrapConfig) -> u64 {
    let path = config.namespace().path(config.root());
    let length = std::fs::metadata(&path).unwrap().len();
    let mut file = std::fs::OpenOptions::new().append(true).open(path).unwrap();
    file.write_all(b"TX").unwrap();
    file.sync_all().unwrap();
    length
}

#[test]
fn explicit_recovery_policy_and_reports_reach_both_actual_state_owners() {
    let root = Temp::new();
    let head = create_iou(&root);
    let cfg = root.config("iou");
    let length = add_short_tail(&cfg);
    assert!(BootstrapCoordinator::default()
        .start(
            cfg.clone(),
            &mut Clients,
            &mut iou_factory(head),
            &BootstrapControl::default()
        )
        .is_err());
    assert_eq!(
        std::fs::metadata(cfg.namespace().path(cfg.root()))
            .unwrap()
            .len(),
        length + 2
    );
    let mut expected = *iou_factory(head).expected();
    expected.recovery = Recovery::DiscardIncompleteTail;
    let mut ready = BootstrapCoordinator::default()
        .start(
            cfg.clone(),
            &mut Clients,
            &mut FundedIouStoreFactory::new(expected),
            &BootstrapControl::default(),
        )
        .unwrap();
    assert_eq!(
        ready.store_mut().recovery_report().unwrap().discarded_bytes,
        2
    );
    assert_eq!(ready.state().head().unwrap(), head);
    assert_eq!(
        std::fs::metadata(cfg.namespace().path(cfg.root()))
            .unwrap()
            .len(),
        length
    );
    drop(ready);

    let pcfg = persistence_config();
    create_joint(&root, &pcfg);
    let cfg = root.config("persistence");
    let length = add_short_tail(&cfg);
    assert!(BootstrapCoordinator::default()
        .start(
            cfg.clone(),
            &mut Clients,
            &mut joint_factory(&pcfg, pcfg.anchor),
            &BootstrapControl::default()
        )
        .is_err());
    let mut expected = joint_factory(&pcfg, pcfg.anchor).expected().clone();
    expected.recovery = Recovery::DiscardIncompleteTail;
    let mut ready = BootstrapCoordinator::default()
        .start(
            cfg.clone(),
            &mut Clients,
            &mut JointStoreFactory::new(expected),
            &BootstrapControl::default(),
        )
        .unwrap();
    assert_eq!(
        ready.store_mut().recovery_report().unwrap().discarded_bytes,
        2
    );
    assert_eq!(ready.state().tip().unwrap(), pcfg.anchor);
    assert_eq!(
        std::fs::metadata(cfg.namespace().path(cfg.root()))
            .unwrap()
            .len(),
        length
    );
}

#[test]
fn missing_state_and_malformed_checkpoint_never_initialize_fallback_state() {
    let root = Temp::new();
    let control = BootstrapControl::default();
    let cfg = root.config("iou");
    let factory = iou_factory(DurableIouHead {
        sequence: 1,
        fingerprint: [0; 32],
    });
    let mut pending = factory.open_checked(&cfg, &control).unwrap();
    assert!(pending.restore_checked(&cfg, &control).is_err());
    assert_eq!(std::fs::read_dir(&root.0).unwrap().count(), 0);
    assert!(matches!(
        pending.restore_checked(&cfg, &control),
        Err(RestoreAdapterError::AlreadyAttempted)
    ));

    let pcfg = persistence_config();
    create_joint(&root, &pcfg);
    let cfg = root.config("persistence");
    let (mut journal, _) = Journal::open_existing(
        cfg.root(),
        cfg.namespace().clone(),
        Recovery::RejectIncompleteTail,
    )
    .unwrap();
    journal
        .commit(vec![Mutation::Put {
            key: "joint/state".to_owned(),
            value: b"malformed complete checkpoint".to_vec(),
        }])
        .unwrap();
    let path = journal.path().to_owned();
    drop(journal);
    let before = std::fs::read(&path).unwrap();
    let result = BootstrapCoordinator::default().start(
        cfg.clone(),
        &mut Clients,
        &mut joint_factory(&pcfg, pcfg.anchor),
        &control,
    );
    assert!(matches!(
        result,
        Err(BootstrapError::Adapter {
            stage: BootstrapStage::RestoreState,
            ..
        })
    ));
    assert_eq!(std::fs::read(&path).unwrap(), before);
    let _reopened = Journal::open_existing(
        cfg.root(),
        cfg.namespace().clone(),
        Recovery::RejectIncompleteTail,
    )
    .unwrap();
}

#[test]
fn independent_head_tip_and_policy_mismatches_release_opened_locks() {
    let root = Temp::new();
    let head = create_iou(&root);
    let cfg = root.config("iou");
    let mut wrong_head = head;
    wrong_head.sequence += 1;
    assert!(BootstrapCoordinator::default()
        .start(
            cfg.clone(),
            &mut Clients,
            &mut iou_factory(wrong_head),
            &BootstrapControl::default()
        )
        .is_err());
    let mut wrong_policy = FundedIouStoreFactory::new(FundedIouRestoreConfig {
        network: TradeLayerMainnet::Bitcoin,
        atom_policy: PayoutAtomPolicy::Down,
        expected_head: head,
        recovery: Recovery::RejectIncompleteTail,
    });
    assert!(BootstrapCoordinator::default()
        .start(
            cfg.clone(),
            &mut Clients,
            &mut wrong_policy,
            &BootstrapControl::default()
        )
        .is_err());
    let ready = BootstrapCoordinator::default()
        .start(
            cfg,
            &mut Clients,
            &mut iou_factory(head),
            &BootstrapControl::default(),
        )
        .unwrap();
    drop(ready);
    let pcfg = persistence_config();
    create_joint(&root, &pcfg);
    let cfg = root.config("persistence");
    let wrong_tip = Tip {
        height: 1,
        id: BlockId([1; 32]),
    };
    assert!(BootstrapCoordinator::default()
        .start(
            cfg.clone(),
            &mut Clients,
            &mut joint_factory(&pcfg, wrong_tip),
            &BootstrapControl::default()
        )
        .is_err());
    let _ready = BootstrapCoordinator::default()
        .start(
            cfg,
            &mut Clients,
            &mut joint_factory(&pcfg, pcfg.anchor),
            &BootstrapControl::default(),
        )
        .unwrap();
}

struct CancelFactory<F>(F);
struct CancelStore<S>(S);
impl<F: BootstrapStoreFactory> BootstrapStoreFactory for CancelFactory<F> {
    type Store = CancelStore<F::Store>;
    fn open(
        &mut self,
        config: &BootstrapConfig,
        control: &BootstrapControl,
    ) -> Result<Self::Store, String> {
        self.0.open(config, control).map(CancelStore)
    }
}
impl<S: BootstrapStore> BootstrapStore for CancelStore<S> {
    type State = S::State;
    fn restore(
        &mut self,
        config: &BootstrapConfig,
        control: &BootstrapControl,
    ) -> Result<RestoredState<Self::State>, String> {
        let restored = self.0.restore(config, control)?;
        control.cancel();
        Ok(restored)
    }
}

#[test]
fn cancellation_after_each_actual_restore_drops_owner_and_releases_journal_lock() {
    let root = Temp::new();
    let head = create_iou(&root);
    let cfg = root.config("iou");
    let result = BootstrapCoordinator::default().start(
        cfg.clone(),
        &mut Clients,
        &mut CancelFactory(iou_factory(head)),
        &BootstrapControl::default(),
    );
    assert!(matches!(
        result,
        Err(BootstrapError::Stopped {
            stage: BootstrapStage::RestoreState,
            reason: StopReason::Cancelled
        })
    ));
    let ready = BootstrapCoordinator::default()
        .start(
            cfg,
            &mut Clients,
            &mut iou_factory(head),
            &BootstrapControl::default(),
        )
        .unwrap();
    drop(ready);
    let pcfg = persistence_config();
    create_joint(&root, &pcfg);
    let cfg = root.config("persistence");
    let result = BootstrapCoordinator::default().start(
        cfg.clone(),
        &mut Clients,
        &mut CancelFactory(joint_factory(&pcfg, pcfg.anchor)),
        &BootstrapControl::default(),
    );
    assert!(matches!(
        result,
        Err(BootstrapError::Stopped {
            stage: BootstrapStage::RestoreState,
            reason: StopReason::Cancelled
        })
    ));
    let _ready = BootstrapCoordinator::default()
        .start(
            cfg,
            &mut Clients,
            &mut joint_factory(&pcfg, pcfg.anchor),
            &BootstrapControl::default(),
        )
        .unwrap();
}

#[test]
fn cancellation_before_restore_opens_nothing_and_retains_one_shot_for_uncancelled_control() {
    let root = Temp::new();
    let head = create_iou(&root);
    let cfg = root.config("iou");
    let factory = iou_factory(head);
    let control = BootstrapControl::default();
    let mut pending = factory.open_checked(&cfg, &control).unwrap();
    control.cancel();
    assert!(matches!(
        pending.restore_checked(&cfg, &control),
        Err(RestoreAdapterError::Stopped(StopReason::Cancelled))
    ));
    let owner = pending
        .restore_checked(&cfg, &BootstrapControl::default())
        .unwrap()
        .state;
    assert_eq!(owner.head().unwrap(), head);
    assert!(matches!(
        pending.restore_checked(&cfg, &BootstrapControl::default()),
        Err(RestoreAdapterError::AlreadyAttempted)
    ));
    assert!(matches!(
        Journal::open_existing(
            cfg.root(),
            cfg.namespace().clone(),
            Recovery::RejectIncompleteTail
        ),
        Err(StorageError::Locked)
    ));
    drop(owner);
    let namespace = Namespace::for_mainnet(TradeLayerMainnet::Bitcoin, "iou").unwrap();
    let _journal =
        Journal::open_existing(&root.0, namespace, Recovery::RejectIncompleteTail).unwrap();
}
