// Included in source_contract_replay::source_durable; private replay helpers
// and its actual command executor are reused, without changing TLCR formats.
use crate::storage::{Journal, Mutation, Namespace, Recovery, StorageError};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
pub const MAX_DURABLE_SEGMENT_COMMANDS: usize = 256;
pub const MAX_DURABLE_REPLAY_SEGMENTS: usize = 65_536;
const DURABLE_CONFIG: &str = "source-durable/config-v1";
const DURABLE_PUBLICATION: &str = "source-durable/publication-v1";
const DURABLE_SEGMENTS: &str = "source-command-segments";
const DURABLE_MAGIC: &[u8; 5] = b"TLDS\x01";
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceDurableContext {
    pub chain: String,
    pub test: bool,
    pub policy: SourceContractRuntimePolicy,
    pub filesystem: SourceDurableFilesystem,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceDurableFilesystem {
    FileSyncOnly,
    FileAndDirectorySync,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceDurableHistoryHead {
    pub commands: u64,
    pub segments: u64,
    pub source_receipt_head: [u8; 32],
    pub source_state: [u8; 32],
    pub last_segment: [u8; 32],
    pub commitment: [u8; 32],
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceDurablePublication {
    pub revision: u64,
    pub history: SourceDurableHistoryHead,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceDurableReceipt {
    pub absolute_sequence: u64,
    pub source: SourceContractReceipt,
}
#[derive(Debug)]
pub enum SourceDurableError {
    Replay(SourceContractReplayError),
    Storage(StorageError),
    Io(std::io::Error),
    WrongContext,
    WrongHead,
    NotAncestor,
    Poisoned,
    InvalidArchive,
    Capacity,
    UnsupportedFilesystem,
    PublicationFailed {
        attempted: Box<SourceDurablePublication>,
        detail: String,
    },
}
impl std::fmt::Display for SourceDurableError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for SourceDurableError {}
impl From<SourceContractReplayError> for SourceDurableError {
    fn from(e: SourceContractReplayError) -> Self {
        Self::Replay(e)
    }
}
impl From<StorageError> for SourceDurableError {
    fn from(e: StorageError) -> Self {
        Self::Storage(e)
    }
}
impl From<std::io::Error> for SourceDurableError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
type DurableResult<T> = std::result::Result<T, SourceDurableError>;
#[derive(Debug, Clone)]
struct DurableSegment {
    previous: SourceDurableHistoryHead,
    label: String,
    entries: Vec<Entry>,
}
#[derive(Debug)]
pub struct SourceDurableReplay {
    journal: Journal,
    root: PathBuf,
    context: SourceDurableContext,
    context_bytes: Vec<u8>,
    context_hash: [u8; 32],
    replay: SourceContractReplay,
    publication: SourceDurablePublication,
    poisoned: bool,
    #[cfg(test)]
    test_fault: Option<DurableTestFault>,
}
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DurableTestFault {
    TornTemporary,
    AfterSegmentSync,
    AfterPublicationSync,
}
#[cfg(test)]
fn durable_test_terminate() {
    if std::env::var_os("TL_SOURCE_DURABLE_TERMINATE").is_some() {
        std::process::exit(86);
    }
}
fn durable_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|v| format!("{v:02x}")).collect()
}
#[cfg(test)]
#[derive(Default)]
struct DurableDirectorySyncProbe {
    segment_attempts: usize,
    failures_remaining: usize,
}
#[cfg(test)]
thread_local! {
    static DURABLE_DIRECTORY_SYNC_PROBE: std::cell::RefCell<Option<DurableDirectorySyncProbe>> = const { std::cell::RefCell::new(None) };
}
fn durable_directory_sync_supported() -> bool {
    #[cfg(test)]
    if DURABLE_DIRECTORY_SYNC_PROBE.with(|probe| probe.borrow().is_some()) {
        return true;
    }
    cfg!(unix)
}
// Only tests may replace the filesystem barrier. The probe is thread-local so
// Windows can test the strong profile's ordering without claiming OS support.
#[cfg(test)]
fn durable_injected_directory_sync(path: &Path) -> Option<DurableResult<()>> {
    DURABLE_DIRECTORY_SYNC_PROBE.with(|probe| {
        let mut probe = probe.borrow_mut();
        probe.as_mut().map(|probe| {
            if path.file_name() == Some(std::ffi::OsStr::new(DURABLE_SEGMENTS)) {
                probe.segment_attempts += 1;
                if probe.failures_remaining != 0 {
                    probe.failures_remaining -= 1;
                    return Err(
                        std::io::Error::other("injected segment-directory sync failure").into(),
                    );
                }
            }
            Ok(())
        })
    })
}
fn durable_context(context: &SourceDurableContext) -> DurableResult<Vec<u8>> {
    Namespace::new(&context.chain, context.test, "consensus")?;
    let fresh = SourceContractReplay::with_policy(context.policy)?;
    let checkpoint = fresh.checkpoint()?;
    if context.filesystem == SourceDurableFilesystem::FileAndDirectorySync
        && !durable_directory_sync_supported()
    {
        return Err(SourceDurableError::UnsupportedFilesystem);
    }
    Ok(diagnostic(&obj(&[
        ("schema", s("source-command-archive-v1")),
        (
            "source",
            s(std::str::from_utf8(SOURCE_PIN).map_err(|_| SourceDurableError::WrongContext)?),
        ),
        ("policy", s(&durable_hex(&checkpoint[..5]))),
        ("chain", s(&context.chain)),
        ("test", V::Bool(context.test)),
        (
            "filesystem",
            s(match context.filesystem {
                SourceDurableFilesystem::FileSyncOnly => "file-sync-only",
                SourceDurableFilesystem::FileAndDirectorySync => "file-and-directory-sync",
            }),
        ),
    ]))?)
}
fn durable_put_head(out: &mut Vec<u8>, head: SourceDurableHistoryHead) {
    out.extend_from_slice(&head.commands.to_be_bytes());
    out.extend_from_slice(&head.segments.to_be_bytes());
    for hash in [
        head.source_receipt_head,
        head.source_state,
        head.last_segment,
        head.commitment,
    ] {
        out.extend_from_slice(&hash);
    }
}
fn durable_read_head(c: &mut Cursor<'_>) -> DurableResult<SourceDurableHistoryHead> {
    let bad = |_| SourceDurableError::InvalidArchive;
    Ok(SourceDurableHistoryHead {
        commands: c.u64().map_err(bad)?,
        segments: c.u64().map_err(bad)?,
        source_receipt_head: c.fixed().map_err(bad)?,
        source_state: c.fixed().map_err(bad)?,
        last_segment: c.fixed().map_err(bad)?,
        commitment: c.fixed().map_err(bad)?,
    })
}
fn durable_publication_bytes(p: SourceDurablePublication) -> DurableResult<Vec<u8>> {
    let mut bytes = b"TLDP\x01".to_vec();
    bytes.extend_from_slice(&p.revision.to_be_bytes());
    durable_put_head(&mut bytes, p.history);
    with_crc(bytes).map_err(|_| SourceDurableError::Capacity)
}
fn durable_parse_publication(bytes: &[u8]) -> DurableResult<SourceDurablePublication> {
    let payload = verify_crc(bytes).map_err(|_| SourceDurableError::InvalidArchive)?;
    let mut c = Cursor::new(payload);
    let bad = |_| SourceDurableError::InvalidArchive;
    if c.take(5).map_err(bad)? != b"TLDP\x01" {
        return Err(SourceDurableError::InvalidArchive);
    }
    let p = SourceDurablePublication {
        revision: c.u64().map_err(bad)?,
        history: durable_read_head(&mut c)?,
    };
    c.end().map_err(bad)?;
    if durable_publication_bytes(p)? != bytes {
        return Err(SourceDurableError::InvalidArchive);
    }
    Ok(p)
}
fn durable_bootstrap(
    context_hash: [u8; 32],
    replay: &SourceContractReplay,
) -> DurableResult<SourceDurableHistoryHead> {
    let state = state_commitment(replay.runtime())?;
    let receipt = replay.head().commitment;
    let mut seed = b"TradeLayer/SourceDurableBootstrap/v1/".to_vec();
    seed.extend_from_slice(&context_hash);
    seed.extend_from_slice(&state);
    seed.extend_from_slice(&receipt);
    Ok(SourceDurableHistoryHead {
        commands: 0,
        segments: 0,
        source_receipt_head: receipt,
        source_state: state,
        last_segment: [0; 32],
        commitment: sha256(&seed),
    })
}
fn durable_after(
    context_hash: [u8; 32],
    segment: &DurableSegment,
    digest: [u8; 32],
) -> DurableResult<SourceDurableHistoryHead> {
    let last = segment
        .entries
        .last()
        .ok_or(SourceDurableError::InvalidArchive)?;
    let commands = segment
        .previous
        .commands
        .checked_add(segment.entries.len() as u64)
        .ok_or(SourceDurableError::Capacity)?;
    let segments = segment
        .previous
        .segments
        .checked_add(1)
        .filter(|n| *n <= MAX_DURABLE_REPLAY_SEGMENTS as u64)
        .ok_or(SourceDurableError::Capacity)?;
    let mut bytes = b"TradeLayer/SourceDurableHistory/v1/".to_vec();
    bytes.extend_from_slice(&context_hash);
    bytes.extend_from_slice(&segment.previous.commitment);
    bytes.extend_from_slice(&digest);
    bytes.extend_from_slice(&commands.to_be_bytes());
    bytes.extend_from_slice(&segments.to_be_bytes());
    bytes.extend_from_slice(&last.receipt.state_commitment);
    bytes.extend_from_slice(&last.receipt.head);
    Ok(SourceDurableHistoryHead {
        commands,
        segments,
        source_receipt_head: last.receipt.head,
        source_state: last.receipt.state_commitment,
        last_segment: digest,
        commitment: sha256(&bytes),
    })
}
fn durable_encode_segment(
    context_hash: [u8; 32],
    segment: &DurableSegment,
) -> DurableResult<Vec<u8>> {
    if segment.entries.is_empty()
        || segment.entries.len() > MAX_DURABLE_SEGMENT_COMMANDS
        || segment.label.len() > 256
    {
        return Err(SourceDurableError::Capacity);
    }
    let mut bytes = DURABLE_MAGIC.to_vec();
    bytes.extend_from_slice(&context_hash);
    durable_put_head(&mut bytes, segment.previous);
    put_frame(&mut bytes, &diagnostic(&s(&segment.label))?)?;
    bytes.extend_from_slice(&(segment.entries.len() as u32).to_be_bytes());
    for entry in &segment.entries {
        let r = &entry.receipt;
        bytes.extend_from_slice(&r.sequence.to_be_bytes());
        bytes.extend_from_slice(&r.previous_head);
        put_frame(&mut bytes, &diagnostic(&entry.command.value())?)?;
        put_frame(&mut bytes, &diagnostic(&r.outcome.value())?)?;
        if r.effects.len() > MAX_EFFECTS {
            return Err(SourceDurableError::Capacity);
        }
        bytes.extend_from_slice(&(r.effects.len() as u32).to_be_bytes());
        for effect in &r.effects {
            put_frame(&mut bytes, &diagnostic(effect)?)?;
        }
        for hash in [r.state_commitment, r.effects_commitment, r.head] {
            bytes.extend_from_slice(&hash);
        }
    }
    if bytes.len().saturating_add(4) > MAX_CHECKPOINT_BYTES {
        return Err(SourceDurableError::Capacity);
    }
    with_crc(bytes).map_err(|_| SourceDurableError::Capacity)
}
fn durable_decode_segment(context_hash: [u8; 32], bytes: &[u8]) -> DurableResult<DurableSegment> {
    if bytes.len() > MAX_CHECKPOINT_BYTES {
        return Err(SourceDurableError::Capacity);
    }
    let payload = verify_crc(bytes).map_err(|_| SourceDurableError::InvalidArchive)?;
    let mut c = Cursor::new(payload);
    let bad = |_| SourceDurableError::InvalidArchive;
    if c.take(5).map_err(bad)? != DURABLE_MAGIC || c.fixed::<32>().map_err(bad)? != context_hash {
        return Err(SourceDurableError::WrongContext);
    }
    let previous = durable_read_head(&mut c)?;
    let label = string(read_frame(&mut c)?)?;
    if label.len() > 256 {
        return Err(SourceDurableError::Capacity);
    }
    let count = c.u32().map_err(bad)? as usize;
    if count == 0 || count > MAX_DURABLE_SEGMENT_COMMANDS {
        return Err(SourceDurableError::Capacity);
    }
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        let sequence = c.u64().map_err(bad)?;
        let previous_head = c.fixed().map_err(bad)?;
        let command = SourceContractCommand::from_value(&read_frame(&mut c)?)?;
        let outcome = SourceContractCommandOutcome::from_value(&read_frame(&mut c)?)?;
        let n = c.u32().map_err(bad)? as usize;
        if n > MAX_EFFECTS {
            return Err(SourceDurableError::Capacity);
        }
        let mut effects = Vec::with_capacity(n);
        for _ in 0..n {
            effects.push(read_frame(&mut c)?);
        }
        entries.push(Entry {
            command,
            receipt: SourceContractReceipt {
                sequence,
                previous_head,
                outcome,
                effects,
                state_commitment: c.fixed().map_err(bad)?,
                effects_commitment: c.fixed().map_err(bad)?,
                head: c.fixed().map_err(bad)?,
            },
        });
    }
    c.end().map_err(bad)?;
    let segment = DurableSegment {
        previous,
        label,
        entries,
    };
    if durable_encode_segment(context_hash, &segment)? != bytes {
        return Err(SourceDurableError::InvalidArchive);
    }
    Ok(segment)
}
// No owner field is reset here. Only the bounded proof journal is released;
// the next segment starts local receipt sequence0 and links the prior hash.
fn durable_rotate(mut replay: SourceContractReplay) -> SourceContractReplay {
    replay.journal.clear();
    replay.encoded_bytes = 77;
    replay.head.commands = 0;
    replay
}
fn durable_sync_directory(profile: SourceDurableFilesystem, path: &Path) -> DurableResult<()> {
    if profile == SourceDurableFilesystem::FileAndDirectorySync {
        #[cfg(test)]
        if let Some(result) = durable_injected_directory_sync(path) {
            return result;
        }
        #[cfg(unix)]
        {
            File::open(path)?.sync_all()?;
        }
        #[cfg(not(unix))]
        {
            let _ = path;
            return Err(SourceDurableError::UnsupportedFilesystem);
        }
    }
    Ok(())
}
fn durable_directory(root: &Path) -> DurableResult<PathBuf> {
    let m = fs::symlink_metadata(root)?;
    if m.file_type().is_symlink() || !m.is_dir() {
        return Err(SourceDurableError::InvalidArchive);
    }
    Ok(fs::canonicalize(root)?)
}
fn durable_segment_path(root: &Path, digest: [u8; 32]) -> PathBuf {
    root.join(DURABLE_SEGMENTS)
        .join(format!("{}.tlds", durable_hex(&digest)))
}
fn durable_read_segment(
    root: &Path,
    context_hash: [u8; 32],
    digest: [u8; 32],
) -> DurableResult<DurableSegment> {
    durable_directory(&root.join(DURABLE_SEGMENTS))?;
    let path = durable_segment_path(root, digest);
    let m = fs::symlink_metadata(&path)?;
    if m.file_type().is_symlink() || !m.is_file() || m.len() > MAX_CHECKPOINT_BYTES as u64 {
        return Err(SourceDurableError::InvalidArchive);
    }
    let file = File::open(path)?;
    let mut bytes = vec![];
    file.take(MAX_CHECKPOINT_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_CHECKPOINT_BYTES || sha256(&bytes) != digest {
        return Err(SourceDurableError::InvalidArchive);
    }
    durable_decode_segment(context_hash, &bytes)
}
fn durable_restore(
    root: &Path,
    context: &SourceDurableContext,
    context_hash: [u8; 32],
    target: SourceDurableHistoryHead,
) -> DurableResult<SourceContractReplay> {
    if target.segments > MAX_DURABLE_REPLAY_SEGMENTS as u64 {
        return Err(SourceDurableError::Capacity);
    }
    let mut replay = SourceContractReplay::with_policy(context.policy)?;
    let bootstrap = durable_bootstrap(context_hash, &replay)?;
    let mut cursor = target;
    let mut reverse = Vec::with_capacity(target.segments as usize);
    while cursor.segments > 0 {
        let segment = durable_read_segment(root, context_hash, cursor.last_segment)?;
        if durable_after(context_hash, &segment, cursor.last_segment)? != cursor {
            return Err(SourceDurableError::WrongHead);
        }
        reverse.push(cursor.last_segment);
        cursor = segment.previous;
    }
    if cursor != bootstrap {
        return Err(SourceDurableError::WrongHead);
    }
    let mut head = bootstrap;
    for digest in reverse.into_iter().rev() {
        let segment = durable_read_segment(root, context_hash, digest)?;
        if segment.previous != head
            || replay.head.commitment != head.source_receipt_head
            || state_commitment(replay.runtime())? != head.source_state
        {
            return Err(SourceDurableError::WrongHead);
        }
        for entry in &segment.entries {
            if replay.apply(entry.command.clone())? != entry.receipt {
                return Err(SourceDurableError::Replay(
                    SourceContractReplayError::ReplayMismatch,
                ));
            }
        }
        head = durable_after(context_hash, &segment, digest)?;
        replay = durable_rotate(replay);
    }
    if head != target
        || state_commitment(replay.runtime())? != target.source_state
        || replay.head.commitment != target.source_receipt_head
    {
        return Err(SourceDurableError::WrongHead);
    }
    Ok(replay)
}
static DURABLE_TEMP_SERIAL: AtomicU64 = AtomicU64::new(0);
impl SourceDurableReplay {
    pub fn initialize(root: &Path, context: SourceDurableContext) -> DurableResult<Self> {
        let root = durable_directory(root)?;
        let context_bytes = durable_context(&context)?;
        let context_hash = sha256(&context_bytes);
        let namespace = Namespace::new(&context.chain, context.test, "consensus")?;
        let (mut journal, _) = Journal::open(&root, namespace, Recovery::RejectIncompleteTail)?;
        if journal.sequence()? != 0 || journal.entries()?.next().is_some() {
            return Err(SourceDurableError::WrongContext);
        }
        let directory = root.join(DURABLE_SEGMENTS);
        if !directory.exists() {
            fs::create_dir(&directory)?;
        }
        durable_directory(&directory)?;
        durable_sync_directory(context.filesystem, &root)?;
        let replay = SourceContractReplay::with_policy(context.policy)?;
        let publication = SourceDurablePublication {
            revision: 0,
            history: durable_bootstrap(context_hash, &replay)?,
        };
        journal.commit(vec![
            Mutation::Put {
                key: DURABLE_CONFIG.into(),
                value: context_bytes.clone(),
            },
            Mutation::Put {
                key: DURABLE_PUBLICATION.into(),
                value: durable_publication_bytes(publication)?,
            },
        ])?;
        Ok(Self {
            journal,
            root,
            context,
            context_bytes,
            context_hash,
            replay,
            publication,
            poisoned: false,
            #[cfg(test)]
            test_fault: None,
        })
    }
    /// Expected publication is an external CAS/freshness pin, not chain authority.
    /// Complete archive execution is mandatory; no decoded current-state input.
    pub fn open(
        root: &Path,
        context: SourceDurableContext,
        expected: SourceDurablePublication,
        recovery: Recovery,
    ) -> DurableResult<Self> {
        let root = durable_directory(root)?;
        let context_bytes = durable_context(&context)?;
        let context_hash = sha256(&context_bytes);
        let namespace = Namespace::new(&context.chain, context.test, "consensus")?;
        let (journal, _) = Journal::open_existing(&root, namespace, recovery)?;
        if journal.get(DURABLE_CONFIG)? != Some(context_bytes.as_slice())
            || journal
                .entries()?
                .any(|(key, _)| key != DURABLE_CONFIG && key != DURABLE_PUBLICATION)
        {
            return Err(SourceDurableError::WrongContext);
        }
        let publication = durable_parse_publication(
            journal
                .get(DURABLE_PUBLICATION)?
                .ok_or(SourceDurableError::InvalidArchive)?,
        )?;
        if publication != expected
            || journal.sequence()?
                != publication
                    .revision
                    .checked_add(1)
                    .ok_or(SourceDurableError::Capacity)?
        {
            return Err(SourceDurableError::WrongHead);
        }
        let replay = durable_restore(&root, &context, context_hash, publication.history)?;
        Ok(Self {
            journal,
            root,
            context,
            context_bytes,
            context_hash,
            replay,
            publication,
            poisoned: false,
            #[cfg(test)]
            test_fault: None,
        })
    }
    fn health(&self) -> DurableResult<()> {
        if self.poisoned {
            return Err(SourceDurableError::Poisoned);
        }
        self.journal.sequence()?;
        Ok(())
    }
    pub fn publication(&self) -> DurableResult<SourceDurablePublication> {
        self.health()?;
        Ok(self.publication)
    }
    pub fn runtime(&self) -> DurableResult<&SourceContractRuntime> {
        self.health()?;
        Ok(self.replay.runtime())
    }
    pub fn context(&self) -> &SourceDurableContext {
        &self.context
    }
    pub fn context_bytes(&self) -> &[u8] {
        &self.context_bytes
    }
    fn write_segment(&mut self, digest: [u8; 32], bytes: &[u8]) -> DurableResult<()> {
        let target = durable_segment_path(&self.root, digest);
        if target.exists() {
            let existing = durable_read_segment(&self.root, self.context_hash, digest)?;
            if durable_encode_segment(self.context_hash, &existing)? != bytes {
                return Err(SourceDurableError::InvalidArchive);
            }
            // An earlier attempt may have renamed this file and then failed
            // its directory barrier. Reuse must complete that barrier before
            // the publication can refer to this immutable segment.
            durable_sync_directory(self.context.filesystem, &self.root.join(DURABLE_SEGMENTS))?;
            return Ok(());
        }
        let directory = self.root.join(DURABLE_SEGMENTS);
        durable_directory(&directory)?;
        let temporary = directory.join(format!(
            "pending-{}-{}.tmp",
            std::process::id(),
            DURABLE_TEMP_SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        #[cfg(test)]
        if self.test_fault == Some(DurableTestFault::TornTemporary) {
            file.write_all(&bytes[..bytes.len() / 2])?;
            file.sync_all()?;
            self.test_fault = None;
            durable_test_terminate();
            return Err(SourceDurableError::InvalidArchive);
        }
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, &target)?;
        durable_sync_directory(self.context.filesystem, &directory)?;
        Ok(())
    }
    /// A batch is the durability boundary. Source exceptions keep their literal
    /// prefixes; local source refusals use the existing replay rollback behavior.
    pub fn commit_batch(
        &mut self,
        expected: SourceDurablePublication,
        label: &str,
        commands: Vec<SourceContractCommand>,
    ) -> DurableResult<Vec<SourceDurableReceipt>> {
        self.health()?;
        if expected != self.publication {
            return Err(SourceDurableError::WrongHead);
        }
        if commands.is_empty() || commands.len() > MAX_DURABLE_SEGMENT_COMMANDS || label.len() > 256
        {
            return Err(SourceDurableError::Capacity);
        }
        let mut candidate = self.replay.clone();
        for command in commands {
            candidate.apply(command)?;
        }
        let segment = DurableSegment {
            previous: self.publication.history,
            label: label.into(),
            entries: candidate.journal.clone(),
        };
        let bytes = durable_encode_segment(self.context_hash, &segment)?;
        let digest = sha256(&bytes);
        let next = SourceDurablePublication {
            revision: self
                .publication
                .revision
                .checked_add(1)
                .ok_or(SourceDurableError::Capacity)?,
            history: durable_after(self.context_hash, &segment, digest)?,
        };
        let receipts = segment
            .entries
            .iter()
            .enumerate()
            .map(|(i, e)| SourceDurableReceipt {
                absolute_sequence: segment.previous.commands + i as u64,
                source: e.receipt.clone(),
            })
            .collect();
        self.poisoned = true;
        let written = (|| -> DurableResult<()> {
            self.write_segment(digest, &bytes)?;
            #[cfg(test)]
            if self.test_fault == Some(DurableTestFault::AfterSegmentSync) {
                self.test_fault = None;
                durable_test_terminate();
                return Err(SourceDurableError::InvalidArchive);
            }
            self.journal.commit(vec![Mutation::Put {
                key: DURABLE_PUBLICATION.into(),
                value: durable_publication_bytes(next)?,
            }])?;
            #[cfg(test)]
            if self.test_fault == Some(DurableTestFault::AfterPublicationSync) {
                self.test_fault = None;
                durable_test_terminate();
                return Err(SourceDurableError::InvalidArchive);
            }
            Ok(())
        })();
        if let Err(error) = written {
            return Err(SourceDurableError::PublicationFailed {
                attempted: Box::new(next),
                detail: error.to_string(),
            });
        }
        self.replay = durable_rotate(candidate);
        self.publication = next;
        self.poisoned = false;
        Ok(receipts)
    }
    /// Rollback of this source history only, never Bitcoin fork-choice authority.
    /// The target must be an ancestor and is reexecuted from bootstrap.
    pub fn rewind_to(
        &mut self,
        expected: SourceDurablePublication,
        target: SourceDurableHistoryHead,
    ) -> DurableResult<()> {
        self.health()?;
        if expected != self.publication {
            return Err(SourceDurableError::WrongHead);
        }
        let mut cursor = self.publication.history;
        while cursor.segments > target.segments {
            let segment = durable_read_segment(&self.root, self.context_hash, cursor.last_segment)?;
            if durable_after(self.context_hash, &segment, cursor.last_segment)? != cursor {
                return Err(SourceDurableError::WrongHead);
            }
            cursor = segment.previous;
        }
        if cursor != target {
            return Err(SourceDurableError::NotAncestor);
        }
        let candidate = durable_restore(&self.root, &self.context, self.context_hash, target)?;
        let next = SourceDurablePublication {
            revision: self
                .publication
                .revision
                .checked_add(1)
                .ok_or(SourceDurableError::Capacity)?,
            history: target,
        };
        self.poisoned = true;
        if let Err(error) = self.journal.commit(vec![Mutation::Put {
            key: DURABLE_PUBLICATION.into(),
            value: durable_publication_bytes(next)?,
        }]) {
            return Err(SourceDurableError::PublicationFailed {
                attempted: Box::new(next),
                detail: error.to_string(),
            });
        }
        self.replay = candidate;
        self.publication = next;
        self.poisoned = false;
        Ok(())
    }
    pub const fn require_authenticated_input_and_state(
        &self,
    ) -> std::result::Result<(), SourceRuleError> {
        Err(SourceRuleError::UnauthenticatedInputAndState)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    include!("source_durable_replay_tests.rs");
}
