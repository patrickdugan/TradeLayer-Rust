use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use tradelayer_core::chain::{BlockHeader, BlockId, Tip};
use tradelayer_core::joint::{JointOperation, UnverifiedJointTx};
use tradelayer_core::joint_chain::{JointChainState, UnverifiedJointBlock};
use tradelayer_core::ledger::{AccountId, Ledger, Operation, PropertyId, PropertyKind, TxId};
use tradelayer_core::network_profile::TradeLayerMainnet;
use tradelayer_core::persistence::{
    characterize_source_snapshot_choice, source_checkpoint_due, source_cleanup_before,
    PersistenceConfig, PersistenceError, PersistentJointChain, MAX_HEADER_RETENTION,
};
use tradelayer_core::registry::RegistryAction;
use tradelayer_core::spot::SpotIntent;
use tradelayer_core::storage::{Journal, Mutation, Namespace, Recovery};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "tl-persistence-test-{}-{}-{}",
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
        for network in [
            TradeLayerMainnet::Bitcoin,
            TradeLayerMainnet::Litecoin,
            TradeLayerMainnet::Dogecoin,
        ] {
            let file = Namespace::for_mainnet(network, "persistence")
                .unwrap()
                .path(&self.0);
            let _ = fs::remove_file(&file);
            let _ = fs::remove_dir(file.parent().unwrap());
        }
        let _ = fs::remove_dir(&self.0);
    }
}
fn a(text: &str) -> AccountId {
    text.try_into().unwrap()
}
fn p(number: u32) -> PropertyId {
    number.try_into().unwrap()
}
fn id(number: u8) -> TxId {
    TxId::from_bytes([number; 32])
}
fn config(network: TradeLayerMainnet) -> PersistenceConfig {
    PersistenceConfig {
        network,
        anchor: Tip {
            height: 0,
            id: network.genesis(),
        },
        admin: a("admin"),
        permitted_spot_properties: BTreeSet::from([p(7), p(8)]),
        rollback_limit: 4,
        header_retention: 8,
    }
}
fn initial(config: &PersistenceConfig) -> JointChainState {
    let mut ledger = Ledger::new();
    for (asset, owner, number) in [(p(7), "seller", 1), (p(8), "buyer", 2)] {
        ledger
            .apply(
                id(number),
                Operation::CreateProperty {
                    id: asset,
                    issuer: a(owner),
                    kind: PropertyKind::Fixed,
                    initial_atoms: 1000,
                },
            )
            .unwrap();
    }
    JointChainState::new_unverified(
        config.network,
        config.anchor,
        config.admin.clone(),
        ledger,
        config.permitted_spot_properties.clone(),
        config.rollback_limit,
    )
    .unwrap()
}
fn create(dir: &Directory, config: &PersistenceConfig) -> PersistentJointChain {
    PersistentJointChain::initialize(&dir.0, config.clone(), initial(config), config.anchor)
        .unwrap()
}
fn tx(number: u8, actor: &str, operation: JointOperation) -> UnverifiedJointTx {
    UnverifiedJointTx {
        txid: id(number),
        actor: a(actor),
        operation,
    }
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
fn activate() -> UnverifiedJointTx {
    tx(
        10,
        "admin",
        JointOperation::Registry(RegistryAction::Activate {
            types: vec![1, 2, 5, 6, 11, 12],
            code_hash: [0x29; 32],
        }),
    )
}
fn send(number: u8, amount: u128) -> UnverifiedJointTx {
    tx(
        number,
        "seller",
        JointOperation::Send {
            id: p(7),
            recipient: a("alice"),
            amount_atoms: amount,
        },
    )
}

#[test]
fn complete_state_restart_atomic_multirollback_and_branch_replay() {
    for network in [
        TradeLayerMainnet::Bitcoin,
        TradeLayerMainnet::Litecoin,
        TradeLayerMainnet::Dogecoin,
    ] {
        let dir = Directory::new();
        let cfg = config(network);
        let mut owner = create(&dir, &cfg);
        let b1 = block(&owner, 20, vec![activate()]);
        let first_tip = owner.apply_unverified_block(&b1).unwrap();
        owner
            .apply_unverified_block(&block(&owner, 21, vec![send(11, 100)]))
            .unwrap();
        let latest = owner
            .apply_unverified_block(&block(
                &owner,
                22,
                vec![
                    send(12, 50),
                    tx(
                        13,
                        "seller",
                        JointOperation::SpotPlace(SpotIntent {
                            offered: p(7),
                            desired: p(8),
                            offered_atoms: 200,
                            expected_atoms: 400,
                            post: false,
                            stop: false,
                        }),
                    ),
                    tx(
                        14,
                        "buyer",
                        JointOperation::SpotPlace(SpotIntent {
                            offered: p(8),
                            desired: p(7),
                            offered_atoms: 220,
                            expected_atoms: 100,
                            post: false,
                            stop: false,
                        }),
                    ),
                ],
            ))
            .unwrap();
        assert_eq!(owner.state().unwrap().state().spot().open_count(), 1);
        assert_eq!(owner.state().unwrap().state().spot().fills().len(), 1);
        assert_eq!(
            owner
                .state()
                .unwrap()
                .state()
                .ledger()
                .available_atoms(p(7), &a("alice")),
            150
        );
        let complete = owner.state().unwrap().clone();
        assert_eq!(owner.sequence().unwrap(), 4);
        drop(owner);
        let (mut owner, report) =
            PersistentJointChain::load(&dir.0, cfg.clone(), latest, Recovery::RejectIncompleteTail)
                .unwrap();
        assert_eq!(report.discarded_bytes, 0);
        assert_eq!(owner.state().unwrap(), &complete);
        assert!(owner
            .apply_unverified_block(&block(&owner, 23, vec![send(11, 1)]))
            .is_err());
        assert_eq!(owner.sequence().unwrap(), 4);
        owner.rollback_to(latest, first_tip).unwrap();
        assert_eq!(owner.sequence().unwrap(), 5);
        assert_eq!(
            owner
                .state()
                .unwrap()
                .state()
                .ledger()
                .available_atoms(p(7), &a("alice")),
            0
        );
        assert_eq!(owner.get_block_header(2).unwrap(), None);
        assert_eq!(owner.get_block_header(3).unwrap(), None);
        drop(owner);
        let (mut owner, _) =
            PersistentJointChain::load(&dir.0, cfg, first_tip, Recovery::RejectIncompleteTail)
                .unwrap();
        owner
            .apply_unverified_block(&block(&owner, 24, vec![send(11, 7)]))
            .unwrap();
        assert_eq!(
            owner
                .state()
                .unwrap()
                .state()
                .ledger()
                .available_atoms(p(7), &a("alice")),
            7
        );
        assert_eq!(owner.headers().unwrap().count(), 2);
    }
}

#[test]
fn rejected_mixed_block_and_wrong_rollback_leave_durable_state_unchanged() {
    let dir = Directory::new();
    let cfg = config(TradeLayerMainnet::Bitcoin);
    let mut owner = create(&dir, &cfg);
    owner
        .apply_unverified_block(&block(&owner, 20, vec![activate()]))
        .unwrap();
    let tip = owner.tip().unwrap();
    let sequence = owner.sequence().unwrap();
    let before = owner.state().unwrap().clone();
    let length = fs::metadata(owner.path()).unwrap().len();
    assert!(owner
        .apply_unverified_block(&block(&owner, 21, vec![send(11, 10), send(12, 10000)]))
        .is_err());
    assert!(owner.rollback_tip(cfg.anchor).is_err());
    assert!(owner
        .rollback_to(
            tip,
            Tip {
                height: 0,
                id: BlockId([0xff; 32])
            }
        )
        .is_err());
    assert_eq!(owner.sequence().unwrap(), sequence);
    assert_eq!(owner.state().unwrap(), &before);
    assert_eq!(fs::metadata(owner.path()).unwrap().len(), length);
    drop(owner);
    let (owner, _) =
        PersistentJointChain::load(&dir.0, cfg, tip, Recovery::RejectIncompleteTail).unwrap();
    assert_eq!(owner.state().unwrap(), &before);
}

#[test]
fn existing_load_has_no_creation_and_initialization_cannot_replace_a_store() {
    let dir = Directory::new();
    let cfg = config(TradeLayerMainnet::Bitcoin);
    assert!(PersistentJointChain::load(
        &dir.0,
        cfg.clone(),
        cfg.anchor,
        Recovery::RejectIncompleteTail
    )
    .is_err());
    assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 0);
    let wrong_tip = Tip {
        height: 0,
        id: BlockId([9; 32]),
    };
    assert!(matches!(
        PersistentJointChain::initialize(&dir.0, cfg.clone(), initial(&cfg), wrong_tip),
        Err(PersistenceError::UnexpectedTip { .. })
    ));
    assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 0);
    let owner = create(&dir, &cfg);
    let path = owner.path();
    drop(owner);
    let before = fs::read(&path).unwrap();
    assert!(matches!(
        PersistentJointChain::initialize(&dir.0, cfg.clone(), initial(&cfg), cfg.anchor),
        Err(PersistenceError::AlreadyInitialized)
    ));
    assert_eq!(fs::read(path).unwrap(), before);
}

