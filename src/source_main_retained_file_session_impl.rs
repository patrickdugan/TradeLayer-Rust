// First explicit file/caller borrower. The automatic block checkpoint→native
// tail handoff is a separate pending component, never an invented block result.
pub const NATIVE_MAIN_RETAINED_FILE_PROFILE: &str = "TL-MAIN-RETAINED-CALLER-FILE-RETRY-13-1";
pub struct SourceMainNativeCallerFileRetriesV1 {
    caller: Box<SourceMainNativeRetainedCallerV1>,
    files: crate::source_main_step_kernel::MainStepKernelV1,
    events: Vec<crate::source_main_step_kernel::MainStepEvaluationV1>,
    terminal: Option<u8>,
}
impl SourceMainNativeSessionV13 {
    pub fn into_retained_caller_with_file_retries(
        self,
        expected: &NativeMainSessionExpectation,
        command: NativeMainRetainedCallerV1,
        scope: u64,
        generation: u64,
        documents: V,
    ) -> Result<SourceMainNativeCallerFileRetriesV1, Box<RejectedNativeMainRetainedCallerV1>> {
        let caller = self.into_retained_main_caller_profile(
            expected, command, scope, generation, documents, true,
        )?;
        Ok(SourceMainNativeCallerFileRetriesV1 {
            caller: Box::new(caller),
            files: crate::source_main_step_kernel::MainStepKernelV1::with_file_retries(generation),
            events: vec![],
            terminal: None,
        })
    }
}
impl SourceMainNativeCallerFileRetriesV1 {
    pub fn profile(&self) -> &'static str {
        NATIVE_MAIN_RETAINED_FILE_PROFILE
    }
    pub fn apply_file(
        &mut self,
        command: crate::source_main_step_kernel::MainStepCommandV1,
    ) -> Result<
        &crate::source_main_step_kernel::MainStepEvaluationV1,
        crate::source_main_step_kernel::MainStepError,
    > {
        use crate::source_main_step_kernel::{MainStepError, MainStepStop};
        if self.terminal.is_some() {
            return Err(MainStepError::AlreadyExited);
        }
        if self.events.len() >= 4096 {
            return Err(MainStepError::Capacity);
        }
        if !native_main_retained_file_command_admitted(&command) {
            return Err(MainStepError::InvalidInput);
        }
        self.events
            .try_reserve(1)
            .map_err(|_| MainStepError::Capacity)?;
        let caller = &mut self.caller;
        let evaluation = caller
            .owner
            .financial
            .native_main_runtime_mut()
            .native_main_retained_file_apply(&caller.owner.index, &mut self.files, command)?;
        if let MainStepStop::ProcessExited(code) = evaluation.stop {
            self.terminal = Some(code)
        }
        self.events.push(evaluation);
        Ok(self.events.last().expect("actual file evaluation retained"))
    }
    pub fn next_file_timer(&self) -> Option<u64> {
        self.files.next_timer()
    }
    pub fn file_kernel_value(&self) -> V {
        self.files.value()
    }
    pub fn effects(&self) -> &[V] {
        self.caller.effects()
    }
    pub fn financial_state(&self) -> Result<V, SourceRuleError> {
        self.caller.financial_state()
    }
    pub fn visit_canonical_fields(
        &self,
        visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
    ) -> Result<(), SourceRuleError> {
        visitor("profile", &s(NATIVE_MAIN_RETAINED_FILE_PROFILE))?;
        visitor(
            "terminal",
            &self
                .terminal
                .map_or(V::Null, |code| V::number(f64::from(code))),
        )?;
        self.caller
            .visit_canonical_fields(&mut |p, v| visitor(&format!("retainedCaller/{p}"), v))?;
        self.files
            .visit_canonical_fields(&mut |p, v| visitor(&format!("fileKernel/{p}"), v))?;
        visitor("events/count", &s(&self.events.len().to_string()))?;
        for (at, event) in self.events.iter().enumerate() {
            event.visit_canonical_fields(&mut |path, value| {
                visitor(&format!("events/{at}/{path}"), value)
            })?;
        }
        Ok(())
    }
}
