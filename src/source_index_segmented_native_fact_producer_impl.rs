pub const ACTUAL_VALIDITY_APPEND_FACT_PROFILE: &str = "TL-TXINDEX-ACTUAL-VALIDITY-APPEND-FACT-13-1";
/// Owns the result of exactly one actual native call. A serialization/closure
/// refusal retains this object; there is no supplied validity/source verdict.
pub struct ActualValidityAppendAttemptV1 {
    result: Result<StreamedValidityEvaluation>,
    pending: crate::source_main_active_service_wal::ActualValidityPendingFactV1,
    io: Option<
        std::result::Result<crate::source_main_active_service_wal::ActualValidityIoDataV1, String>,
    >,
    post: Option<SourceIndexHistoryPinV1>,
}
impl ActualValidityAppendAttemptV1 {
    pub fn actual_result(&self) -> &Result<StreamedValidityEvaluation> {
        &self.result
    }
    pub fn pending_fact(
        &self,
    ) -> &crate::source_main_active_service_wal::ActualValidityPendingFactV1 {
        &self.pending
    }
    pub fn verified_post(&self) -> Option<&SourceIndexHistoryPinV1> {
        self.post.as_ref()
    }
    pub fn into_actual_result(self) -> Result<StreamedValidityEvaluation> {
        self.result
    }
    pub fn canonical_io_fact_bytes(&self) -> Result<Vec<u8>> {
        let outcome = match &self.result {
            Ok(_) => {
                if self.post.is_none() {
                    return Err(CompactIndexError::WrongContext);
                }
                prepared_obj(vec![("kind", prepared_text("NativeAppendReturned"))])
            }
            Err(CompactIndexError::Storage(StorageError::Io(_))) => {
                let data = self
                    .io
                    .as_ref()
                    .and_then(|v| v.as_ref().ok())
                    .ok_or(CompactIndexError::WrongContext)?;
                prepared_obj(vec![
                    ("kind", prepared_text("NativeIoError")),
                    ("io", data.value()),
                ])
            }
            // Structural/resource/validation results are NOT environmental IO facts.
            Err(_) => return Err(CompactIndexError::WrongContext),
        };
        let value = prepared_obj(vec![
            (
                "profile",
                prepared_text(ACTUAL_VALIDITY_APPEND_FACT_PROFILE),
            ),
            (
                "actualPendingFactSHA256",
                prepared_text(prepared_hex(&crate::hash256::sha256(
                    self.pending.canonical_fact_bytes(),
                ))),
            ),
            (
                "stage",
                prepared_text("SourceCompactIndex.commit_prepared_validity"),
            ),
            ("actualOutcome", outcome),
            (
                "verifiedPostHex",
                match &self.post {
                    Some(p) => prepared_text(prepared_hex(&p.canonical_scalar_bytes()?)),
                    None => V::Null,
                },
            ),
        ]);
        let raw = value.diagnostic_json()?.into_bytes();
        if raw.len() > crate::source_main_active_service_wal::MAX_BLOB {
            return Err(CompactIndexError::Limit);
        }
        Ok(raw)
    }
}
impl SourceCompactIndex {
    pub fn commit_validity_capture_actual_fact(
        &mut self,
        plan: &NativePreparedValidityWriteV1,
        permit: Option<&SourceIndexGenerationEscrowPermitV1>,
        wal: &crate::source_main_active_service_wal::ActiveServiceWalV1,
        binding: &crate::source_main_active_service_wal::ActualValidityFactBindingV1,
    ) -> Result<ActualValidityAppendAttemptV1> {
        self.commit_validity_capture_actual_fact_before(plan, permit, wal, binding, || ())
    }
    pub(crate) fn commit_validity_capture_actual_fact_before(
        &mut self,
        plan: &NativePreparedValidityWriteV1,
        permit: Option<&SourceIndexGenerationEscrowPermitV1>,
        wal: &crate::source_main_active_service_wal::ActiveServiceWalV1,
        binding: &crate::source_main_active_service_wal::ActualValidityFactBindingV1,
        before: impl FnOnce(),
    ) -> Result<ActualValidityAppendAttemptV1> {
        self.require_live_metadata_write()?;
        let pending = binding
            .capture_pending_fact(wal, self, plan)
            .map_err(|_| CompactIndexError::WrongContext)?;
        binding
            .require_synced_intent(wal)
            .map_err(|_| CompactIndexError::WrongContext)?;
        before();
        let result = self.commit_prepared_validity_with_escrow(plan, permit);
        let io = match &result {
            Err(CompactIndexError::Storage(StorageError::Io(error))) => Some(
                crate::source_main_active_service_wal::ActualValidityIoDataV1::capture(error)
                    .map_err(|e| e.to_string()),
            ),
            _ => None,
        };
        let post = if result.is_ok() {
            self.capture_verified_history_pin().ok()
        } else {
            None
        };
        Ok(ActualValidityAppendAttemptV1 {
            result,
            pending,
            io,
            post,
        })
    }
}