#[test]
fn historical_empty_journal_cannot_be_reinitialized_but_new_empty_header_can() {
    let dir = Directory::new();
    let cfg = config(TradeLayerMainnet::Bitcoin);
    let namespace = Namespace::for_mainnet(cfg.network, "persistence").unwrap();
    let (mut journal, _) =
        Journal::open(&dir.0, namespace, Recovery::RejectIncompleteTail).unwrap();
    journal
        .commit(vec![Mutation::Put {
            key: "temporary".into(),
            value: vec![1],
        }])
        .unwrap();
    journal
        .commit(vec![Mutation::Delete {
            key: "temporary".into(),
        }])
        .unwrap();
    let path = journal.path().to_path_buf();
    assert_eq!(journal.entries().unwrap().count(), 0);
    drop(journal);
    let before = fs::read(&path).unwrap();
    assert!(matches!(
        PersistentJointChain::initialize(&dir.0, cfg.clone(), initial(&cfg), cfg.anchor),
        Err(PersistenceError::AlreadyInitialized)
    ));
    assert_eq!(fs::read(path).unwrap(), before);
    let empty = Directory::new();
    let (journal, _) = Journal::open(
        &empty.0,
        Namespace::for_mainnet(cfg.network, "persistence").unwrap(),
        Recovery::RejectIncompleteTail,
    )
    .unwrap();
    drop(journal);
    let owner = create(&empty, &cfg);
    assert_eq!(owner.sequence().unwrap(), 1);
}

