use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use tradelayer_core::decimal::Decimal;
use tradelayer_core::durable_iou::{
    DurableFundedIou, DurableIouError, IouOperation, IouOperationResult, MAX_DURABLE_IOU_OPERATIONS,
};
use tradelayer_core::iou::{IouKey, IouStore};
use tradelayer_core::iou_ledger::{
    FundedIouError, FundedIouLedger, IouInput, MatchProfit, PayoutAtomPolicy,
};
use tradelayer_core::ledger::{AccountId, Ledger, Operation, PropertyId, PropertyKind, TxId};
use tradelayer_core::network_profile::TradeLayerMainnet;
use tradelayer_core::storage::{Journal, Mutation, Namespace, Recovery, StorageError};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "tl-durable-iou-{}-{}-{}",
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
}
impl Drop for Directory {
    fn drop(&mut self) {
        assert!(self.0.is_absolute() && self.0.starts_with(std::env::temp_dir()));
        assert!(self
            .0
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("tl-durable-iou-"));
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn account(value: &str) -> AccountId {
    AccountId::try_from(value).unwrap()
}
fn property() -> PropertyId {
    PropertyId::try_from(1).unwrap()
}
fn key() -> IouKey {
    IouKey::new(42, 1)
}
fn decimal(value: &str) -> Decimal {
    value.parse().unwrap()
}
fn id(n: u8) -> TxId {
    TxId::from_bytes([n; 32])
}
fn input(n: u8, height: u64) -> IouInput {
    IouInput {
        network: TradeLayerMainnet::Bitcoin,
        txid: id(n),
        height,
    }
}
fn initial(network: TradeLayerMainnet) -> FundedIouLedger {
    let mut ledger = Ledger::new();
    ledger
        .apply(
            id(255),
            Operation::CreateProperty {
                id: property(),
                issuer: account("payer"),
                kind: PropertyKind::Fixed,
                initial_atoms: 1_000_000_000,
            },
        )
        .unwrap();
    FundedIouLedger::from_ledger_unverified(
        network,
        PayoutAtomPolicy::Exact,
        ledger,
        IouStore::new(),
    )
    .unwrap()
}
fn loss(n: u8, height: u64, amount: u128) -> IouOperation {
    IouOperation::CollectLoss {
        input: input(n, height),
        key: key(),
        payer: account("payer"),
        amount_atoms: amount,
    }
}

#[test]
fn full_profit_collection_payment_batch_is_one_durable_commit() {
    let directory = Directory::new();
    let mut owner =
        DurableFundedIou::initialize_unverified(&directory.0, initial(TradeLayerMainnet::Bitcoin))
            .unwrap();
    let old = owner.head().unwrap();
    let commit = owner
        .apply_batch_unverified(
            old,
            &[
                IouOperation::AccrueMatchProfit {
                    input: input(1, 10),
                    profit: MatchProfit {
                        key: key(),
                        buyer: account("buyer"),
                        seller: account("seller"),
                        buyer_pnl: decimal("6"),
                        seller_pnl: decimal("-2"),
                    },
                },
                loss(2, 11, 400_000_000),
                IouOperation::Pay {
                    input: input(3, 11),
                    key: key(),
                    budget_atoms: 400_000_000,
                },
            ],
        )
        .unwrap();
    assert_eq!(commit.head.sequence, old.sequence + 1);
    assert_ne!(commit.head.fingerprint, old.fingerprint);
    assert_eq!(commit.results.len(), 3);
    assert!(
        matches!(&commit.results[2], IouOperationResult::Paid(receipt) if receipt.credited_atoms==400_000_000)
    );
    let state = owner.state().unwrap().clone();
    assert_eq!(
        state
            .ledger()
            .available_atoms(property(), &account("buyer")),
        400_000_000
    );
    assert_eq!(
        state
            .ledger()
            .available_atoms(property(), &account("payer")),
        600_000_000
    );
    state.verify().unwrap();
    drop(owner);
    let (mut owner, report) = DurableFundedIou::open_existing(
        &directory.0,
        TradeLayerMainnet::Bitcoin,
        PayoutAtomPolicy::Exact,
        Recovery::RejectIncompleteTail,
        Some(commit.head),
    )
    .unwrap();
    assert_eq!(report.discarded_bytes, 0);
    assert_eq!(owner.state().unwrap(), &state);
    for n in 1..=3 {
        assert!(owner.is_processed_input(id(n)).unwrap());
    }
    assert!(matches!(
        owner.apply_batch_unverified(commit.head, &[loss(2, 11, 1)]),
        Err(DurableIouError::Operation(FundedIouError::Replay(_)))
    ));
}

#[test]
fn invalid_second_operation_rolls_back_funding_and_ids_before_any_disk_write() {
    let directory = Directory::new();
    let mut owner =
        DurableFundedIou::initialize_unverified(&directory.0, initial(TradeLayerMainnet::Bitcoin))
            .unwrap();
    let head = owner.head().unwrap();
    let before = owner.state().unwrap().clone();
    let length = std::fs::metadata(owner.path()).unwrap().len();
    assert!(owner
        .apply_batch_unverified(head, &[loss(1, 1, 100), loss(2, 1, 2_000_000_000)])
        .is_err());
    assert_eq!(owner.head().unwrap(), head);
    assert_eq!(owner.state().unwrap(), &before);
    assert!(!owner.is_processed_input(id(1)).unwrap());
    assert_eq!(std::fs::metadata(owner.path()).unwrap().len(), length);
    owner
        .apply_batch_unverified(head, &[loss(1, 1, 100)])
        .unwrap();
}

#[test]
fn stale_head_duplicate_ids_empty_and_oversized_batches_preserve_state() {
    let directory = Directory::new();
    let mut owner =
        DurableFundedIou::initialize_unverified(&directory.0, initial(TradeLayerMainnet::Bitcoin))
            .unwrap();
    let head = owner.head().unwrap();
    assert!(matches!(
        owner.apply_batch_unverified(head, &[]),
        Err(DurableIouError::InvalidBatch)
    ));
    assert!(matches!(
        owner.apply_batch_unverified(head, &vec![loss(1, 1, 1); MAX_DURABLE_IOU_OPERATIONS + 1]),
        Err(DurableIouError::InvalidBatch)
    ));
    assert!(matches!(
        owner.apply_batch_unverified(head, &[loss(1, 1, 1), loss(1, 1, 1)]),
        Err(DurableIouError::Operation(FundedIouError::Replay(_)))
    ));
    assert_eq!(owner.head().unwrap(), head);
    let next = owner
        .apply_batch_unverified(head, &[loss(1, 1, 1)])
        .unwrap()
        .head;
    assert!(matches!(
        owner.apply_batch_unverified(head, &[loss(2, 1, 1)]),
        Err(DurableIouError::HeadMismatch)
    ));
    assert_eq!(owner.head().unwrap(), next);
    assert!(!owner.is_processed_input(id(2)).unwrap());
}

#[test]
fn initialize_never_overwrites_and_reopen_never_bootstraps_missing_files() {
    let directory = Directory::new();
    assert!(DurableFundedIou::open_existing(
        &directory.0,
        TradeLayerMainnet::Bitcoin,
        PayoutAtomPolicy::Exact,
        Recovery::RejectIncompleteTail,
        None
    )
    .is_err());
    assert!(!directory.0.join("btc-main").exists());
    let owner =
        DurableFundedIou::initialize_unverified(&directory.0, initial(TradeLayerMainnet::Bitcoin))
            .unwrap();
    assert!(matches!(
        DurableFundedIou::initialize_unverified(&directory.0, initial(TradeLayerMainnet::Bitcoin)),
        Err(DurableIouError::Storage(StorageError::Locked))
    ));
    let head = owner.head().unwrap();
    drop(owner);
    assert!(matches!(
        DurableFundedIou::initialize_unverified(&directory.0, initial(TradeLayerMainnet::Bitcoin)),
        Err(DurableIouError::AlreadyInitialized)
    ));
    let owner = DurableFundedIou::open_existing(
        &directory.0,
        TradeLayerMainnet::Bitcoin,
        PayoutAtomPolicy::Exact,
        Recovery::RejectIncompleteTail,
        Some(head),
    )
    .unwrap()
    .0;
    assert_eq!(owner.head().unwrap(), head);
}

#[test]
fn independent_policy_head_and_namespace_checks_fail_closed() {
    let directory = Directory::new();
    let owner =
        DurableFundedIou::initialize_unverified(&directory.0, initial(TradeLayerMainnet::Bitcoin))
            .unwrap();
    let mut head = owner.head().unwrap();
    drop(owner);
    assert!(DurableFundedIou::open_existing(
        &directory.0,
        TradeLayerMainnet::Bitcoin,
        PayoutAtomPolicy::Down,
        Recovery::RejectIncompleteTail,
        None
    )
    .is_err());
    head.fingerprint[0] ^= 1;
    assert!(matches!(
        DurableFundedIou::open_existing(
            &directory.0,
            TradeLayerMainnet::Bitcoin,
            PayoutAtomPolicy::Exact,
            Recovery::RejectIncompleteTail,
            Some(head)
        ),
        Err(DurableIouError::HeadMismatch)
    ));
    let (mut journal, _) = Journal::open_existing(
        &directory.0,
        Namespace::for_mainnet(TradeLayerMainnet::Bitcoin, "iou").unwrap(),
        Recovery::RejectIncompleteTail,
    )
    .unwrap();
    journal
        .commit(vec![Mutation::Put {
            key: "unexpected".into(),
            value: vec![],
        }])
        .unwrap();
    drop(journal);
    assert!(matches!(
        DurableFundedIou::open_existing(
            &directory.0,
            TradeLayerMainnet::Bitcoin,
            PayoutAtomPolicy::Exact,
            Recovery::RejectIncompleteTail,
            None
        ),
        Err(DurableIouError::UnexpectedRecords)
    ));
}

#[test]
fn network_owners_use_independent_files_and_deny_cross_network_commands() {
    let directory = Directory::new();
    let mut bitcoin =
        DurableFundedIou::initialize_unverified(&directory.0, initial(TradeLayerMainnet::Bitcoin))
            .unwrap();
    let litecoin =
        DurableFundedIou::initialize_unverified(&directory.0, initial(TradeLayerMainnet::Litecoin))
            .unwrap();
    assert_ne!(bitcoin.path(), litecoin.path());
    assert_ne!(
        bitcoin.head().unwrap().fingerprint,
        litecoin.head().unwrap().fingerprint
    );
    let head = bitcoin.head().unwrap();
    let mut operation = loss(1, 1, 1);
    if let IouOperation::CollectLoss { input, .. } = &mut operation {
        input.network = TradeLayerMainnet::Litecoin;
    }
    assert!(matches!(
        bitcoin.apply_batch_unverified(head, &[operation]),
        Err(DurableIouError::Operation(FundedIouError::WrongNetwork))
    ));
    assert_eq!(bitcoin.head().unwrap(), head);
}

#[test]
fn trusted_previous_head_detects_valid_complete_prefix_rollback() {
    let directory = Directory::new();
    let mut owner =
        DurableFundedIou::initialize_unverified(&directory.0, initial(TradeLayerMainnet::Bitcoin))
            .unwrap();
    let path = owner.path().to_owned();
    let prefix = std::fs::metadata(&path).unwrap().len();
    let head = owner
        .apply_batch_unverified(owner.head().unwrap(), &[loss(1, 1, 1)])
        .unwrap()
        .head;
    drop(owner);
    std::fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .set_len(prefix)
        .unwrap();
    assert!(matches!(
        DurableFundedIou::open_existing(
            &directory.0,
            TradeLayerMainnet::Bitcoin,
            PayoutAtomPolicy::Exact,
            Recovery::RejectIncompleteTail,
            Some(head)
        ),
        Err(DurableIouError::HeadMismatch)
    ));
    let owner = DurableFundedIou::open_existing(
        &directory.0,
        TradeLayerMainnet::Bitcoin,
        PayoutAtomPolicy::Exact,
        Recovery::RejectIncompleteTail,
        None,
    )
    .unwrap()
    .0;
    assert!(!owner.is_processed_input(id(1)).unwrap());
}
