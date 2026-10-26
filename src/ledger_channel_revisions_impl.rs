// Included inside the actual Ledger owner; transcripts are never balances.
use crate::channel_revision::{
    self, AllocationRow, ChannelRevisionPolicy, RevisionContext, RevisionError, RevisionHead,
    RevisionStatus, SignedRevision,
};
pub type ExpectedChannelRevisionPolicies = BTreeMap<AccountId, ChannelRevisionPolicy>;
#[derive(Debug, Clone, PartialEq, Eq)]
struct AppliedChannelRevision {
    context: RevisionContext,
    signed: SignedRevision,
}
#[derive(Debug, Clone, PartialEq, Eq)]
struct EnrolledChannelRevision {
    policy: ChannelRevisionPolicy,
    enrollment_txid: TxId,
    context: RevisionContext,
    bootstrap_ledger: Vec<u8>,
    initial: Vec<AllocationRow>,
    head: RevisionHead,
    journal: Vec<AppliedChannelRevision>,
}
#[derive(Default)]
struct RevisionVerifyBudget {
    bytes: usize,
    records: usize,
    ordinary_records: usize,
    enrollments: usize,
}
impl RevisionVerifyBudget {
    fn ordinary(&mut self, n: usize) -> Result<(), SnapshotError> {
        self.ordinary_records = self
            .ordinary_records
            .checked_add(n)
            .ok_or_else(|| revision_snapshot_error(RevisionError::Capacity))?;
        if self.ordinary_records > MAX_SNAPSHOT_ENTRIES {
            return Err(revision_snapshot_error(RevisionError::Capacity));
        }
        Ok(())
    }
    fn enrollments(&mut self, n: usize) -> Result<(), SnapshotError> {
        self.enrollments = self
            .enrollments
            .checked_add(n)
            .ok_or_else(|| revision_snapshot_error(RevisionError::Capacity))?;
        if self.enrollments > channel_revision::MAX_CHANNELS {
            return Err(revision_snapshot_error(RevisionError::Capacity));
        }
        Ok(())
    }
    fn witness(&mut self, n: usize) -> Result<(), SnapshotError> {
        self.bytes = self
            .bytes
            .checked_add(n)
            .ok_or_else(|| revision_snapshot_error(RevisionError::Capacity))?;
        if self.bytes > channel_revision::MAX_HISTORY_BYTES {
            return Err(revision_snapshot_error(RevisionError::Capacity));
        }
        Ok(())
    }
    fn records(&mut self, n: usize) -> Result<(), SnapshotError> {
        self.records = self
            .records
            .checked_add(n)
            .ok_or_else(|| revision_snapshot_error(RevisionError::Capacity))?;
        if self.records > channel_revision::MAX_REVISIONS {
            return Err(revision_snapshot_error(RevisionError::Capacity));
        }
        Ok(())
    }
}
/// Fixed local metadata identity; this is never an authenticated L1 transaction id.
pub fn channel_enrollment_replay_id(
    policy: &ChannelRevisionPolicy,
    context: RevisionContext,
) -> Result<TxId, LedgerError> {
    let mut bytes = b"TradeLayer/ChannelEnrollment/v1/".to_vec();
    bytes.extend_from_slice(&channel_revision::policy_id(policy)?);
    revision_context(&mut bytes, context);
    Ok(TxId::from_bytes(crate::hash256::sha256(
        &crate::hash256::sha256(&bytes),
    )))
}
fn revision_application_snapshot(e: SnapshotError) -> LedgerError {
    match e {
        SnapshotError::InvalidLedger(e) => e,
        _ => RevisionError::Capacity.into(),
    }
}
impl From<RevisionError> for LedgerError {
    fn from(e: RevisionError) -> Self {
        Self::ChannelRevision(e)
    }
}
impl Ledger {
    pub fn has_signed_channel_revisions(&self) -> bool {
        !self.channel_revisions.is_empty()
    }
    pub fn channel_revision_head(&self, channel: &AccountId) -> Option<RevisionHead> {
        self.channel_revisions.get(channel).map(|e| e.head)
    }
    pub fn channel_revision_status(&self, channel: &AccountId, id: [u8; 32]) -> RevisionStatus {
        let Some(e) = self.channel_revisions.get(channel) else {
            return RevisionStatus::Unknown;
        };
        if e.head.revision_id == id {
            return RevisionStatus::Current;
        }
        if channel_revision::initial_head(&e.policy, &e.initial).is_ok_and(|h| h.revision_id == id)
            || e.journal
                .iter()
                .any(|r| channel_revision::revision_id(&r.signed.revision).is_ok_and(|h| h == id))
        {
            RevisionStatus::Superseded
        } else {
            RevisionStatus::Unknown
        }
    }
    /// An immutable current-state relation, not a persistent spend capability.
    pub fn require_current_signed_channel_head(
        &self,
        channel: &AccountId,
        expected: RevisionHead,
    ) -> Result<(), LedgerError> {
        match self.channel_revision_head(channel) {
            Some(h) if h == expected && h.sequence > 0 => Ok(()),
            _ => Err(RevisionError::WrongPreviousHead.into()),
        }
    }
    pub fn current_channel_allocation(
        &self,
        channel: &AccountId,
    ) -> Result<Vec<AllocationRow>, LedgerError> {
        let c = self
            .channels
            .get(channel)
            .ok_or_else(|| LedgerError::UnknownChannel(channel.clone()))?;
        let ids: BTreeSet<_> = c
            .escrow_a
            .keys()
            .chain(c.escrow_b.keys())
            .copied()
            .collect();
        if ids.len() > channel_revision::MAX_PROPERTIES {
            return Err(RevisionError::Capacity.into());
        }
        let mut rows = Vec::with_capacity(ids.len());
        for property in ids {
            let atoms_a = c.atoms(ChannelSide::A, property);
            let atoms_b = c.atoms(ChannelSide::B, property);
            atoms_a
                .checked_add(atoms_b)
                .ok_or(LedgerError::ArithmeticOverflow)?;
            rows.push(AllocationRow {
                property,
                atoms_a,
                atoms_b,
            });
        }
        Ok(rows)
    }
    /// Expected policies are independent deployment configuration. This method
    /// accepts a channel selector, never a descriptor supplied by the candidate.
    pub fn enroll_channel_revision_unverified(
        &mut self,
        txid: TxId,
        channel: &AccountId,
        expected: &ExpectedChannelRevisionPolicies,
        context: RevisionContext,
    ) -> Result<RevisionHead, LedgerError> {
        let policy = expected
            .get(channel)
            .filter(|p| &p.channel == channel)
            .ok_or(RevisionError::WrongPolicy)?;
        if txid != channel_enrollment_replay_id(policy, context)? {
            return Err(RevisionError::WrongBootstrap.into());
        }
        if self.processed.contains(&txid) {
            return Err(LedgerError::AlreadyProcessed(txid));
        }
        if self.channel_revisions.contains_key(channel) {
            return Err(RevisionError::WrongPolicy.into());
        }
        if self.channel_revisions.len() >= channel_revision::MAX_CHANNELS {
            return Err(RevisionError::Capacity.into());
        }
        channel_revision::policy_id(policy)?;
        if context.network != policy.network
            || context.parent != policy.bootstrap
            || context.parent.height.checked_add(1) != Some(context.height)
            || context.height > channel_revision::MAX_SAFE_HEIGHT
        {
            return Err(RevisionError::WrongContext.into());
        }
        self.require_revision_channel_quiescent(policy)?;
        let initial = self.current_channel_allocation(channel)?;
        let head = channel_revision::initial_head(policy, &initial)?;
        let bootstrap_ledger = self
            .snapshot_bytes()
            .map_err(revision_application_snapshot)?;
        if channel_revision::bootstrap_ledger_commitment(&bootstrap_ledger)
            != policy.bootstrap_ledger_commitment
        {
            return Err(RevisionError::WrongBootstrap.into());
        }
        let entry = EnrolledChannelRevision {
            policy: policy.clone(),
            enrollment_txid: txid,
            context,
            bootstrap_ledger,
            initial,
            head,
            journal: Vec::new(),
        };
        self.preflight_revision_metadata(Some((&entry, None)))?;
        let mut next = self.clone();
        next.processed.insert(txid);
        next.channel_revisions.insert(channel.clone(), entry);
        next.snapshot_bytes()
            .map_err(revision_application_snapshot)?;
        *self = next;
        Ok(head)
    }
    /// Reallocate the one actual channel pool; exact revision_id is its global
    /// replay identity. Parent/context are supplied by the enclosing owner.
    pub fn apply_channel_revision_unverified(
        &mut self,
        signed: &SignedRevision,
        context: RevisionContext,
    ) -> Result<RevisionHead, LedgerError> {
        let id = channel_revision::revision_id(&signed.revision)?;
        let txid = TxId::from_bytes(id);
        if self.processed.contains(&txid) {
            return Err(LedgerError::AlreadyProcessed(txid));
        }
        let (channel, e) = self
            .channel_revisions
            .iter()
            .find(|(_, e)| {
                channel_revision::policy_id(&e.policy).ok() == Some(signed.revision.policy_id)
            })
            .ok_or(RevisionError::WrongPolicy)?;
        let before = self.current_channel_allocation(channel)?;
        self.require_revision_channel_quiescent(&e.policy)?;
        if context.height
            < e.journal
                .last()
                .map_or(e.context.height, |r| r.context.height)
        {
            return Err(RevisionError::WrongContext.into());
        }
        self.preflight_revision_metadata(Some((e, Some(signed))))?;
        let head =
            channel_revision::validate_transition(&e.policy, &e.head, &before, context, signed)?;
        let channel = channel.clone();
        let mut next = self.clone();
        let book = next
            .channels
            .get_mut(&channel)
            .ok_or_else(|| LedgerError::UnknownChannel(channel.clone()))?;
        for row in &signed.revision.allocation {
            book.put(ChannelSide::A, row.property, row.atoms_a);
            book.put(ChannelSide::B, row.property, row.atoms_b);
        }
        let e = next
            .channel_revisions
            .get_mut(&channel)
            .expect("existing staged enrollment");
        e.head = head;
        e.journal.push(AppliedChannelRevision {
            context,
            signed: signed.clone(),
        });
        next.processed.insert(txid);
        next.snapshot_bytes()
            .map_err(revision_application_snapshot)?;
        *self = next;
        Ok(head)
    }
    fn require_revision_channel_quiescent(
        &self,
        p: &ChannelRevisionPolicy,
    ) -> Result<(), LedgerError> {
        if self.channel_owner(&p.channel, ChannelSide::A) != Some(&p.participant_a)
            || self.channel_owner(&p.channel, ChannelSide::B) != Some(&p.participant_b)
        {
            return Err(RevisionError::WrongCustody.into());
        }
        if self
            .pending_withdrawals
            .values()
            .any(|r| r.channel == p.channel)
            || self
                .pending_net_settlements
                .values()
                .any(|r| r.channel == p.channel)
        {
            return Err(LedgerError::SignedChannelFenced(p.channel.clone()));
        }
        Ok(())
    }
    fn require_unsigned_channel(&self, channel: &AccountId) -> Result<(), LedgerError> {
        if self.channel_revisions.contains_key(channel) {
            Err(LedgerError::SignedChannelFenced(channel.clone()))
        } else {
            Ok(())
        }
    }
    fn fence_legacy_channel_operation(&self, op: &Operation) -> Result<(), LedgerError> {
        match op {
            Operation::ChannelCommit { channel, .. }
            | Operation::ChannelWithdraw { channel, .. }
            | Operation::ChannelNetClaim { channel, .. }
            | Operation::ChannelKingSettle { channel, .. } => {
                self.require_unsigned_channel(channel)
            }
            Operation::ChannelTransfer {
                from_channel,
                to_channel,
                ..
            } => {
                self.require_unsigned_channel(from_channel)?;
                self.require_unsigned_channel(to_channel)
            }
            _ => Ok(()),
        }
    }
    fn preflight_revision_metadata(
        &self,
        extra: Option<(&EnrolledChannelRevision, Option<&SignedRevision>)>,
    ) -> Result<(), LedgerError> {
        let mut count = 0usize;
        let mut bytes = 0usize;
        for e in self.channel_revisions.values() {
            count = count
                .checked_add(e.journal.len())
                .ok_or(RevisionError::Capacity)?;
            bytes = bytes
                .checked_add(Self::revision_entry_bytes(e)?)
                .ok_or(RevisionError::Capacity)?;
        }
        if let Some((e, signed)) = extra {
            if let Some(s) = signed {
                count = count.checked_add(1).ok_or(RevisionError::Capacity)?;
                bytes = bytes
                    .checked_add(channel_revision::canonical_signed_revision_bytes(s)?.len() + 52)
                    .ok_or(RevisionError::Capacity)?;
            } else {
                bytes = bytes
                    .checked_add(Self::revision_entry_bytes(e)?)
                    .ok_or(RevisionError::Capacity)?;
            }
        }
        if count > channel_revision::MAX_REVISIONS || bytes > channel_revision::MAX_HISTORY_BYTES {
            return Err(RevisionError::Capacity.into());
        }
        Ok(())
    }
    fn revision_entry_bytes(e: &EnrolledChannelRevision) -> Result<usize, LedgerError> {
        let mut n = channel_revision::canonical_policy_bytes(&e.policy)?.len()
            + channel_revision::canonical_allocation_bytes(&e.initial)?.len()
            + e.bootstrap_ledger.len()
            + 4
            + 32
            + 48
            + 72
            + 16;
        for r in &e.journal {
            n = n
                .checked_add(
                    channel_revision::canonical_signed_revision_bytes(&r.signed)?.len() + 52,
                )
                .ok_or(RevisionError::Capacity)?;
        }
        Ok(n)
    }
    fn verify_channel_revision_state(&self) -> Result<(), LedgerError> {
        self.verify_channel_revision_state_for_owner(false)
    }
    pub(crate) fn snapshot_bytes_for_channel_owner(&self) -> Result<Vec<u8>, SnapshotError> {
        self.verify_base_conservation()
            .map_err(SnapshotError::InvalidLedger)?;
        self.verify_channel_revision_state_for_owner(true)
            .map_err(SnapshotError::InvalidLedger)?;
        self.snapshot_bytes_verified()
    }
    fn verify_channel_revision_state_for_owner(&self, enabled: bool) -> Result<(), LedgerError> {
        let expected = self
            .channel_revisions
            .iter()
            .map(|(c, e)| (c.clone(), e.policy.clone()))
            .collect();
        let mut budget = RevisionVerifyBudget::default();
        if enabled || !self.channel_revisions.is_empty() {
            budget
                .enrollments(self.channel_revisions.len())
                .map_err(revision_application_snapshot)?;
            budget
                .ordinary(
                    self.properties.len()
                        + self.available.len()
                        + self.processed.len()
                        + self.channels.len()
                        + self.pending_withdrawals.len()
                        + self.pending_net_settlements.len()
                        + self.swept_net_settlements.len()
                        + self.king_settlements.len(),
                )
                .map_err(revision_application_snapshot)?;
            for channel in self.channels.values() {
                budget
                    .ordinary(channel.escrow_a.len())
                    .map_err(revision_application_snapshot)?;
                budget
                    .ordinary(channel.escrow_b.len())
                    .map_err(revision_application_snapshot)?;
            }
            for receipt in self.king_settlements.values() {
                budget
                    .ordinary(receipt.claim_txids.len())
                    .map_err(revision_application_snapshot)?;
            }
        }
        for e in self.channel_revisions.values() {
            budget
                .records(e.journal.len())
                .map_err(revision_application_snapshot)?;
        }
        for e in self.channel_revisions.values() {
            budget
                .witness(e.bootstrap_ledger.len())
                .map_err(revision_application_snapshot)?;
            let before = Self::decode_snapshot_channel_scoped(
                &e.bootstrap_ledger,
                Some(&expected),
                1,
                &mut budget,
            )
            .map_err(revision_application_snapshot)?;
            self.verify_channel_enrollment_witness(e, &before)?;
        }
        self.verify_channel_revision_state_without_witness()
    }
    fn verify_channel_enrollment_witness(
        &self,
        e: &EnrolledChannelRevision,
        before: &Self,
    ) -> Result<(), LedgerError> {
        if channel_revision::bootstrap_ledger_commitment(&e.bootstrap_ledger)
            != e.policy.bootstrap_ledger_commitment
            || before.processed.contains(&e.enrollment_txid)
            || !before.processed.is_subset(&self.processed)
            || before.channel_revisions.contains_key(&e.policy.channel)
            || before.channel_revisions.len() >= self.channel_revisions.len()
            || before.current_channel_allocation(&e.policy.channel)? != e.initial
        {
            return Err(RevisionError::WrongBootstrap.into());
        }
        before.require_revision_channel_quiescent(&e.policy)?;
        before.require_channel_revision_prefix(self)?;
        Ok(())
    }
    pub(crate) fn require_channel_revision_prefix(&self, later: &Self) -> Result<(), LedgerError> {
        if !self.processed.is_subset(&later.processed) {
            return Err(RevisionError::WrongPreviousHead.into());
        }
        for (channel, old) in &self.channel_revisions {
            let new = later
                .channel_revisions
                .get(channel)
                .ok_or(RevisionError::WrongPreviousHead)?;
            if old.policy != new.policy
                || old.enrollment_txid != new.enrollment_txid
                || old.context != new.context
                || old.bootstrap_ledger != new.bootstrap_ledger
                || old.initial != new.initial
                || !new.journal.starts_with(&old.journal)
            {
                return Err(RevisionError::WrongPreviousHead.into());
            }
        }
        Ok(())
    }
    pub(crate) fn require_channel_revision_transition(
        &self,
        later: &Self,
        parent: crate::chain::Tip,
        height: u64,
    ) -> Result<(), LedgerError> {
        self.require_channel_revision_prefix(later)?;
        if parent.height.checked_add(1) != Some(height) {
            return Err(RevisionError::WrongContext.into());
        }
        for (channel, new) in &later.channel_revisions {
            let old_len = if let Some(old) = self.channel_revisions.get(channel) {
                old.journal.len()
            } else {
                if new.context.height != height
                    || new.context.parent != parent
                    || self.processed.contains(&new.enrollment_txid)
                {
                    return Err(RevisionError::WrongContext.into());
                }
                0
            };
            for r in &new.journal[old_len..] {
                let id = TxId::from_bytes(channel_revision::revision_id(&r.signed.revision)?);
                if r.context.height != height
                    || r.context.parent != parent
                    || self.processed.contains(&id)
                {
                    return Err(RevisionError::WrongContext.into());
                }
            }
        }
        later.validate_channel_revision_block_parent(parent, height)
    }
    fn verify_channel_revision_state_without_witness(&self) -> Result<(), LedgerError> {
        if self.channel_revisions.len() > channel_revision::MAX_CHANNELS {
            return Err(RevisionError::Capacity.into());
        }
        self.preflight_revision_metadata(None)?;
        let mut ids = BTreeSet::new();
        for (channel, e) in &self.channel_revisions {
            if channel != &e.policy.channel
                || e.context.network != e.policy.network
                || e.context.parent != e.policy.bootstrap
                || e.context.parent.height.checked_add(1) != Some(e.context.height)
                || e.context.height > channel_revision::MAX_SAFE_HEIGHT
                || e.enrollment_txid != channel_enrollment_replay_id(&e.policy, e.context)?
                || !self.processed.contains(&e.enrollment_txid)
                || !ids.insert(e.enrollment_txid)
            {
                return Err(RevisionError::WrongBootstrap.into());
            }
            self.require_revision_channel_quiescent(&e.policy)?;
            for row in &e.initial {
                if !self.properties.contains_key(&row.property) {
                    return Err(LedgerError::UnknownProperty(row.property));
                }
            }
            let mut head = channel_revision::initial_head(&e.policy, &e.initial)?;
            let mut before = e.initial.clone();
            let mut height = e.context.height;
            for r in &e.journal {
                if r.context.height < height {
                    return Err(RevisionError::WrongContext.into());
                }
                head = channel_revision::validate_transition(
                    &e.policy, &head, &before, r.context, &r.signed,
                )?;
                let id = TxId::from_bytes(head.revision_id);
                if !self.processed.contains(&id) || !ids.insert(id) {
                    return Err(RevisionError::WrongPreviousHead.into());
                }
                before = r.signed.revision.allocation.clone();
                height = r.context.height;
            }
            if head != e.head || before != self.current_channel_allocation(channel)? {
                return Err(RevisionError::WrongCustody.into());
            }
        }
        Ok(())
    }
    pub(crate) fn validate_channel_revision_block_parent(
        &self,
        parent: crate::chain::Tip,
        height: u64,
    ) -> Result<(), LedgerError> {
        for e in self.channel_revisions.values() {
            if e.context.height == height && e.context.parent != parent {
                return Err(RevisionError::WrongContext.into());
            }
            if e.journal
                .iter()
                .any(|r| r.context.height == height && r.context.parent != parent)
            {
                return Err(RevisionError::WrongContext.into());
            }
        }
        Ok(())
    }
    pub(crate) fn validate_channel_revision_owner_context(
        &self,
        network: crate::network_profile::TradeLayerMainnet,
        height: u64,
        expected: &ExpectedChannelRevisionPolicies,
    ) -> Result<(), LedgerError> {
        for (channel, e) in &self.channel_revisions {
            if e.policy.network != network
                || expected.get(channel) != Some(&e.policy)
                || e.context.height > height
                || e.journal.iter().any(|r| r.context.height > height)
            {
                return Err(RevisionError::WrongContext.into());
            }
        }
        Ok(())
    }
    fn append_channel_revision_snapshot(&self, out: &mut Vec<u8>) -> Result<(), SnapshotError> {
        out.extend_from_slice(&(self.channel_revisions.len() as u32).to_be_bytes());
        for e in self.channel_revisions.values() {
            revision_blob(
                out,
                &channel_revision::canonical_policy_bytes(&e.policy)
                    .map_err(revision_snapshot_error)?,
            )?;
            out.extend_from_slice(&e.enrollment_txid.as_bytes());
            revision_context(out, e.context);
            revision_blob(out, &e.bootstrap_ledger)?;
            revision_blob(
                out,
                &channel_revision::canonical_allocation_bytes(&e.initial)
                    .map_err(revision_snapshot_error)?,
            )?;
            out.extend_from_slice(&e.head.sequence.to_be_bytes());
            out.extend_from_slice(&e.head.revision_id);
            out.extend_from_slice(&e.head.allocation_digest);
            out.extend_from_slice(&(e.journal.len() as u32).to_be_bytes());
            for r in &e.journal {
                revision_context(out, r.context);
                revision_blob(
                    out,
                    &channel_revision::canonical_signed_revision_bytes(&r.signed)
                        .map_err(revision_snapshot_error)?,
                )?;
            }
        }
        Ok(())
    }
    fn read_channel_revision_snapshot(
        &mut self,
        r: &mut crate::codec::Cursor<'_>,
        expected: &ExpectedChannelRevisionPolicies,
        depth: usize,
        budget: &mut RevisionVerifyBudget,
    ) -> Result<(), SnapshotError> {
        let n = r.u32().map_err(snapshot_codec_error)? as usize;
        if n == 0 || n > channel_revision::MAX_CHANNELS {
            return Err(revision_snapshot_error(RevisionError::Capacity));
        }
        budget.enrollments(n)?;
        let mut witnesses = Vec::new();
        let mut last = None;
        let mut count = 0usize;
        let mut bytes = 0usize;
        for _ in 0..n {
            let p = channel_revision::parse_policy(revision_read_blob(
                r,
                channel_revision::MAX_POLICY_BYTES,
                &mut bytes,
            )?)
            .map_err(revision_snapshot_error)?;
            if last.as_ref().is_some_and(|a| a >= &p.channel)
                || expected.get(&p.channel) != Some(&p)
            {
                return Err(revision_snapshot_error(RevisionError::WrongPolicy));
            }
            last = Some(p.channel.clone());
            let enrollment_txid = TxId::from_bytes(r.fixed().map_err(snapshot_codec_error)?);
            let context = read_revision_context(r, p.network)?;
            let witness = revision_read_blob(r, channel_revision::MAX_HISTORY_BYTES, &mut bytes)?;
            if enrollment_txid
                != channel_enrollment_replay_id(&p, context)
                    .map_err(SnapshotError::InvalidLedger)?
                || channel_revision::bootstrap_ledger_commitment(witness)
                    != p.bootstrap_ledger_commitment
            {
                return Err(revision_snapshot_error(RevisionError::WrongBootstrap));
            }
            budget.witness(witness.len())?;
            // Charge recursive bytes/records before copying a witness into retained metadata.
            let before =
                Self::decode_snapshot_channel_scoped(witness, Some(expected), depth + 1, budget)?;
            let bootstrap_ledger = witness.to_vec();
            let initial = channel_revision::parse_allocation(revision_read_blob(
                r,
                2 + 36 * channel_revision::MAX_PROPERTIES,
                &mut bytes,
            )?)
            .map_err(revision_snapshot_error)?;
            let head = RevisionHead {
                sequence: r.u64().map_err(snapshot_codec_error)?,
                revision_id: r.fixed().map_err(snapshot_codec_error)?,
                allocation_digest: r.fixed().map_err(snapshot_codec_error)?,
            };
            let records = r.u32().map_err(snapshot_codec_error)? as usize;
            budget.records(records)?;
            count = count
                .checked_add(records)
                .ok_or_else(|| revision_snapshot_error(RevisionError::Capacity))?;
            if count > channel_revision::MAX_REVISIONS {
                return Err(revision_snapshot_error(RevisionError::Capacity));
            }
            let mut journal = Vec::with_capacity(records);
            for _ in 0..records {
                let context = read_revision_context(r, p.network)?;
                let signed = channel_revision::parse_signed_revision(revision_read_blob(
                    r,
                    channel_revision::MAX_SIGNED_REVISION_BYTES,
                    &mut bytes,
                )?)
                .map_err(revision_snapshot_error)?;
                journal.push(AppliedChannelRevision { context, signed });
            }
            let entry = EnrolledChannelRevision {
                policy: p,
                enrollment_txid,
                context,
                bootstrap_ledger,
                initial,
                head,
                journal,
            };
            // All current entries need to exist for prefix comparison; retain decoded
            // witness for deferred comparison below without another parse.
            witnesses.push((entry.policy.channel.clone(), before));
            self.channel_revisions
                .insert(entry.policy.channel.clone(), entry);
        }
        for (channel, before) in witnesses {
            self.verify_channel_enrollment_witness(&self.channel_revisions[&channel], &before)
                .map_err(SnapshotError::InvalidLedger)?;
        }
        Ok(())
    }
}
fn revision_snapshot_error(e: RevisionError) -> SnapshotError {
    SnapshotError::InvalidLedger(LedgerError::ChannelRevision(e))
}
fn revision_blob(out: &mut Vec<u8>, bytes: &[u8]) -> Result<(), SnapshotError> {
    let n =
        u32::try_from(bytes.len()).map_err(|_| revision_snapshot_error(RevisionError::Capacity))?;
    out.extend_from_slice(&n.to_be_bytes());
    out.extend_from_slice(bytes);
    Ok(())
}
fn revision_context(out: &mut Vec<u8>, c: RevisionContext) {
    out.extend_from_slice(&c.parent.height.to_be_bytes());
    out.extend_from_slice(&c.parent.id.0);
    out.extend_from_slice(&c.height.to_be_bytes());
}
fn read_revision_context(
    r: &mut crate::codec::Cursor<'_>,
    network: crate::network_profile::TradeLayerMainnet,
) -> Result<RevisionContext, SnapshotError> {
    Ok(RevisionContext {
        network,
        parent: crate::chain::Tip {
            height: r.u64().map_err(snapshot_codec_error)?,
            id: crate::chain::BlockId(r.fixed().map_err(snapshot_codec_error)?),
        },
        height: r.u64().map_err(snapshot_codec_error)?,
    })
}
fn revision_read_blob<'a>(
    r: &mut crate::codec::Cursor<'a>,
    max: usize,
    total: &mut usize,
) -> Result<&'a [u8], SnapshotError> {
    let n = r.u32().map_err(snapshot_codec_error)? as usize;
    *total = total
        .checked_add(n)
        .ok_or_else(|| revision_snapshot_error(RevisionError::Capacity))?;
    if n > max || *total > channel_revision::MAX_HISTORY_BYTES {
        return Err(revision_snapshot_error(RevisionError::Capacity));
    }
    r.take(n).map_err(snapshot_codec_error)
}

