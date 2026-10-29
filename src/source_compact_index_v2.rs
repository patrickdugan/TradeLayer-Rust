//! Paged literal TxIndex metadata over one exclusively locked native Journal.
//! Candidate discovery remains a separate, explicitly pinned input profile.
use crate::compact_candidate_snapshot::{
    CandidateSnapshotError, CompactCandidateSnapshot, StructurallyCompleteCandidateBlock,
};
use crate::compact_journal::{
    CatalogPublication, CompactReadSnapshot, CompactingJournal as Journal,
};
use crate::storage::{Mutation, Namespace, Recovery, StorageError};
use crate::validity_source::{
    DependencyCall, DependencyReturn, SourceBackend, SourceException, SourceRuleError,
    SourceValue as V,
};
use std::path::Path;

/// This preserves NeDB's ordinary *memory* values (including undefined) across
/// restart. It is deliberately distinct from NeDB's lossy JSON disk encoding.
pub const METADATA_PROFILE: &str = "TL-TXINDEX-SOURCE-MEMORY-PAGED-GENERATIONS-2";
pub const DISCOVERY_PROFILE: &str = "TL-TXINDEX-FIRST-NULLDATA-COMPLETE-RAW-1";
pub const PAGE_ROWS: usize = 64;
pub const MAX_PAGES: usize = 4094;
pub const MAX_ROW_BYTES: usize = 64 * 1024;
const DIRECTORY: &str = "source-txindex-directory";

