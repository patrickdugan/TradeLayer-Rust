// TLSW5 is archive-backed. No supplied current financial state is decoded.
const DURABLE_WITNESS_MAGIC: &[u8; 5] = b"TLSW\x05";
const DURABLE_WITNESS_DOMAIN: &[u8] = b"TradeLayer/SourceDurableTransitionWitness/v5/";
pub const SOURCE_DURABLE_TRANSITION_SCHEMA: &str = "tl-literal-source-durable-command-slice-v5";
pub const MAX_SOURCE_DURABLE_TRANSITION_BYTES: usize = MAX_CHECKPOINT_BYTES;

#[derive(Debug)]
pub enum SourceDurableTransitionError {
    Capacity,
    InvalidEnvelope,
    WrongContext,
    WrongPreHistory,
    WrongPostHistory,
    WrongPreContext,
    WrongPostContext,
    WrongCommands,
    WrongReceipts,
    NotAncestor,
    UnownedHistory,
    UnavailableExecution,
    Archive(SourceDurableError),
    Source(SourceFullDispatchError),
}
impl std::fmt::Display for SourceDurableTransitionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for SourceDurableTransitionError {}
impl From<StorageError> for SourceDurableTransitionError {
    fn from(value: StorageError) -> Self {
        Self::Archive(SourceDurableError::Storage(value))
    }
}
impl From<SourceDurableError> for SourceDurableTransitionError {
    fn from(v: SourceDurableError) -> Self {
        Self::Archive(v)
    }
}
impl From<SourceContractReplayError> for SourceDurableTransitionError {
    fn from(v: SourceContractReplayError) -> Self {
        Self::Archive(SourceDurableError::Replay(v))
    }
}
impl From<SourceFullDispatchError> for SourceDurableTransitionError {
    fn from(v: SourceFullDispatchError) -> Self {
        Self::Source(v)
    }
}
type DurableWitnessResult<T> = std::result::Result<T, SourceDurableTransitionError>;
use SourceDurableTransitionError as DWError;

