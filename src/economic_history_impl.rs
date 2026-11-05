// Included inside economic.rs: bounded trade-receipt history.
//
// A state keeps only the most recent `RECEIPT_WINDOW` receipts. At the end of
// every block the older ones leave the state and enter an append-only receipt
// log. The state commits to everything that left through a `ReceiptArchive`:
// the count, a running hash over the canonical bytes of each archived receipt,
// the running totals the audits need (loss pool, bad debt, insurance requests),
// and the fee book rows as they stood after the archived accruals. Consensus
// never reads an archived receipt: the audits extend the committed totals over
// the window.
//
// The log is node-local runtime data. It is not serialized with the state, not
// compared and not printed. A state decoded from a snapshot starts with a
// pruned log whose base is the archive head; `attach_receipt_log_prefix` joins
// the earlier receipts to it after checking them against that head. A full
// audit checks the log against the committed hash and, when the log reaches
// back to the first receipt, recomputes every committed total from it.
//
// Archiving applies when `marking` is not configured. A marking configuration
// pairs each ordinary receipt with a mark event in histories that replay from
// the first receipt (marks, liquidations, the funded AMM, accepted contracts),
// so those configurations keep every receipt in the state.

/// Receipts kept in the state after every block.
pub const RECEIPT_WINDOW: usize = 256;
const RECEIPT_LOG_CHUNK: usize = 256;
const RECEIPT_LOG_DOMAIN: &[u8] = b"TradeLayer/EconomicReceiptLog/v1";
const ECONOMIC_HISTORY_MAGIC: &[u8; 5] = b"TLEC\x0b";

/// Commitment to the receipts that left the state.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct ReceiptArchive {
    count: u64,
    last_height: u64,
    hash: [u8; 32],
    collected_atoms: u128,
    paid_atoms: u128,
    unfunded_loss_atoms: u128,
    insurance_atoms: u128,
    /// Fee book rows after every archived accrual, with an empty journal.
    fees: FeeBook,
}

/// Append-only log of archived receipts, shared between state clones.
#[derive(Clone, Default)]
pub struct ReceiptLog {
    base_count: u64,
    base_hash: [u8; 32],
    chunks: Vec<Arc<Vec<TradeReceipt>>>,
    len: u64,
}
impl ReceiptLog {
    fn pruned_at(count: u64, hash: [u8; 32]) -> Self {
        Self {
            base_count: count,
            base_hash: hash,
            chunks: Vec::new(),
            len: 0,
        }
    }
    /// Index of the first receipt this log holds.
    pub fn base_count(&self) -> u64 {
        self.base_count
    }
    /// Receipts this log holds.
    pub fn len(&self) -> u64 {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    /// True when the log starts at the first receipt ever archived.
    pub fn is_complete(&self) -> bool {
        self.base_count == 0
    }
    pub fn iter(&self) -> impl Iterator<Item = &TradeReceipt> + '_ {
        self.chunks.iter().flat_map(|chunk| chunk.iter())
    }
    fn push_all(&mut self, receipts: Vec<TradeReceipt>) {
        self.len += receipts.len() as u64;
        let mut rest = receipts.into_iter();
        if let Some(last) = self.chunks.last_mut() {
            if last.len() < RECEIPT_LOG_CHUNK {
                let tail = Arc::make_mut(last);
                while tail.len() < RECEIPT_LOG_CHUNK {
                    match rest.next() {
                        Some(receipt) => tail.push(receipt),
                        None => break,
                    }
                }
            }
        }
        loop {
            let part: Vec<_> = rest.by_ref().take(RECEIPT_LOG_CHUNK).collect();
            if part.is_empty() {
                break;
            }
            self.chunks.push(Arc::new(part));
        }
    }
    /// The first `keep` receipts of this log, or `None` when it holds fewer.
    fn truncated(&self, keep: u64) -> Option<Self> {
        if keep > self.len {
            return None;
        }
        let mut out = Self {
            base_count: self.base_count,
            base_hash: self.base_hash,
            chunks: Vec::new(),
            len: 0,
        };
        let mut remaining = keep as usize;
        for chunk in &self.chunks {
            if remaining == 0 {
                break;
            }
            if chunk.len() <= remaining {
                remaining -= chunk.len();
                out.len += chunk.len() as u64;
                out.chunks.push(chunk.clone());
            } else {
                out.len += remaining as u64;
                out.chunks.push(Arc::new(chunk[..remaining].to_vec()));
                remaining = 0;
            }
        }
        Some(out)
    }
}
impl core::fmt::Debug for ReceiptLog {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ReceiptLog")
            .field("base_count", &self.base_count)
            .field("len", &self.len)
            .finish()
    }
}

