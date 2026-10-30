// New typed source-only handoff; never rerun constructor/1500/global500/300.
impl SourceContractRuntime {
    pub(crate) fn native_main_raw_startup_suffix_preflight(
        &self,
        prefix: &crate::source_main_startup_prefix::MainStartupPrefixV1,
        io: &NativeMainStartupIoStateV1,
        scope: u64,
        documents: &V,
        production_io: &V,
    ) -> Result<(), SourceRuleError> {
        if self.policy() != SourceContractRuntimePolicy::MainOrchestrationV12
            || prefix.stop()
                != crate::source_main_startup_prefix::StartupStopV1::InitializeSuffixReady
            || !prefix.database_complete()
            || io.invocation.is_none()
            || io.suffix_attempted
            || io.main_outcome.is_some()
            || scope == 0
            || scope > u64::MAX - 4096
        {
            return Err(SourceRuleError::InvalidInput);
        }
        documents.validate()?;
        production_io.validate()?;
        let V::Array(rows) = documents else {
            return Err(SourceRuleError::InvalidInput);
        };
        for row in rows {
            let V::Object(fields) = row else {
                return Err(SourceRuleError::InvalidInput);
            };
            if fields
                .iter()
                .any(|(key, _)| !["id", "raw", "document"].contains(&key.as_str()))
                || !matches!(row.get("id"), V::String(_))
                || !matches!(row.get("raw"), V::Undefined | V::String(_))
            {
                return Err(SourceRuleError::InvalidInput);
            }
        }
        let V::Object(fields) = production_io else {
            return Err(SourceRuleError::InvalidInput);
        };
        if fields.iter().any(|(key, _)| {
            !["height", "blocks", "networkInfo", "failures"].contains(&key.as_str())
        }) {
            return Err(SourceRuleError::InvalidInput);
        }
        if let V::Array(failures) = production_io.get("failures") {
            for failure in failures {
                if ![
                    s("Client.getBlockCount"),
                    s("Client.getBlock"),
                    s("Client.getNetworkInfo"),
                ]
                .contains(&failure.get("method"))
                {
                    // Cannot retroactively supply failures for already-executed
                    // prefix getChain/getTest or client capability requests.
                    return Err(SourceRuleError::InvalidInput);
                }
            }
        } else if production_io.get("failures") != V::Undefined {
            return Err(SourceRuleError::InvalidInput);
        }
        Ok(())
    }
    pub(crate) fn native_main_raw_startup_suffix_begin(
        &mut self,
        index: &mut SourceCompactIndex,
        io: &mut NativeMainStartupIoStateV1,
        scope: u64,
        generation: u64,
        production_io: V,
    ) -> Result<NativeMainRetainedCallerFrameV1, SourceRuleError> {
        if io.suffix_attempted || io.invocation.is_none() {
            return Err(SourceRuleError::InvalidInput);
        }
        io.suffix_attempted = true;
        let snapshot = index.snapshot().map_err(|_| SourceRuleError::Backend)?;
        let binding = obj(&[
            ("profile", s("TL-MAIN-RAW-STARTUP-TXINDEX-13-1")),
            ("chain", s(&io.inputs.chain)),
            ("test", V::Bool(io.inputs.test)),
            ("context", s(snapshot.context)),
            ("metadataProfile", s(snapshot.profile)),
        ]);
        self.native_main_bind(binding)?;
        self.core
            .main_production
            .as_mut()
            .ok_or(SourceRuleError::InvalidInput)?
            .io = production_io;
        let (instance, genesis) = match self.native_main_retained_initialize_suffix(
            index,
            Some(&io.inputs.main_client),
            Some(&io.inputs.sdk_client_fields),
        ) {
            Ok(value) => value,
            Err(Fault::Thrown(error)) => {
                let invocation = io
                    .invocation
                    .take()
                    .expect("retained actual Main invocation");
                io.main_outcome =
                    Some(self.source_main_invocation_finish(invocation, Err(Fault::Thrown(error))));
                return Err(SourceRuleError::Backend);
            }
            Err(Fault::Local(error)) => return Err(error),
        };
        let inline = self.core.native_main_inline_index_begin(
            scope + 1,
            crate::txindex_source_owner::TxIndexInlineMethodV1::Initialize,
            vec![genesis.clone()],
        )?;
        let effect_start = io.main_effect_start.ok_or(SourceRuleError::InvalidInput)?;
        Ok(NativeMainRetainedCallerFrameV1 {
            command: NativeMainRetainedCallerV1::Initialize,
            coercive_sync: false,
            null_group_member: false,
            invocation: io.invocation.take(),
            caller_scope: scope,
            pc: NativeMainRetainedCallerPcV1::InstanceInit,
            max: V::Undefined,
            genesis,
            instance,
            inline: Some(Box::new(inline)),
            extract: None,
            extract_io: Default::default(),
            kernel: crate::source_main_step_kernel::MainStepKernelV1::new(generation),
            timer_started: false,
            timer_done: false,
            caller: None,
            final_result: None,
            local_stop: None,
            effect_start,
        })
    }
}