/// The current archive publication is a separately retained consistency and
/// freshness expectation at verification time. It is NOT input/chain authority.
/// Prehistory must be its ancestor; command/receipt/end pins are independent.
pub struct SourceDurableTransitionExpectation<'a> {
    pub context: &'a SourceDurableContext,
    pub archive_publication: SourceDurablePublication,
    pub descriptor: &'a SourceFullDispatchDescriptor,
    pub scope: SourceFullDispatchScope,
    pub pre_history: SourceDurableHistoryHead,
    pub pre_context_commitment: [u8; 32],
    pub commands_commitment: [u8; 32],
    pub receipts_commitment: [u8; 32],
    pub post_history: SourceDurableHistoryHead,
    pub post_context_commitment: [u8; 32],
}
#[derive(Debug, Clone)]
pub struct SourceDurableTransitionWitness {
    context: Vec<u8>,
    descriptor: SourceFullDispatchDescriptor,
    scope: SourceFullDispatchScope,
    pre_history: SourceDurableHistoryHead,
    pre_context: V,
    pre_context_hash: [u8; 32],
    segment: Vec<u8>,
    commands: [u8; 32],
    receipts: [u8; 32],
    post_history: SourceDurableHistoryHead,
    post_context: V,
    post_context_hash: [u8; 32],
}
#[derive(Debug)]
pub struct ConditionalSourceDurableTransition {
    scope: SourceFullDispatchScope,
    runtime: SourceContractRuntime,
    receipts: Vec<SourceDurableReceipt>,
    context: SourceDurableContext,
    archive_publication: SourceDurablePublication,
    pre_history: SourceDurableHistoryHead,
    post_history: SourceDurableHistoryHead,
}
impl ConditionalSourceDurableTransition {
    pub const fn claim(&self) -> &'static str {
        match self.scope {
            SourceFullDispatchScope::SourceCommandSlice => {
                "conditional-archive-backed-source-command-slice-only-v5"
            }
            SourceFullDispatchScope::MainCallableSlice => {
                "conditional-archive-backed-source-main-callable-slice-only-v5"
            }
        }
    }
    pub fn runtime(&self) -> &SourceContractRuntime {
        &self.runtime
    }
    pub fn receipts(&self) -> &[SourceDurableReceipt] {
        &self.receipts
    }
    pub fn claimed_context(&self) -> &SourceDurableContext {
        &self.context
    }
    pub const fn verified_archive_publication(&self) -> SourceDurablePublication {
        self.archive_publication
    }
    pub const fn pre_history(&self) -> SourceDurableHistoryHead {
        self.pre_history
    }
    pub const fn post_history(&self) -> SourceDurableHistoryHead {
        self.post_history
    }
    pub const fn require_authenticated_input_and_state(
        &self,
    ) -> std::result::Result<(), SourceRuleError> {
        Err(SourceRuleError::UnauthenticatedInputAndState)
    }
}
fn dw_context(context: &SourceDurableContext) -> DurableWitnessResult<(Vec<u8>, [u8; 32])> {
    full_policy(context.policy)?;
    let bytes = durable_context(context)?;
    let hash = sha256(&bytes);
    Ok((bytes, hash))
}
fn dw_count(pre: SourceDurableHistoryHead, count: usize) -> DurableWitnessResult<()> {
    if count == 0
        || count > MAX_DURABLE_SEGMENT_COMMANDS
        || pre.segments >= MAX_DURABLE_REPLAY_SEGMENTS as u64
        || pre.commands.checked_add(count as u64).is_none()
    {
        return Err(DWError::Capacity);
    }
    Ok(())
}
pub fn source_durable_transition_context_commitment(
    archive: &SourceDurableReplay,
    scope: SourceFullDispatchScope,
) -> DurableWitnessResult<[u8; 32]> {
    archive.health()?;
    Ok(source_full_dispatch_context_commitment(
        &archive.replay,
        scope,
    )?)
}
pub fn source_durable_transition_commands_commitment(
    context: &SourceDurableContext,
    scope: SourceFullDispatchScope,
    pre: SourceDurableHistoryHead,
    label: &str,
    commands: &[SourceContractCommand],
) -> DurableWitnessResult<[u8; 32]> {
    dw_count(pre, commands.len())?;
    if label.len() > 256 {
        return Err(DWError::Capacity);
    }
    let (_, hash) = dw_context(context)?;
    let mut bytes = b"TradeLayer/SourceDurableTransitionCommands/v5/".to_vec();
    bytes.extend(hash);
    bytes.push(scope.tag());
    durable_put_head(&mut bytes, pre);
    put_frame(&mut bytes, &diagnostic(&s(label))?)?;
    bytes.extend(source_full_dispatch_commands_commitment(
        context.policy,
        scope,
        commands,
    )?);
    Ok(sha256(&bytes))
}
pub fn source_durable_transition_receipts_commitment(
    context: &SourceDurableContext,
    scope: SourceFullDispatchScope,
    pre: SourceDurableHistoryHead,
    receipts: &[SourceDurableReceipt],
) -> DurableWitnessResult<[u8; 32]> {
    dw_count(pre, receipts.len())?;
    let mut source = Vec::with_capacity(receipts.len());
    let mut previous = pre.source_receipt_head;
    for (i, r) in receipts.iter().enumerate() {
        if r.absolute_sequence != pre.commands + i as u64
            || r.source.sequence != i as u64
            || r.source.previous_head != previous
        {
            return Err(DWError::WrongReceipts);
        }
        if matches!(
            r.source.outcome,
            SourceContractCommandOutcome::LocalError(_)
        ) {
            return Err(DWError::UnavailableExecution);
        }
        previous = r.source.head;
        source.push(r.source.clone());
    }
    let (_, hash) = dw_context(context)?;
    let mut bytes = b"TradeLayer/SourceDurableTransitionReceipts/v5/".to_vec();
    bytes.extend(hash);
    bytes.push(scope.tag());
    durable_put_head(&mut bytes, pre);
    bytes.extend(source_full_dispatch_receipts_commitment(
        context.policy,
        scope,
        &source,
    )?);
    Ok(sha256(&bytes))
}
fn dw_receipts(segment: &DurableSegment) -> Vec<SourceDurableReceipt> {
    segment
        .entries
        .iter()
        .enumerate()
        .map(|(i, e)| SourceDurableReceipt {
            absolute_sequence: segment.previous.commands + i as u64,
            source: e.receipt.clone(),
        })
        .collect()
}
fn dw_expected(e: &SourceDurableTransitionExpectation<'_>) -> DurableWitnessResult<()> {
    full_policy(e.context.policy)?;
    full_descriptor(e.descriptor)?;
    if e.pre_history.segments > MAX_DURABLE_REPLAY_SEGMENTS as u64
        || e.post_history.segments > MAX_DURABLE_REPLAY_SEGMENTS as u64
        || e.archive_publication.history.segments > MAX_DURABLE_REPLAY_SEGMENTS as u64
    {
        return Err(DWError::Capacity);
    }
    Ok(())
}
fn dw_prefix(
    root: &Path,
    e: &SourceDurableTransitionExpectation<'_>,
    hash: [u8; 32],
) -> DurableWitnessResult<SourceContractReplay> {
    let archive = SourceDurableReplay::open(
        root,
        e.context.clone(),
        e.archive_publication,
        Recovery::RejectIncompleteTail,
    )?;
    dw_prefix_archive(&archive, e, hash)
}
fn dw_prefix_archive(
    archive: &SourceDurableReplay,
    e: &SourceDurableTransitionExpectation<'_>,
    hash: [u8; 32],
) -> DurableWitnessResult<SourceContractReplay> {
    archive.health()?;
    let bytes = durable_context(e.context)?;
    if archive.context != *e.context
        || archive.context_bytes != bytes
        || archive.context_hash != hash
        || archive.journal.get(DURABLE_CONFIG)? != Some(bytes.as_slice())
        || archive
            .journal
            .entries()?
            .any(|(k, _)| k != DURABLE_CONFIG && k != DURABLE_PUBLICATION)
    {
        return Err(DWError::WrongContext);
    }
    let publication = durable_parse_publication(
        archive
            .journal
            .get(DURABLE_PUBLICATION)?
            .ok_or(SourceDurableError::InvalidArchive)?,
    )?;
    if publication != e.archive_publication
        || archive.publication != publication
        || archive.journal.sequence()?
            != publication
                .revision
                .checked_add(1)
                .ok_or(DWError::Capacity)?
    {
        return Err(SourceDurableError::WrongHead.into());
    }
    let mut cursor = publication.history;
    while cursor.segments > e.pre_history.segments {
        let segment = durable_read_segment(&archive.root, hash, cursor.last_segment)?;
        if durable_after(hash, &segment, cursor.last_segment)? != cursor {
            return Err(DWError::NotAncestor);
        }
        cursor = segment.previous;
    }
    if cursor != e.pre_history {
        return Err(DWError::NotAncestor);
    }
    let fresh = SourceContractReplay::with_policy(e.context.policy)?;
    let mut captured = if e.pre_history.commands == 0 {
        Some(fresh)
    } else {
        None
    };
    let mut unowned = false;
    let current = durable_restore_observed(
        &archive.root,
        e.context,
        hash,
        publication.history,
        |absolute, replay, _, receipt| {
            if absolute < e.pre_history.commands
                && matches!(receipt.outcome, SourceContractCommandOutcome::LocalError(_))
            {
                unowned = true;
                return Err(SourceDurableError::InvalidArchive);
            }
            if absolute.checked_add(1) == Some(e.pre_history.commands) {
                captured = Some(durable_rotate(replay.clone()));
            }
            Ok(())
        },
    );
    if unowned {
        return Err(DWError::UnownedHistory);
    }
    let current = current?;
    if current.runtime() != archive.replay.runtime() {
        return Err(DWError::WrongPreHistory);
    }
    captured.ok_or(DWError::WrongPreHistory)
}
fn dw_endpoint(
    replay: &SourceContractReplay,
    e: &SourceDurableTransitionExpectation<'_>,
    post: bool,
) -> DurableWitnessResult<V> {
    source_full_dispatch_projection_consistency(replay)?;
    let expected = if post { e.post_history } else { e.pre_history };
    if replay.runtime().policy() != e.context.policy
        || replay.head.commitment != expected.source_receipt_head
        || state_commitment(replay.runtime())? != expected.source_state
    {
        return Err(if post {
            DWError::WrongPostHistory
        } else {
            DWError::WrongPreHistory
        });
    }
    let value = source_full_dispatch_context(replay, e.scope)?;
    if full_context_digest(e.context.policy, e.scope, &value)?
        != if post {
            e.post_context_commitment
        } else {
            e.pre_context_commitment
        }
    {
        return Err(if post {
            DWError::WrongPostContext
        } else {
            DWError::WrongPreContext
        });
    }
    Ok(value)
}
impl SourceDurableTransitionWitness {
    pub fn produce(
        root: &Path,
        label: &str,
        commands: &[SourceContractCommand],
        e: &SourceDurableTransitionExpectation<'_>,
    ) -> DurableWitnessResult<Self> {
        Self::produce_using(label, commands, e, |hash| dw_prefix(root, e, hash))
    }
    pub fn produce_from_archive(
        archive: &SourceDurableReplay,
        label: &str,
        commands: &[SourceContractCommand],
        e: &SourceDurableTransitionExpectation<'_>,
    ) -> DurableWitnessResult<Self> {
        Self::produce_using(label, commands, e, |hash| {
            dw_prefix_archive(archive, e, hash)
        })
    }
    fn produce_using(
        label: &str,
        commands: &[SourceContractCommand],
        e: &SourceDurableTransitionExpectation<'_>,
        prefix: impl FnOnce([u8; 32]) -> DurableWitnessResult<SourceContractReplay>,
    ) -> DurableWitnessResult<Self> {
        dw_expected(e)?;
        dw_count(e.pre_history, commands.len())?;
        if source_durable_transition_commands_commitment(
            e.context,
            e.scope,
            e.pre_history,
            label,
            commands,
        )? != e.commands_commitment
        {
            return Err(DWError::WrongCommands);
        }
        let (context, hash) = dw_context(e.context)?;
        let mut replay = prefix(hash)?;
        let pre_context = dw_endpoint(&replay, e, false)?;
        let mut entries = Vec::with_capacity(commands.len());
        for command in commands {
            full_command(e.context.policy, e.scope, command)?;
            let receipt = replay.apply(command.clone())?;
            if matches!(receipt.outcome, SourceContractCommandOutcome::LocalError(_)) {
                return Err(DWError::UnavailableExecution);
            }
            entries.push(Entry {
                command: command.clone(),
                receipt,
            });
        }
        let segment = DurableSegment {
            previous: e.pre_history,
            label: label.into(),
            entries,
        };
        let bytes = durable_encode_segment(hash, &segment)?;
        if durable_after(hash, &segment, sha256(&bytes))? != e.post_history {
            return Err(DWError::WrongPostHistory);
        }
        if source_durable_transition_receipts_commitment(
            e.context,
            e.scope,
            e.pre_history,
            &dw_receipts(&segment),
        )? != e.receipts_commitment
        {
            return Err(DWError::WrongReceipts);
        }
        let post_context = dw_endpoint(&replay, e, true)?;
        let witness = Self {
            context,
            descriptor: e.descriptor.clone(),
            scope: e.scope,
            pre_history: e.pre_history,
            pre_context,
            pre_context_hash: e.pre_context_commitment,
            segment: bytes,
            commands: e.commands_commitment,
            receipts: e.receipts_commitment,
            post_history: e.post_history,
            post_context,
            post_context_hash: e.post_context_commitment,
        };
        witness.canonical_bytes()?;
        Ok(witness)
    }
    pub fn verify(
        &self,
        root: &Path,
        e: &SourceDurableTransitionExpectation<'_>,
    ) -> DurableWitnessResult<ConditionalSourceDurableTransition> {
        self.verify_using(e, |hash| dw_prefix(root, e, hash))
    }
    pub fn verify_from_archive(
        &self,
        archive: &SourceDurableReplay,
        e: &SourceDurableTransitionExpectation<'_>,
    ) -> DurableWitnessResult<ConditionalSourceDurableTransition> {
        self.verify_using(e, |hash| dw_prefix_archive(archive, e, hash))
    }
    fn verify_using(
        &self,
        e: &SourceDurableTransitionExpectation<'_>,
        prefix: impl FnOnce([u8; 32]) -> DurableWitnessResult<SourceContractReplay>,
    ) -> DurableWitnessResult<ConditionalSourceDurableTransition> {
        dw_expected(e)?;
        let (context, hash) = dw_context(e.context)?;
        if self.context != context || self.descriptor != *e.descriptor || self.scope != e.scope {
            return Err(DWError::WrongContext);
        }
        if self.pre_history != e.pre_history {
            return Err(DWError::WrongPreHistory);
        }
        if self.post_history != e.post_history {
            return Err(DWError::WrongPostHistory);
        }
        if self.pre_context_hash != e.pre_context_commitment
            || full_context_digest(e.context.policy, e.scope, &self.pre_context)?
                != e.pre_context_commitment
        {
            return Err(DWError::WrongPreContext);
        }
        if self.post_context_hash != e.post_context_commitment
            || full_context_digest(e.context.policy, e.scope, &self.post_context)?
                != e.post_context_commitment
        {
            return Err(DWError::WrongPostContext);
        }
        let segment = durable_decode_segment(hash, &self.segment)?;
        dw_count(e.pre_history, segment.entries.len())?;
        if segment.previous != e.pre_history {
            return Err(DWError::WrongPreHistory);
        }
        let commands = segment
            .entries
            .iter()
            .map(|v| v.command.clone())
            .collect::<Vec<_>>();
        if self.commands != e.commands_commitment
            || source_durable_transition_commands_commitment(
                e.context,
                e.scope,
                e.pre_history,
                &segment.label,
                &commands,
            )? != e.commands_commitment
        {
            return Err(DWError::WrongCommands);
        }
        let receipts = dw_receipts(&segment);
        if self.receipts != e.receipts_commitment
            || source_durable_transition_receipts_commitment(
                e.context,
                e.scope,
                e.pre_history,
                &receipts,
            )? != e.receipts_commitment
        {
            return Err(DWError::WrongReceipts);
        }
        if durable_after(hash, &segment, sha256(&self.segment))? != e.post_history {
            return Err(DWError::WrongPostHistory);
        }
        let mut replay = prefix(hash)?;
        if dw_endpoint(&replay, e, false)? != self.pre_context {
            return Err(DWError::WrongPreContext);
        }
        for entry in &segment.entries {
            full_command(e.context.policy, e.scope, &entry.command)?;
            let actual = replay.apply(entry.command.clone())?;
            if matches!(actual.outcome, SourceContractCommandOutcome::LocalError(_)) {
                return Err(DWError::UnavailableExecution);
            }
            if actual != entry.receipt {
                return Err(DWError::WrongReceipts);
            }
        }
        if dw_endpoint(&replay, e, true)? != self.post_context {
            return Err(DWError::WrongPostContext);
        }
        Ok(ConditionalSourceDurableTransition {
            scope: self.scope,
            runtime: replay.runtime,
            receipts,
            context: e.context.clone(),
            archive_publication: e.archive_publication,
            pre_history: e.pre_history,
            post_history: e.post_history,
        })
    }
    pub fn canonical_bytes(&self) -> DurableWitnessResult<Vec<u8>> {
        full_descriptor(&self.descriptor)?;
        if self.segment.len() > MAX_CHECKPOINT_BYTES {
            return Err(DWError::Capacity);
        }
        let mut bytes = DURABLE_WITNESS_MAGIC.to_vec();
        bytes.push(self.scope.tag());
        put_frame(&mut bytes, &diagnostic(&self.descriptor.value())?)?;
        put_frame(&mut bytes, &self.context)?;
        durable_put_head(&mut bytes, self.pre_history);
        put_frame(&mut bytes, &diagnostic(&self.pre_context)?)?;
        bytes.extend(self.pre_context_hash);
        if bytes
            .len()
            .saturating_add(4)
            .saturating_add(self.segment.len())
            > MAX_SOURCE_DURABLE_TRANSITION_BYTES
        {
            return Err(DWError::Capacity);
        }
        bytes.extend((self.segment.len() as u32).to_be_bytes());
        bytes.extend(&self.segment);
        bytes.extend(self.commands);
        bytes.extend(self.receipts);
        durable_put_head(&mut bytes, self.post_history);
        put_frame(&mut bytes, &diagnostic(&self.post_context)?)?;
        bytes.extend(self.post_context_hash);
        let mut preimage = DURABLE_WITNESS_DOMAIN.to_vec();
        preimage.extend(&bytes);
        bytes.extend(sha256(&preimage));
        if bytes.len().saturating_add(4) > MAX_SOURCE_DURABLE_TRANSITION_BYTES {
            return Err(DWError::Capacity);
        }
        with_crc(bytes).map_err(|_| DWError::Capacity)
    }
    pub fn verify_bytes(
        root: &Path,
        bytes: &[u8],
        e: &SourceDurableTransitionExpectation<'_>,
    ) -> DurableWitnessResult<ConditionalSourceDurableTransition> {
        Self::parse_bytes(bytes, e)?.verify(root, e)
    }
    pub fn verify_bytes_from_archive(
        archive: &SourceDurableReplay,
        bytes: &[u8],
        e: &SourceDurableTransitionExpectation<'_>,
    ) -> DurableWitnessResult<ConditionalSourceDurableTransition> {
        Self::parse_bytes(bytes, e)?.verify_from_archive(archive, e)
    }
    fn parse_bytes(
        bytes: &[u8],
        e: &SourceDurableTransitionExpectation<'_>,
    ) -> DurableWitnessResult<Self> {
        dw_expected(e)?;
        if bytes.len() > MAX_SOURCE_DURABLE_TRANSITION_BYTES {
            return Err(DWError::Capacity);
        }
        let bad = |_| DWError::InvalidEnvelope;
        let raw = verify_crc(bytes).map_err(bad)?;
        let at = raw.len().checked_sub(32).ok_or(DWError::InvalidEnvelope)?;
        let (payload, digest) = raw.split_at(at);
        let mut preimage = DURABLE_WITNESS_DOMAIN.to_vec();
        preimage.extend(payload);
        if sha256(&preimage).as_slice() != digest {
            return Err(DWError::InvalidEnvelope);
        }
        let mut c = Cursor::new(payload);
        if c.take(5).map_err(bad)? != DURABLE_WITNESS_MAGIC || c.u8().map_err(bad)? != e.scope.tag()
        {
            return Err(DWError::InvalidEnvelope);
        }
        if read_frame(&mut c)? != e.descriptor.value() {
            return Err(DWError::WrongContext);
        }
        let context = diagnostic(&read_frame(&mut c)?)?;
        if context != dw_context(e.context)?.0 {
            return Err(DWError::WrongContext);
        }
        let pre_history = durable_read_head(&mut c)?;
        let pre_context = read_frame(&mut c)?;
        let pre_context_hash = c.fixed().map_err(bad)?;
        let size = c.u32().map_err(bad)? as usize;
        if size > MAX_CHECKPOINT_BYTES {
            return Err(DWError::Capacity);
        }
        let segment = c.take(size).map_err(bad)?.to_vec();
        let commands = c.fixed().map_err(bad)?;
        let receipts = c.fixed().map_err(bad)?;
        let post_history = durable_read_head(&mut c)?;
        let post_context = read_frame(&mut c)?;
        let post_context_hash = c.fixed().map_err(bad)?;
        c.end().map_err(bad)?;
        let witness = Self {
            context,
            descriptor: e.descriptor.clone(),
            scope: e.scope,
            pre_history,
            pre_context,
            pre_context_hash,
            segment,
            commands,
            receipts,
            post_history,
            post_context,
            post_context_hash,
        };
        if witness.canonical_bytes()?.as_slice() != bytes {
            return Err(DWError::InvalidEnvelope);
        }
        Ok(witness)
    }
}

#[cfg(test)]
mod durable_witness_tests {
    use super::*;
    include!("source_durable_transition_witness_tests.rs");
}
