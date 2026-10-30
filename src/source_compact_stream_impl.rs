// Included in source_compact_index_v2.rs. Source aggregate APIs stay bounded.
pub const STREAM_PROFILE: &str = "TL-TXINDEX-COMPLETE-ROW-STREAM-2";
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RowStreamReceipt {
    pub profile: String,
    pub snapshot: MetadataSnapshot,
    pub rows: u64,
    pub chain_sha256d: [u8; 32],
    pub complete: bool,
}
#[derive(Clone)]
pub struct SourceIndexReadSnapshot {
    pin: MetadataSnapshot,
    directory: Directory,
    data: CompactReadSnapshot,
}
impl SourceIndexReadSnapshot {
    pub fn pin(&self) -> &MetadataSnapshot {
        &self.pin
    }
    pub fn cursor(&self) -> SourceIndexCursor {
        SourceIndexCursor {
            snapshot: MainIndexReadViewV1::from_original(self.clone()),
            page: 0,
            position: 0,
            buffer: vec![],
            seen: 0,
            chain: crate::hash256::double_sha256(STREAM_PROFILE.as_bytes()),
            complete: false,
        }
    }
    fn page(&self, page: usize) -> Result<Vec<V>> {
        let p = self
            .directory
            .pages
            .get(page)
            .ok_or(CompactIndexError::Corrupt)?;
        let rows = decode_page(
            self.data
                .get(&page_key(p.number))
                .ok_or(CompactIndexError::Corrupt)?,
        )?;
        if row_id(&rows[0])? != p.first {
            return Err(CompactIndexError::Corrupt);
        }
        Ok(rows)
    }
}
pub struct SourceIndexCursor {
    snapshot: MainIndexReadViewV1,
    page: usize,
    position: usize,
    buffer: Vec<V>,
    seen: u64,
    chain: [u8; 32],
    complete: bool,
}
impl SourceIndexCursor {
    pub fn next_row(&mut self) -> Result<Option<V>> {
        if self.complete {
            return Ok(None);
        }
        if self.position == self.buffer.len() {
            if self.page == self.snapshot.directory().pages.len() {
                if self.seen != self.snapshot.directory().rows {
                    return Err(CompactIndexError::Corrupt);
                }
                self.complete = true;
                return Ok(None);
            }
            self.buffer = self.snapshot.page(self.page)?;
            self.page += 1;
            self.position = 0;
        }
        let row = self.buffer[self.position].clone();
        self.position += 1;
        let encoded = row.diagnostic_json()?;
        let mut bytes = self.chain.to_vec();
        bytes.extend_from_slice(&self.seen.to_be_bytes());
        bytes.extend_from_slice(&(encoded.len() as u32).to_be_bytes());
        bytes.extend_from_slice(encoded.as_bytes());
        self.chain = crate::hash256::double_sha256(&bytes);
        self.seen += 1;
        Ok(Some(row))
    }
    pub fn receipt(&self) -> RowStreamReceipt {
        RowStreamReceipt {
            profile: STREAM_PROFILE.into(),
            snapshot: self.snapshot.pin().clone(),
            rows: self.seen,
            chain_sha256d: self.chain,
            complete: self.complete,
        }
    }
}
impl SourceCompactIndex {
    pub fn read_generation(&self, expected: &MetadataSnapshot) -> Result<SourceIndexReadSnapshot> {
        self.require_snapshot(expected)?;
        let data = self.journal.read_snapshot()?;
        if data.sequence() != expected.sequence || data.publication() != &expected.publication {
            return Err(CompactIndexError::StaleSnapshot);
        }
        Ok(SourceIndexReadSnapshot {
            pin: expected.clone(),
            directory: self.directory.clone(),
            data,
        })
    }
    pub fn compact_metadata(
        &mut self,
        expected: &MetadataSnapshot,
    ) -> Result<(MetadataSnapshot, crate::compact_journal::CompactionReport)> {
        self.require_live_metadata_write()?;
        self.require_snapshot(expected)?;
        let plan = self.journal.prepare_compaction()?;
        let report = self.journal.publish_compaction(&plan)?;
        drop(plan);
        self.journal.prune_retention()?;
        Ok((self.snapshot()?, report))
    }
    pub fn configure_compaction(
        &mut self,
        config: crate::compact_journal::CompactionConfig,
    ) -> Result<()> {
        self.require_live_metadata_write()?;
        self.journal.configure(config)?;
        Ok(())
    }
    pub fn storage_footprint(&self) -> Result<crate::compact_journal::CatalogFootprint> {
        Ok(self.journal.footprint()?)
    }
    pub fn storage_io_counters(&self) -> Result<crate::compact_journal::CatalogIoCounters> {
        Ok(self.journal.io_counters()?.clone())
    }
}
