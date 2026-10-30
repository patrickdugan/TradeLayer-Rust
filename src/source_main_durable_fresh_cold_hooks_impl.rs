// Private no-op-default seam only. Old public13-2 remains the same actor.
trait NativeMainActualFactInterleaveV1 {
    fn before_source(
        &mut self,
        _state: &mut NativeMainColdServiceStateV1,
        _history: &mut NativeHistoricalValidityPlaybackV1<'_>,
        _capability: &NativeColdIndexExecutionV1,
        _transcript: &crate::source_main_active_service_wal::VerifiedActualSourceFactTraceV1,
        _sequence: u64,
    ) -> Result<(), String> {
        Ok(())
    }
    fn after_source(
        &mut self,
        _state: &mut NativeMainColdServiceStateV1,
        _history: &mut NativeHistoricalValidityPlaybackV1<'_>,
        _capability: &NativeColdIndexExecutionV1,
        _transcript: &crate::source_main_active_service_wal::VerifiedActualSourceFactTraceV1,
        _sequence: u64,
    ) -> Result<(), String> {
        Ok(())
    }
    fn finish(
        &mut self,
        _state: &mut NativeMainColdServiceStateV1,
        _history: &mut NativeHistoricalValidityPlaybackV1<'_>,
        _capability: &NativeColdIndexExecutionV1,
        _transcript: &crate::source_main_active_service_wal::VerifiedActualSourceFactTraceV1,
        _completed: u64,
    ) -> Result<(), String> {
        Ok(())
    }
}
struct NativeMainNoActualFactInterleaveV1;
impl NativeMainActualFactInterleaveV1 for NativeMainNoActualFactInterleaveV1 {}
