// Actual original native read at executor time. No writer/plan or supplied rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NativeMainStartupIndexReadKindV1 {
    FindMax,
    Fixed,
}
pub(crate) struct NativeMainStartupIndexScopeV1 {
    writer: crate::compact_journal::SourceQueuedLiveWriterV1,
    original_request: MetadataSnapshot,
    physical_identity: String,
    catalog: std::path::PathBuf,
}
impl NativeMainStartupIndexScopeV1 {
    pub(crate) fn visit_canonical_fields(
        &self,
        visitor: &mut impl FnMut(&str, &V) -> std::result::Result<(), SourceRuleError>,
    ) -> std::result::Result<(), SourceRuleError> {
        visitor("physicalIdentity", &prepared_text(&self.physical_identity))?;
        visitor("catalog", &prepared_text(self.catalog.to_string_lossy()))?;
        startup_index_visit_value(
            "originalRequest",
            &prepared_snapshot_value(&self.original_request),
            visitor,
        )
    }
}
pub(crate) struct NativeMainStartupIndexReadV1 {
    writer: crate::compact_journal::SourceQueuedLiveWriterV1,
    kind: NativeMainStartupIndexReadKindV1,
    request: MetadataSnapshot,
    sampled: bool,
    callback: bool,
    consumed: bool,
    read: Option<MainIndexReadViewV1>,
    result: Option<DependencyReturn>,
    rows: Option<RowStreamReceipt>,
    state: Option<NativeMetadataStateReceiptV1>,
    stopped: Option<String>,
}
impl NativeMainStartupIndexReadV1 {
    pub(crate) fn kind(&self) -> NativeMainStartupIndexReadKindV1 {
        self.kind
    }
    pub(crate) fn sampled(&self) -> bool {
        self.sampled
    }
    pub(crate) fn callback_delivered(&self) -> bool {
        self.callback
    }
    pub(crate) fn actual_returned(&self) -> Option<&DependencyReturn> {
        self.result.as_ref()
    }
    pub(crate) fn actual_view(&self) -> Option<&MainIndexReadViewV1> {
        self.read.as_ref()
    }
    pub(crate) fn actual_rows(&self) -> Option<&RowStreamReceipt> {
        self.rows.as_ref()
    }
    pub(crate) fn stopped(&self) -> Option<&str> {
        self.stopped.as_deref()
    }
    pub(crate) fn consume_callback(&mut self) -> Result<()> {
        if !self.callback || self.consumed || self.stopped.is_some() {
            return Err(SourceRuleError::InvalidInput.into());
        }
        self.consumed = true;
        Ok(())
    }
    // Complete values are streamed individually; no 1024-row aggregate is made.
    pub(crate) fn visit_canonical_fields(
        &self,
        visitor: &mut impl FnMut(&str, &V) -> std::result::Result<(), SourceRuleError>,
    ) -> std::result::Result<(), SourceRuleError> {
        visitor(
            "profile",
            &prepared_text("TL-MAIN-ACTUAL-STARTUP-INDEX-READ-13-1"),
        )?;
        visitor(
            "kind",
            &prepared_text(match self.kind {
                NativeMainStartupIndexReadKindV1::FindMax => "FindMax",
                NativeMainStartupIndexReadKindV1::Fixed => "Fixed",
            }),
        )?;
        startup_index_visit_value("request", &prepared_snapshot_value(&self.request), visitor)?;
        for (key, value) in [
            ("sampled", self.sampled),
            ("callback", self.callback),
            ("consumed", self.consumed),
        ] {
            visitor(key, &V::Bool(value))?;
        }
        visitor(
            "stopped",
            &self.stopped.as_ref().map_or(V::Null, prepared_text),
        )?;
        visitor("result/present", &V::Bool(self.result.is_some()))?;
        if let Some(result) = &self.result {
            let (kind, value) = match result {
                DependencyReturn::Immediate(v) => ("Immediate", v.clone()),
                DependencyReturn::Pending(v) => ("Pending", v.clone()),
                DependencyReturn::Throw(e) => (
                    "Throw",
                    prepared_obj(vec![
                        ("name", prepared_text(&e.name)),
                        ("message", prepared_text(&e.message)),
                    ]),
                ),
                DependencyReturn::PendingThrow(e) => (
                    "PendingThrow",
                    prepared_obj(vec![
                        ("name", prepared_text(&e.name)),
                        ("message", prepared_text(&e.message)),
                    ]),
                ),
            };
            visitor("result/kind", &prepared_text(kind))?;
            startup_index_visit_value("result/value", &value, visitor)?;
        }
        visitor("read/present", &V::Bool(self.read.is_some()))?;
        if let Some(read) = &self.read {
            startup_index_visit_value("read/pin", &prepared_snapshot_value(read.pin()), visitor)?;
            visitor("read/copiedFile", &V::Null)?;
            let mut cursor = read.cursor();
            let mut at = 0u64;
            while let Some(row) = cursor.next_row().map_err(startup_index_rule)? {
                startup_index_visit_value(&format!("read/rows/{at}"), &row, visitor)?;
                at += 1;
            }
            visitor("read/rows/count", &prepared_text(at.to_string()))?;
        }
        visitor("state/present", &V::Bool(self.state.is_some()))?;
        if let Some(state) = &self.state {
            startup_index_visit_value("state", &prepared_state_value(state), visitor)?;
        }
        visitor("rows/present", &V::Bool(self.rows.is_some()))?;
        if let Some(rows) = &self.rows {
            visitor("rows/profile", &prepared_text(&rows.profile))?;
            startup_index_visit_value(
                "rows/snapshot",
                &prepared_snapshot_value(&rows.snapshot),
                visitor,
            )?;
            visitor("rows/count", &prepared_text(rows.rows.to_string()))?;
            visitor(
                "rows/chain",
                &prepared_text(prepared_hex(&rows.chain_sha256d)),
            )?;
            visitor("rows/complete", &V::Bool(rows.complete))?;
        }
        Ok(())
    }
}
fn startup_index_rule(error: CompactIndexError) -> SourceRuleError {
    match error {
        CompactIndexError::Source(error) => error,
        CompactIndexError::Limit => SourceRuleError::Capacity,
        _ => SourceRuleError::Backend,
    }
}
fn startup_index_visit_value(
    path: &str,
    value: &V,
    visitor: &mut impl FnMut(&str, &V) -> std::result::Result<(), SourceRuleError>,
) -> std::result::Result<(), SourceRuleError> {
    match value {
        V::Array(items) => {
            visitor(&format!("{path}/kind"), &prepared_text("Array"))?;
            visitor(
                &format!("{path}/length"),
                &prepared_text(items.len().to_string()),
            )?;
            for (at, item) in items.iter().enumerate() {
                startup_index_visit_value(&format!("{path}/{at}"), item, visitor)?;
            }
            Ok(())
        }
        V::Object(fields) => {
            visitor(&format!("{path}/kind"), &prepared_text("Object"))?;
            visitor(
                &format!("{path}/length"),
                &prepared_text(fields.len().to_string()),
            )?;
            for (at, (key, item)) in fields.iter().enumerate() {
                visitor(&format!("{path}/{at}/key"), &prepared_text(key))?;
                startup_index_visit_value(&format!("{path}/{at}/value"), item, visitor)?;
            }
            Ok(())
        }
        value => visitor(path, value),
    }
}
impl SourceCompactIndex {
    pub(crate) fn capture_startup_source_index_scope(
        &self,
    ) -> Result<NativeMainStartupIndexScopeV1> {
        self.require_live_metadata_write()?;
        let (physical_identity, catalog) = self.native_store_context()?;
        Ok(NativeMainStartupIndexScopeV1 {
            writer: self.journal.capture_source_queued_live_writer()?,
            original_request: self.snapshot()?,
            physical_identity,
            catalog,
        })
    }
    pub(crate) fn require_startup_source_index_scope(
        &self,
        scope: &NativeMainStartupIndexScopeV1,
    ) -> Result<()> {
        self.require_live_metadata_write()?;
        self.journal
            .require_source_queued_live_writer(&scope.writer)?;
        Ok(())
    }
    pub(crate) fn startup_source_index_read_request(
        &self,
        kind: NativeMainStartupIndexReadKindV1,
    ) -> Result<NativeMainStartupIndexReadV1> {
        // This new entry is live-only. No old/cold reader is widened.
        self.require_live_metadata_write()?;
        Ok(NativeMainStartupIndexReadV1 {
            writer: self.journal.capture_source_queued_live_writer()?,
            kind,
            request: self.snapshot()?,
            sampled: false,
            callback: false,
            consumed: false,
            read: None,
            result: None,
            rows: None,
            state: None,
            stopped: None,
        })
    }
    pub(crate) fn startup_source_index_read_sample_actual(
        &self,
        query: &mut NativeMainStartupIndexReadV1,
    ) -> Result<()> {
        if query.sampled || query.callback || query.consumed || query.stopped.is_some() {
            return Err(SourceRuleError::InvalidInput.into());
        }
        self.journal
            .require_source_queued_live_writer(&query.writer)?;
        // A request retains writer identity, NOT an early row snapshot. Capture
        // exactly the actual current original generation at executor time.
        query.sampled = true; // no repeat sampling after a read/post-read fault
        let actual: Result<()> = (|| {
            let pin = self.snapshot()?;
            let read = MainIndexReadViewV1::from_original(self.read_generation(&pin)?);
            query.read = Some(read);
            let read = query.read.as_ref().ok_or(CompactIndexError::Corrupt)?;
            query.state = Some(read.complete_metadata_state()?);
            let mut cursor = read.cursor();
            while cursor.next_row()?.is_some() {}
            query.rows = Some(cursor.receipt());
            if query.kind == NativeMainStartupIndexReadKindV1::FindMax {
                query.result = Some(read.source_database_read(
                    "findOneAsync",
                    &[prepared_obj(vec![("_id", prepared_text("MaxHeight"))])],
                )?);
            }
            Ok(())
        })();
        if let Err(error) = &actual {
            query.stopped = Some(error.to_string());
        }
        actual
    }
    pub(crate) fn startup_source_index_read_deliver_callback(
        &self,
        query: &mut NativeMainStartupIndexReadV1,
    ) -> Result<()> {
        if !query.sampled || query.callback || query.consumed || query.stopped.is_some() {
            return Err(SourceRuleError::InvalidInput.into());
        }
        self.journal
            .require_source_queued_live_writer(&query.writer)?;
        // Later same-writer commits are allowed. The held original read/result
        // cannot be rebound to a foreign/reopened writer or a newer snapshot.
        query.callback = true;
        Ok(())
    }
}
