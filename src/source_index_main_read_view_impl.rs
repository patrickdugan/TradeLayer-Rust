// Read-only metadata sharing for fixed Main query/grouping and historical IO.
// Both variants retain their actual data brand; a copied file is never turned
// into an Original SourceIndexReadSnapshot or a native writer.
#[derive(Clone)]
enum MainIndexReadDataV1 {
    Original(std::sync::Arc<SourceIndexReadSnapshot>),
    Verified(std::sync::Arc<VerifiedOriginalMetadataViewV1>),
}
#[derive(Clone)]
pub struct MainIndexReadViewV1 {
    data: MainIndexReadDataV1,
}
impl MainIndexReadViewV1 {
    pub fn from_original(snapshot: SourceIndexReadSnapshot) -> Self {
        Self {
            data: MainIndexReadDataV1::Original(std::sync::Arc::new(snapshot)),
        }
    }
    pub fn from_verified(view: VerifiedOriginalMetadataViewV1) -> Self {
        Self::from_shared_verified(std::sync::Arc::new(view))
    }
    pub(crate) fn from_shared_verified(
        view: std::sync::Arc<VerifiedOriginalMetadataViewV1>,
    ) -> Self {
        Self {
            data: MainIndexReadDataV1::Verified(view),
        }
    }
    pub fn pin(&self) -> &MetadataSnapshot {
        match &self.data {
            MainIndexReadDataV1::Original(data) => data.pin(),
            MainIndexReadDataV1::Verified(data) => &data.original().metadata,
        }
    }
    pub fn copied_file(&self) -> Option<&Path> {
        match &self.data {
            MainIndexReadDataV1::Original(_) => None,
            MainIndexReadDataV1::Verified(data) => data.copied_file(),
        }
    }
    pub fn original_history(&self) -> Option<&SourceIndexHistoryPinV1> {
        match &self.data {
            MainIndexReadDataV1::Original(_) => None,
            MainIndexReadDataV1::Verified(data) => Some(data.original()),
        }
    }
    fn directory(&self) -> &Directory {
        match &self.data {
            MainIndexReadDataV1::Original(data) => &data.directory,
            MainIndexReadDataV1::Verified(data) => data.directory(),
        }
    }
    fn raw_get(&self, key: &str) -> Option<&[u8]> {
        match &self.data {
            MainIndexReadDataV1::Original(data) => data.data.get(key),
            MainIndexReadDataV1::Verified(data) => data.raw_get(key),
        }
    }
    pub(crate) fn raw_entries(&self) -> Box<dyn Iterator<Item = (&str, &[u8])> + '_> {
        match &self.data {
            MainIndexReadDataV1::Original(data) => Box::new(data.data.entries()),
            MainIndexReadDataV1::Verified(data) => data.raw_entries(),
        }
    }
    pub fn complete_metadata_state(&self) -> Result<NativeMetadataStateReceiptV1> {
        prepared_metadata_state_entries(self.raw_entries(), &[])
    }
    fn page(&self, at: usize) -> Result<Vec<V>> {
        if let MainIndexReadDataV1::Original(data) = &self.data {
            return data.page(at);
        }
        let page = self
            .directory()
            .pages
            .get(at)
            .ok_or(CompactIndexError::Corrupt)?;
        let rows = decode_page(
            self.raw_get(&page_key(page.number))
                .ok_or(CompactIndexError::Corrupt)?,
        )?;
        if row_id(&rows[0])? != page.first {
            return Err(CompactIndexError::Corrupt);
        }
        Ok(rows)
    }
    pub fn cursor(&self) -> SourceIndexCursor {
        SourceIndexCursor {
            snapshot: self.clone(),
            page: 0,
            position: 0,
            buffer: vec![],
            seen: 0,
            chain: crate::hash256::double_sha256(STREAM_PROFILE.as_bytes()),
            complete: false,
        }
    }
}
