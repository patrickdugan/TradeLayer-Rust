//! Single-owner synced IOU transactions. All input remains caller asserted.
//! A complete funded ledger and its replay IDs commit in one journal batch.

use crate::hash256::double_sha256_parts;
use crate::iou::IouKey;
use crate::iou_ledger::{
    FundedIouError, FundedIouLedger, IouInput, IouPayoutReceipt, MatchProfit, PayoutAtomPolicy,
};
use crate::ledger::{AccountId, TxId};
use crate::network_profile::TradeLayerMainnet;
use crate::storage::{Journal, Mutation, Namespace, Recovery, RecoveryReport, StorageError};
use std::path::Path;

const STATE_KEY: &str = "funded-iou/state/v1";
pub const MAX_DURABLE_IOU_OPERATIONS: usize = 256;
pub const MAX_DURABLE_IOU_ALLOCATIONS: usize = crate::iou::MAX_IOU_CLAIMS;
pub const MAX_DURABLE_IOU_RECEIPT_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug)]
pub enum DurableIouError {
    Storage(StorageError),
    Operation(FundedIouError),
    AlreadyInitialized,
    MissingState,
    UnexpectedRecords,
    HeadMismatch,
    InvalidBatch,
    ReceiptLimit,
}
impl std::fmt::Display for DurableIouError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for DurableIouError {}
impl From<StorageError> for DurableIouError {
    fn from(error: StorageError) -> Self {
        Self::Storage(error)
    }
}
impl From<FundedIouError> for DurableIouError {
    fn from(error: FundedIouError) -> Self {
        Self::Operation(error)
    }
}

