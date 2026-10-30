//! Metadata-only active-service barriers. No financial runtime/state importer.
use crate::hash256::{double_sha256, double_sha256_parts, sha256};
use crate::source_contract_replay::source_durable::SourceDurableFilesystem;
use crate::storage::{
    Journal, JournalCompleteKvReceiptV1, Mutation, Namespace, Recovery, VerifiedJournalPrefixV1,
    COMPLETE_KV_RECEIPT_PROFILE,
};
use crate::validity_source::SourceValue as V;
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
pub const PROFILE: &str = "TL-MAIN-ACTIVE-SERVICE-WAL-13-1";
pub const MAX_BLOB: usize = 64 * 1024 * 1024;
const CONTEXT: &str = "active-service/context/v1";
const RECORD: &str = "active-service/record/v1";
const BLOBS: &str = "active-service-blobs-v1";
static NEXT: AtomicU64 = AtomicU64::new(1);
type Result<T> = std::result::Result<T, ActiveServiceWalErrorV1>;
#[derive(Debug)]
pub enum ActiveServiceWalErrorV1 {
    Io(std::io::Error),
    Storage(crate::storage::StorageError),
    WrongContext,
    WrongPin,
    WrongPlan,
    AlreadyAttempted,
    Ambiguous,
    InvalidRecord,
    Capacity,
    UnsupportedFilesystem,
}
impl std::fmt::Display for ActiveServiceWalErrorV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for ActiveServiceWalErrorV1 {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Storage(e) => Some(e),
            _ => None,
        }
    }
}
impl From<std::io::Error> for ActiveServiceWalErrorV1 {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
impl From<crate::storage::StorageError> for ActiveServiceWalErrorV1 {
    fn from(e: crate::storage::StorageError) -> Self {
        Self::Storage(e)
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActiveServiceWalBlobV1 {
    pub bytes: u64,
    pub sha256: [u8; 32],
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActiveServiceWalPinV1 {
    pub profile: String,
    pub root: PathBuf,
    pub prefix: VerifiedJournalPrefixV1,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActiveServiceWalResolvedV1 {
    Before,
    Post,
}
/// Sealed from a healthy actual store. Not constructible from persisted claims.
pub struct PreparedActiveServiceWalWriteV1 {
    owner: u64,
    before: ActiveServiceWalPinV1,
    post: ActiveServiceWalPinV1,
    record: Vec<u8>,
    blobs: Vec<Vec<u8>>,
    references: Vec<ActiveServiceWalBlobV1>,
    attempted: bool,
}
impl PreparedActiveServiceWalWriteV1 {
    pub fn before(&self) -> &ActiveServiceWalPinV1 {
        &self.before
    }
    pub fn post(&self) -> &ActiveServiceWalPinV1 {
        &self.post
    }
    pub fn references(&self) -> &[ActiveServiceWalBlobV1] {
        &self.references
    }
    pub fn record(&self) -> Result<V> {
        parse(&self.record)
    }
    pub fn attempted(&self) -> bool {
        self.attempted
    }
}
pub struct ActiveServiceWalV1 {
    root: PathBuf,
    namespace: Namespace,
    journal: Option<Journal>,
    context: Vec<u8>,
    filesystem: SourceDurableFilesystem,
    owner: u64,
    ambiguous: bool,
    #[cfg(test)]
    fault: Option<TestFault>,
    #[cfg(test)]
    commit_probe: Option<fn(&PreparedActiveServiceWalWriteV1, bool)>,
}
#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum TestFault {
    BeforeCommit,
    AfterCommit,
}
fn s(v: impl Into<String>) -> V {
    V::String(v.into())
}
fn obj(fields: &[(&str, V)]) -> V {
    V::Object(
        fields
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
    )
}
fn hex(b: &[u8]) -> String {
    b.iter().map(|b| format!("{b:02x}")).collect()
}
fn bytes(v: &V) -> Result<Vec<u8>> {
    v.diagnostic_json()
        .map(String::into_bytes)
        .map_err(|_| ActiveServiceWalErrorV1::InvalidRecord)
}
fn parse(b: &[u8]) -> Result<V> {
    let value = crate::property_catalog::parse_diagnostic_value(
        std::str::from_utf8(b).map_err(|_| ActiveServiceWalErrorV1::InvalidRecord)?,
    )
    .map_err(|_| ActiveServiceWalErrorV1::InvalidRecord)?;
    if bytes(&value)? != b {
        return Err(ActiveServiceWalErrorV1::InvalidRecord);
    }
    Ok(value)
}
fn directory(p: &Path) -> Result<PathBuf> {
    let meta = fs::symlink_metadata(p)?;
    if meta.file_type().is_symlink() || !meta.is_dir() {
        return Err(ActiveServiceWalErrorV1::WrongContext);
    }
    Ok(p.canonicalize()?)
}
fn sync_dir(mode: SourceDurableFilesystem, p: &Path) -> Result<()> {
    if mode == SourceDurableFilesystem::FileAndDirectorySync {
        #[cfg(unix)]
        File::open(p)?.sync_all()?;
        #[cfg(not(unix))]
        {
            let _ = p;
            return Err(ActiveServiceWalErrorV1::UnsupportedFilesystem);
        }
    }
    Ok(())
}
fn complete(entries: &BTreeMap<String, Vec<u8>>) -> Result<JournalCompleteKvReceiptV1> {
    let mut r = JournalCompleteKvReceiptV1 {
        profile: COMPLETE_KV_RECEIPT_PROFILE.into(),
        keys: 0,
        bytes: 0,
        chain_sha256d: double_sha256(COMPLETE_KV_RECEIPT_PROFILE.as_bytes()),
        complete: false,
    };
    for (k, v) in entries {
        let kl = u32::try_from(k.len())
            .map_err(|_| ActiveServiceWalErrorV1::Capacity)?
            .to_be_bytes();
        r.chain_sha256d = double_sha256_parts(&[
            &r.chain_sha256d,
            &r.keys.to_be_bytes(),
            &kl,
            k.as_bytes(),
            &(v.len() as u64).to_be_bytes(),
            v,
        ]);
        r.keys = r
            .keys
            .checked_add(1)
            .ok_or(ActiveServiceWalErrorV1::Capacity)?;
        r.bytes = r
            .bytes
            .checked_add(k.len() as u64)
            .and_then(|n| n.checked_add(v.len() as u64))
            .ok_or(ActiveServiceWalErrorV1::Capacity)?;
    }
    r.complete = true;
    Ok(r)
}
impl ActiveServiceWalBlobV1 {
    pub fn of(raw: &[u8]) -> Result<Self> {
        if raw.is_empty() || raw.len() > MAX_BLOB {
            return Err(ActiveServiceWalErrorV1::Capacity);
        }
        Ok(Self {
            bytes: raw.len() as u64,
            sha256: sha256(raw),
        })
    }
    pub fn value(&self) -> V {
        obj(&[
            ("bytes", s(self.bytes.to_string())),
            ("sha256", s(hex(&self.sha256))),
        ])
    }
}
impl ActiveServiceWalV1 {
    #[cfg(test)]
    pub(crate) fn set_test_commit_probe(
        &mut self,
        probe: fn(&PreparedActiveServiceWalWriteV1, bool),
    ) {
        self.commit_probe = Some(probe);
    }

    #[cfg(test)]
    pub(crate) fn set_test_fault(&mut self, fault: TestFault) {
        self.fault = Some(fault);
    }
    pub fn create(
        root: &Path,
        namespace: Namespace,
        origin: V,
        filesystem: SourceDurableFilesystem,
    ) -> Result<Self> {
        let root = directory(root)?;
        sync_dir(filesystem, &root)?;
        let owner = NEXT
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| ActiveServiceWalErrorV1::Capacity)?;
        let journal_path = namespace.path(&root);
        let parent = journal_path
            .parent()
            .ok_or(ActiveServiceWalErrorV1::WrongContext)?;
        if !parent.exists() {
            fs::create_dir(parent)?;
        }
        if directory(parent)? != parent {
            return Err(ActiveServiceWalErrorV1::WrongContext);
        }
        sync_dir(filesystem, &root)?;
        let (mut journal, _) =
            Journal::open(&root, namespace.clone(), Recovery::RejectIncompleteTail)?;
        sync_dir(filesystem, parent)?;
        if journal.sequence()? != 0 || journal.entries()?.next().is_some() {
            return Err(ActiveServiceWalErrorV1::WrongContext);
        }
        let blob_dir = root.join(BLOBS);
        fs::create_dir(&blob_dir)?;
        sync_dir(filesystem, &root)?;
        let context = bytes(&obj(&[
            ("profile", s(PROFILE)),
            ("origin", origin),
            ("filesystem", s(format!("{filesystem:?}"))),
            (
                "creation",
                s(format!(
                    "{}:{}:{}",
                    std::process::id(),
                    owner,
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_err(|_| ActiveServiceWalErrorV1::WrongContext)?
                        .as_nanos()
                )),
            ),
        ]))?;
        let initial = bytes(&obj(&[("profile", s(PROFILE)), ("phase", s("Created"))]))?;
        journal.commit(vec![
            Mutation::Put {
                key: CONTEXT.into(),
                value: context.clone(),
            },
            Mutation::Put {
                key: RECORD.into(),
                value: initial,
            },
        ])?;
        sync_dir(filesystem, &root)?;
        Ok(Self {
            root,
            namespace,
            journal: Some(journal),
            context,
            filesystem,
            owner,
            ambiguous: false,
            #[cfg(test)]
            fault: None,
            #[cfg(test)]
            commit_probe: None,
        })
    }
    fn healthy(&self) -> Result<&Journal> {
        if self.ambiguous {
            return Err(ActiveServiceWalErrorV1::Ambiguous);
        }
        self.journal
            .as_ref()
            .ok_or(ActiveServiceWalErrorV1::Ambiguous)
    }
    pub fn pin(&self) -> Result<ActiveServiceWalPinV1> {
        Ok(ActiveServiceWalPinV1 {
            profile: PROFILE.into(),
            root: self.root.clone(),
            prefix: self.healthy()?.verified_prefix_pin()?,
        })
    }
    pub fn origin_context(&self) -> Result<V> {
        parse(&self.context)
    }
    pub fn record(&self) -> Result<V> {
        parse(
            self.healthy()?
                .get(RECORD)?
                .ok_or(ActiveServiceWalErrorV1::InvalidRecord)?,
        )
    }
    pub fn journal_path(&self) -> PathBuf {
        self.namespace.path(&self.root)
    }
    pub fn prepare(
        &self,
        expected: &ActiveServiceWalPinV1,
        record: V,
        blobs: Vec<Vec<u8>>,
        references: Vec<ActiveServiceWalBlobV1>,
    ) -> Result<PreparedActiveServiceWalWriteV1> {
        if self.pin()? != *expected {
            return Err(ActiveServiceWalErrorV1::WrongPin);
        }
        let record = bytes(&record)?;
        if blobs.len() > 16 || references.len() > 64 {
            return Err(ActiveServiceWalErrorV1::Capacity);
        }
        for raw in &blobs {
            ActiveServiceWalBlobV1::of(raw)?;
        }
        // Every retained dependency is pinned now. New blobs are verified at
        // write; already-existing referenced blobs must really be present.
        for pin in &references {
            if !blobs
                .iter()
                .any(|raw| ActiveServiceWalBlobV1::of(raw).ok().as_ref() == Some(pin))
            {
                self.read_blob(pin)?;
            }
        }
        let mut entries = self
            .healthy()?
            .entries()?
            .map(|(k, v)| (k.to_owned(), v.to_vec()))
            .collect::<BTreeMap<_, _>>();
        entries.insert(RECORD.into(), record.clone());
        let mut post = expected.clone();
        post.prefix.physical_sequence = post
            .prefix
            .physical_sequence
            .checked_add(1)
            .ok_or(ActiveServiceWalErrorV1::Capacity)?;
        post.prefix.content = complete(&entries)?;
        Ok(PreparedActiveServiceWalWriteV1 {
            owner: self.owner,
            before: expected.clone(),
            post,
            record,
            blobs,
            references,
            attempted: false,
        })
    }
    fn require_plan(&self, plan: &PreparedActiveServiceWalWriteV1) -> Result<()> {
        if plan.owner != self.owner
            || plan.before.root != self.root
            || plan.before.profile != PROFILE
            || plan.post.root != self.root
        {
            return Err(ActiveServiceWalErrorV1::WrongPlan);
        }
        Ok(())
    }
    pub fn attempt(
        &mut self,
        plan: &mut PreparedActiveServiceWalWriteV1,
    ) -> Result<ActiveServiceWalPinV1> {
        self.require_plan(plan)?;
        if plan.attempted {
            return Err(ActiveServiceWalErrorV1::AlreadyAttempted);
        }
        if self.pin()? != plan.before {
            return Err(ActiveServiceWalErrorV1::WrongPin);
        }
        plan.attempted = true;
        self.ambiguous = true;
        for raw in &plan.blobs {
            self.write_blob(raw)?;
        }
        #[cfg(test)]
        if self.fault == Some(TestFault::BeforeCommit) {
            self.fault = None;
            return Err(std::io::Error::other("actual WAL before commit").into());
        }
        #[cfg(test)]
        if let Some(probe) = self.commit_probe {
            probe(plan, false);
        }
        self.journal
            .as_mut()
            .ok_or(ActiveServiceWalErrorV1::Ambiguous)?
            .commit(vec![Mutation::Put {
                key: RECORD.into(),
                value: plan.record.clone(),
            }])?;
        #[cfg(test)]
        if let Some(probe) = self.commit_probe {
            probe(plan, true);
        }
        #[cfg(test)]
        if self.fault == Some(TestFault::AfterCommit) {
            self.fault = None;
            return Err(std::io::Error::other("actual WAL synced, return lost").into());
        }
        let post = ActiveServiceWalPinV1 {
            profile: PROFILE.into(),
            root: self.root.clone(),
            prefix: self
                .journal
                .as_ref()
                .ok_or(ActiveServiceWalErrorV1::Ambiguous)?
                .verified_prefix_pin()?,
        };
        if post != plan.post {
            return Err(ActiveServiceWalErrorV1::WrongPin);
        }
        for reference in &plan.references {
            self.read_blob_unchecked(reference)?;
        }
        self.ambiguous = false;
        Ok(post)
    }
    pub fn resolve(
        &mut self,
        plan: &PreparedActiveServiceWalWriteV1,
    ) -> Result<ActiveServiceWalResolvedV1> {
        self.require_plan(plan)?;
        if !plan.attempted {
            return Err(ActiveServiceWalErrorV1::WrongPlan);
        }
        // Drop only the uncertain WAL writer; source/index/frame stay owned by
        // the containing controller. Expected pins were sealed BEFORE write.
        self.ambiguous = true;
        drop(self.journal.take());
        for (candidate, result) in [
            (&plan.post, ActiveServiceWalResolvedV1::Post),
            (&plan.before, ActiveServiceWalResolvedV1::Before),
        ] {
            if let Ok((journal, _)) = Journal::open_existing_verified_prefix(
                &self.root,
                self.namespace.clone(),
                Recovery::DiscardIncompleteTail,
                &candidate.prefix,
            ) {
                if journal.get(CONTEXT)? != Some(self.context.as_slice()) {
                    return Err(ActiveServiceWalErrorV1::WrongContext);
                }
                self.journal = Some(journal);
                self.ambiguous = false;
                if result == ActiveServiceWalResolvedV1::Post {
                    for reference in &plan.references {
                        if let Err(error) = self.read_blob(reference) {
                            self.ambiguous = true;
                            return Err(error);
                        }
                    }
                }
                return Ok(result);
            }
        }
        Err(ActiveServiceWalErrorV1::WrongPin)
    }
    pub fn rearm_before(
        &self,
        plan: &PreparedActiveServiceWalWriteV1,
    ) -> Result<PreparedActiveServiceWalWriteV1> {
        self.require_plan(plan)?;
        if !plan.attempted || self.pin()? != plan.before {
            return Err(ActiveServiceWalErrorV1::WrongPin);
        }
        self.prepare(
            &plan.before,
            parse(&plan.record)?,
            plan.blobs.clone(),
            plan.references.clone(),
        )
    }
    fn write_blob(&self, raw: &[u8]) -> Result<ActiveServiceWalBlobV1> {
        let pin = ActiveServiceWalBlobV1::of(raw)?;
        let dir = directory(&self.root.join(BLOBS))?;
        let target = dir.join(format!("{}.asw1", hex(&pin.sha256)));
        if target.exists() {
            self.read_blob_unchecked(&pin)?;
            sync_dir(self.filesystem, &dir)?;
            return Ok(pin);
        }
        let tmp = dir.join(format!(
            "pending-{}-{}.tmp",
            std::process::id(),
            NEXT.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
                .map_err(|_| ActiveServiceWalErrorV1::Capacity)?
        ));
        let mut file = OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        file.write_all(raw)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp, &target)?;
        sync_dir(self.filesystem, &dir)?;
        Ok(pin)
    }
    fn read_blob_unchecked(&self, pin: &ActiveServiceWalBlobV1) -> Result<Vec<u8>> {
        if pin.bytes == 0 || pin.bytes > MAX_BLOB as u64 {
            return Err(ActiveServiceWalErrorV1::Capacity);
        }
        let dir = directory(&self.root.join(BLOBS))?;
        let path = dir.join(format!("{}.asw1", hex(&pin.sha256)));
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() != pin.bytes {
            return Err(ActiveServiceWalErrorV1::InvalidRecord);
        }
        let mut bytes = Vec::new();
        File::open(path)?
            .take(pin.bytes + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 != pin.bytes || sha256(&bytes) != pin.sha256 {
            return Err(ActiveServiceWalErrorV1::InvalidRecord);
        }
        Ok(bytes)
    }
    pub fn read_blob(&self, pin: &ActiveServiceWalBlobV1) -> Result<Vec<u8>> {
        self.healthy()?;
        self.read_blob_unchecked(pin)
    }
}

include!("source_main_active_service_cold_history_impl.rs");
include!("source_main_mode4_cold_history_impl.rs");
include!("source_main_mode4_inline_closure_impl.rs");
include!("source_main_mode4_checked_read_impl.rs");
include!("source_main_mode4_pending_facts_impl.rs");
#[cfg(test)]
#[path = "source_main_mode4_cold_history_tests.rs"]
mod mode4_cold_history_tests;

#[cfg(test)]
#[path = "source_main_active_service_wal_tests.rs"]
mod tests;