#[test]
fn independent_policy_anchor_tip_and_namespace_are_checked() {
    let dir = Directory::new();
    let cfg = config(TradeLayerMainnet::Bitcoin);
    drop(create(&dir, &cfg));
    for change in 0..6 {
        let mut wrong = cfg.clone();
        let mut expected_tip = cfg.anchor;
        match change {
            0 => wrong.network = TradeLayerMainnet::Litecoin,
            1 => wrong.anchor.id = BlockId([0x77; 32]),
            2 => wrong.admin = a("other"),
            3 => {
                wrong.permitted_spot_properties.remove(&p(7));
            }
            4 => wrong.rollback_limit = 3,
            5 => expected_tip.height = 1,
            _ => unreachable!(),
        }
        assert!(
            PersistentJointChain::load(&dir.0, wrong, expected_tip, Recovery::RejectIncompleteTail)
                .is_err(),
            "policy mutation {change}"
        );
    }
    let (owner, _) = PersistentJointChain::load(
        &dir.0,
        cfg.clone(),
        cfg.anchor,
        Recovery::RejectIncompleteTail,
    )
    .unwrap();
    assert_eq!(owner.tip().unwrap(), cfg.anchor);
}

#[test]
fn retained_headers_and_oldest_floor_rollback_survive_restart() {
    let dir = Directory::new();
    let mut cfg = config(TradeLayerMainnet::Bitcoin);
    cfg.rollback_limit = 2;
    cfg.header_retention = 2;
    let mut owner = create(&dir, &cfg);
    for marker in 20..26 {
        owner
            .apply_unverified_block(&block(&owner, marker, vec![]))
            .unwrap();
    }
    assert_eq!(
        owner
            .headers()
            .unwrap()
            .map(|h| h.height)
            .collect::<Vec<_>>(),
        vec![5, 6]
    );
    assert_eq!(owner.get_block_header(4).unwrap(), None);
    let tip = owner.tip().unwrap();
    owner.rollback_tip(tip).unwrap();
    let tip = owner.tip().unwrap();
    let floor = owner.rollback_tip(tip).unwrap();
    assert_eq!(floor.height, 4);
    assert_eq!(owner.state().unwrap().rollback_depth(), 0);
    assert_eq!(owner.headers().unwrap().count(), 0);
    let sequence = owner.sequence().unwrap();
    assert!(owner.rollback_tip(floor).is_err());
    assert_eq!(owner.sequence().unwrap(), sequence);
    drop(owner);
    let (mut owner, _) =
        PersistentJointChain::load(&dir.0, cfg, floor, Recovery::RejectIncompleteTail).unwrap();
    assert_eq!(owner.tip().unwrap(), floor);
    owner
        .apply_unverified_block(&block(&owner, 30, vec![]))
        .unwrap();
    assert_eq!(owner.tip().unwrap().height, 5);
}

