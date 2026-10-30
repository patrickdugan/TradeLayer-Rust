impl MainStepKernelV1 {
    // Only the typed startup borrower calls this before the first live job.
    // Old Kernel constructors and all their bytes remain unchanged.
    pub(crate) fn from_closed_startup_clock(
        generation: u64,
        clock: crate::source_main_startup_prefix::MainStartupClockHandoffV1,
    ) -> Self {
        let (logical_time, next_timer) = clock.into_parts();
        let mut kernel = Self::new(generation);
        kernel.logical_time = n(logical_time as f64);
        kernel.next_timer = next_timer;
        kernel
    }
}