fn revision_snapshot_count(
    reader: &mut crate::codec::Cursor<'_>,
    budget: Option<&mut RevisionVerifyBudget>,
) -> Result<usize, SnapshotError> {
    let n = snapshot_count(reader)?;
    if let Some(b) = budget {
        b.ordinary(n)?;
    }
    Ok(n)
}
#[cfg(test)]
mod signed_channel_owner_private_tests {
    use super::*;
    fn hex(s: &str) -> Vec<u8> {
        s.as_bytes()
            .chunks_exact(2)
            .map(|b| u8::from_str_radix(core::str::from_utf8(b).unwrap(), 16).unwrap())
            .collect()
    }
    fn base() -> (Ledger, ChannelRevisionPolicy, RevisionContext) {
        let line = include_str!("../fixtures/channel_owner_vectors.tsv")
            .lines()
            .find(|l| l.starts_with("1-first\t"))
            .unwrap();
        let policy =
            channel_revision::parse_policy(&hex(line.split('\t').nth(1).unwrap())).unwrap();
        let context = RevisionContext {
            network: policy.network,
            parent: policy.bootstrap,
            height: 3,
        };
        let l = Ledger::from_snapshot_bytes(&hex(include_str!(
            "../fixtures/channel_owner_bootstrap.hex"
        )
        .trim()))
        .unwrap();
        (l, policy, context)
    }
    fn enrolled() -> (Ledger, ExpectedChannelRevisionPolicies) {
        let (mut l, p, c) = base();
        let expected = ExpectedChannelRevisionPolicies::from([(p.channel.clone(), p.clone())]);
        l.enroll_channel_revision_unverified(
            channel_enrollment_replay_id(&p, c).unwrap(),
            &p.channel,
            &expected,
            c,
        )
        .unwrap();
        (l, expected)
    }
    #[test]
    fn forged_pending_maturity_cannot_bypass_signed_fixed_pool() {
        let (mut l, _) = enrolled();
        let channel = AccountId::try_from("channel").unwrap();
        let property = PropertyId::try_from(7).unwrap();
        l.channels
            .get_mut(&channel)
            .unwrap()
            .put(ChannelSide::A, property, 599);
        l.pending_withdrawals.insert(
            TxId::from_bytes([91; 32]),
            PendingWithdrawal {
                channel: channel.clone(),
                side: ChannelSide::A,
                owner: AccountId::try_from("alice").unwrap(),
                id: property,
                atoms: 1,
                unlock_height: 4,
            },
        );
        let before = l.clone();
        assert!(
            matches!(l.settle_due_withdrawals_unverified(4),Err(LedgerError::SignedChannelFenced(c))if c==channel)
        );
        assert_eq!(l, before);
        assert!(l.verify_conservation().is_err());
        assert!(l.snapshot_bytes().is_err());
    }
    #[test]
    fn recursive_witness_budget_is_shared_before_child_copy() {
        let (l, expected) = enrolled();
        let bytes = l.snapshot_bytes().unwrap();
        for budget in [
            RevisionVerifyBudget {
                bytes: channel_revision::MAX_HISTORY_BYTES,
                ..Default::default()
            },
            RevisionVerifyBudget {
                enrollments: channel_revision::MAX_CHANNELS,
                ..Default::default()
            },
            RevisionVerifyBudget {
                ordinary_records: MAX_SNAPSHOT_ENTRIES,
                ..Default::default()
            },
        ] {
            let mut budget = budget;
            assert!(Ledger::decode_snapshot_channel_scoped(
                &bytes,
                Some(&expected),
                0,
                &mut budget
            )
            .is_err());
        }
        let mut budget = RevisionVerifyBudget::default();
        assert!(Ledger::decode_snapshot_channel_scoped(
            &bytes,
            Some(&expected),
            channel_revision::MAX_CHANNELS + 1,
            &mut budget
        )
        .is_err());
    }
    #[test]
    fn actual_two_enrollments_replay_nested_witness_and_aggregate_record_budget() {
        let (mut l, mut first, context) = base();
        let other = AccountId::try_from("other").unwrap();
        let p = PropertyId::try_from(7).unwrap();
        for (n, side) in [(91, ChannelSide::A), (92, ChannelSide::B)] {
            l.apply(
                TxId::from_bytes([n; 32]),
                Operation::ChannelTransfer {
                    id: p,
                    from_channel: first.channel.clone(),
                    from_side: side,
                    to_channel: other.clone(),
                    to_side: side,
                    amount_atoms: 100,
                },
            )
            .unwrap();
        }
        first.bootstrap_ledger_commitment =
            channel_revision::bootstrap_ledger_commitment(&l.snapshot_bytes().unwrap());
        first.bootstrap_allocation_commitment = channel_revision::bootstrap_allocation_commitment(
            &l.current_channel_allocation(&first.channel).unwrap(),
        )
        .unwrap();
        let mut expected =
            ExpectedChannelRevisionPolicies::from([(first.channel.clone(), first.clone())]);
        l.enroll_channel_revision_unverified(
            channel_enrollment_replay_id(&first, context).unwrap(),
            &first.channel,
            &expected,
            context,
        )
        .unwrap();
        let mut second = first;
        second.channel = other.clone();
        second.bootstrap_ledger_commitment =
            channel_revision::bootstrap_ledger_commitment(&l.snapshot_bytes().unwrap());
        second.bootstrap_allocation_commitment = channel_revision::bootstrap_allocation_commitment(
            &l.current_channel_allocation(&other).unwrap(),
        )
        .unwrap();
        expected.insert(other.clone(), second.clone());
        l.enroll_channel_revision_unverified(
            channel_enrollment_replay_id(&second, context).unwrap(),
            &other,
            &expected,
            context,
        )
        .unwrap();
        let bytes = l.snapshot_bytes().unwrap();
        assert_eq!(
            l,
            Ledger::from_snapshot_bytes_with_channel_policies(&bytes, &expected).unwrap()
        );
        let mut budget = RevisionVerifyBudget::default();
        let restored =
            Ledger::decode_snapshot_channel_scoped(&bytes, Some(&expected), 0, &mut budget)
                .unwrap();
        assert_eq!(restored, l);
        assert_eq!(budget.enrollments, 3);
        assert!(
            budget.bytes
                > l.channel_revisions
                    .values()
                    .map(|e| e.bootstrap_ledger.len())
                    .sum::<usize>()
        );
        let mut short = RevisionVerifyBudget {
            enrollments: channel_revision::MAX_CHANNELS - 2,
            ..Default::default()
        };
        assert!(
            Ledger::decode_snapshot_channel_scoped(&bytes, Some(&expected), 0, &mut short).is_err()
        );
    }
    #[test]
    fn live_and_restore_count_escrow_rows_identically_at_exact_record_limit() {
        let (mut l, expected) = enrolled();
        let alice = AccountId::try_from("alice").unwrap();
        let property = PropertyId::try_from(900).unwrap();
        l.properties.insert(
            property,
            Property {
                issuer: alice.clone(),
                kind: PropertyKind::Managed,
                total_supply_atoms: 1,
            },
        );
        l.available.insert((property, alice.clone()), 1);
        let mut budget = RevisionVerifyBudget::default();
        Ledger::decode_snapshot_channel_scoped(
            &l.snapshot_bytes().unwrap(),
            Some(&expected),
            0,
            &mut budget,
        )
        .unwrap();
        let fill = MAX_SNAPSHOT_ENTRIES - budget.ordinary_records;
        for i in 0..fill {
            l.properties.insert(
                PropertyId::try_from(1000 + i as u32).unwrap(),
                Property {
                    issuer: alice.clone(),
                    kind: PropertyKind::Managed,
                    total_supply_atoms: 0,
                },
            );
        }
        let bytes = l.snapshot_bytes().unwrap();
        assert_eq!(
            l,
            Ledger::from_snapshot_bytes_with_channel_policies(&bytes, &expected).unwrap()
        );
        let before = l.clone();
        assert!(matches!(
            l.apply(
                TxId::from_bytes([240; 32]),
                Operation::Transfer {
                    id: property,
                    from: alice.clone(),
                    to: alice,
                    amount_atoms: 1
                }
            ),
            Err(LedgerError::ChannelRevision(RevisionError::Capacity))
        ));
        assert_eq!(l, before);
    }
    #[test]
    fn enabled_before_enrollment_has_same_aggregate_domain_as_aware_decoder() {
        let (mut l, policy, _) = base();
        let alice = AccountId::try_from("alice").unwrap();
        let expected = ExpectedChannelRevisionPolicies::from([(policy.channel.clone(), policy)]);
        let mut budget = RevisionVerifyBudget::default();
        Ledger::decode_snapshot_channel_scoped(
            &l.snapshot_bytes().unwrap(),
            Some(&expected),
            0,
            &mut budget,
        )
        .unwrap();
        for i in 0..=MAX_SNAPSHOT_ENTRIES - budget.ordinary_records {
            l.properties.insert(
                PropertyId::try_from(1000 + i as u32).unwrap(),
                Property {
                    issuer: alice.clone(),
                    kind: PropertyKind::Managed,
                    total_supply_atoms: 0,
                },
            );
        }
        // Legacy no-policy bytes keep their existing per-component record domain.
        let bytes = l.snapshot_bytes().unwrap();
        assert!(Ledger::from_snapshot_bytes(&bytes).is_ok());
        assert!(l.snapshot_bytes_for_channel_owner().is_err());
        assert!(Ledger::from_snapshot_bytes_with_channel_policies(&bytes, &expected).is_err());
    }
}