#[test]
fn valid_journal_crc_does_not_bypass_checkpoint_header_layout_checks() {
    for mutation in 0..4 {
        let dir = Directory::new();
        let cfg = config(TradeLayerMainnet::Bitcoin);
        let mut owner = create(&dir, &cfg);
        let tip = owner
            .apply_unverified_block(&block(&owner, 20, vec![]))
            .unwrap();
        drop(owner);
        let (mut raw, _) = Journal::open_existing(
            &dir.0,
            Namespace::for_mainnet(cfg.network, "persistence").unwrap(),
            Recovery::RejectIncompleteTail,
        )
        .unwrap();
        let key = "joint/header/00000000000000000001".to_string();
        let change = match mutation {
            0 => Mutation::Delete { key },
            1 => {
                let mut bytes = 1_u64.to_be_bytes().to_vec();
                bytes.extend_from_slice(&[99; 64]);
                Mutation::Put { key, value: bytes }
            }
            2 => Mutation::Put {
                key: "unknown".into(),
                value: vec![0; 72],
            },
            3 => Mutation::Put {
                key: "joint/config".into(),
                value: vec![1],
            },
            _ => unreachable!(),
        };
        raw.commit(vec![change]).unwrap();
        drop(raw);
        assert!(
            PersistentJointChain::load(&dir.0, cfg, tip, Recovery::RejectIncompleteTail).is_err(),
            "mutation {mutation}"
        );
    }
}

#[test]
fn invalid_configuration_rejects_before_disk_changes() {
    for mutation in 0..4 {
        let dir = Directory::new();
        let mut cfg = config(TradeLayerMainnet::Bitcoin);
        let state = initial(&cfg);
        match mutation {
            0 => cfg.rollback_limit = 0,
            1 => cfg.header_retention = 3,
            2 => cfg.header_retention = MAX_HEADER_RETENTION + 1,
            3 => cfg.rollback_limit = 65,
            _ => unreachable!(),
        }
        assert!(matches!(
            PersistentJointChain::initialize(&dir.0, cfg.clone(), state, cfg.anchor),
            Err(PersistenceError::InvalidConfig)
        ));
        assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 0);
    }
}

#[test]
fn actual_source_integer_metadata_characterizations() {
    let mut checked = 0;
    for line in include_str!("../fixtures/persistence_vectors.tsv")
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
    {
        let f: Vec<_> = line.split('\t').collect();
        match f[1] {
            "cadence" => {
                assert_eq!(
                    source_checkpoint_due(f[2].parse().unwrap(), f[3].parse().unwrap()).to_string(),
                    f[4]
                );
                checked += 1;
            }
            "cleanup" => {
                assert_eq!(
                    source_cleanup_before(f[2].parse().unwrap()).to_string(),
                    f[4]
                );
                checked += 1;
            }
            "selection" => {
                let values: Vec<u64> = f[3]
                    .split(',')
                    .filter(|s| !s.is_empty())
                    .map(|s| s.parse().unwrap())
                    .collect();
                let actual = characterize_source_snapshot_choice(f[2].parse().unwrap(), &values)
                    .map(|h| h.to_string())
                    .unwrap_or_else(|| "null".into());
                assert_eq!(actual, f[4]);
                checked += 1;
            }
            "observation" => {}
            operation => panic!("unknown source vector operation {operation}"),
        }
    }
    assert_eq!(checked, 26);
}
