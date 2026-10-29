// Distinct verified old-data view: ORIGINAL facts and actual COPIED physical
// identity remain separate. No copied CompactReadSnapshot is relabelled.
enum VerifiedOriginalMetadataDataV1 {
    Original(Box<SourceIndexReadSnapshot>),
    Copy(Box<SourceIndexEscrowReadSnapshotV1>),
}
pub struct VerifiedOriginalMetadataViewV1 {
    original: SourceIndexHistoryPinV1,
    data: VerifiedOriginalMetadataDataV1,
}
impl VerifiedOriginalMetadataViewV1 {
    pub fn original(&self) -> &SourceIndexHistoryPinV1 {
        &self.original
    }
    pub fn copied_file(&self) -> Option<&Path> {
        match &self.data {
            VerifiedOriginalMetadataDataV1::Original(_) => None,
            VerifiedOriginalMetadataDataV1::Copy(data) => Some(data.copied_file()),
        }
    }
    pub fn copied_generation(&self) -> Option<&crate::compact_journal::GenerationEscrowPinV1> {
        match &self.data {
            VerifiedOriginalMetadataDataV1::Original(_) => None,
            VerifiedOriginalMetadataDataV1::Copy(data) => Some(&data.data.escrow),
        }
    }
    pub fn open_original(
        index: &mut SourceCompactIndex,
        expected: &SourceIndexHistoryPinV1,
    ) -> Result<Self> {
        let data = index.read_verified_history_generation(expected)?;
        let view = Self {
            original: expected.clone(),
            data: VerifiedOriginalMetadataDataV1::Original(Box::new(data)),
        };
        view.require_rows()?;
        Ok(view)
    }
    pub fn open_copy(
        escrow: &SourceIndexGenerationEscrowPinV1,
        expected: &SourceIndexHistoryPinV1,
    ) -> Result<Self> {
        let data = escrow.open_independently_pinned_history(expected)?;
        let view = Self {
            original: expected.clone(),
            data: VerifiedOriginalMetadataDataV1::Copy(Box::new(data)),
        };
        view.require_rows()?;
        Ok(view)
    }
    fn require_rows(&self) -> Result<()> {
        if self.visit_rows(|_| Ok(true))? != self.original.rows {
            return Err(CompactIndexError::WrongContext);
        }
        Ok(())
    }
    fn directory(&self) -> &Directory {
        match &self.data {
            VerifiedOriginalMetadataDataV1::Original(data) => &data.directory,
            VerifiedOriginalMetadataDataV1::Copy(data) => &data.directory,
        }
    }
    pub(crate) fn raw_get(&self, key: &str) -> Option<&[u8]> {
        match &self.data {
            VerifiedOriginalMetadataDataV1::Original(data) => data.data.get(key),
            VerifiedOriginalMetadataDataV1::Copy(data) => data.raw_get(key),
        }
    }
    pub(crate) fn raw_entries(&self) -> Box<dyn Iterator<Item = (&str, &[u8])> + '_> {
        match &self.data {
            VerifiedOriginalMetadataDataV1::Original(data) => Box::new(data.data.entries()),
            VerifiedOriginalMetadataDataV1::Copy(data) => Box::new(data.raw_entries()),
        }
    }
    pub fn get(&self, id: &str) -> Result<Option<V>> {
        let directory = self.directory();
        if directory.pages.is_empty() {
            return Ok(None);
        }
        let i = directory
            .pages
            .partition_point(|p| p.first.as_str() <= id)
            .saturating_sub(1);
        let rows = decode_page(
            self.raw_get(&page_key(directory.pages[i].number))
                .ok_or(CompactIndexError::Corrupt)?,
        )?;
        for row in rows {
            let key = row_id(&row)?;
            if key == id {
                return Ok(Some(row));
            }
            if key.as_str() > id {
                break;
            }
        }
        Ok(None)
    }
    pub fn visit_rows(
        &self,
        mut visitor: impl FnMut(&V) -> Result<bool>,
    ) -> Result<RowStreamReceipt> {
        if self.raw_entries().count() != self.directory().pages.len() + 1 {
            return Err(CompactIndexError::Corrupt);
        }
        let mut receipt = RowStreamReceipt {
            profile: STREAM_PROFILE.into(),
            snapshot: self.original.metadata.clone(),
            rows: 0,
            chain_sha256d: crate::hash256::double_sha256(STREAM_PROFILE.as_bytes()),
            complete: false,
        };
        let mut last = None;
        for page in &self.directory().pages {
            let rows = decode_page(
                self.raw_get(&page_key(page.number))
                    .ok_or(CompactIndexError::Corrupt)?,
            )?;
            if row_id(&rows[0])? != page.first {
                return Err(CompactIndexError::Corrupt);
            }
            for row in rows {
                let id = row_id(&row)?;
                if last.as_ref().is_some_and(|p: &String| p >= &id) {
                    return Err(CompactIndexError::Corrupt);
                }
                last = Some(id);
                let raw = row.diagnostic_json()?;
                let mut bytes = receipt.chain_sha256d.to_vec();
                bytes.extend_from_slice(&receipt.rows.to_be_bytes());
                bytes.extend_from_slice(&(raw.len() as u32).to_be_bytes());
                bytes.extend_from_slice(raw.as_bytes());
                receipt.chain_sha256d = crate::hash256::double_sha256(&bytes);
                receipt.rows += 1;
                if !visitor(&row)? {
                    return Ok(receipt);
                }
            }
        }
        if receipt.rows != self.directory().rows {
            return Err(CompactIndexError::Corrupt);
        }
        receipt.complete = true;
        Ok(receipt)
    }
}
