impl SourceMainNativeDurableActiveServiceV1 {
    fn append_mixed_ordinary_scope_observation(raw: &mut Vec<u8>) -> ActiveWalResult<()> {
        let mut fields =
            NativeFieldCollector::new("Native13/MixedOrdinaryNativeFacts/v1", true, 1024);
        fields
            .emit(
                "requiredOrdinaryProfile",
                &s(crate::source_main_active_service_wal::ACTUAL_VALIDITY_PLAN_FACT_PROFILE),
            )
            .map_err(active_wal_backend)?;
        fields
            .emit(
                "ordinaryScope",
                &s(crate::source_main_active_service_wal::ACTUAL_VALIDITY_FACT_SCOPE),
            )
            .map_err(active_wal_backend)?;
        fields
            .emit("sourceScope", &s(MIXED_SOURCE_FACT_SCOPE))
            .map_err(active_wal_backend)?;
        raw.extend_from_slice(fields.frames.as_deref().expect("retained explicit scope"));
        Ok(())
    }
    fn append_mixed_recorded_scope(raw: &mut Vec<u8>, active: bool) -> ActiveWalResult<()> {
        let mut fields =
            NativeFieldCollector::new("Native13/MixedRecordedNativeScope/v1", true, 1024);
        fields
            .emit("recordedNativeOnlyActive", &V::Bool(active))
            .map_err(active_wal_backend)?;
        raw.extend_from_slice(fields.frames.as_deref().expect("retained actual scope"));
        Ok(())
    }
}
