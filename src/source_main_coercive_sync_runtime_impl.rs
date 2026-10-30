impl NativeMainRetainedCallerFrameV1 {
    pub(crate) fn native_main_coercive_sync_enabled(&self) -> bool {
        self.coercive_sync
    }
}
impl SourceContractRuntime {
    pub(crate) fn native_main_coercive_sync_preflight(
        &self,
        max: &V,
        documents: &V,
    ) -> Result<(), SourceRuleError> {
        self.native_main_service_preflight(documents)?;
        max.validate()?;
        if !native_main_sync_admitted_max(max)
            || !native_main_sync_scalar(
                &self
                    .core
                    .main_production
                    .as_ref()
                    .ok_or(SourceRuleError::InvalidInput)?
                    .io
                    .get("height"),
            )
        {
            // Null max requires the source's additional detached initOrLoad
            // job. Objects require identity/ToPrimitive semantics. Neither is
            // silently normalized into this explicitly primitive profile.
            return Err(SourceRuleError::UnknownDependency);
        }
        Ok(())
    }
    pub(crate) fn native_main_coercive_sync_begin(
        &mut self,
        index: &mut SourceCompactIndex,
        max: V,
        scope: u64,
        generation: u64,
        documents: &V,
    ) -> Result<NativeMainRetainedCallerFrameV1, SourceRuleError> {
        self.native_main_coercive_sync_preflight(&max, documents)?;
        let mut frame = self.native_main_retained_caller_begin(
            index,
            NativeMainRetainedCallerV1::SyncIndex { max },
            scope,
            generation,
            documents,
        )?;
        frame.coercive_sync = true;
        Ok(frame)
    }
    fn native_main_coercive_sync_step(&mut self, f: &mut NativeMainRetainedCallerFrameV1) -> R<()> {
        let tip = match self
            .core
            .source_main_production_io("Client.getBlockCount", &[])
        {
            Ok(value) => value,
            // Main.syncIndex catches this awaited RPC failure. By contrast
            // its returned consensus Promise is not awaited inside the try;
            // that genuine child is handled by the unchanged 3a borrower.
            Err(Fault::Thrown(_)) => {
                self.native_main_retained_sync_return(f);
                return Ok(());
            }
            Err(error) => return Err(error),
        };
        if !native_main_sync_scalar(&tip) {
            return Err(SourceRuleError::UnknownDependency.into());
        }
        let decision = native_main_sync_scalar_decision(&f.max, &tip, number)?;
        let start = match decision {
            NativeMainCoerciveSyncDecision::ExtractOriginal => Some(f.max.clone()),
            NativeMainCoerciveSyncDecision::ExtractGenesis => Some(f.genesis.clone()),
            NativeMainCoerciveSyncDecision::Consensus => {
                f.pc = NativeMainRetainedCallerPcV1::Consensus;
                None
            }
            NativeMainCoerciveSyncDecision::Return => {
                self.native_main_retained_sync_return(f);
                None
            }
        };
        if let Some(start) = start {
            let owner = &self
                .core
                .main_production
                .as_ref()
                .ok_or(SourceRuleError::InvalidInput)?
                .index;
            f.extract = Some(Box::new(
                owner.begin_retained_extract(f.caller_scope + 3, start)?,
            ));
            f.pc = NativeMainRetainedCallerPcV1::Extract;
        }
        Ok(())
    }
}