/// Runtime-only history settings and the receipt log. Clones keep both;
/// equality ignores them, so snapshot bytes and state equality do not depend
/// on them. The limits are the consensus constants outside of tests.
#[derive(Clone)]
struct HistoryRuntime {
    window: usize,
    receipt_cap: usize,
    log: ReceiptLog,
}
impl Default for HistoryRuntime {
    fn default() -> Self {
        Self {
            window: RECEIPT_WINDOW,
            receipt_cap: MAX_ECONOMIC_RECEIPTS,
            log: ReceiptLog::default(),
        }
    }
}
impl HistoryRuntime {
    fn pruned_at(archive: &ReceiptArchive) -> Self {
        Self {
            log: ReceiptLog::pruned_at(archive.count, archive.hash),
            ..Self::default()
        }
    }
}
impl PartialEq for HistoryRuntime {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}
impl Eq for HistoryRuntime {}

fn receipt_log_step(previous: &[u8; 32], receipt: &TradeReceipt) -> Result<[u8; 32]> {
    let mut bytes = EconomicWriter(Vec::new());
    econ_write_receipt(&mut bytes, receipt)?;
    Ok(crate::hash256::double_sha256_parts(&[
        RECEIPT_LOG_DOMAIN,
        previous,
        &bytes.0,
    ]))
}

impl EconomicState {
    /// The receipts still in the state: the most recent ones, in order. Older
    /// receipts are in the receipt log; `receipt_count` counts all of them.
    pub fn receipts(&self) -> &[TradeReceipt] {
        &self.receipts
    }

    /// Every receipt ever settled, archived or not.
    pub fn receipt_count(&self) -> u64 {
        self.receipt_archive.count + self.receipts.len() as u64
    }

    /// Receipts that have left the state for the receipt log.
    pub fn archived_receipt_count(&self) -> u64 {
        self.receipt_archive.count
    }

    /// The receipts settled after the first `total_before`, as far as the state
    /// still holds them. `total_before` is an earlier `receipt_count()`.
    pub fn receipts_since(&self, total_before: u64) -> &[TradeReceipt] {
        let start = total_before.saturating_sub(self.receipt_archive.count);
        usize::try_from(start)
            .ok()
            .and_then(|start| self.receipts.get(start..))
            .unwrap_or(&[])
    }

    /// Count and running hash committed by the state for archived receipts.
    pub fn receipt_log_commitment(&self) -> (u64, [u8; 32]) {
        (self.receipt_archive.count, self.receipt_archive.hash)
    }

    /// This node's copy of the archived receipts.
    pub fn receipt_log(&self) -> &ReceiptLog {
        &self.history.log
    }

    /// Drops this node's copy of the archived receipts. Consensus does not read
    /// them; the state keeps its commitment.
    pub fn prune_receipt_log(&mut self) {
        self.history.log =
            ReceiptLog::pruned_at(self.receipt_archive.count, self.receipt_archive.hash);
    }

