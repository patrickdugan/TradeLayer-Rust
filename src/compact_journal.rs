//! Explicit generational catalog over ordinary, unchanged native Journals.
//! One active generation and OS writer lease; read snapshots are immutable.
use crate::codec::crc32;
use crate::hash256::double_sha256;
use crate::storage::{
    Journal, JournalReadSnapshot, Mutation, Namespace, Recovery, RecoveryReport, StorageError,
    MAX_BATCH_BYTES, MAX_BATCH_OPERATIONS, MAX_LOG_BYTES,
};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions, TryLockError},
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, Weak},
};
pub const PROFILE: &str = "TL-COMPACT-JOURNAL-GENERATIONS-4";
include!("source_native_append_plan_impl.rs");
include!("compact_native_supersession_binding_impl.rs");
include!("compact_native_main_owner_access_impl.rs");
include!("compact_verified_prefix_impl.rs");
const MAX_GENERATIONS: usize = 64;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatalogPublication {
    pub generation: u64,
    pub parent: Option<u64>,
    pub physical_creation_sequence: u64,
    pub creation_sequence: u64,
    pub manifest_sha256d: [u8; 32],
}
#[derive(Clone, Debug)]
pub struct CompactionConfig {
    pub target_log_bytes: u64,
    pub retained_ancestors: usize,
}
impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            target_log_bytes: 8 * 1024 * 1024,
            retained_ancestors: 1,
        }
    }
}
#[derive(Clone, Debug)]
pub struct CompactReadSnapshot {
    data: JournalReadSnapshot,
    logical_sequence: u64,
    publication: CatalogPublication,
    _lease: Arc<ReaderLease>,
}
#[derive(Debug)]
struct ReaderLease {
    _file: File,
}
impl CompactReadSnapshot {
    pub const fn sequence(&self) -> u64 {
        self.logical_sequence
    }
    pub fn publication(&self) -> &CatalogPublication {
        &self.publication
    }
    pub fn get(&self, k: &str) -> Option<&[u8]> {
        self.data.get(k)
    }
    pub fn entries(&self) -> impl Iterator<Item = (&str, &[u8])> {
        self.data.entries()
    }
}
pub struct CompactionPlan {
    old: CatalogPublication,
    new: CatalogPublication,
    sequence: u64,
    data: CompactReadSnapshot,
    namespace: String,
    manifest: Vec<u8>,
    owner_token: u64,
    origin_catalog: PathBuf,
}
impl CompactionPlan {
    pub fn old_publication(&self) -> &CatalogPublication {
        &self.old
    }
    pub fn new_publication(&self) -> &CatalogPublication {
        &self.new
    }
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }
}
#[derive(Clone, Debug)]
pub struct CompactionReport {
    pub old: CatalogPublication,
    pub new: CatalogPublication,
    pub logical_sequence: u64,
    pub previous_log_bytes: u64,
    pub compacted_log_bytes: u64,
    pub removed_generations: Vec<u64>,
    pub pinned_generations: Vec<u64>,
}
#[derive(Clone, Debug)]
pub struct CatalogFootprint {
    pub active_generation: u64,
    pub logical_sequence: u64,
    pub active_log_bytes: u64,
    pub retained_disk_bytes: u64,
    pub live_key_value_bytes: u64,
    pub generation_count: usize,
}
/// Per-open observability only. These counters are not a replay/publication pin.
#[derive(Clone, Debug, Default)]
pub struct CatalogIoCounters {
    pub confirmed_append_bytes: u64,
    pub copied_checkpoint_bytes: u64,
    pub published_compactions: u64,
}
pub struct CompactingJournal {
    inner: Journal,
    namespace: Namespace,
    catalog: PathBuf,
    catalog_identity: String,
    owner_token: u64,
    publication: CatalogPublication,
    config: CompactionConfig,
    _writer_lease: File,
    reads: Mutex<BTreeMap<u64, Weak<ReaderLease>>>,
    poisoned: bool,
    pending: Option<CatalogPublication>,
    io: CatalogIoCounters,
    #[cfg(test)]
    fault: Option<CompactionFault>,
}
fn logical_sequence(publication: &CatalogPublication, physical: u64) -> Result<u64, StorageError> {
    let appended = physical
        .checked_sub(publication.physical_creation_sequence)
        .ok_or_else(|| bad("generation prefix missing"))?;
    publication
        .creation_sequence
        .checked_add(appended)
        .ok_or(StorageError::LimitExceeded)
}
fn bad(message: &'static str) -> StorageError {
    StorageError::Corrupt(message)
}
fn next_owner_token() -> Result<u64, StorageError> {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_update(
        std::sync::atomic::Ordering::SeqCst,
        std::sync::atomic::Ordering::SeqCst,
        |n| n.checked_add(1),
    )
    .map_err(|_| StorageError::LimitExceeded)
}
fn catalog_identity(
    catalog: &Path,
    namespace: &Namespace,
    create: bool,
) -> Result<String, StorageError> {
    let file = catalog.join("IDENTITY");
    if !file.exists() {
        if !create {
            return Err(bad("missing store identity"));
        }
        let mut input = catalog
            .canonicalize()?
            .as_os_str()
            .to_string_lossy()
            .as_bytes()
            .to_vec();
        input.extend_from_slice(&std::process::id().to_be_bytes());
        input.extend_from_slice(&next_owner_token()?.to_be_bytes());
        input.extend_from_slice(
            &std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|_| bad("identity clock"))?
                .as_nanos()
                .to_be_bytes(),
        );
        let mut bytes = double_sha256(&input).to_vec();
        bytes.extend_from_slice(&crc32(&bytes).to_be_bytes());
        write_new(&file, &bytes)?;
        sync_dir(catalog)?;
    }
    real_file(&file)?;
    let bytes = fs::read(file)?;
    if bytes.len() != 36
        || crc32(&bytes[..32]) != u32::from_be_bytes(bytes[32..].try_into().unwrap())
    {
        return Err(bad("store identity encoding"));
    }
    let id = bytes[..32]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    Ok(format!(
        "{}\nCATALOG\t{}\nIDENTITY\t{id}",
        identity(namespace),
        catalog.canonicalize()?.to_string_lossy()
    ))
}
fn identity(namespace: &Namespace) -> String {
    namespace
        .path(Path::new(""))
        .components()
        .map(|c| c.as_os_str().to_str().expect("ASCII Namespace"))
        .collect::<Vec<_>>()
        .join("/")
}
fn gen_name(g: u64) -> String {
    format!("g-{g:020}")
}
fn gen_path(catalog: &Path, g: u64) -> PathBuf {
    catalog.join(gen_name(g))
}
fn u64_at(b: &[u8], p: usize) -> Result<u64, StorageError> {
    Ok(u64::from_be_bytes(
        b.get(p..p + 8)
            .ok_or_else(|| bad("catalog field"))?
            .try_into()
            .map_err(|_| bad("catalog field"))?,
    ))
}
fn manifest(namespace: &str, g: u64, parent: Option<u64>, base: u64, sequence: u64) -> Vec<u8> {
    let mut out = b"TLCJ\x04".to_vec();
    out.extend_from_slice(&(namespace.len() as u32).to_be_bytes());
    out.extend_from_slice(namespace.as_bytes());
    for n in [g, parent.unwrap_or(u64::MAX), base, sequence] {
        out.extend_from_slice(&n.to_be_bytes());
    }
    out.extend_from_slice(&crc32(&out).to_be_bytes());
    out
}
fn parse_manifest(
    bytes: &[u8],
    expected_namespace: &str,
) -> Result<CatalogPublication, StorageError> {
    if bytes.len() > 4096 || bytes.len() < 45 || bytes.get(..5) != Some(b"TLCJ\x04") {
        return Err(bad("catalog header"));
    }
    let n = u32::from_be_bytes(bytes[5..9].try_into().unwrap()) as usize;
    if bytes.len() != 9 + n + 36 || bytes.get(9..9 + n) != Some(expected_namespace.as_bytes()) {
        return Err(StorageError::WrongNamespace);
    }
    if crc32(&bytes[..bytes.len() - 4])
        != u32::from_be_bytes(bytes[bytes.len() - 4..].try_into().unwrap())
    {
        return Err(bad("catalog CRC"));
    }
    let g = u64_at(bytes, 9 + n)?;
    let p = u64_at(bytes, 17 + n)?;
    let base = u64_at(bytes, 25 + n)?;
    let creation = u64_at(bytes, 33 + n)?;
    if p != u64::MAX && p >= g || base > crate::storage::MAX_ENTRIES as u64 {
        return Err(bad("catalog sequence"));
    }
    Ok(CatalogPublication {
        generation: g,
        parent: (p != u64::MAX).then_some(p),
        physical_creation_sequence: base,
        creation_sequence: creation,
        manifest_sha256d: double_sha256(bytes),
    })
}
fn real_dir(path: &Path) -> Result<(), StorageError> {
    let m = fs::symlink_metadata(path)?;
    if !m.is_dir() || reparse(&m) {
        return Err(bad("nonordinary catalog directory"));
    }
    Ok(())
}
fn real_file(path: &Path) -> Result<(), StorageError> {
    let m = fs::symlink_metadata(path)?;
    if !m.is_file() || reparse(&m) {
        return Err(bad("nonordinary catalog file"));
    }
    Ok(())
}
#[cfg(windows)]
fn reparse(m: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    m.file_attributes() & 0x400 != 0
}
#[cfg(not(windows))]
fn reparse(m: &fs::Metadata) -> bool {
    m.file_type().is_symlink()
}
fn sync_dir(path: &Path) -> Result<(), StorageError> {
    #[cfg(unix)]
    {
        File::open(path)?.sync_all()?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}
fn write_new(path: &Path, bytes: &[u8]) -> Result<(), StorageError> {
    let mut f = OpenOptions::new().write(true).create_new(true).open(path)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    Ok(())
}
fn batch_size(v: &[Mutation]) -> Result<u64, StorageError> {
    let mut n = 36u64;
    for m in v {
        let add = match m {
            Mutation::Put { key, value } => 7 + key.len() + value.len(),
            Mutation::Delete { key } => 3 + key.len(),
        };
        n = n
            .checked_add(add as u64)
            .ok_or(StorageError::LimitExceeded)?;
    }
    Ok(n)
}
fn copy_batch_count(data: &CompactReadSnapshot) -> Result<u64, StorageError> {
    let mut count = 0;
    let mut ops = 0;
    let mut bytes = 4;
    for (k, v) in data.entries() {
        let size = 7 + k.len() + v.len();
        if ops == MAX_BATCH_OPERATIONS || bytes + size > MAX_BATCH_BYTES {
            count += 1;
            ops = 0;
            bytes = 4;
        }
        ops += 1;
        bytes += size;
    }
    if ops > 0 {
        count += 1;
    }
    Ok(count)
}
impl CompactingJournal {
    pub fn open(
        root: &Path,
        namespace: Namespace,
        recovery: Recovery,
    ) -> Result<(Self, RecoveryReport), StorageError> {
        Self::open_mode(root, namespace, recovery, true, None)
    }
    pub fn open_existing(
        root: &Path,
        namespace: Namespace,
        recovery: Recovery,
    ) -> Result<(Self, RecoveryReport), StorageError> {
        Self::open_mode(root, namespace, recovery, false, None)
    }
    pub fn open_pinned(
        root: &Path,
        namespace: Namespace,
        recovery: Recovery,
        expected: &CatalogPublication,
        expected_sequence: u64,
    ) -> Result<(Self, RecoveryReport), StorageError> {
        Self::open_mode(
            root,
            namespace,
            recovery,
            false,
            Some((expected, expected_sequence)),
        )
    }
    fn open_mode_verified(
        root: &Path,
        namespace: Namespace,
        recovery: Recovery,
        create: bool,
        expected: Option<(&CatalogPublication, u64)>,
        expected_prefix: Option<&VerifiedCompactPrefixV1>,
    ) -> Result<(Self, RecoveryReport), StorageError> {
        real_dir(root)?;
        if namespace.path(root).file_name() != Some(std::ffi::OsStr::new("txIndex.tldb")) {
            return Err(StorageError::WrongNamespace);
        }
        let legacy = namespace.path(root);
        let parent = legacy.parent().ok_or_else(|| bad("namespace parent"))?;
        if create {
            match fs::create_dir(parent) {
                Ok(()) => sync_dir(root)?,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
                Err(e) => return Err(e.into()),
            }
        }
        real_dir(parent)?;
        let catalog = legacy.with_extension("compact-v4");
        if create
            && !catalog.exists()
            && (legacy.exists()
                || legacy.with_extension("compact-v2").exists()
                || legacy.with_extension("compact-v3").exists())
        {
            return Err(StorageError::WrongNamespace);
        }
        if create {
            match fs::create_dir(&catalog) {
                Ok(()) => sync_dir(parent)?,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
                Err(e) => return Err(e.into()),
            }
        }
        real_dir(&catalog)?;
        let lease_path = catalog.join("WRITER");
        let lease = if create {
            OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(&lease_path)?
        } else {
            OpenOptions::new()
                .read(true)
                .write(true)
                .open(&lease_path)?
        };
        match lease.try_lock() {
            Ok(()) => (),
            Err(TryLockError::WouldBlock) => return Err(StorageError::Locked),
            Err(TryLockError::Error(e)) => return Err(e.into()),
        }
        lease.sync_all()?;
        sync_dir(&catalog)?;
        let catalog_identity = catalog_identity(&catalog, &namespace, create)?;
        if let Some(pin) = expected_prefix {
            if pin.namespace != namespace
                || pin.persistent_identity != catalog_identity
                || pin.canonical_catalog != catalog.canonicalize()?
            {
                return Err(bad(
                    "unexpected independently pinned compact recovery context",
                ));
            }
        }
        let current = catalog.join("CURRENT");
        if !current.exists() {
            if !create {
                return Err(bad("missing CURRENT"));
            }
            let g = gen_path(&catalog, 0);
            fs::create_dir(&g)?;
            let (j, _) = Journal::open(&g, namespace.clone(), Recovery::RejectIncompleteTail)?;
            let bytes = manifest(&catalog_identity, 0, None, 0, 0);
            write_new(&g.join("READERS"), PROFILE.as_bytes())?;
            write_new(&g.join("GENINFO"), &bytes)?;
            sync_dir(&g)?;
            sync_dir(&catalog)?;
            write_new(&catalog.join(".CURRENT-0.pending"), &bytes)?;
            fs::rename(catalog.join(".CURRENT-0.pending"), &current)?;
            sync_dir(&catalog)?;
            drop(j);
        }
        real_file(&current)?;
        let publication = parse_manifest(&fs::read(&current)?, &catalog_identity)?;
        if expected.is_some_and(|(pin, _)| pin != &publication) {
            return Err(bad("unexpected independently pinned publication"));
        }
        let g = gen_path(&catalog, publication.generation);
        real_dir(&g)?;
        let info = g.join("GENINFO");
        real_file(&info)?;
        real_file(&g.join("READERS"))?;
        if fs::read(info)? != fs::read(&current)? {
            return Err(bad("generation/CURRENT mismatch"));
        }
        let (journal, report) = if let Some((_, logical)) = expected {
            let delta = logical
                .checked_sub(publication.creation_sequence)
                .ok_or_else(|| bad("unexpected independently pinned sequence"))?;
            let physical = publication
                .physical_creation_sequence
                .checked_add(delta)
                .ok_or(StorageError::LimitExceeded)?;
            if let Some(pin) = expected_prefix {
                let expected_generation =
                    gen_path(&pin.canonical_catalog, pin.publication.generation);
                let prefix = crate::storage::VerifiedJournalPrefixV1 {
                    profile: crate::storage::VERIFIED_PREFIX_OPEN_PROFILE.into(),
                    namespace: namespace.clone(),
                    canonical_file: namespace.path(&expected_generation),
                    physical_sequence: physical,
                    content: pin.content.clone(),
                };
                Journal::open_existing_verified_prefix(&g, namespace.clone(), recovery, &prefix)?
            } else {
                Journal::open_existing_at_sequence(&g, namespace.clone(), recovery, physical)?
            }
        } else {
            Journal::open_existing(&g, namespace.clone(), recovery)?
        };
        logical_sequence(&publication, journal.sequence()?)?;
        Ok((
            Self {
                inner: journal,
                namespace,
                catalog_identity,
                owner_token: next_owner_token()?,
                catalog,
                publication,
                config: CompactionConfig::default(),
                _writer_lease: lease,
                reads: Mutex::new(BTreeMap::new()),
                poisoned: false,
                pending: None,
                io: CatalogIoCounters::default(),
                #[cfg(test)]
                fault: None,
            },
            report,
        ))
    }
    pub fn configure(&mut self, config: CompactionConfig) -> Result<(), StorageError> {
        self.check()?;
        if config.target_log_bytes < 65536
            || config.target_log_bytes > MAX_LOG_BYTES
            || config.retained_ancestors > 8
        {
            return Err(StorageError::LimitExceeded);
        }
        self.config = config;
        Ok(())
    }
    fn check(&self) -> Result<(), StorageError> {
        if self.poisoned {
            return Err(StorageError::Poisoned);
        }
        self.inner.sequence()?;
        Ok(())
    }
    pub fn sequence(&self) -> Result<u64, StorageError> {
        self.check()?;
        logical_sequence(&self.publication, self.inner.sequence()?)
    }
    pub fn publication(&self) -> Result<&CatalogPublication, StorageError> {
        self.check()?;
        Ok(&self.publication)
    }
    pub fn pending_publication(&self) -> Option<&CatalogPublication> {
        self.pending.as_ref()
    }
    pub fn io_counters(&self) -> Result<&CatalogIoCounters, StorageError> {
        self.check()?;
        Ok(&self.io)
    }
    pub fn get(&self, k: &str) -> Result<Option<&[u8]>, StorageError> {
        self.check()?;
        self.inner.get(k)
    }
    pub fn entries(&self) -> Result<impl Iterator<Item = (&str, &[u8])>, StorageError> {
        self.check()?;
        self.inner.entries()
    }
    pub fn path(&self) -> &Path {
        self.inner.path()
    }
    pub fn namespace(&self) -> &Namespace {
        &self.namespace
    }
    pub fn read_snapshot(&self) -> Result<CompactReadSnapshot, StorageError> {
        self.check()?;
        let mut reads = self.reads.lock().map_err(|_| StorageError::Poisoned)?;
        let lease = if let Some(a) = reads
            .get(&self.publication.generation)
            .and_then(Weak::upgrade)
        {
            a
        } else {
            let path = gen_path(&self.catalog, self.publication.generation).join("READERS");
            real_file(&path)?;
            let file = File::open(path)?;
            match file.try_lock_shared() {
                Ok(()) => (),
                Err(TryLockError::WouldBlock) => return Err(StorageError::Locked),
                Err(TryLockError::Error(e)) => return Err(e.into()),
            }
            let a = Arc::new(ReaderLease { _file: file });
            reads.insert(self.publication.generation, Arc::downgrade(&a));
            a
        };
        Ok(CompactReadSnapshot {
            data: self.inner.read_snapshot()?,
            logical_sequence: self.sequence()?,
            publication: self.publication.clone(),
            _lease: lease,
        })
    }
    pub fn prepare_compaction(&self) -> Result<CompactionPlan, StorageError> {
        self.check()?;
        let data = self.read_snapshot()?;
        let sequence = self.sequence()?;
        let copies = copy_batch_count(&data)?;
        let mut generation = self
            .publication
            .generation
            .checked_add(1)
            .ok_or(StorageError::LimitExceeded)?;
        let mut found = false;
        for _ in 0..MAX_GENERATIONS {
            if !gen_path(&self.catalog, generation).exists() {
                found = true;
                break;
            }
            generation = generation
                .checked_add(1)
                .ok_or(StorageError::LimitExceeded)?;
        }
        if !found {
            return Err(StorageError::LimitExceeded);
        }
        let base = copies;
        let namespace = self.catalog_identity.clone();
        let bytes = manifest(
            &namespace,
            generation,
            Some(self.publication.generation),
            base,
            sequence,
        );
        let new = parse_manifest(&bytes, &namespace)?;
        Ok(CompactionPlan {
            old: self.publication.clone(),
            new,
            sequence,
            data,
            namespace,
            manifest: bytes,
            owner_token: self.owner_token,
            origin_catalog: self.catalog.canonicalize()?,
        })
    }
    pub fn publish_compaction(
        &mut self,
        plan: &CompactionPlan,
    ) -> Result<CompactionReport, StorageError> {
        self.check()?;
        if plan.old != self.publication
            || plan.sequence != self.sequence()?
            || plan.namespace != self.catalog_identity
            || plan.owner_token != self.owner_token
            || plan.origin_catalog != self.catalog.canonicalize()?
        {
            return Err(bad("stale compaction intent"));
        }
        let previous_log_bytes = fs::metadata(self.inner.path())?.len();
        let g = gen_path(&self.catalog, plan.new.generation);
        fs::create_dir(&g)?;
        self.hit(FaultPoint::BeforeGeneration)?;
        let (mut replacement, _) =
            Journal::open(&g, self.namespace.clone(), Recovery::RejectIncompleteTail)?;
        let mut batch = vec![];
        let mut bytes = 4;
        for (k, v) in plan.data.entries() {
            let size = 7 + k.len() + v.len();
            if batch.len() == MAX_BATCH_OPERATIONS || bytes + size > MAX_BATCH_BYTES {
                replacement.commit(std::mem::take(&mut batch))?;
                bytes = 4;
                self.hit(FaultPoint::DuringGeneration)?;
            }
            batch.push(Mutation::Put {
                key: k.into(),
                value: v.to_vec(),
            });
            bytes += size;
        }
        if !batch.is_empty() {
            replacement.commit(batch)?;
        }
        replacement.read_snapshot()?;
        if !replacement.entries()?.eq(plan.data.entries()) {
            return Err(bad("compaction state mismatch"));
        }
        self.io.copied_checkpoint_bytes = self
            .io
            .copied_checkpoint_bytes
            .saturating_add(fs::metadata(replacement.path())?.len());
        write_new(&g.join("READERS"), PROFILE.as_bytes())?;
        write_new(&g.join("GENINFO"), &plan.manifest)?;
        sync_dir(
            replacement
                .path()
                .parent()
                .ok_or_else(|| bad("generation namespace"))?,
        )?;
        sync_dir(&g)?;
        sync_dir(&self.catalog)?;
        self.hit(FaultPoint::AfterGenerationSync)?;
        let temp = self
            .catalog
            .join(format!(".CURRENT-{}.pending", plan.new.generation));
        write_new(&temp, &plan.manifest)?;
        self.hit(FaultPoint::BeforeCurrent)?;
        self.poisoned = true;
        self.pending = Some(plan.new.clone());
        fs::rename(&temp, self.catalog.join("CURRENT"))?;
        self.hit(FaultPoint::AfterCurrent)?;
        sync_dir(&self.catalog)?;
        self.hit(FaultPoint::AfterCurrentSync)?;
        self.inner = replacement;
        self.publication = plan.new.clone();
        self.io.published_compactions = self.io.published_compactions.saturating_add(1);
        self.poisoned = false;
        // The caller's plan pins the previous generation. Drop the plan before a
        // later prune if immediate reclamation of that previous file is desired.
        let (removed, pinned) = self.prune_retention()?;
        self.pending = None;
        Ok(CompactionReport {
            old: plan.old.clone(),
            new: plan.new.clone(),
            logical_sequence: self.sequence()?,
            previous_log_bytes,
            compacted_log_bytes: fs::metadata(self.inner.path())?.len(),
            removed_generations: removed,
            pinned_generations: pinned,
        })
    }
    pub fn commit(&mut self, mutations: Vec<Mutation>) -> Result<u64, StorageError> {
        self.check()?;
        self.inner.validate_commit(&mutations)?;
        let live = self
            .inner
            .entries()?
            .map(|(k, v)| (k.len() + v.len()) as u64)
            .sum::<u64>();
        let limit = self
            .config
            .target_log_bytes
            .max(live.saturating_mul(3).saturating_add(65536))
            .min(MAX_LOG_BYTES.saturating_sub(MAX_BATCH_BYTES as u64 + 65536));
        if fs::metadata(self.inner.path())?
            .len()
            .saturating_add(batch_size(&mutations)?)
            > limit
        {
            let plan = self.prepare_compaction()?;
            self.publish_compaction(&plan)?;
            drop(plan);
            self.prune_retention()?;
        }
        let bytes = batch_size(&mutations)?;
        self.inner.commit(mutations)?;
        self.io.confirmed_append_bytes = self.io.confirmed_append_bytes.saturating_add(bytes);
        self.sequence()
    }
    pub fn commit_if_sequence(
        &mut self,
        expected: u64,
        mutations: Vec<Mutation>,
    ) -> Result<u64, StorageError> {
        if self.sequence()? != expected {
            return Err(bad("stale logical mutation"));
        }
        self.commit(mutations)
    }
    pub fn prune_retention(&mut self) -> Result<(Vec<u64>, Vec<u64>), StorageError> {
        self.check()?;
        self.poisoned = true;
        self.pending = Some(self.publication.clone());
        let result = self.prune_retention_inner();
        if result.is_ok() {
            self.poisoned = false;
            self.pending = None;
        }
        result
    }
    fn prune_retention_inner(&mut self) -> Result<(Vec<u64>, Vec<u64>), StorageError> {
        let mut kept = vec![self.publication.generation];
        let mut parent = self.publication.parent;
        for _ in 0..self.config.retained_ancestors {
            if let Some(g) = parent {
                let info = gen_path(&self.catalog, g).join("GENINFO");
                if !info.exists() {
                    break;
                }
                real_file(&info)?;
                kept.push(g);
                parent = parse_manifest(&fs::read(info)?, &self.catalog_identity)?.parent;
            }
        }
        let mut candidates = vec![];
        let mut pending_files = vec![];
        let mut entries = 0;
        for entry in fs::read_dir(&self.catalog)? {
            let e = entry?;
            entries += 1;
            if entries > MAX_GENERATIONS * 3 + 16 {
                return Err(StorageError::LimitExceeded);
            }
            let name = e.file_name();
            let name = name.to_str().ok_or_else(|| bad("catalog name"))?;
            if let Some(n) = name.strip_prefix("g-") {
                if n.len() != 20 || !n.bytes().all(|b| b.is_ascii_digit()) {
                    return Err(bad("generation name"));
                }
                let g = n.parse::<u64>().map_err(|_| bad("generation number"))?;
                if !kept.contains(&g) {
                    candidates.push(g);
                }
            } else if let Some(n) = name
                .strip_prefix(".CURRENT-")
                .and_then(|n| n.strip_suffix(".pending"))
            {
                let g = n
                    .parse::<u64>()
                    .map_err(|_| bad("pending publication name"))?;
                if g != self.publication.generation {
                    pending_files.push(e.path());
                }
            } else if name != "CURRENT" && name != "WRITER" && name != "IDENTITY" {
                return Err(bad("unowned catalog entry"));
            }
        }
        if candidates.len() > MAX_GENERATIONS {
            return Err(StorageError::LimitExceeded);
        }
        let mut removed = vec![];
        let mut pinned = vec![];
        for g in candidates {
            if self
                .reads
                .lock()
                .map_err(|_| StorageError::Poisoned)?
                .get(&g)
                .and_then(Weak::upgrade)
                .is_some()
            {
                pinned.push(g);
                continue;
            }
            self.hit(FaultPoint::BeforeRetention)?;
            let dir = gen_path(&self.catalog, g);
            real_dir(&dir)?;
            if dir.canonicalize()?.parent() != Some(self.catalog.canonicalize()?.as_path()) {
                return Err(bad("retention path escaped catalog"));
            }
            let data = self.namespace.path(&dir);
            let sub = data.parent().ok_or_else(|| bad("generation namespace"))?;
            for e in fs::read_dir(&dir)? {
                let e = e?;
                let p = e.path();
                if p != sub && p != dir.join("GENINFO") && p != dir.join("READERS") {
                    return Err(bad("unowned generation entry"));
                }
            }
            if sub.exists() {
                real_dir(sub)?;
                for e in fs::read_dir(sub)? {
                    if e?.path() != data {
                        return Err(bad("unowned generation file"));
                    }
                }
            }
            let readers = dir.join("READERS");
            let mut reader_guard = None;
            if readers.exists() {
                real_file(&readers)?;
                let f = OpenOptions::new().read(true).write(true).open(&readers)?;
                match f.try_lock() {
                    Ok(()) => reader_guard = Some(f),
                    Err(TryLockError::WouldBlock) => {
                        pinned.push(g);
                        continue;
                    }
                    Err(TryLockError::Error(e)) => return Err(e.into()),
                }
            }
            let mut data_guard = None;
            if data.exists() {
                real_file(&data)?;
                let f = OpenOptions::new().read(true).write(true).open(&data)?;
                match f.try_lock() {
                    Ok(()) => data_guard = Some(f),
                    Err(TryLockError::WouldBlock) => {
                        pinned.push(g);
                        continue;
                    }
                    Err(TryLockError::Error(e)) => return Err(e.into()),
                }
                fs::remove_file(&data)?;
            }
            self.hit(FaultPoint::DuringRetention)?;
            if sub.exists() {
                fs::remove_dir(sub)?;
            }
            let info = dir.join("GENINFO");
            if info.exists() {
                real_file(&info)?;
                fs::remove_file(info)?;
            }
            if readers.exists() {
                fs::remove_file(&readers)?;
            }
            drop(data_guard);
            drop(reader_guard);
            fs::remove_dir(&dir)?;
            sync_dir(&self.catalog)?;
            removed.push(g);
            self.hit(FaultPoint::AfterRetention)?;
        }
        for p in pending_files {
            real_file(&p)?;
            if p.parent() != Some(self.catalog.as_path()) {
                return Err(bad("pending path"));
            }
            fs::remove_file(p)?;
        }
        sync_dir(&self.catalog)?;
        Ok((removed, pinned))
    }
    pub fn footprint(&self) -> Result<CatalogFootprint, StorageError> {
        self.check()?;
        let mut total = 0;
        let mut count = 0;
        for e in fs::read_dir(&self.catalog)? {
            let e = e?;
            let p = e.path();
            let n = e.file_name();
            if n.to_str().is_some_and(|n| n.starts_with("g-")) {
                real_dir(&p)?;
                count += 1;
                let file = self.namespace.path(&p);
                if file.exists() {
                    real_file(&file)?;
                    total += fs::metadata(file)?.len();
                }
                let info = p.join("GENINFO");
                if info.exists() {
                    real_file(&info)?;
                    total += fs::metadata(info)?.len();
                }
            } else if e.file_type()?.is_file() {
                total += e.metadata()?.len();
            }
        }
        Ok(CatalogFootprint {
            active_generation: self.publication.generation,
            logical_sequence: self.sequence()?,
            active_log_bytes: fs::metadata(self.inner.path())?.len(),
            retained_disk_bytes: total,
            live_key_value_bytes: self
                .inner
                .entries()?
                .map(|(k, v)| (k.len() + v.len()) as u64)
                .sum(),
            generation_count: count,
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FaultPoint {
    BeforeGeneration,
    DuringGeneration,
    AfterGenerationSync,
    BeforeCurrent,
    AfterCurrent,
    AfterCurrentSync,
    BeforeRetention,
    DuringRetention,
    AfterRetention,
}
#[cfg(test)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct CompactionFault {
    point: FaultPoint,
    exit: bool,
}
impl CompactingJournal {
    #[cfg(test)]
    pub(crate) fn test_fail_after_sync(&mut self) {
        self.inner.test_fail_after_sync();
    }
    #[cfg(test)]
    pub(crate) fn test_fail_after_partial_write(&mut self) {
        self.inner.test_fail_after_partial_write();
    }
    fn hit(&mut self, point: FaultPoint) -> Result<(), StorageError> {
        #[cfg(test)]
        if self.fault.as_ref().is_some_and(|f| f.point == point) {
            let f = self.fault.take().unwrap();
            if f.exit {
                std::process::exit(73);
            }
            return Err(std::io::Error::other(format!("injected compaction {point:?}")).into());
        }
        let _ = point;
        Ok(())
    }
    #[cfg(test)]
    pub(crate) fn test_fault(&mut self, name: &str, exit: bool) {
        let point = match name {
            "before-generation" => FaultPoint::BeforeGeneration,
            "during-generation" => FaultPoint::DuringGeneration,
            "after-generation-sync" => FaultPoint::AfterGenerationSync,
            "before-current" => FaultPoint::BeforeCurrent,
            "after-current" => FaultPoint::AfterCurrent,
            "after-current-sync" => FaultPoint::AfterCurrentSync,
            "before-retention" => FaultPoint::BeforeRetention,
            "during-retention" => FaultPoint::DuringRetention,
            "after-retention" => FaultPoint::AfterRetention,
            _ => panic!("unknown fault"),
        };
        self.fault = Some(CompactionFault { point, exit });
    }
}

#[cfg(test)]
#[path = "source_native_append_plan_tests.rs"]
mod source_native_append_plan_tests;
