//! Regression for aggregate receipt retention, using real imported IOUs,
//! actual funded accounting and the synced journal rather than fabricated
//! receipts. Source pool reuse permits many zero-atom Down-policy payouts.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use tradelayer_core::decimal::Decimal;
use tradelayer_core::durable_iou::{
    DurableFundedIou, DurableIouError, IouOperation, IouOperationResult,
    MAX_DURABLE_IOU_ALLOCATIONS, MAX_DURABLE_IOU_OPERATIONS,
};
use tradelayer_core::iou::{Claim, IouKey, IouStore};
use tradelayer_core::iou_ledger::{FundedIouLedger, IouInput, PayoutAtomPolicy};
use tradelayer_core::ledger::{AccountId, Ledger, Operation, PropertyId, PropertyKind, TxId};
use tradelayer_core::network_profile::TradeLayerMainnet;
use tradelayer_core::storage::Recovery;

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "tl-durable-iou-capacity-{}-{}-{}",
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
impl Drop for Directory {
    fn drop(&mut self) {
        assert!(self.0.is_absolute() && self.0.starts_with(std::env::temp_dir()));
        assert!(self
            .0
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("tl-durable-iou-capacity-"));
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn account(value: &str) -> AccountId {
    AccountId::try_from(value).unwrap()
}
fn decimal(value: &str) -> Decimal {
    value.parse().unwrap()
}
fn property() -> PropertyId {
    PropertyId::try_from(1).unwrap()
}
fn key() -> IouKey {
    IouKey::new(42, 1)
}
fn id(value: u32) -> TxId {
    let mut bytes = [0u8; 32];
    bytes[..4].copy_from_slice(&value.to_be_bytes());
    TxId::from_bytes(bytes)
}
fn input(value: u32) -> IouInput {
    IouInput {
        network: TradeLayerMainnet::Bitcoin,
        txid: id(value),
        height: 10,
    }
}
fn pay(value: u32) -> IouOperation {
    IouOperation::Pay {
        input: input(value),
        key: key(),
        budget_atoms: 1,
    }
}

fn imported_funded_dust(claimants: usize) -> FundedIouLedger {
    let mut ledger = Ledger::new();
    ledger
        .apply(
            id(u32::MAX),
            Operation::CreateProperty {
                id: property(),
                issuer: account("payer"),
                kind: PropertyKind::Fixed,
                initial_atoms: 1_000_000,
            },
        )
        .unwrap();
    let mut ious = IouStore::new();
    ious.save_claim_map(
        key(),
        10,
        (0..claimants)
            .map(|index| Claim {
                address: account(&format!("dust-claimant-{index:04}")),
                amount: decimal("1"),
            })
            .collect(),
    )
    .unwrap();
    let mut state = FundedIouLedger::from_ledger_unverified(
        TradeLayerMainnet::Bitcoin,
        PayoutAtomPolicy::Down,
        ledger,
        ious,
    )
    .unwrap();
    state
        .collect_loss_unverified(input(0), key(), account("payer"), 1)
        .unwrap();
    assert_eq!(
        state.ious().bucket(key()).unwrap().block_losses,
        decimal("0.00000001")
    );
    state
}

fn assert_zero_atom_receipts(results: &[IouOperationResult], claimants: usize) -> usize {
    let mut allocations = 0;
    for result in results {
        let IouOperationResult::Paid(receipt) = result else {
            panic!("expected payout receipt");
        };
        assert_eq!(receipt.budget_atoms, 1);
        assert_eq!(receipt.credited_atoms, 0);
        assert_eq!(receipt.allocations.len(), claimants);
        assert!(receipt
            .allocations
            .iter()
            .all(|allocation| allocation.credited_atoms == 0
                && allocation.source_amount > Decimal::zero()
                && allocation.atom_rounding_delta.is_negative()));
        allocations += receipt.allocations.len();
    }
    allocations
}

#[test]
fn aggregate_zero_atom_receipt_overflow_rejects_whole_batch_and_allows_retry() {
    let directory = Directory::new();
    let initial = imported_funded_dust(65);
    let escrow = initial.escrow_for_audit(key()).unwrap();
    let owner = DurableFundedIou::initialize_unverified(&directory.0, initial).unwrap();
    let before = owner.state().unwrap().clone();
    let head = owner.head().unwrap();
    let path = owner.path().to_owned();
    // Windows enforces the journal's exclusive byte-range lock for reads too.
    // Read the durable image only while no owner holds that lock.
    drop(owner);
    let journal_before = std::fs::read(&path).unwrap();
    let (mut owner, _) = DurableFundedIou::open_existing(
        &directory.0,
        TradeLayerMainnet::Bitcoin,
        PayoutAtomPolicy::Down,
        Recovery::RejectIncompleteTail,
        Some(head),
    )
    .unwrap();
    let operations: Vec<_> = (1..=MAX_DURABLE_IOU_OPERATIONS)
        .map(|n| pay(u32::try_from(n).unwrap()))
        .collect();
    assert!(65 * operations.len() > MAX_DURABLE_IOU_ALLOCATIONS);
    assert!(matches!(
        owner.apply_batch_unverified(head, &operations),
        Err(DurableIouError::ReceiptLimit)
    ));
    assert_eq!(owner.state().unwrap(), &before);
    assert_eq!(owner.head().unwrap(), head);
    for n in 1..=MAX_DURABLE_IOU_OPERATIONS {
        assert!(!owner
            .is_processed_input(id(u32::try_from(n).unwrap()))
            .unwrap());
    }
    assert!(owner.is_processed_input(id(0)).unwrap());
    drop(owner);
    assert_eq!(
        std::fs::read(&path).unwrap(),
        journal_before,
        "receipt rejection must precede journal I/O"
    );
    let (mut owner, _) = DurableFundedIou::open_existing(
        &directory.0,
        TradeLayerMainnet::Bitcoin,
        PayoutAtomPolicy::Down,
        Recovery::RejectIncompleteTail,
        Some(head),
    )
    .unwrap();

    // Retry the same rejected IDs in a bounded first batch. The original failed
    // transaction must have consumed neither claims, replay markers nor funds.
    let first_count = MAX_DURABLE_IOU_ALLOCATIONS / 65;
    assert!(first_count > 1 && first_count < operations.len());
    let first = owner
        .apply_batch_unverified(head, &operations[..first_count])
        .unwrap();
    assert_eq!(
        assert_zero_atom_receipts(&first.results, 65),
        first_count * 65
    );
    assert_eq!(first.head.sequence, head.sequence + 1);
    for n in 1..=first_count {
        assert!(owner
            .is_processed_input(id(u32::try_from(n).unwrap()))
            .unwrap());
    }
    assert!(!owner
        .is_processed_input(id(u32::try_from(first_count + 1).unwrap()))
        .unwrap());
    let second = owner
        .apply_batch_unverified(first.head, &operations[first_count..])
        .unwrap();
    assert_eq!(
        assert_zero_atom_receipts(&second.results, 65),
        (operations.len() - first_count) * 65
    );
    assert_eq!(second.head.sequence, head.sequence + 2);
    assert_eq!(
        owner
            .state()
            .unwrap()
            .ledger()
            .available_atoms(property(), &escrow),
        1
    );
    assert_eq!(
        owner
            .state()
            .unwrap()
            .ledger()
            .available_atoms(property(), &account("payer")),
        999_999
    );
    assert_eq!(
        owner
            .state()
            .unwrap()
            .ious()
            .claim_map(key())
            .unwrap()
            .claims()
            .len(),
        65
    );
    owner.state().unwrap().verify().unwrap();

    let final_state = owner.state().unwrap().clone();
    drop(owner);
    let (owner, recovery) = DurableFundedIou::open_existing(
        &directory.0,
        TradeLayerMainnet::Bitcoin,
        PayoutAtomPolicy::Down,
        Recovery::RejectIncompleteTail,
        Some(second.head),
    )
    .unwrap();
    assert_eq!(recovery.discarded_bytes, 0);
    assert_eq!(owner.state().unwrap(), &final_state);
    for n in 1..=MAX_DURABLE_IOU_OPERATIONS {
        assert!(owner
            .is_processed_input(id(u32::try_from(n).unwrap()))
            .unwrap());
    }
}

#[test]
fn exact_aggregate_allocation_limit_accepts_real_dust_receipts() {
    let directory = Directory::new();
    assert_eq!(MAX_DURABLE_IOU_ALLOCATIONS % MAX_DURABLE_IOU_OPERATIONS, 0);
    let claimants = MAX_DURABLE_IOU_ALLOCATIONS / MAX_DURABLE_IOU_OPERATIONS;
    let mut owner =
        DurableFundedIou::initialize_unverified(&directory.0, imported_funded_dust(claimants))
            .unwrap();
    let head = owner.head().unwrap();
    let operations: Vec<_> = (1..=MAX_DURABLE_IOU_OPERATIONS)
        .map(|n| pay(u32::try_from(n).unwrap()))
        .collect();
    let commit = owner.apply_batch_unverified(head, &operations).unwrap();
    assert_eq!(
        assert_zero_atom_receipts(&commit.results, claimants),
        MAX_DURABLE_IOU_ALLOCATIONS
    );
    assert_eq!(commit.head.sequence, head.sequence + 1);
    owner.state().unwrap().verify().unwrap();
}