/// Local comparison token. Fingerprint and sequence do not authenticate a tip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DurableIouHead {
    pub sequence: u64,
    pub fingerprint: [u8; 32],
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IouOperation {
    CollectLoss {
        input: IouInput,
        key: IouKey,
        payer: AccountId,
        amount_atoms: u128,
    },
    AccrueMatchProfit {
        input: IouInput,
        profit: MatchProfit,
    },
    Pay {
        input: IouInput,
        key: IouKey,
        budget_atoms: u128,
    },
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IouOperationResult {
    LossCollected,
    MatchProfitAccrued,
    Paid(IouPayoutReceipt),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DurableIouCommit {
    pub head: DurableIouHead,
    /// Same order as the submitted operations. Returned only after sync.
    pub results: Vec<IouOperationResult>,
}

#[derive(Debug)]
pub struct DurableFundedIou {
    journal: Journal,
    state: FundedIouLedger,
    head: DurableIouHead,
}
impl DurableFundedIou {
    /// Explicit import into an empty namespace. Imported economics are unverified.
    pub fn initialize_unverified(
        root: &Path,
        state: FundedIouLedger,
    ) -> Result<Self, DurableIouError> {
        let namespace = Namespace::for_mainnet(state.network(), "iou")?;
        let (mut journal, _) = Journal::open(root, namespace, Recovery::RejectIncompleteTail)?;
        if journal.sequence()? != 0 || journal.entries()?.next().is_some() {
            return Err(DurableIouError::AlreadyInitialized);
        }
        let bytes = snapshot(&state)?;
        let fingerprint = fingerprint(&bytes);
        let sequence = journal.commit(vec![Mutation::Put {
            key: STATE_KEY.to_owned(),
            value: bytes,
        }])?;
        Ok(Self {
            journal,
            state,
            head: DurableIouHead {
                sequence,
                fingerprint,
            },
        })
    }

    /// Existing files only. Expected head can come from independent deployment
    /// state; omitting it accepts any internally valid local prefix, not a tip proof.
    pub fn open_existing(
        root: &Path,
        network: TradeLayerMainnet,
        policy: PayoutAtomPolicy,
        recovery: Recovery,
        expected_head: Option<DurableIouHead>,
    ) -> Result<(Self, RecoveryReport), DurableIouError> {
        let namespace = Namespace::for_mainnet(network, "iou")?;
        let (journal, report) = Journal::open_existing(root, namespace, recovery)?;
        let mut entries = journal.entries()?;
        let Some((key, bytes)) = entries.next() else {
            return Err(DurableIouError::MissingState);
        };
        if key != STATE_KEY || entries.next().is_some() {
            return Err(DurableIouError::UnexpectedRecords);
        }
        let head = DurableIouHead {
            sequence: journal.sequence()?,
            fingerprint: fingerprint(bytes),
        };
        if expected_head.is_some_and(|expected| expected != head) {
            return Err(DurableIouError::HeadMismatch);
        }
        let state = FundedIouLedger::from_snapshot_bytes(bytes, network, policy)
            .map_err(StorageError::Snapshot)?;
        drop(entries);
        Ok((
            Self {
                journal,
                state,
                head,
            },
            report,
        ))
    }

    pub fn state(&self) -> Result<&FundedIouLedger, DurableIouError> {
        self.journal.sequence()?;
        Ok(&self.state)
    }
    pub fn head(&self) -> Result<DurableIouHead, DurableIouError> {
        self.journal.sequence()?;
        Ok(self.head)
    }
    pub fn is_processed_input(&self, txid: TxId) -> Result<bool, DurableIouError> {
        Ok(self.state()?.is_processed_input(txid))
    }
    pub fn path(&self) -> &Path {
        self.journal.path()
    }

    /// Compare head, stage the complete batch, serialize, sync, then publish.
    /// A validation error leaves both memory and disk unchanged. An I/O error
    /// can mean the batch reached disk: all subsequent cached reads are denied
    /// until drop/reopen and replay-ID reconciliation.
    pub fn apply_batch_unverified(
        &mut self,
        expected_head: DurableIouHead,
        operations: &[IouOperation],
    ) -> Result<DurableIouCommit, DurableIouError> {
        if self.head()? != expected_head {
            return Err(DurableIouError::HeadMismatch);
        }
        if operations.is_empty() || operations.len() > MAX_DURABLE_IOU_OPERATIONS {
            return Err(DurableIouError::InvalidBatch);
        }
        let mut staged = self.state.clone();
        let mut results = Vec::with_capacity(operations.len());
        let mut allocation_count = 0_usize;
        let mut receipt_bytes = 0_usize;
        for operation in operations {
            let result = match operation {
                IouOperation::CollectLoss {
                    input,
                    key,
                    payer,
                    amount_atoms,
                } => {
                    staged.collect_loss_unverified(*input, *key, payer.clone(), *amount_atoms)?;
                    IouOperationResult::LossCollected
                }
                IouOperation::AccrueMatchProfit { input, profit } => {
                    staged.accrue_match_profit_unverified(*input, profit.clone())?;
                    IouOperationResult::MatchProfitAccrued
                }
                IouOperation::Pay {
                    input,
                    key,
                    budget_atoms,
                } => {
                    IouOperationResult::Paid(staged.pay_unverified(*input, *key, *budget_atoms)?)
                }
            };
            if let IouOperationResult::Paid(receipt) = &result {
                allocation_count = allocation_count
                    .checked_add(receipt.allocations.len())
                    .ok_or(DurableIouError::ReceiptLimit)?;
                if allocation_count > MAX_DURABLE_IOU_ALLOCATIONS {
                    return Err(DurableIouError::ReceiptLimit);
                }
                for allocation in &receipt.allocations {
                    let bytes = allocation.address.as_str().len()
                        + allocation.source_amount.to_string().len()
                        + allocation.atom_rounding_delta.to_string().len()
                        + 16;
                    receipt_bytes = receipt_bytes
                        .checked_add(bytes)
                        .ok_or(DurableIouError::ReceiptLimit)?;
                    if receipt_bytes > MAX_DURABLE_IOU_RECEIPT_BYTES {
                        return Err(DurableIouError::ReceiptLimit);
                    }
                }
            }
            results.push(result);
        }
        let bytes = snapshot(&staged)?;
        let fingerprint = fingerprint(&bytes);
        let sequence = self.journal.commit(vec![Mutation::Put {
            key: STATE_KEY.to_owned(),
            value: bytes,
        }])?;
        self.state = staged;
        self.head = DurableIouHead {
            sequence,
            fingerprint,
        };
        Ok(DurableIouCommit {
            head: self.head,
            results,
        })
    }
}
fn snapshot(state: &FundedIouLedger) -> Result<Vec<u8>, DurableIouError> {
    state
        .snapshot_bytes()
        .map_err(|error| StorageError::Snapshot(error).into())
}
fn fingerprint(bytes: &[u8]) -> [u8; 32] {
    double_sha256_parts(&[b"TradeLayer/durable-funded-iou/state/v1", bytes])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::iou::IouStore;
    use crate::ledger::{Ledger, Operation, PropertyId, PropertyKind};
    use std::path::PathBuf;

    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "tl-durable-iou-fault-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
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
                .starts_with("tl-durable-iou-fault-"));
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn setup(root: &Path) -> DurableFundedIou {
        let mut ledger = Ledger::new();
        ledger
            .apply(
                TxId::from_bytes([255; 32]),
                Operation::CreateProperty {
                    id: PropertyId::try_from(1).unwrap(),
                    issuer: AccountId::try_from("payer").unwrap(),
                    kind: PropertyKind::Fixed,
                    initial_atoms: 1000,
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
        DurableFundedIou::initialize_unverified(root, state).unwrap()
    }
    fn operation() -> IouOperation {
        IouOperation::CollectLoss {
            input: IouInput {
                network: TradeLayerMainnet::Bitcoin,
                txid: TxId::from_bytes([1; 32]),
                height: 1,
            },
            key: IouKey::new(42, 1),
            payer: AccountId::try_from("payer").unwrap(),
            amount_atoms: 100,
        }
    }
    fn reopen(
        root: &Path,
        recovery: Recovery,
        head: Option<DurableIouHead>,
    ) -> Result<(DurableFundedIou, RecoveryReport), DurableIouError> {
        DurableFundedIou::open_existing(
            root,
            TradeLayerMainnet::Bitcoin,
            PayoutAtomPolicy::Exact,
            recovery,
            head,
        )
    }
    fn assert_poisoned(owner: &mut DurableFundedIou, old: DurableIouHead) {
        assert!(matches!(
            owner.state(),
            Err(DurableIouError::Storage(StorageError::Poisoned))
        ));
        assert!(matches!(
            owner.head(),
            Err(DurableIouError::Storage(StorageError::Poisoned))
        ));
        assert!(matches!(
            owner.is_processed_input(TxId::from_bytes([1; 32])),
            Err(DurableIouError::Storage(StorageError::Poisoned))
        ));
        assert!(matches!(
            owner.apply_batch_unverified(old, &[operation()]),
            Err(DurableIouError::Storage(StorageError::Poisoned))
        ));
    }

    #[test]
    fn lost_acknowledgement_blocks_cached_access_then_reopens_committed_replay_marker() {
        let directory = Directory::new();
        let mut owner = setup(&directory.0);
        let old = owner.head().unwrap();
        owner.journal.test_fail_after_sync();
        assert!(matches!(
            owner.apply_batch_unverified(old, &[operation()]),
            Err(DurableIouError::Storage(StorageError::Io(_)))
        ));
        assert_poisoned(&mut owner, old);
        drop(owner);
        assert!(matches!(
            reopen(&directory.0, Recovery::RejectIncompleteTail, Some(old)),
            Err(DurableIouError::HeadMismatch)
        ));
        let (mut owner, report) =
            reopen(&directory.0, Recovery::RejectIncompleteTail, None).unwrap();
        assert_eq!(report.discarded_bytes, 0);
        let head = owner.head().unwrap();
        assert_eq!(head.sequence, old.sequence + 1);
        assert!(owner.is_processed_input(TxId::from_bytes([1; 32])).unwrap());
        assert!(matches!(
            owner.apply_batch_unverified(head, &[operation()]),
            Err(DurableIouError::Operation(FundedIouError::Replay(_)))
        ));
    }

    #[test]
    fn partial_write_requires_explicit_recovery_and_retry_uses_unconsumed_input() {
        let directory = Directory::new();
        let mut owner = setup(&directory.0);
        let old = owner.head().unwrap();
        owner.journal.test_fail_after_partial_write();
        assert!(matches!(
            owner.apply_batch_unverified(old, &[operation()]),
            Err(DurableIouError::Storage(StorageError::Io(_)))
        ));
        assert_poisoned(&mut owner, old);
        drop(owner);
        assert!(matches!(
            reopen(&directory.0, Recovery::RejectIncompleteTail, Some(old)),
            Err(DurableIouError::Storage(
                StorageError::IncompleteTail { .. }
            ))
        ));
        let (mut owner, report) =
            reopen(&directory.0, Recovery::DiscardIncompleteTail, Some(old)).unwrap();
        assert!(report.discarded_bytes > 0);
        assert!(!owner.is_processed_input(TxId::from_bytes([1; 32])).unwrap());
        owner.apply_batch_unverified(old, &[operation()]).unwrap();
        assert!(owner.is_processed_input(TxId::from_bytes([1; 32])).unwrap());
    }
}
