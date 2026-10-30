impl NativeMainRetainedCallerRouteV1 {
    pub(crate) fn from_actual_raw_startup_suffix(
        startup: &crate::source_main_raw_startup::SourceMainRawStartupV1,
        caller: &crate::source_contract_runtime::NativeMainRetainedCallerFrameV1,
    ) -> Result<Self, SourceRuleError> {
        let invocation = caller.native_main_retained_invocation_route()?;
        Ok(Self {
            value: obj(&[
                ("profile", s("TL-MAIN-ACTUAL-RAW-STARTUP-ROUTE-13-1")),
                ("actualStartup", startup.actual_live_route_fields()?),
                ("retainedInvocation", invocation),
            ]),
        })
    }
}