#[derive(Debug)]
pub enum CompactIndexError {
    Storage(StorageError),
    Source(SourceRuleError),
    WrongContext,
    StaleSnapshot,
    Corrupt,
    Limit,
    ColdWriteRefused,
}
impl From<StorageError> for CompactIndexError {
    fn from(v: StorageError) -> Self {
        Self::Storage(v)
    }
}
impl From<SourceRuleError> for CompactIndexError {
    fn from(v: SourceRuleError) -> Self {
        Self::Source(v)
    }
}
impl std::fmt::Display for CompactIndexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for CompactIndexError {}
type Result<T> = std::result::Result<T, CompactIndexError>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetadataSnapshot {
    pub profile: String,
    /// Independently retained network/anchor/import identity, never authority.
    pub context: String,
    pub sequence: u64,
    pub publication: CatalogPublication,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetadataFootprint {
    pub rows: u64,
    pub pages: usize,
    pub publication_sequence: u64,
    pub live_key_and_value_bytes: u64,
    pub journal_file_bytes: u64,
}
#[derive(Clone, Debug)]
struct Page {
    number: u32,
    first: String,
}
#[derive(Clone, Debug)]
struct Directory {
    context: String,
    next_page: u32,
    rows: u64,
    pages: Vec<Page>,
}

/// One authoritative metadata store. There is no Core.tables mirror or Ledger.
/// The Journal owns both the page bytes and its publication sequence/OS lock.
pub struct SourceCompactIndex {
    journal: Journal,
    directory: Directory,
    cold_execution: Option<u64>,
    inline_escrow_guard: Option<u64>,
}
fn page_key(n: u32) -> String {
    format!("source-txindex-page-{n:08x}")
}
fn put_u32(out: &mut Vec<u8>, n: usize) -> Result<()> {
    out.extend_from_slice(
        &u32::try_from(n)
            .map_err(|_| CompactIndexError::Limit)?
            .to_be_bytes(),
    );
    Ok(())
}
fn put_text(out: &mut Vec<u8>, t: &str) -> Result<()> {
    put_u32(out, t.len())?;
    out.extend_from_slice(t.as_bytes());
    Ok(())
}
struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}
impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.at.checked_add(n).ok_or(CompactIndexError::Limit)?;
        let s = self
            .bytes
            .get(self.at..end)
            .ok_or(CompactIndexError::Corrupt)?;
        self.at = end;
        Ok(s)
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_be_bytes(
            self.take(4)?
                .try_into()
                .map_err(|_| CompactIndexError::Corrupt)?,
        ))
    }
    fn text(&mut self, limit: usize) -> Result<String> {
        let n = self.u32()? as usize;
        if n > limit {
            return Err(CompactIndexError::Limit);
        }
        String::from_utf8(self.take(n)?.to_vec()).map_err(|_| CompactIndexError::Corrupt)
    }
    fn end(&self) -> Result<()> {
        if self.at == self.bytes.len() {
            Ok(())
        } else {
            Err(CompactIndexError::Corrupt)
        }
    }
}
impl Directory {
    fn encode(&self) -> Result<Vec<u8>> {
        let mut out = b"TLTM\x01".to_vec();
        put_text(&mut out, METADATA_PROFILE)?;
        put_text(&mut out, &self.context)?;
        out.extend_from_slice(&self.next_page.to_be_bytes());
        out.extend_from_slice(&self.rows.to_be_bytes());
        put_u32(&mut out, self.pages.len())?;
        for p in &self.pages {
            out.extend_from_slice(&p.number.to_be_bytes());
            put_text(&mut out, &p.first)?;
        }
        Ok(out)
    }
    fn decode(bytes: &[u8]) -> Result<Self> {
        let mut c = Cursor { bytes, at: 0 };
        if c.take(5)? != b"TLTM\x01" || c.text(128)? != METADATA_PROFILE {
            return Err(CompactIndexError::WrongContext);
        }
        let context = c.text(4096)?;
        let next_page = c.u32()?;
        let rows = u64::from_be_bytes(
            c.take(8)?
                .try_into()
                .map_err(|_| CompactIndexError::Corrupt)?,
        );
        let n = c.u32()? as usize;
        if n > MAX_PAGES || rows > (MAX_PAGES * PAGE_ROWS) as u64 {
            return Err(CompactIndexError::Limit);
        }
        let mut pages = Vec::with_capacity(n);
        let mut ids = std::collections::BTreeSet::new();
        for _ in 0..n {
            let p = Page {
                number: c.u32()?,
                first: c.text(256)?,
            };
            valid_id(&p.first)?;
            if p.number >= next_page
                || !ids.insert(p.number)
                || pages
                    .last()
                    .is_some_and(|last: &Page| last.first >= p.first)
            {
                return Err(CompactIndexError::Corrupt);
            }
            pages.push(p);
        }
        c.end()?;
        if (rows == 0) != pages.is_empty() {
            return Err(CompactIndexError::Corrupt);
        }
        Ok(Self {
            context,
            next_page,
            rows,
            pages,
        })
    }
}
fn valid_id(id: &str) -> Result<()> {
    if id.is_empty() || id.len() > 256 || !id.bytes().all(|b| (0x20..=0x7e).contains(&b)) {
        return Err(SourceRuleError::UnsupportedNumericDomain.into());
    }
    Ok(())
}
fn row_id(row: &V) -> Result<String> {
    if let V::String(id) = row.get("_id") {
        valid_id(&id)?;
        Ok(id)
    } else {
        Err(SourceRuleError::UnsupportedNumericDomain.into())
    }
}
fn encode_page(rows: &[V]) -> Result<Vec<u8>> {
    if rows.is_empty() || rows.len() > PAGE_ROWS {
        return Err(CompactIndexError::Limit);
    }
    let mut out = b"TLTP\x01".to_vec();
    put_u32(&mut out, rows.len())?;
    for row in rows {
        profile_value(row)?;
        let text = row.diagnostic_json()?;
        if text.len() > MAX_ROW_BYTES {
            return Err(CompactIndexError::Limit);
        }
        put_text(&mut out, &text)?;
    }
    if out.len() > crate::storage::MAX_VALUE_BYTES {
        return Err(CompactIndexError::Limit);
    }
    Ok(out)
}
fn decode_page(bytes: &[u8]) -> Result<Vec<V>> {
    let mut c = Cursor { bytes, at: 0 };
    if c.take(5)? != b"TLTP\x01" {
        return Err(CompactIndexError::Corrupt);
    }
    let n = c.u32()? as usize;
    if n == 0 || n > PAGE_ROWS {
        return Err(CompactIndexError::Corrupt);
    }
    let mut rows = Vec::with_capacity(n);
    let mut previous = None;
    for _ in 0..n {
        let row = V::from_diagnostic_json(&c.text(MAX_ROW_BYTES)?)?;
        profile_value(&row)?;
        let id = row_id(&row)?;
        if previous.as_ref().is_some_and(|p: &String| p >= &id) {
            return Err(CompactIndexError::Corrupt);
        }
        previous = Some(id);
        rows.push(row);
    }
    c.end()?;
    Ok(rows)
}
impl SourceCompactIndex {
    pub fn create(root: &Path, namespace: Namespace, context: &str) -> Result<Self> {
        require_namespace(root, &namespace)?;
        if context.is_empty() || context.len() > 4096 {
            return Err(CompactIndexError::WrongContext);
        }
        let (mut journal, _) = Journal::open(root, namespace, Recovery::RejectIncompleteTail)?;
        if journal.entries()?.next().is_some() {
            return Err(CompactIndexError::WrongContext);
        }
        let directory = Directory {
            context: context.into(),
            next_page: 0,
            rows: 0,
            pages: vec![],
        };
        journal.commit(vec![Mutation::Put {
            key: DIRECTORY.into(),
            value: directory.encode()?,
        }])?;
        Ok(Self {
            journal,
            directory,
            cold_execution: None,
            inline_escrow_guard: None,
        })
    }
    pub fn open(root: &Path, namespace: Namespace, expected: &MetadataSnapshot) -> Result<Self> {
        require_namespace(root, &namespace)?;
        let (journal, _) = Journal::open_existing(root, namespace, Recovery::RejectIncompleteTail)?;
        let directory =
            Directory::decode(journal.get(DIRECTORY)?.ok_or(CompactIndexError::Corrupt)?)?;
        let result = Self {
            journal,
            directory,
            cold_execution: None,
            inline_escrow_guard: None,
        };
        result.require_snapshot(expected)?;
        result.audit()?;
        Ok(result)
    }
    pub fn snapshot(&self) -> Result<MetadataSnapshot> {
        Ok(MetadataSnapshot {
            profile: METADATA_PROFILE.into(),
            context: self.directory.context.clone(),
            sequence: self.journal.sequence()?,
            publication: self.journal.publication()?.clone(),
        })
    }
    /// Actual retained bytes, not an estimated capacity or throughput claim.
    pub fn footprint(&self) -> Result<MetadataFootprint> {
        let sequence = self.journal.sequence()?;
        let live = self
            .journal
            .entries()?
            .map(|(k, v)| (k.len() + v.len()) as u64)
            .sum();
        let bytes = std::fs::metadata(self.journal.path())
            .map_err(StorageError::Io)?
            .len();
        Ok(MetadataFootprint {
            rows: self.directory.rows,
            pages: self.directory.pages.len(),
            publication_sequence: sequence,
            live_key_and_value_bytes: live,
            journal_file_bytes: bytes,
        })
    }
    pub fn require_snapshot(&self, expected: &MetadataSnapshot) -> Result<()> {
        let current = self.snapshot()?;
        if current.profile != expected.profile || current.context != expected.context {
            return Err(CompactIndexError::WrongContext);
        }
        if current.sequence != expected.sequence || current.publication != expected.publication {
            return Err(CompactIndexError::StaleSnapshot);
        }
        Ok(())
    }
    pub fn len(&self) -> Result<u64> {
        self.journal.sequence()?;
        Ok(self.directory.rows)
    }
    pub fn is_empty(&self) -> Result<bool> {
        Ok(self.len()? == 0)
    }
    fn page(&self, p: &Page) -> Result<Vec<V>> {
        let rows = decode_page(
            self.journal
                .get(&page_key(p.number))?
                .ok_or(CompactIndexError::Corrupt)?,
        )?;
        if row_id(&rows[0])? != p.first {
            return Err(CompactIndexError::Corrupt);
        }
        Ok(rows)
    }
    /// Streams one page at a time, in actual NeDB string-_id index order.
    /// No all-row Vec or second metadata authority is constructed.
    pub fn visit_rows(&self, mut visitor: impl FnMut(&V) -> Result<bool>) -> Result<()> {
        self.journal.sequence()?;
        for p in &self.directory.pages {
            for row in self.page(p)? {
                if !visitor(&row)? {
                    return Ok(());
                }
            }
        }
        Ok(())
    }
    fn audit(&self) -> Result<()> {
        let mut rows = 0u64;
        let mut last = None;
        self.visit_rows(|row| {
            let id = row_id(row)?;
            if last.as_ref().is_some_and(|p: &String| p >= &id) {
                return Err(CompactIndexError::Corrupt);
            }
            last = Some(id);
            rows += 1;
            Ok(true)
        })?;
        if rows != self.directory.rows
            || self.journal.entries()?.count() != self.directory.pages.len() + 1
        {
            return Err(CompactIndexError::Corrupt);
        }
        Ok(())
    }
    pub fn get(&self, id: &str) -> Result<Option<V>> {
        self.journal.sequence()?;
        valid_id(id)?;
        let i = self
            .directory
            .pages
            .partition_point(|p| p.first.as_str() <= id);
        if i == 0 {
            return Ok(None);
        }
        Ok(self
            .page(&self.directory.pages[i - 1])?
            .into_iter()
            .find(|v| v.get("_id") == V::String(id.into())))
    }
    /// Each actual source DB operation is one Journal publication. Two writes
    /// inside saveMaxHeight are deliberately two publications, not one repair.
    fn put(&mut self, row: V) -> Result<()> {
        self.require_live_metadata_write()?;
        let id = row_id(&row)?;
        let mut directory = self.directory.clone();
        let i = directory
            .pages
            .partition_point(|p| p.first <= id)
            .saturating_sub(1);
        let mut page = if directory.pages.is_empty() {
            vec![]
        } else {
            self.page(&directory.pages[i])?
        };
        let insertion = page.partition_point(|v| row_id(v).is_ok_and(|k| k < id));
        if page
            .get(insertion)
            .is_some_and(|v| v.get("_id") == V::String(id.clone()))
        {
            page[insertion] = row;
        } else {
            page.insert(insertion, row);
            directory.rows += 1;
        }
        let mut mutations = vec![];
        if directory.pages.is_empty() {
            let n = directory.next_page;
            directory.next_page = directory
                .next_page
                .checked_add(1)
                .ok_or(CompactIndexError::Limit)?;
            directory.pages.push(Page {
                number: n,
                first: id,
            });
            mutations.push(Mutation::Put {
                key: page_key(n),
                value: encode_page(&page)?,
            });
        } else if page.len() <= PAGE_ROWS {
            directory.pages[i].first = row_id(&page[0])?;
            mutations.push(Mutation::Put {
                key: page_key(directory.pages[i].number),
                value: encode_page(&page)?,
            });
        } else {
            if directory.pages.len() >= MAX_PAGES {
                return Err(CompactIndexError::Limit);
            }
            let right = page.split_off(page.len() / 2);
            let n = directory.next_page;
            directory.next_page = directory
                .next_page
                .checked_add(1)
                .ok_or(CompactIndexError::Limit)?;
            directory.pages[i].first = row_id(&page[0])?;
            mutations.push(Mutation::Put {
                key: page_key(directory.pages[i].number),
                value: encode_page(&page)?,
            });
            mutations.push(Mutation::Put {
                key: page_key(n),
                value: encode_page(&right)?,
            });
            directory.pages.insert(
                i + 1,
                Page {
                    number: n,
                    first: row_id(&right[0])?,
                },
            );
        }
        mutations.push(Mutation::Put {
            key: DIRECTORY.into(),
            value: directory.encode()?,
        });
        self.commit_inline_metadata(mutations)?;
        self.directory = directory;
        Ok(())
    }
    /// Explicit metadata suffix rollback, never automatic chain selection.
    /// Call only with independently selected context and a retained exact pin.
    /// All requested IDs and the context change publish in one bounded batch.
    pub fn discard_rows(
        &mut self,
        expected: &MetadataSnapshot,
        ids: &[String],
        new_context: &str,
    ) -> Result<MetadataSnapshot> {
        self.require_live_metadata_write()?;
        self.require_snapshot(expected)?;
        if ids.len() > 1024 || new_context.is_empty() || new_context.len() > 4096 {
            return Err(CompactIndexError::Limit);
        }
        let ids = ids
            .iter()
            .map(|id| {
                valid_id(id)?;
                Ok(id.as_str())
            })
            .collect::<Result<std::collections::BTreeSet<_>>>()?;
        let mut directory = self.directory.clone();
        let mut mutations = vec![];
        let mut pages = vec![];
        for p in &self.directory.pages {
            let mut rows = self.page(p)?;
            let old = rows.len();
            rows.retain(|r| !ids.contains(row_id(r).unwrap_or_default().as_str()));
            if rows.len() != old {
                directory.rows -= (old - rows.len()) as u64;
                if rows.is_empty() {
                    mutations.push(Mutation::Delete {
                        key: page_key(p.number),
                    });
                } else {
                    mutations.push(Mutation::Put {
                        key: page_key(p.number),
                        value: encode_page(&rows)?,
                    });
                }
            }
            if !rows.is_empty() {
                pages.push(Page {
                    number: p.number,
                    first: row_id(&rows[0])?,
                });
            }
        }
        directory.pages = pages;
        directory.context = new_context.into();
        mutations.push(Mutation::Put {
            key: DIRECTORY.into(),
            value: directory.encode()?,
        });
        if mutations.len() > crate::storage::MAX_BATCH_OPERATIONS {
            return Err(CompactIndexError::Limit);
        }
        self.commit_inline_metadata(mutations)?;
        self.directory = directory;
        self.snapshot()
    }
    /// Import one already-decoded old row at a time. The caller binds the
    /// migration source and controls restart/retry; conflicting IDs refuse.
    pub fn import_row(&mut self, expected: &MetadataSnapshot, row: V) -> Result<MetadataSnapshot> {
        self.require_snapshot(expected)?;
        profile_value(&row)?;
        let row = memory_copy(&row)
            .map_err(|_| CompactIndexError::Source(SourceRuleError::InvalidInput))?;
        let id = row_id(&row)?;
        if let Some(old) = self.get(&id)? {
            if old == row {
                return self.snapshot();
            }
            return Err(CompactIndexError::WrongContext);
        }
        self.put(row)?;
        self.snapshot()
    }
    fn find(&self, query: &V, one: bool) -> Result<V> {
        let matcher = Query::parse(query)?;
        let mut rows = vec![];
        if let Query::Id(id) = &matcher {
            return Ok(if one {
                self.get(id)?.unwrap_or(V::Null)
            } else {
                V::Array(self.get(id)?.into_iter().collect())
            });
        }
        self.visit_rows(|row| {
            if matcher.matches(&row_id(row)?) {
                if rows.len() >= 1024 {
                    return Err(CompactIndexError::Limit);
                }
                rows.push(row.clone());
                if one {
                    return Ok(false);
                }
            }
            Ok(true)
        })?;
        let result = if one {
            rows.into_iter().next().unwrap_or(V::Null)
        } else {
            V::Array(rows)
        };
        result.validate()?;
        Ok(result)
    }
    fn database(&mut self, method: &str, a: &[V]) -> Result<DependencyReturn> {
        let query = a.first().cloned().unwrap_or(V::Undefined);
        let pending = |v| Ok(DependencyReturn::Pending(v));
        match method {
            "findOneAsync" => pending(self.find(&query, true)?),
            "findAsync" => pending(self.find(&query, false)?),
            "insertAsync" => {
                profile_value(&query)?;
                let row = match memory_copy(&query) {
                    Ok(row) => row,
                    Err(e) => return Ok(DependencyReturn::PendingThrow(e)),
                };
                let id = row_id(&row)?;
                if self.get(&id)?.is_some() {
                    return Ok(source_error(format!(
                        "Can't insert key {id}, it violates the unique constraint"
                    )));
                }
                self.put(row.clone())?;
                pending(row)
            }
            "updateAsync" => {
                let Query::Id(id) = Query::parse(&query)? else {
                    return Err(SourceRuleError::UnknownDependency.into());
                };
                let old = self.get(&id)?;
                let upsert = a.get(2).is_some_and(|v| v.get("upsert") == V::Bool(true));
                if old.is_none() && !upsert {
                    return pending(V::number(0.0));
                }
                let update = a.get(1).ok_or(SourceRuleError::InvalidInput)?;
                let mut row =
                    old.unwrap_or_else(|| V::Object(vec![("_id".into(), V::String(id.clone()))]));
                if let V::Object(fields) = update {
                    if fields.len() == 1 && fields[0].0 == "$set" {
                        let V::Object(sets) = &fields[0].1 else {
                            return Err(SourceRuleError::UnknownDependency.into());
                        };
                        for (k, v) in sets {
                            if k == "_id" || k.starts_with('$') || k.contains('.') {
                                return Err(SourceRuleError::UnknownDependency.into());
                            }
                            row.insert(k, v.clone())?;
                        }
                    } else if fields.iter().all(|(k, _)| !k.starts_with('$')) {
                        row = update.clone();
                    } else {
                        return Err(SourceRuleError::UnknownDependency.into());
                    }
                } else {
                    return Err(SourceRuleError::InvalidInput.into());
                }
                if row.get("_id") == V::Undefined {
                    row.insert("_id", V::String(id.clone()))?;
                }
                if row_id(&row)? != id {
                    return Ok(source_error("You cannot change a document's _id"));
                }
                profile_value(&row)?;
                let row = match memory_copy(&row) {
                    Ok(row) => row,
                    Err(e) => return Ok(DependencyReturn::PendingThrow(e)),
                };
                self.put(row)?;
                pending(V::number(1.0))
            }
            _ => Err(SourceRuleError::UnknownDependency.into()),
        }
    }
    #[cfg(test)]
    pub(crate) fn test_fail_after_sync(&mut self) {
        self.journal.test_fail_after_sync();
    }
    #[cfg(test)]
    pub(crate) fn test_fail_after_partial_write(&mut self) {
        self.journal.test_fail_after_partial_write();
    }
}
fn require_namespace(root: &Path, namespace: &Namespace) -> Result<()> {
    if namespace.path(root).file_name() != Some(std::ffi::OsStr::new("txIndex.tldb")) {
        return Err(CompactIndexError::WrongContext);
    }
    Ok(())
}
/// Logical Error objects and genuine BigNumber internals have no disjoint
/// representation in SourceValue. Refuse those domains locally rather than
/// turn a missing conversion into a purported JavaScript exception.
fn profile_value(v: &V) -> Result<()> {
    v.validate()?;
    fn walk(v: &V) -> Result<()> {
        match v {
            V::BigNumber(_) => return Err(SourceRuleError::UnsupportedNumericDomain.into()),
            V::Object(fields) => {
                for (k, v) in fields {
                    if k == "__proto__"
                        || k == "$error"
                        || matches!(
                            k.as_str(),
                            "$$date" | "$$deleted" | "$$indexCreated" | "$$indexRemoved"
                        )
                    {
                        return Err(SourceRuleError::UnsupportedNumericDomain.into());
                    }
                    walk(v)?;
                }
            }
            V::Array(values) => {
                for v in values {
                    walk(v)?;
                }
            }
            _ => (),
        }
        Ok(())
    }
    walk(v)
}
fn source_error(message: impl Into<String>) -> DependencyReturn {
    DependencyReturn::PendingThrow(SourceException {
        name: "Error".into(),
        message: message.into(),
    })
}
fn memory_copy(v: &V) -> std::result::Result<V, SourceException> {
    Ok(match v {
        V::Promise => V::object(),
        V::Object(fields) => V::Object(
            fields
                .iter()
                .map(|(k, v)| {
                    if k.starts_with('$') {
                        return Err(SourceException {
                            name: "Error".into(),
                            message: "Field names cannot begin with the $ character".into(),
                        });
                    }
                    if k.contains('.') {
                        return Err(SourceException {
                            name: "Error".into(),
                            message: "Field names cannot contain a .".into(),
                        });
                    }
                    Ok((k.clone(), memory_copy(v)?))
                })
                .collect::<std::result::Result<_, _>>()?,
        ),
        V::Array(rows) => V::Array(
            rows.iter()
                .map(memory_copy)
                .collect::<std::result::Result<_, _>>()?,
        ),
        _ => v.clone(),
    })
}
enum Query {
    All,
    Id(String),
    Prefix,
    Transaction(String),
}
impl Query {
    fn parse(v: &V) -> Result<Self> {
        if let V::Object(fields) = v {
            if fields.is_empty() {
                return Ok(Self::All);
            }
            if fields.len() != 1 || fields[0].0 != "_id" {
                return Err(SourceRuleError::UnknownDependency.into());
            }
        } else {
            return Err(SourceRuleError::InvalidInput.into());
        }
        match v.get("_id") {
            V::String(id) => {
                valid_id(&id)?;
                Ok(Self::Id(id))
            }
            V::Object(_) => {
                let re = v.get("_id").get("$regex");
                if re.get("$flags") != V::String(String::new()) {
                    return Err(SourceRuleError::UnknownDependency.into());
                }
                let V::String(pattern) = re.get("$regexp") else {
                    return Err(SourceRuleError::UnknownDependency.into());
                };
                if pattern == "^tx" {
                    return Ok(Self::Prefix);
                }
                let id = pattern
                    .strip_prefix("^tx-\\d+-")
                    .and_then(|p| p.strip_suffix('$'))
                    .ok_or(SourceRuleError::UnknownDependency)?;
                if !id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b':'))
                {
                    return Err(SourceRuleError::UnsupportedNumericDomain.into());
                }
                Ok(Self::Transaction(id.into()))
            }
            _ => Err(SourceRuleError::UnsupportedNumericDomain.into()),
        }
    }
    fn matches(&self, id: &str) -> bool {
        match self {
            Self::All => true,
            Self::Id(v) => id == v,
            Self::Prefix => id.starts_with("tx"),
            Self::Transaction(v) => {
                let Some(tail) = id.strip_prefix("tx-") else {
                    return false;
                };
                let Some((height, tx)) = tail.split_once('-') else {
                    return false;
                };
                !height.is_empty() && height.bytes().all(|b| b.is_ascii_digit()) && tx == v
            }
        }
    }
}

