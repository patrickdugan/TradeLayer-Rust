use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use tradelayer_core::decimal::Decimal;
use tradelayer_core::iou::{Claim, IouKey, IouStore};
use tradelayer_core::iou_ledger::{FundedIouLedger, IouInput, PayoutAtomPolicy};
use tradelayer_core::ledger::{AccountId, Ledger, Operation, PropertyId, PropertyKind, TxId};
use tradelayer_core::network_profile::TradeLayerMainnet;
use tradelayer_core::storage::{Journal, Mutation, Namespace, Recovery, StorageError, CATEGORIES};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "tl-storage-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        Self(root)
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
            .starts_with("tl-storage-"));
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn namespace() -> Namespace {
    Namespace::new("BTC", false, "persistence").unwrap()
}
fn open(root: &Path) -> Journal {
    Journal::open(root, namespace(), Recovery::RejectIncompleteTail)
        .unwrap()
        .0
}
fn put(key: &str, value: &[u8]) -> Mutation {
    Mutation::Put {
        key: key.to_owned(),
        value: value.to_vec(),
    }
}

#[test]
fn atomic_batches_restart_and_exclusive_lock() {
    let root = Temp::new();
    let mut journal = open(&root.0);
    assert!(matches!(
        Journal::open(&root.0, namespace(), Recovery::RejectIncompleteTail),
        Err(StorageError::Locked)
    ));
    assert_eq!(
        journal
            .commit(vec![put("z", b"last"), put("a", b"first")])
            .unwrap(),
        1
    );
    assert_eq!(
        journal
            .commit(vec![
                put("a", b"changed"),
                Mutation::Delete { key: "z".into() },
                put("empty", b"")
            ])
            .unwrap(),
        2
    );
    let before: Vec<_> = journal
        .entries()
        .unwrap()
        .map(|(key, value)| (key.to_owned(), value.to_vec()))
        .collect();
    drop(journal);
    let journal = open(&root.0);
    assert_eq!(journal.sequence().unwrap(), 2);
    assert_eq!(
        journal
            .entries()
            .unwrap()
            .map(|(key, value)| (key.to_owned(), value.to_vec()))
            .collect::<Vec<_>>(),
        before
    );
    assert_eq!(journal.get("a").unwrap(), Some(b"changed".as_slice()));
    assert_eq!(journal.get("z").unwrap(), None);
    assert_eq!(journal.get("empty").unwrap(), Some(b"".as_slice()));
}

#[test]
fn invalid_batches_never_append_or_publish() {
    let root = Temp::new();
    let mut journal = open(&root.0);
    journal.commit(vec![put("kept", b"old")]).unwrap();
    let path = journal.path().to_owned();
    drop(journal);
    let bytes = std::fs::read(&path).unwrap();
    let mut journal = open(&root.0);
    assert!(matches!(
        journal.commit(vec![]),
        Err(StorageError::EmptyBatch)
    ));
    assert!(matches!(
        journal.commit(vec![put("dup", b"a"), put("dup", b"b")]),
        Err(StorageError::DuplicateKey)
    ));
    assert!(matches!(
        journal.commit(vec![put("kept", b"new"), put("", b"bad")]),
        Err(StorageError::InvalidKey)
    ));
    assert!(matches!(
        journal.commit(vec![put(&"k".repeat(257), b"bad")]),
        Err(StorageError::InvalidKey)
    ));
    assert!(matches!(
        journal.commit((0..1025).map(|n| put(&format!("key-{n}"), b"x")).collect()),
        Err(StorageError::LimitExceeded)
    ));
    assert_eq!(journal.sequence().unwrap(), 1);
    assert_eq!(journal.get("kept").unwrap(), Some(b"old".as_slice()));
    drop(journal);
    assert_eq!(std::fs::read(path).unwrap(), bytes);
}

#[test]
fn every_torn_second_frame_requires_explicit_recovery_and_retains_first_batch() {
    let root = Temp::new();
    let mut journal = open(&root.0);
    journal.commit(vec![put("kept", b"old")]).unwrap();
    let boundary = std::fs::metadata(journal.path()).unwrap().len() as usize;
    journal
        .commit(vec![put("kept", b"new"), put("second", b"batch")])
        .unwrap();
    let path = journal.path().to_owned();
    drop(journal);
    let bytes = std::fs::read(&path).unwrap();
    for cut in boundary + 1..bytes.len() {
        std::fs::write(&path, &bytes[..cut]).unwrap();
        assert!(
            matches!(Journal::open(&root.0, namespace(), Recovery::RejectIncompleteTail), Err(StorageError::IncompleteTail { offset }) if offset == boundary as u64),
            "cut={cut}"
        );
        assert_eq!(std::fs::metadata(&path).unwrap().len(), cut as u64);
        let (mut recovered, report) =
            Journal::open(&root.0, namespace(), Recovery::DiscardIncompleteTail).unwrap();
        assert_eq!(report.discarded_bytes, (cut - boundary) as u64);
        assert_eq!(recovered.sequence().unwrap(), 1);
        assert_eq!(recovered.get("kept").unwrap(), Some(b"old".as_slice()));
        assert_eq!(recovered.get("second").unwrap(), None);
        recovered.commit(vec![put("continued", b"yes")]).unwrap();
        drop(recovered);
        assert_eq!(
            open(&root.0).get("continued").unwrap(),
            Some(b"yes".as_slice())
        );
    }
}