    /// Joins the receipts before this node's log to it. `prefix` must be exactly
    /// the receipts `0..log.base_count()` and chain to the log's base hash.
    pub fn attach_receipt_log_prefix(&mut self, prefix: Vec<TradeReceipt>) -> Result<()> {
        let log = &self.history.log;
        if prefix.len() as u64 != log.base_count {
            return Err(EconomicError::InvalidInput);
        }
        let mut hash = [0_u8; 32];
        for receipt in &prefix {
            hash = receipt_log_step(&hash, receipt)?;
        }
        if hash != log.base_hash {
            return Err(EconomicError::InvalidInput);
        }
        let mut joined = ReceiptLog::default();
        joined.push_all(prefix);
        joined.base_count = 0;
        joined.base_hash = [0; 32];
        joined.len = log.base_count;
        joined.chunks.extend(log.chunks.iter().cloned());
        joined.len += log.len;
        self.history.log = joined;
        Ok(())
    }

    /// Carries this node's receipt log over to `restored`, a state decoded from
    /// an earlier snapshot of the same history. The log is cut to the restored
    /// archive head when it covers it; otherwise `restored` keeps its pruned log.
    pub(crate) fn inherit_receipt_log(&mut self, from: &Self) {
        let head = self.receipt_archive.count;
        let log = &from.history.log;
        if head < log.base_count || head > log.base_count + log.len {
            return;
        }
        if let Some(cut) = log.truncated(head - log.base_count) {
            let mut hash = cut.base_hash;
            for receipt in cut.iter() {
                match receipt_log_step(&hash, receipt) {
                    Ok(next) => hash = next,
                    Err(_) => return,
                }
            }
            if hash == self.receipt_archive.hash {
                self.history.log = cut;
            }
        }
    }

    /// Moves every receipt older than the window into the receipt log and folds
    /// it into the committed archive. Block finishers call it after the block's
    /// audits; the result is the same for every node. States that load from a
    /// snapshot taken before the archive existed hold more than the window and
    /// are brought to the same form by their first block, or by this call.
    pub fn migrate_receipt_history_unverified(&mut self) -> Result<()> {
        self.archive_receipts_unverified()
    }

    fn archives_receipts(&self) -> bool {
        self.config.marking.is_none()
    }

    fn archive_receipts_unverified(&mut self) -> Result<()> {
        if !self.archives_receipts() || self.receipts.len() <= self.history.window {
            return Ok(());
        }
        let moving = self.receipts.len() - self.history.window;
        let mut archive = self.receipt_archive.clone();
        let mut entries = Vec::with_capacity(moving);
        for receipt in &self.receipts[..moving] {
            if receipt.height < archive.last_height {
                return Err(EconomicError::Invariant);
            }
            archive.hash = receipt_log_step(&archive.hash, receipt)?;
            archive.collected_atoms = add(archive.collected_atoms, receipt.collected_loss_atoms)?;
            archive.paid_atoms = add(archive.paid_atoms, receipt.paid_claim_atoms)?;
            archive.unfunded_loss_atoms =
                add(archive.unfunded_loss_atoms, receipt.unfunded_loss_atoms)?;
            archive.last_height = receipt.height;
            entries.push((
                self.config.collateral,
                self.config.fee_route,
                receipt.retained_fee_atoms,
                receipt.height,
            ));
        }
        archive.count = archive
            .count
            .checked_add(moving as u64)
            .ok_or(EconomicError::Arithmetic)?;
        archive.insurance_atoms = add(
            archive.insurance_atoms,
            archive.fees.accrue_all_unchecked(entries)?,
        )?;
        // The state's journal holds the entries of these accruals first. They
        // are exactly the entries just replayed on the archive's book.
        let produced = archive
            .fees
            .drain_journal_prefix(archive.fees.journal().len())?;
        if self.fees.journal().get(..produced.len()) != Some(&produced[..]) {
            return Err(EconomicError::Invariant);
        }
        self.fees.drain_journal_prefix(produced.len())?;
        let moved: Vec<_> = self.receipts.drain(..moving).collect();
        self.history.log.push_all(moved);
        self.receipt_archive = archive;
        Ok(())
    }