/// Only literal TxIndex DB calls are intercepted. RPC reads stay in the one
/// explicitly supplied factual input provider; there is no stored actor verdict.
pub struct CompactTxIndexBackend<'a, B> {
    pub metadata: &'a mut SourceCompactIndex,
    pub inputs: &'a mut B,
}
impl<B: SourceBackend> SourceBackend for CompactTxIndexBackend<'_, B> {
    fn invoke(
        &mut self,
        call: &DependencyCall,
    ) -> std::result::Result<DependencyReturn, SourceRuleError> {
        if call.target == "db.getDatabase" {
            if call.arguments != [V::String("txIndex".into())] {
                return Err(SourceRuleError::UnknownDependency);
            }
            return Ok(DependencyReturn::Pending(V::Object(vec![(
                "$database".into(),
                V::String("txIndex".into()),
            )])));
        }
        if let Some(method) = call.target.strip_prefix("db.") {
            if call
                .arguments
                .first()
                .is_none_or(|v| v.get("$database") != V::String("txIndex".into()))
            {
                return Err(SourceRuleError::InvalidInput);
            }
            return self
                .metadata
                .database(method, &call.arguments[1..])
                .map_err(|e| match e {
                    CompactIndexError::Source(e) => e,
                    CompactIndexError::Limit => SourceRuleError::Capacity,
                    _ => SourceRuleError::Backend,
                });
        }
        if call.target == "ClientWrapper.getInstance" || call.target.starts_with("client.") {
            return self.inputs.invoke(call);
        }
        Err(SourceRuleError::UnknownDependency)
    }
}

