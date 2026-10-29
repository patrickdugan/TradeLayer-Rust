//! Explicit metadata intent/accepted/ack barriers. No financial owner lives here.
use crate::compact_journal::{CompactingJournal, NativeAppendPin, NativeAppendPlan};
use crate::hash256::sha256;
use crate::source_contract_replay::source_durable::SourceDurableFilesystem;
use crate::storage::{Journal, Mutation, Namespace, Recovery};
use crate::validity_source::SourceValue as V;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
pub const NATIVE_INTENT_STORE_PROFILE: &str = "TL-MAIN-NATIVE-INTENT-ACK-STORE-13-2";
const CONTEXT: &str = "native13/context";
const STATE: &str = "native13/state";
const BLOBS: &str = "native13-control-blobs";
const IDENTITY: &str = "native13-control-identity";
const MAX_BLOB: usize = 64 * 1024 * 1024;
static NEXT_TOKEN: AtomicU64 = AtomicU64::new(1);
static NEXT_TEMP: AtomicU64 = AtomicU64::new(1);
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeIntentStorePinV1 {
    pub canonical_root: PathBuf,
    pub context: Vec<u8>,
    pub sequence: u64,
    pub state_sha256: [u8; 32],
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeIntentBlobV1 {
    pub bytes: u64,
    pub sha256: [u8; 32],
}
pub struct NativeCommandIntentPermitV1 {
    token: u64,
    pin: NativeIntentStorePinV1,
    command: NativeIntentBlobV1,
    before: V,
    receipt_budget: usize,
    reservation: Vec<u8>,
    native_prefix: Option<NativeIntentBlobV1>,
    pending_native: Option<(NativeIntentBlobV1, NativeWriteIntentCandidatesV1)>,
    previous_ack: V,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeWriteIntentCandidatesV1 {
    facts: V,
    post: V,
    raw_mutations: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeObservedAppendBindingV1 {
    binding: V,
}
impl NativeObservedAppendBindingV1 {
    pub fn observe(journal: &CompactingJournal) -> Result<Self> {
        let (identity, catalog) = journal.native_main_store_context()?;
        let pin = NativeAppendPin {
            publication: journal.publication()?.clone(),
            logical_sequence: journal.sequence()?,
        };
        Ok(Self {
            binding: obj(&[
                ("identity", text(&identity)),
                ("catalog", text(&catalog.to_string_lossy())),
                ("append", native_pin_value(&pin)),
            ]),
        })
    }
    pub fn value(&self) -> &V {
        &self.binding
    }
}
fn native_pin_value(pin: &NativeAppendPin) -> V {
    let p = &pin.publication;
    obj(&[
        ("logicalSequence", text(&pin.logical_sequence.to_string())),
        ("generation", text(&p.generation.to_string())),
        ("parent", p.parent.map_or(V::Null, |v| text(&v.to_string()))),
        (
            "physicalCreation",
            text(&p.physical_creation_sequence.to_string()),
        ),
        ("logicalCreation", text(&p.creation_sequence.to_string())),
        ("manifest", text(&hex(&p.manifest_sha256d))),
    ])
}

pub(crate) fn native_prepared_mutation_bytes(mutations: &[Mutation]) -> Result<Vec<u8>> {
    let mut raw_mutations = b"TLNM\x01".to_vec();
    raw_mutations.extend_from_slice(&(mutations.len() as u32).to_be_bytes());
    for mutation in mutations {
        let (tag, key, value) = match mutation {
            Mutation::Put { key, value } => (0u8, key, value.as_slice()),
            Mutation::Delete { key } => (1u8, key, &[][..]),
        };
        raw_mutations.push(tag);
        for part in [key.as_bytes(), value] {
            raw_mutations.extend_from_slice(&(part.len() as u32).to_be_bytes());
            raw_mutations.extend_from_slice(part);
        }
    }
    if raw_mutations.len() > MAX_BLOB {
        return Err(NativeIntentStoreError::Capacity);
    }
    Ok(raw_mutations)
}

impl NativeWriteIntentCandidatesV1 {
    /// Actual opaque plan input. No constructor from a checksum/file record.
    pub fn from_prepared_append(plan: &NativeAppendPlan, read_context: V) -> Result<Self> {
        let (identity, catalog) = plan.catalog_context();
        let post = obj(&[
            ("identity", text(identity)),
            ("catalog", text(&catalog.to_string_lossy())),
            ("append", native_pin_value(plan.after())),
        ]);
        let raw_mutations = native_prepared_mutation_bytes(plan.mutations())?;
        let facts = obj(&[
            ("profile", text("TL-MAIN-NATIVE-PREPARED-INTENT-13-1")),
            ("identity", text(identity)),
            ("catalog", text(&catalog.to_string_lossy())),
            ("before", native_pin_value(plan.before())),
            (
                "intermediate",
                plan.intermediate()
                    .as_ref()
                    .map_or(V::Null, native_pin_value),
            ),
            ("post", post.clone()),
            (
                "targetLogBytes",
                text(&plan.configuration().target_log_bytes.to_string()),
            ),
            (
                "retainedAncestors",
                text(&plan.configuration().retained_ancestors.to_string()),
            ),
            ("readContext", read_context),
        ]);
        bytes(&facts)?;
        Ok(Self {
            facts,
            post,
            raw_mutations,
        })
    }
    pub fn independently_retained_facts(&self) -> &V {
        &self.facts
    }
    pub fn expected_post(&self) -> &V {
        &self.post
    }
    pub fn exact_prepared_mutations(&self) -> &[u8] {
        &self.raw_mutations
    }
}
impl NativeCommandIntentPermitV1 {
    pub fn independently_retained_pin(&self) -> &NativeIntentStorePinV1 {
        &self.pin
    }
    pub fn command(&self) -> &NativeIntentBlobV1 {
        &self.command
    }
}
#[derive(Debug)]
pub enum NativeIntentStoreError {
    Io(std::io::Error),
    Storage(crate::storage::StorageError),
    WrongContext,
    WrongPin,
    ForeignOrReopenedPermit,
    WrongPhase,
    Capacity,
    InvalidRecord,
    Poisoned,
    UnsupportedFilesystem,
}
impl std::fmt::Display for NativeIntentStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for NativeIntentStoreError {}
impl From<std::io::Error> for NativeIntentStoreError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
impl From<crate::storage::StorageError> for NativeIntentStoreError {
    fn from(e: crate::storage::StorageError) -> Self {
        Self::Storage(e)
    }
}
type Result<T> = std::result::Result<T, NativeIntentStoreError>;
pub struct NativeIntentStoreV1 {
    journal: Journal,
    root: PathBuf,
    context: Vec<u8>,
    token: u64,
    filesystem: SourceDurableFilesystem,
    superseding_profile: bool,
    poisoned: bool,
}
fn text(v: &str) -> V {
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
fn bytes(value: &V) -> Result<Vec<u8>> {
    value
        .diagnostic_json()
        .map(String::into_bytes)
        .map_err(|_| NativeIntentStoreError::InvalidRecord)
}
fn parse(raw: &[u8]) -> Result<V> {
    let input = std::str::from_utf8(raw).map_err(|_| NativeIntentStoreError::InvalidRecord)?;
    let value = crate::property_catalog::parse_diagnostic_value(input)
        .map_err(|_| NativeIntentStoreError::InvalidRecord)?;
    if bytes(&value)? != raw {
        return Err(NativeIntentStoreError::InvalidRecord);
    }
    Ok(value)
}
fn hex(value: &[u8]) -> String {
    value.iter().map(|v| format!("{v:02x}")).collect()
}
fn blob_value(blob: &NativeIntentBlobV1) -> V {
    obj(&[
        ("bytes", text(&blob.bytes.to_string())),
        ("sha256", text(&hex(&blob.sha256))),
    ])
}
fn directory(path: &Path) -> Result<PathBuf> {
    let meta = fs::symlink_metadata(path)?;
    if meta.file_type().is_symlink() || !meta.is_dir() {
        return Err(NativeIntentStoreError::WrongContext);
    }
    Ok(path.canonicalize()?)
}
fn journal_location(root: &Path, namespace: &Namespace, create: bool) -> Result<PathBuf> {
    let path = namespace.path(root);
    let parent = path.parent().ok_or(NativeIntentStoreError::WrongContext)?;
    if !parent.exists() && create {
        fs::create_dir(parent)?;
    }
    if directory(parent)? != parent {
        return Err(NativeIntentStoreError::WrongContext);
    }
    match fs::symlink_metadata(&path) {
        Ok(meta) if meta.file_type().is_symlink() || !meta.is_file() => {
            return Err(NativeIntentStoreError::WrongContext)
        }
        Ok(_) => {
            if path.canonicalize()? != path {
                return Err(NativeIntentStoreError::WrongContext);
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && create => (),
        Err(error) => return Err(error.into()),
    }
    Ok(path)
}
fn persistent_identity(root: &Path, create: bool) -> Result<String> {
    let path = root.join(IDENTITY);
    if !path.exists() && create {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| NativeIntentStoreError::WrongContext)?
            .as_nanos();
        let value = format!(
            "TL-NATIVE-CONTROL-IDENTITY-1:{}:{nanos}:{}",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        );
        match OpenOptions::new().create_new(true).write(true).open(&path) {
            Ok(mut file) => {
                file.write_all(value.as_bytes())?;
                file.sync_all()?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (),
            Err(error) => return Err(error.into()),
        }
    }
    let meta = fs::symlink_metadata(&path)?;
    if meta.file_type().is_symlink() || !meta.is_file() || meta.len() > 1024 {
        return Err(NativeIntentStoreError::WrongContext);
    }
    let value = fs::read_to_string(&path)?;
    if !value.starts_with("TL-NATIVE-CONTROL-IDENTITY-1:") || value.len() > 1024 {
        return Err(NativeIntentStoreError::WrongContext);
    }
    Ok(value)
}
fn sync_directory(profile: SourceDurableFilesystem, path: &Path) -> Result<()> {
    if profile == SourceDurableFilesystem::FileAndDirectorySync {
        #[cfg(unix)]
        {
            File::open(path)?.sync_all()?;
        }
        #[cfg(not(unix))]
        {
            let _ = path;
            return Err(NativeIntentStoreError::UnsupportedFilesystem);
        }
    }
    Ok(())
}
impl NativeIntentStoreV1 {
    pub fn create(
        root: &Path,
        namespace: Namespace,
        context: V,
        initial: V,
        filesystem: SourceDurableFilesystem,
    ) -> Result<Self> {
        let root = directory(root)?;
        // Refuse an unsupported durability promise before creating store files.
        sync_directory(filesystem, &root)?;
        let journal_path = journal_location(&root, &namespace, true)?;
        let identity = persistent_identity(&root, true)?;
        let context = bytes(&obj(&[
            ("profile", text(NATIVE_INTENT_STORE_PROFILE)),
            ("identity", text(&identity)),
            ("journalPath", text(&journal_path.to_string_lossy())),
            ("context", context),
        ]))?;
        if context.len() > 65536 {
            return Err(NativeIntentStoreError::Capacity);
        }
        let (mut journal, _) = Journal::open(&root, namespace, Recovery::RejectIncompleteTail)?;
        if journal.sequence()? != 0 || journal.entries()?.next().is_some() {
            return Err(NativeIntentStoreError::WrongContext);
        }
        let dir = root.join(BLOBS);
        if !dir.exists() {
            fs::create_dir(&dir)?;
        }
        directory(&dir)?;
        let state = bytes(&obj(&[("kind", text("Idle")), ("accepted", initial)]))?;
        journal.commit(vec![
            Mutation::Put {
                key: CONTEXT.into(),
                value: context.clone(),
            },
            Mutation::Put {
                key: STATE.into(),
                value: state,
            },
        ])?;
        sync_directory(filesystem, &root)?;
        Ok(Self {
            journal,
            root,
            context,
            token: NEXT_TOKEN.fetch_add(1, Ordering::Relaxed),
            filesystem,
            superseding_profile: false,
            poisoned: false,
        })
    }
    /// Exact independently retained pin only; never truncate/repair or select
    /// a newer prefix merely because its bytes have a valid checksum.
    fn open_profile(
        root: &Path,
        namespace: Namespace,
        expected: &NativeIntentStorePinV1,
        filesystem: SourceDurableFilesystem,
        superseding_profile: bool,
    ) -> Result<Self> {
        let root = directory(root)?;
        if root != expected.canonical_root {
            return Err(NativeIntentStoreError::WrongContext);
        }
        let journal_path = journal_location(&root, &namespace, false)?;
        let context = parse(&expected.context)?;
        if context.get("profile") != text(NATIVE_INTENT_STORE_PROFILE)
            || context.get("journalPath") != text(&journal_path.to_string_lossy())
            || context.get("identity") != text(&persistent_identity(&root, false)?)
        {
            return Err(NativeIntentStoreError::WrongContext);
        }
        sync_directory(filesystem, &root)?;
        let (journal, _) =
            Journal::open_existing(&root, namespace, Recovery::RejectIncompleteTail)?;
        if journal.get(CONTEXT)? != Some(expected.context.as_slice())
            || journal.entries()?.any(|(k, _)| k != CONTEXT && k != STATE)
        {
            return Err(NativeIntentStoreError::WrongContext);
        }
        let state = journal
            .get(STATE)?
            .ok_or(NativeIntentStoreError::InvalidRecord)?;
        let decoded = parse(state)?;
        if superseding_profile {
            if decoded.get("operationProfile") != text(NATIVE_SUPERSEDING_INTENT_PROFILE)
                || ![
                    "Idle",
                    "CommandIntent",
                    "NativeIntent",
                    "Accepted",
                    SUPERSEDING_KIND,
                ]
                .iter()
                .any(|kind| decoded.get("kind") == text(kind))
            {
                return Err(NativeIntentStoreError::InvalidRecord);
            }
        } else if decoded.get("operationProfile") != V::Undefined
            || decoded.get("kind") == text(SUPERSEDING_KIND)
        {
            return Err(NativeIntentStoreError::WrongPhase);
        }
        if journal.sequence()? != expected.sequence || sha256(state) != expected.state_sha256 {
            return Err(NativeIntentStoreError::WrongPin);
        }
        directory(&root.join(BLOBS))?;
        Ok(Self {
            journal,
            root,
            context: expected.context.clone(),
            token: NEXT_TOKEN.fetch_add(1, Ordering::Relaxed),
            filesystem,
            superseding_profile,
            poisoned: false,
        })
    }
    fn health(&self) -> Result<()> {
        if self.poisoned {
            return Err(NativeIntentStoreError::Poisoned);
        }
        self.journal.sequence()?;
        Ok(())
    }
    pub fn pin(&self) -> Result<NativeIntentStorePinV1> {
        self.health()?;
        let state = self
            .journal
            .get(STATE)?
            .ok_or(NativeIntentStoreError::InvalidRecord)?;
        Ok(NativeIntentStorePinV1 {
            canonical_root: self.root.clone(),
            context: self.context.clone(),
            sequence: self.journal.sequence()?,
            state_sha256: sha256(state),
        })
    }
    pub fn state(&self) -> Result<V> {
        self.health()?;
        parse(
            self.journal
                .get(STATE)?
                .ok_or(NativeIntentStoreError::InvalidRecord)?,
        )
    }
    pub fn journal_path(&self) -> &Path {
        self.journal.path()
    }
    fn write_state(&mut self, mut state: V) -> Result<()> {
        self.health()?;
        if self.superseding_profile {
            mark_superseding_state(&mut state)?;
        }
        let raw = bytes(&state)?;
        self.poisoned = true;
        self.journal.commit(vec![Mutation::Put {
            key: STATE.into(),
            value: raw,
        }])?;
        self.poisoned = false;
        Ok(())
    }
    fn write_blob(&self, raw: &[u8]) -> Result<NativeIntentBlobV1> {
        self.health()?;
        if raw.is_empty() || raw.len() > MAX_BLOB {
            return Err(NativeIntentStoreError::Capacity);
        }
        let blob = NativeIntentBlobV1 {
            bytes: raw.len() as u64,
            sha256: sha256(raw),
        };
        let dir = directory(&self.root.join(BLOBS))?;
        let target = dir.join(format!("{}.tlc13", hex(&blob.sha256)));
        if target.exists() {
            if self.read_blob(&blob)? != raw {
                return Err(NativeIntentStoreError::InvalidRecord);
            }
            sync_directory(self.filesystem, &dir)?;
            return Ok(blob);
        }
        let temporary = dir.join(format!(
            "pending-{}-{}.tmp",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)?;
        file.write_all(raw)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, &target)?;
        sync_directory(self.filesystem, &dir)?;
        Ok(blob)
    }
    pub fn read_blob(&self, blob: &NativeIntentBlobV1) -> Result<Vec<u8>> {
        self.health()?;
        if blob.bytes == 0 || blob.bytes > MAX_BLOB as u64 {
            return Err(NativeIntentStoreError::Capacity);
        }
        let dir = directory(&self.root.join(BLOBS))?;
        let path = dir.join(format!("{}.tlc13", hex(&blob.sha256)));
        let meta = fs::symlink_metadata(&path)?;
        if meta.file_type().is_symlink() || !meta.is_file() || meta.len() != blob.bytes {
            return Err(NativeIntentStoreError::InvalidRecord);
        }
        let mut raw = Vec::new();
        File::open(&path)?
            .take(blob.bytes + 1)
            .read_to_end(&mut raw)?;
        if raw.len() as u64 != blob.bytes || sha256(&raw) != blob.sha256 {
            return Err(NativeIntentStoreError::InvalidRecord);
        }
        Ok(raw)
    }
    fn check(&self, permit: &NativeCommandIntentPermitV1) -> Result<()> {
        self.health()?;
        if permit.token != self.token || permit.pin != self.pin()? {
            return Err(NativeIntentStoreError::ForeignOrReopenedPermit);
        }
        Ok(())
    }
    pub fn sync_command_intent(
        &mut self,
        expected: &NativeIntentStorePinV1,
        raw_command: &[u8],
        before: V,
        receipt_budget: usize,
    ) -> Result<NativeCommandIntentPermitV1> {
        self.health()?;
        if self.pin()? != *expected {
            return Err(NativeIntentStoreError::WrongPin);
        }
        let idle = self.state()?;
        if idle.get("kind") != text("Idle") {
            return Err(NativeIntentStoreError::WrongPhase);
        }
        if receipt_budget == 0 || receipt_budget > MAX_BLOB {
            return Err(NativeIntentStoreError::Capacity);
        }
        // Reserve the complete declared host record budget before source work.
        // This is memory reservation, not a promise of free filesystem space.
        let mut reservation = Vec::<u8>::new();
        reservation
            .try_reserve_exact(receipt_budget)
            .map_err(|_| NativeIntentStoreError::Capacity)?;
        let command = self.write_blob(raw_command)?;
        self.write_state(obj(&[
            ("kind", text("CommandIntent")),
            ("command", blob_value(&command)),
            ("before", before.clone()),
            ("receiptBudget", text(&receipt_budget.to_string())),
            ("previousAck", idle.get("ack")),
        ]))?;
        Ok(NativeCommandIntentPermitV1 {
            token: self.token,
            pin: self.pin()?,
            command,
            before,
            receipt_budget,
            reservation,
            native_prefix: None,
            pending_native: None,
            previous_ack: idle.get("ack"),
        })
    }
    /// `candidates` must be derived from the actual sealed native append plan
    /// by the controller. The returned pin is retained BEFORE native mutation.
    pub fn sync_native_intent(
        &mut self,
        permit: &mut NativeCommandIntentPermitV1,
        candidates: &NativeWriteIntentCandidatesV1,
    ) -> Result<NativeIntentStorePinV1> {
        self.check(permit)?;
        if self.state()?.get("kind") != text("CommandIntent") || permit.pending_native.is_some() {
            return Err(NativeIntentStoreError::WrongPhase);
        }
        let mutations = self.write_blob(&candidates.raw_mutations)?;
        let raw = bytes(&obj(&[
            ("command", blob_value(&permit.command)),
            ("candidates", candidates.facts.clone()),
            ("mutations", blob_value(&mutations)),
            (
                "previousNativePrefix",
                permit.native_prefix.as_ref().map_or(V::Null, blob_value),
            ),
        ]))?;
        let intent = self.write_blob(&raw)?;
        self.write_state(obj(&[
            ("kind", text("NativeIntent")),
            ("command", blob_value(&permit.command)),
            ("before", permit.before.clone()),
            ("receiptBudget", text(&permit.receipt_budget.to_string())),
            ("nativeIntent", blob_value(&intent)),
            ("previousAck", permit.previous_ack.clone()),
        ]))?;
        permit.pending_native = Some((intent, candidates.clone()));
        permit.pin = self.pin()?;
        Ok(permit.pin.clone())
    }
    /// Only an independently observed exact post binding resolves this await.
    /// Unknown/intermediate/before retain the pending native intent unchanged.
    pub fn sync_native_prefix_ack(
        &mut self,
        permit: &mut NativeCommandIntentPermitV1,
        observed: &NativeObservedAppendBindingV1,
    ) -> Result<NativeIntentStorePinV1> {
        self.check(permit)?;
        if self.state()?.get("kind") != text("NativeIntent") {
            return Err(NativeIntentStoreError::WrongPhase);
        }
        let (intent, candidates) = permit
            .pending_native
            .as_ref()
            .ok_or(NativeIntentStoreError::WrongPhase)?;
        if observed.binding != candidates.post {
            return Err(NativeIntentStoreError::WrongPin);
        }
        let raw = bytes(&obj(&[
            ("nativeIntent", blob_value(intent)),
            ("observedPost", observed.binding.clone()),
            (
                "previousNativePrefix",
                permit.native_prefix.as_ref().map_or(V::Null, blob_value),
            ),
        ]))?;
        let prefix = self.write_blob(&raw)?;
        self.write_state(obj(&[
            ("kind", text("CommandIntent")),
            ("command", blob_value(&permit.command)),
            ("before", permit.before.clone()),
            ("receiptBudget", text(&permit.receipt_budget.to_string())),
            ("nativePrefix", blob_value(&prefix)),
            ("previousAck", permit.previous_ack.clone()),
        ]))?;
        permit.native_prefix = Some(prefix);
        permit.pending_native = None;
        permit.pin = self.pin()?;
        Ok(permit.pin.clone())
    }
    pub fn sync_accepted(
        &mut self,
        permit: &mut NativeCommandIntentPermitV1,
        accepted_bytes: &[u8],
        after: V,
    ) -> Result<NativeIntentStorePinV1> {
        self.check(permit)?;
        if self.state()?.get("kind") != text("CommandIntent") || permit.pending_native.is_some() {
            return Err(NativeIntentStoreError::WrongPhase);
        }
        if accepted_bytes.len() > permit.receipt_budget {
            return Err(NativeIntentStoreError::Capacity);
        }
        if permit.reservation.capacity() < permit.receipt_budget {
            return Err(NativeIntentStoreError::Capacity);
        }
        let accepted = self.write_blob(accepted_bytes)?;
        self.write_state(obj(&[
            ("kind", text("Accepted")),
            ("command", blob_value(&permit.command)),
            ("before", permit.before.clone()),
            (
                "nativePrefix",
                permit.native_prefix.as_ref().map_or(V::Null, blob_value),
            ),
            ("accepted", blob_value(&accepted)),
            ("after", after),
            ("previousAck", permit.previous_ack.clone()),
        ]))?;
        permit.pin = self.pin()?;
        Ok(permit.pin.clone())
    }
    pub fn sync_ack(
        &mut self,
        permit: &mut NativeCommandIntentPermitV1,
    ) -> Result<NativeIntentStorePinV1> {
        self.check(permit)?;
        let state = self.state()?;
        if state.get("kind") != text("Accepted") {
            return Err(NativeIntentStoreError::WrongPhase);
        }
        let raw = bytes(&obj(&[
            ("command", state.get("command")),
            ("nativePrefix", state.get("nativePrefix")),
            ("acceptedBlob", state.get("accepted")),
            ("after", state.get("after")),
            ("previousAck", permit.previous_ack.clone()),
        ]))?;
        let ack = self.write_blob(&raw)?;
        self.write_state(obj(&[
            ("kind", text("Idle")),
            ("accepted", state.get("after")),
            ("ack", blob_value(&ack)),
        ]))?;
        permit.reservation = Vec::new();
        permit.pin = self.pin()?;
        Ok(permit.pin.clone())
    }
}

include!("source_main_native_superseding_intent_impl.rs");
#[cfg(test)]
#[path = "source_main_native_superseding_intent_tests.rs"]
mod superseding_intent_tests;

include!("source_main_native_prepared_prefix_ack_impl.rs");
include!("source_main_native_completed_barrier_impl.rs");
include!("source_main_native_completed_accepted_impl.rs");
include!("source_main_native_completed_command_ack_impl.rs");
include!("source_main_native_control_store_tests_impl.rs");
#[cfg(test)]
#[path = "source_main_native_prepared_prefix_ack_tests.rs"]
mod prepared_prefix_ack_tests;

include!("source_main_native_completed_ack_binding_impl.rs");

#[cfg(test)]
#[path = "source_main_native_completed_barrier_tests.rs"]
mod completed_barrier_tests;

#[cfg(test)]
#[path = "source_main_native_completed_empty_tests.rs"]
mod completed_empty_tests;
