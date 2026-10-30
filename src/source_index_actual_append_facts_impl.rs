pub const ACTUAL_INLINE_APPEND_FACT_PROFILE: &str = "TL-TXINDEX-ACTUAL-INLINE-APPEND-FACT-13-1";
/// Typed result of the real native call, not a decoded expected outcome. The
/// owned error/result survives fact serialization failure and source stays held.
pub struct ActualInlineAppendAttemptV1 {
    result: Result<()>,
    pending: crate::source_main_active_service_wal::ActualNativePendingPlanFactV1,
    outcome: V,
    post: Option<SourceIndexHistoryPinV1>,
}
impl ActualInlineAppendAttemptV1 {
    pub fn actual_result(&self) -> &Result<()> {
        &self.result
    }
    pub fn pending_fact(
        &self,
    ) -> &crate::source_main_active_service_wal::ActualNativePendingPlanFactV1 {
        &self.pending
    }
    pub fn outcome(&self) -> &V {
        &self.outcome
    }
    pub fn verified_post(&self) -> Option<&SourceIndexHistoryPinV1> {
        self.post.as_ref()
    }
    pub fn into_actual_result(self) -> Result<()> {
        self.result
    }
    pub fn canonical_io_fact_bytes(&self) -> Result<Vec<u8>> {
        let raw = prepared_obj(vec![
            ("profile", prepared_text(ACTUAL_INLINE_APPEND_FACT_PROFILE)),
            (
                "actualPendingFactSHA256",
                prepared_text(prepared_hex(&crate::hash256::sha256(
                    self.pending.canonical_fact_bytes(),
                ))),
            ),
            (
                "stage",
                prepared_text("SourceCompactIndex.append_inline_database"),
            ),
            ("actualOutcome", self.outcome.clone()),
            (
                "verifiedPostHex",
                match &self.post {
                    Some(post) => prepared_text(prepared_hex(&post.canonical_scalar_bytes()?)),
                    None => V::Null,
                },
            ),
        ])
        .diagnostic_json()
        .map_err(|_| CompactIndexError::Limit)?
        .into_bytes();
        if raw.len() > crate::source_main_active_service_wal::MAX_BLOB {
            return Err(CompactIndexError::Limit);
        }
        Ok(raw)
    }
}
fn actual_inline_outcome(result: &Result<()>, write: bool) -> V {
    let fields = match result {
        Ok(()) => vec![(
            "kind",
            prepared_text(if write {
                "NativeAppendReturned"
            } else {
                "NoNativeMutation"
            }),
        )],
        Err(CompactIndexError::Storage(StorageError::Io(error))) => vec![
            ("kind", prepared_text("NativeIoError")),
            ("ioKind", prepared_text(format!("{:?}", error.kind()))),
            (
                "rawOsError",
                error
                    .raw_os_error()
                    .map_or(V::Null, |v| prepared_text(v.to_string())),
            ),
            ("ioDisplay", prepared_text(error.to_string())),
            (
                "exactSourceDisplay",
                prepared_text(result.as_ref().unwrap_err().to_string()),
            ),
        ],
        Err(error) => vec![
            ("kind", prepared_text("StructuralOrResourceRejection")),
            ("errorVariant", prepared_text(format!("{error:?}"))),
            ("exactSourceDisplay", prepared_text(error.to_string())),
        ],
    };
    prepared_obj(fields)
}
impl SourceCompactIndex {
    /// Separate explicit producer. The old append body executes once unchanged;
    /// no source PC is advanced and no expected outcome is decoded/imported.
    pub fn append_inline_database_capture_actual_fact(
        &mut self,
        plan: &NativePreparedInlineDatabaseV1,
        permit: Option<&SourceIndexGenerationEscrowPermitV1>,
        wal: &crate::source_main_active_service_wal::ActiveServiceWalV1,
        binding: &crate::source_main_active_service_wal::ActualNativeFactBindingV1,
    ) -> Result<ActualInlineAppendAttemptV1> {
        self.append_inline_database_capture_actual_fact_before(plan, permit, wal, binding, || ())
    }
    pub(crate) fn append_inline_database_capture_actual_fact_before(
        &mut self,
        plan: &NativePreparedInlineDatabaseV1,
        permit: Option<&SourceIndexGenerationEscrowPermitV1>,
        wal: &crate::source_main_active_service_wal::ActiveServiceWalV1,
        binding: &crate::source_main_active_service_wal::ActualNativeFactBindingV1,
        before: impl FnOnce(),
    ) -> Result<ActualInlineAppendAttemptV1> {
        self.require_live_metadata_write()?; // before any metadata or file IO
        let pending = binding
            .capture_pending_plan_fact(wal, self, plan)
            .map_err(|_| CompactIndexError::WrongContext)?;
        binding
            .require_synced_native_intent(wal)
            .map_err(|_| CompactIndexError::WrongContext)?;
        // Pending facts exist before native IO. Capturing result here is before
        // the Runtime converts native errors to StorageStopped/source observation.
        before();
        let result = self.append_inline_database(plan, permit);
        let outcome = actual_inline_outcome(&result, plan.append_plan().is_some());
        let post = if result.is_ok() {
            self.capture_verified_history_pin().ok()
        } else {
            None
        };
        Ok(ActualInlineAppendAttemptV1 {
            result,
            pending,
            outcome,
            post,
        })
    }
}