#[test]
fn complete_corruption_is_never_tail_recovery() {
    let root = Temp::new();
    let mut journal = open(&root.0);
    let boundary = std::fs::metadata(journal.path()).unwrap().len() as usize;
    journal.commit(vec![put("key", b"value")]).unwrap();
    let path = journal.path().to_owned();
    drop(journal);
    let bytes = std::fs::read(&path).unwrap();
    for index in boundary..bytes.len() {
        let mut damaged = bytes.clone();
        damaged[index] ^= 1;
        std::fs::write(&path, &damaged).unwrap();
        assert!(
            Journal::open(&root.0, namespace(), Recovery::DiscardIncompleteTail).is_err(),
            "byte={index}"
        );
        assert_eq!(std::fs::read(&path).unwrap(), damaged);
    }
}

#[test]
fn bounded_namespaces_and_file_identity() {
    let root = Temp::new();
    assert_eq!(CATEGORIES.len(), 32);
    for category in CATEGORIES {
        let ns = Namespace::new("BTC", false, category).unwrap();
        let (mut journal, _) = Journal::open(&root.0, ns, Recovery::RejectIncompleteTail).unwrap();
        journal.commit(vec![put("x", category.as_bytes())]).unwrap();
    }
    for chain in ["", "../btc", "..", "BTC/evil", "BTC\\evil", "BTC:", "🚀"] {
        assert!(Namespace::new(chain, false, "iou").is_err());
    }
    assert!(Namespace::new("BTC", false, "other").is_err());
    assert_eq!(
        Namespace::new("BtC", true, "iou").unwrap().path(&root.0),
        root.0.join("btc-test/iou.tldb")
    );
    let path = namespace().path(&root.0);
    let other = Namespace::new("LTC", false, "persistence").unwrap();
    std::fs::create_dir_all(other.path(&root.0).parent().unwrap()).unwrap();
    std::fs::copy(path, other.path(&root.0)).unwrap();
    assert!(matches!(
        Journal::open(&root.0, other, Recovery::DiscardIncompleteTail),
        Err(StorageError::WrongNamespace)
    ));
}

#[test]
fn os_lock_child() {
    let Some(root) = std::env::var_os("TL_STORAGE_LOCK_CHILD_ROOT") else {
        return;
    };
    assert!(matches!(
        Journal::open(
            Path::new(&root),
            namespace(),
            Recovery::RejectIncompleteTail
        ),
        Err(StorageError::Locked)
    ));
}

#[test]
fn os_lock_excludes_another_process() {
    let root = Temp::new();
    let journal = open(&root.0);
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "os_lock_child", "--nocapture"])
        .env("TL_STORAGE_LOCK_CHILD_ROOT", &root.0)
        .status()
        .unwrap();
    assert!(status.success());
    drop(journal);
    assert_eq!(open(&root.0).sequence().unwrap(), 0);
}

fn account(value: &str) -> AccountId {
    AccountId::try_from(value).unwrap()
}
fn txid(n: u8) -> TxId {
    TxId::from_bytes([n; 32])
}
fn decimal(value: &str) -> Decimal {
    value.parse().unwrap()
}

#[test]
fn funded_iou_disk_restart_keeps_escrow_policy_and_replay_fences() {
    let root = Temp::new();
    let network = TradeLayerMainnet::Bitcoin;
    let property = PropertyId::try_from(1).unwrap();
    let key = IouKey::new(42, 1);
    let mut ledger = Ledger::new();
    ledger
        .apply(
            txid(250),
            Operation::CreateProperty {
                id: property,
                issuer: account("payer"),
                kind: PropertyKind::Fixed,
                initial_atoms: 1_000_000_000,
            },
        )
        .unwrap();
    let mut claims = IouStore::new();
    claims
        .save_claim_map(
            key,
            0,
            vec![Claim {
                address: account("winner"),
                amount: decimal("2"),
            }],
        )
        .unwrap();
    let mut state =
        FundedIouLedger::from_ledger_unverified(network, PayoutAtomPolicy::Exact, ledger, claims)
            .unwrap();
    let input = IouInput {
        network,
        txid: txid(1),
        height: 1,
    };
    state
        .collect_loss_unverified(input, key, account("payer"), 200_000_000)
        .unwrap();
    let ns = Namespace::for_mainnet(network, "iou").unwrap();
    let (mut journal, _) =
        Journal::open(&root.0, ns.clone(), Recovery::RejectIncompleteTail).unwrap();
    journal.save_funded_iou("tip", &state).unwrap();
    drop(journal);
    let (journal, _) = Journal::open(&root.0, ns, Recovery::RejectIncompleteTail).unwrap();
    let mut restored = journal
        .load_funded_iou("tip", network, PayoutAtomPolicy::Exact)
        .unwrap()
        .unwrap();
    assert_eq!(restored, state);
    assert!(restored
        .collect_loss_unverified(input, key, account("payer"), 200_000_000)
        .is_err());
    assert!(journal
        .load_funded_iou("tip", network, PayoutAtomPolicy::Down)
        .is_err());
    assert!(journal
        .load_funded_iou("tip", TradeLayerMainnet::Litecoin, PayoutAtomPolicy::Exact)
        .is_err());
    let (mut wrong_store, _) =
        Journal::open(&root.0, namespace(), Recovery::RejectIncompleteTail).unwrap();
    assert!(matches!(
        wrong_store.save_funded_iou("tip", &state),
        Err(StorageError::WrongNamespace)
    ));
    assert_eq!(wrong_store.sequence().unwrap(), 0);
}
