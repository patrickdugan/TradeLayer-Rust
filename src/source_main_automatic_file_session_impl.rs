pub use crate::source_contract_runtime::NATIVE_MAIN_AUTOMATIC_FILE_SERVICE_PROFILE;
pub struct SourceMainNativeAutomaticFileServiceV1 {
    active: Box<SourceMainNativeActiveServiceV1>,
    controls: Vec<(
        crate::source_main_step_kernel::MainStepCommandV1,
        Result<u64, MainStepError>,
    )>,
}
impl SourceMainNativeSessionV13 {
    pub fn into_automatic_file_active_service(
        self,
        expected: &NativeMainSessionExpectation,
        start: V,
        generation: u64,
        documents: V,
        observation_limit: usize,
    ) -> Result<SourceMainNativeAutomaticFileServiceV1, Box<RejectedNativeMainActiveServiceV1>>
    {
        let active = self.into_active_service_profile(
            expected,
            start,
            generation,
            documents,
            (observation_limit, true, true),
        )?;
        Ok(SourceMainNativeAutomaticFileServiceV1 {
            active: Box::new(active),
            controls: vec![],
        })
    }
}
impl SourceMainNativeAutomaticFileServiceV1 {
    pub fn profile(&self) -> &'static str {
        NATIVE_MAIN_AUTOMATIC_FILE_SERVICE_PROFILE
    }
    pub fn apply(
        &mut self,
        action: NativeMainActiveServiceActionV1,
    ) -> Result<&NativeMainActiveServiceReceiptV1, NativeMainActiveServiceErrorV1> {
        let expected = self.active.expectation()?;
        self.active.apply(&expected, action)
    }
    /// Additional raw source calls on the SAME Kernel. The file child remains
    /// pending; its result is never a user input or a selected first-frame guess.
    pub fn enqueue_control(
        &mut self,
        command: crate::source_main_step_kernel::MainStepCommandV1,
    ) -> Result<u64, MainStepError> {
        use crate::source_main_step_kernel::MainStepCommandV1 as Command;
        if self
            .active
            .frame
            .as_ref()
            .is_some_and(|frame| frame.terminal_code().is_some())
        {
            return Err(MainStepError::AlreadyExited);
        }
        if self.active.observation_pending() || self.controls.len() >= 4096 {
            return Err(MainStepError::InvalidInput);
        }
        if !matches!(&command, Command::Start { .. })
            || !native_main_retained_file_command_admitted(&command)
        {
            return Err(MainStepError::InvalidInput);
        }
        if Command::from_value(&command.value()).ok().as_ref() != Some(&command) {
            return Err(MainStepError::InvalidInput);
        }
        self.controls
            .try_reserve(1)
            .map_err(|_| MainStepError::Capacity)?;
        let result = {
            let active = &mut self.active;
            let frame = active
                .frame
                .as_deref_mut()
                .ok_or(MainStepError::InvalidInput)?;
            active
                .owner
                .financial
                .native_main_runtime_mut()
                .native_main_automatic_file_enqueue(&mut active.owner.index, frame, command.clone())
        };
        self.controls.push((command, result.clone()));
        result
    }
    pub fn next_timer(&self) -> Option<u64> {
        self.active.next_timer()
    }
    pub fn kernel_value(&self) -> Option<V> {
        self.active.kernel_value()
    }
    pub fn effects(&self) -> &[V] {
        self.active.effects()
    }
    pub fn financial_state(&self) -> Result<V, SourceRuleError> {
        self.active.financial_runtime().state_value()
    }
    pub fn caller_result(&self) -> Option<&Result<DependencyReturn, SourceRuleError>> {
        self.active.caller_result()
    }
    pub fn pending_inline_write(
        &self,
    ) -> Option<&crate::source_compact_index_v2::NativePreparedInlineDatabaseV1> {
        self.active.pending_inline_write()
    }
    pub fn pending_write(
        &self,
    ) -> Option<&crate::source_compact_index_v2::NativePreparedValidityWriteV1> {
        self.active.pending_write()
    }
    pub fn visit_canonical_fields(
        &self,
        visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
    ) -> Result<(), SourceRuleError> {
        visitor("profile", &s(NATIVE_MAIN_AUTOMATIC_FILE_SERVICE_PROFILE))?;
        self.active
            .visit_canonical_fields(&mut |p, v| visitor(&format!("active/{p}"), v))?;
        visitor("controls/count", &s(&self.controls.len().to_string()))?;
        for (at, (command, result)) in self.controls.iter().enumerate() {
            crate::source_contract_runtime::native_main_row_visit_value(
                &format!("controls/{at}/raw"),
                &command.value(),
                visitor,
            )?;
            visitor(&format!("controls/{at}/result"), &s(&format!("{result:?}")))?;
        }
        Ok(())
    }
}
