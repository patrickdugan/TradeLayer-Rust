// Explicit source-request/actual-storage-execution separation. The old strict
// preparation/Post APIs stay unchanged; no Ledger/source outcome is imported.
pub const SOURCE_QUEUED_DATABASE_PROFILE: &str = "TL-NATIVE-QUEUED-SOURCE-DATABASE-13-1";
static SOURCE_QUEUED_REQUEST_ID: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(1);
pub struct NativeQueuedInlineDatabaseV1 {
    request_id: u64,
    writer: crate::compact_journal::SourceQueuedLiveWriterV1,
    request: MetadataSnapshot,
    method: String,
    arguments: Vec<V>,
    plan: Option<Box<NativePreparedInlineDatabaseV1>>,
    attempted: bool,
    append_attempted: bool,
    successful: bool,
    resolved: bool,
    #[cfg(test)]
    append_fault: Option<&'static str>,
}
pub struct NativeAppliedQueuedInlineDatabaseV1 {
    request_id: u64,
    writer: crate::compact_journal::SourceQueuedLiveWriterV1,
    post: SourceIndexReadSnapshot,
    state: NativeMetadataStateReceiptV1,
    returned: DependencyReturn,
}
impl NativeQueuedInlineDatabaseV1 {
    pub fn request(&self) -> &MetadataSnapshot {
        &self.request
    }
    pub fn method(&self) -> &str {
        &self.method
    }
    pub fn arguments(&self) -> &[V] {
        &self.arguments
    }
    pub fn attempted(&self) -> bool {
        self.attempted
    }
    pub fn attempted_plan(&self) -> Option<&NativePreparedInlineDatabaseV1> {
        self.plan.as_deref()
    }
}
impl SourceCompactIndex {
    pub fn queue_source_inline_database(
        &self,
        method: &str,
        arguments: Vec<V>,
    ) -> Result<NativeQueuedInlineDatabaseV1> {
        // Same cold/escrow guard as ordinary mutation entries, before reads.
        self.require_live_metadata_write()?;
        if !["updateAsync", "insertAsync"].contains(&method) || arguments.len() > 3 {
            return Err(SourceRuleError::InvalidInput.into());
        }
        for value in &arguments {
            profile_value(value)?;
        }
        let request_id = SOURCE_QUEUED_REQUEST_ID
            .fetch_update(
                std::sync::atomic::Ordering::Relaxed,
                std::sync::atomic::Ordering::Relaxed,
                |id| id.checked_add(1),
            )
            .map_err(|_| CompactIndexError::Limit)?;
        Ok(NativeQueuedInlineDatabaseV1 {
            request_id,
            writer: self.journal.capture_source_queued_live_writer()?,
            request: self.snapshot()?,
            method: method.into(),
            arguments,
            plan: None,
            attempted: false,
            append_attempted: false,
            successful: false,
            resolved: false,
            #[cfg(test)]
            append_fault: None,
        })
    }
    fn require_queued_source_context(
        &self,
        queued: &NativeQueuedInlineDatabaseV1,
    ) -> Result<MetadataSnapshot> {
        self.journal
            .require_source_queued_live_writer(&queued.writer)?;
        let current = self.snapshot()?;
        if current.profile != queued.request.profile
            || current.context != queued.request.context
            || current.sequence < queued.request.sequence
        {
            return Err(CompactIndexError::WrongContext);
        }
        Ok(current)
    }
    /// The metadata query/update is evaluated NOW against actual current rows.
    /// This does not call source decode/logic/validation/failure hooks again.
    pub fn execute_queued_source_inline_database(
        &mut self,
        queued: &mut NativeQueuedInlineDatabaseV1,
        permit: Option<&SourceIndexGenerationEscrowPermitV1>,
    ) -> Result<NativeAppliedQueuedInlineDatabaseV1> {
        self.prepare_queued_source_inline_database(queued)?;
        self.append_prepared_queued_source_inline_database(queued, permit)
    }
    /// Begin this actual metadata operation at its execution point. The retained
    /// plan permits separately syncing required generation escrow before append.
    pub fn prepare_queued_source_inline_database(
        &self,
        queued: &mut NativeQueuedInlineDatabaseV1,
    ) -> Result<()> {
        self.require_live_metadata_write()?;
        let current = self.require_queued_source_context(queued)?;
        if queued.attempted || queued.plan.is_some() || queued.successful || queued.resolved {
            return Err(CompactIndexError::WrongContext);
        }
        queued.attempted = true;
        let plan =
            self.prepare_inline_database(&current, &queued.method, queued.arguments.clone())?;
        queued.plan = Some(Box::new(plan));
        Ok(())
    }
    /// Append the same actual late-prepared plan once. Other mutations between
    /// preparation and append remain a stale-plan refusal, never silent requery.
    pub fn append_prepared_queued_source_inline_database(
        &mut self,
        queued: &mut NativeQueuedInlineDatabaseV1,
        permit: Option<&SourceIndexGenerationEscrowPermitV1>,
    ) -> Result<NativeAppliedQueuedInlineDatabaseV1> {
        self.require_live_metadata_write()?;
        let current = self.require_queued_source_context(queued)?;
        if !queued.attempted || queued.append_attempted || queued.successful || queued.resolved {
            return Err(CompactIndexError::WrongContext);
        }
        if queued
            .plan
            .as_deref()
            .ok_or(CompactIndexError::WrongContext)?
            .expected
            != current
        {
            return Err(CompactIndexError::WrongContext);
        }
        queued.append_attempted = true;
        #[cfg(test)]
        if let Some(mode) = queued.append_fault {
            queued
                .plan
                .as_deref_mut()
                .expect("same actual queued plan")
                .put
                .as_mut()
                .expect("actual mutating operation")
                .append
                .test_append_fault(mode);
        }
        let plan = queued.plan.as_deref().expect("same actual queued plan");
        self.append_inline_database(plan, permit)?;
        let returned = self.resolve_inline_database_post(plan)?;
        let post = self.read_generation(&self.snapshot()?)?;
        let state = prepared_metadata_state(&post.data, &[])?;
        if state != plan.after_state {
            return Err(CompactIndexError::Corrupt);
        }
        queued.successful = true;
        Ok(NativeAppliedQueuedInlineDatabaseV1 {
            request_id: queued.request_id,
            writer: self.journal.capture_source_queued_live_writer()?,
            post,
            state,
            returned,
        })
    }
    /// Resolve the actual successful operation, even after later ordered commits
    /// on the SAME live writer. A read lease binds the original successful Post.
    pub fn resolve_applied_queued_source_inline_database(
        &self,
        queued: &mut NativeQueuedInlineDatabaseV1,
        applied: &NativeAppliedQueuedInlineDatabaseV1,
    ) -> Result<DependencyReturn> {
        self.journal
            .require_source_queued_live_writer(&applied.writer)?;
        if queued.request_id != applied.request_id {
            return Err(CompactIndexError::WrongContext);
        }
        let current = self.require_queued_source_context(queued)?;
        let plan = queued
            .plan
            .as_deref()
            .ok_or(CompactIndexError::WrongContext)?;
        if !queued.successful
            || !queued.attempted
            || !queued.append_attempted
            || queued.resolved
            || current.sequence < applied.post.pin().sequence
            || applied.post.pin().context != queued.request.context
            || applied.state != plan.after_state
            || applied.returned != plan.returned
            || prepared_metadata_state(&applied.post.data, &[])? != applied.state
        {
            return Err(CompactIndexError::WrongContext);
        }
        let original_post = if let Some(put) = &plan.put {
            let mut pin = plan.expected.clone();
            pin.publication = put.append.after().publication.clone();
            pin.sequence = put.append.after().logical_sequence;
            pin
        } else {
            plan.expected.clone()
        };
        if applied.post.pin() != &original_post {
            return Err(CompactIndexError::WrongContext);
        }
        queued.resolved = true;
        Ok(applied.returned.clone())
    }
}