/// The canonical candidate list is deliberately NOT used as the source TX
/// selection. Every actual transaction is available in block position order,
/// including a first non-TL nulldata that masks a later canonical TL output.
pub struct SourceCompactBlock {
    block: StructurallyCompleteCandidateBlock,
}
impl SourceCompactBlock {
    pub fn read(
        snapshot: &CompactCandidateSnapshot,
        height: u32,
        raw: &[u8],
    ) -> std::result::Result<Self, CandidateSnapshotError> {
        Ok(Self {
            block: snapshot.read_block(height, raw)?,
        })
    }
    pub const fn profile(&self) -> &'static str {
        DISCOVERY_PROFILE
    }
    pub fn canonical_evidence(&self) -> &StructurallyCompleteCandidateBlock {
        &self.block
    }
    pub fn raw_transactions(&self) -> impl Iterator<Item = (u32, &[u8])> {
        (0..self.block.identity().transaction_count).map(|i| {
            (
                i,
                self.block
                    .raw_transaction(i)
                    .expect("validated complete transaction ranges"),
            )
        })
    }
}

include!("source_compact_stream_impl.rs");
include!("source_index_history_read_impl.rs");
include!("source_index_history_pin_codec_impl.rs");

include!("source_main_index_stream_impl.rs");

include!("source_index_validity_stream_impl.rs");
include!("source_main_native_owner_access_impl.rs");
include!("source_main_native_validity_impl.rs");
include!("source_index_prepared_put_impl.rs");
include!("source_index_inline_database_impl.rs");
include!("source_index_inline_recovery_impl.rs");
#[cfg(test)]
#[path = "source_index_inline_database_tests.rs"]
mod inline_database_tests;
#[cfg(test)]
#[path = "source_index_inline_recovery_tests.rs"]
mod inline_recovery_tests;
include!("source_index_prepared_validity_impl.rs");
include!("source_index_cold_execution_impl.rs");
include!("source_index_historical_validity_impl.rs");
include!("source_index_historical_source_read_impl.rs");
include!("source_index_row_binding_impl.rs");
include!("source_main_native_no_index_index_tests_impl.rs");
include!("source_index_verified_reopen_impl.rs");
include!("source_index_supersession_binding_impl.rs");
include!("source_index_generation_escrow_impl.rs");
include!("source_index_inline_escrow_guard_impl.rs");
#[cfg(test)]
#[path = "source_index_prepared_validity_tests.rs"]
mod prepared_validity_tests;
#[cfg(test)]
#[path = "source_index_verified_reopen_tests.rs"]
mod verified_reopen_tests;

#[cfg(test)]
#[path = "source_index_history_read_tests.rs"]
mod history_read_tests;

#[cfg(test)]
#[path = "source_index_historical_validity_tests.rs"]
mod historical_validity_tests;

#[cfg(test)]
#[path = "source_index_cold_execution_tests.rs"]
mod cold_execution_tests;

#[cfg(test)]
#[path = "source_index_cold_discard_tests.rs"]
mod cold_discard_tests;

#[cfg(test)]
#[path = "source_index_generation_escrow_tests.rs"]
mod generation_escrow_index_tests;

#[cfg(test)]
#[path = "source_index_historical_native_await_tests.rs"]
mod historical_native_await_tests;

#[cfg(test)]
#[path = "source_index_cold_escrow_guard_tests.rs"]
mod cold_escrow_guard_tests;

#[cfg(test)]
#[path = "source_index_history_pin_codec_tests.rs"]
mod history_pin_codec_tests;

#[cfg(test)]
#[path = "source_index_historical_source_read_tests.rs"]
mod historical_source_read_tests;