    /// The checks every receipt must pass, archived or not.
    fn check_receipt(&self, receipt: &TradeReceipt, last: u64) -> Result<()> {
        if receipt.height < last
            || receipt.height == 0
            || self.finalized_height().is_none_or(|h| receipt.height > h)
            || !self.book.has_seen(receipt.buy_txid)
            || !self.book.has_seen(receipt.sell_txid)
            || receipt.buy_txid == receipt.sell_txid
            || receipt.quantity <= Decimal::zero()
            || receipt.price <= Decimal::zero()
        {
            return Err(EconomicError::Invariant);
        }
        bounded(&receipt.quantity)?;
        bounded(&receipt.price)?;
        Ok(())
    }

    /// Structure of the committed archive, independent of any log.
    fn verify_receipt_archive(&self) -> Result<()> {
        let archive = &self.receipt_archive;
        if (!self.archives_receipts() || archive.count == 0)
            && *archive != ReceiptArchive::default()
        {
            return Err(EconomicError::Invariant);
        }
        if archive.fees.queued().next().is_some() || !archive.fees.journal().is_empty() {
            return Err(EconomicError::Invariant);
        }
        archive.fees.expected_custody()?;
        Ok(())
    }

    /// Full audit of the receipt log against the committed archive. A log that
    /// starts at the first receipt also recomputes every committed total.
    fn verify_receipt_log(&self) -> Result<()> {
        let log = &self.history.log;
        let archive = &self.receipt_archive;
        if log.base_count.checked_add(log.len) != Some(archive.count) {
            return Err(EconomicError::Invariant);
        }
        let mut hash = log.base_hash;
        let mut last = 0;
        let mut collected = 0;
        let mut paid = 0;
        let mut bad_debt = 0;
        let mut insurance = 0;
        let mut fees = FeeBook::new();
        for receipt in log.iter() {
            self.check_receipt(receipt, last)?;
            hash = receipt_log_step(&hash, receipt)?;
            last = receipt.height;
            if log.is_complete() {
                collected = add(collected, receipt.collected_loss_atoms)?;
                paid = add(paid, receipt.paid_claim_atoms)?;
                bad_debt = add(bad_debt, receipt.unfunded_loss_atoms)?;
                insurance = add(
                    insurance,
                    fees.accrue_all_unchecked([(
                        self.config.collateral,
                        self.config.fee_route,
                        receipt.retained_fee_atoms,
                        receipt.height,
                    )])?,
                )?;
                let journal = fees.journal().len();
                fees.drain_journal_prefix(journal)?;
            }
        }
        if hash != archive.hash || (archive.count != 0 && last > archive.last_height) {
            return Err(EconomicError::Invariant);
        }
        if log.is_complete()
            && (collected != archive.collected_atoms
                || paid != archive.paid_atoms
                || bad_debt != archive.unfunded_loss_atoms
                || insurance != archive.insurance_atoms
                || fees != archive.fees)
        {
            return Err(EconomicError::Invariant);
        }
        Ok(())
    }

    /// Test hook: changes an archived receipt in this node's log.
    #[cfg(test)]
    pub(crate) fn corrupt_archived_receipt_for_test(&mut self) {
        let first = self
            .history
            .log
            .chunks
            .first_mut()
            .expect("an archived receipt");
        Arc::make_mut(first)[0].retained_fee_atoms += 1;
    }

    /// Test hook: smaller limits than the consensus constants.
    #[cfg(test)]
    pub(crate) fn set_history_limits_for_test(
        &mut self,
        window: usize,
        receipt_cap: usize,
        order_id_cap: usize,
    ) {
        self.history.window = window;
        self.history.receipt_cap = receipt_cap;
        self.book.set_id_limit_for_test(order_id_cap);
    }
}
