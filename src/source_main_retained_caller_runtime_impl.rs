// Explicit source caller component. Its one invocation owns the borrowed queue
// until all admitted detached work is done. ConsensusReady is a retained handoff,
// not caller fulfillment or permission to release the financial owner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeMainRetainedCallerV1 {
    Initialize,
    InitOrLoad,
    SyncIndex { max: V },
}
impl NativeMainRetainedCallerV1 {
    pub(crate) fn kind(&self) -> &'static str {
        match self {
            Self::Initialize => "Initialize",
            Self::InitOrLoad => "InitOrLoad",
            Self::SyncIndex { .. } => "SyncIndex",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeMainRetainedCallerStopV1 {
    PendingTimer,
    CallerReturned,
    ConsensusReady,
    Complete,
    Native(NativeMainRowStopV1),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeMainRetainedCallerPendingV1 {
    caller_scope: u64,
    source_job: u64,
    method_scope: u64,
    operation_ordinal: u64,
    role: crate::txindex_source_owner::TxIndexExtractAwaitRoleV1,
}
impl NativeMainRetainedCallerPendingV1 {
    pub fn value(&self) -> V {
        obj(&[
            ("profile", s("TL-MAIN-CALLER-PENDING-13-1")),
            ("callerScope", s(self.caller_scope.to_string())),
            ("sourceJob", s(self.source_job.to_string())),
            ("methodScope", s(self.method_scope.to_string())),
            ("operationOrdinal", s(self.operation_ordinal.to_string())),
            ("role", s(format!("{:?}", self.role))),
        ])
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NativeMainRetainedCallerPcV1 {
    Delay,
    InitializePrefix,
    InstanceInit,
    Check,
    InitMax,
    StaticInit,
    Sync,
    Extract,
    Detached,
    Consensus,
    Done,
}
pub(crate) struct NativeMainRetainedCallerFrameV1 {
    command: NativeMainRetainedCallerV1,
    coercive_sync: bool,
    null_group_member: bool,
    invocation: Option<SourceMainInvocationV1>,
    caller_scope: u64,
    pc: NativeMainRetainedCallerPcV1,
    max: V,
    genesis: V,
    instance: V,
    inline: Option<Box<NativeMainInlineIndexFrameV1>>,
    extract: Option<Box<crate::txindex_source_owner::TxIndexExtractContinuationV1>>,
    extract_io: NativeMainRetainedInlineIoV1,
    kernel: crate::source_main_step_kernel::MainStepKernelV1,
    timer_started: bool,
    timer_done: bool,
    caller: Option<DependencyReturn>,
    final_result: Option<Result<DependencyReturn, SourceRuleError>>,
    local_stop: Option<SourceRuleError>,
    effect_start: usize,
}
// This loan executes no financial/source primitive: the actual Delay frame
// allocates/fires its timer entirely inside the existing production Kernel.
struct NativeMainCallerDelayLoan<'a> {
    core: &'a mut ContractCore,
}
impl crate::source_main_step_kernel::sealed::Sealed for NativeMainCallerDelayLoan<'_> {}
impl crate::source_main_step_kernel::MainStepOwner for NativeMainCallerDelayLoan<'_> {
    fn preflight(
        &self,
        _: &crate::source_main_step_kernel::MainStepPrimitive,
    ) -> Result<(), SourceRuleError> {
        if self.core.effects_exhausted {
            Err(SourceRuleError::Capacity)
        } else {
            Err(SourceRuleError::UnknownDependency)
        }
    }
    fn execute(
        &mut self,
        _: &crate::source_main_step_kernel::MainStepPrimitive,
    ) -> crate::source_main_step_kernel::MainStepPrimitiveExecution {
        crate::source_main_step_kernel::MainStepPrimitiveExecution {
            outcome: crate::source_main_step_kernel::MainStepPrimitiveOutcome::Stopped(
                SourceRuleError::UnknownDependency,
            ),
            effects: vec![],
        }
    }
}
impl NativeMainRetainedCallerFrameV1 {
    pub(crate) fn caller_result(&self) -> Option<&DependencyReturn> {
        self.caller.as_ref()
    }
    pub(crate) fn next_timer(&self) -> Option<u64> {
        self.kernel.next_timer()
    }
    pub(crate) fn pending_descriptor(&self) -> Option<NativeMainRetainedCallerPendingV1> {
        if let Some(inline) = &self.inline {
            inline.source.pending_call()?;
            return Some(NativeMainRetainedCallerPendingV1 {
                caller_scope: self.caller_scope,
                source_job: 0,
                method_scope: inline.scope,
                operation_ordinal: inline.source.pending_write_ordinal(),
                role: crate::txindex_source_owner::TxIndexExtractAwaitRoleV1::Awaited,
            });
        }
        let pending = self.extract.as_ref()?.pending_descriptor()?;
        Some(NativeMainRetainedCallerPendingV1 {
            caller_scope: self.caller_scope,
            source_job: pending.source_job().checked_add(1)?,
            method_scope: pending.method_scope(),
            operation_ordinal: pending.operation_ordinal(),
            role: pending.role(),
        })
    }
    pub(crate) fn pending_write(
        &self,
    ) -> Option<&crate::source_compact_index_v2::NativePreparedInlineDatabaseV1> {
        self.inline
            .as_ref()
            .and_then(|f| f.pending_write())
            .or(self.extract_io.write.as_deref())
    }
    pub(crate) fn visit_canonical_fields(
        &self,
        visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
    ) -> Result<(), SourceRuleError> {
        visitor("profile", &s("TL-MAIN-RETAINED-CALLER-SOURCE-13-1"))?;
        visitor("command", &s(self.command.kind()))?;
        if let NativeMainRetainedCallerV1::SyncIndex { max } = &self.command {
            native_main_row_visit_value("command/max", max, visitor)?;
        }
        if self.coercive_sync {
            visitor("coerciveSyncProfile", &s(NATIVE_MAIN_COERCIVE_SYNC_PROFILE))?;
        }
        if self.null_group_member {
            visitor("nullSyncGroupMember", &V::Bool(true))?;
        }
        visitor("callerScope", &s(self.caller_scope.to_string()))?;
        visitor("pc", &s(format!("{:?}", self.pc)))?;
        for (name, value) in [
            ("max", &self.max),
            ("genesis", &self.genesis),
            ("instance", &self.instance),
        ] {
            native_main_row_visit_value(name, value, visitor)?;
        }
        visitor("effectStart", &s(self.effect_start.to_string()))?;
        visitor("timerStarted", &V::Bool(self.timer_started))?;
        visitor("timerDone", &V::Bool(self.timer_done))?;
        self.kernel
            .visit_canonical_fields(&mut |p, v| visitor(&format!("kernel/{p}"), v))?;
        visitor("invocation/present", &V::Bool(self.invocation.is_some()))?;
        if let Some(i) = &self.invocation {
            visitor("invocation/target", &s(&i.target))?;
            visitor("invocation/spotMain", &V::Bool(i.spot_main))?;
            visitor(
                "invocation/rejectedBefore",
                &s(i.rejected_before.to_string()),
            )?;
            visitor(
                "invocation/transferRejectedBefore",
                &s(i.transfer_rejected_before.to_string()),
            )?;
        }
        visitor("inline/present", &V::Bool(self.inline.is_some()))?;
        if let Some(inline) = &self.inline {
            inline.visit_fields(&mut |p, v| visitor(&format!("inline/{p}"), v))?;
        }
        visitor("extract/present", &V::Bool(self.extract.is_some()))?;
        if let Some(extract) = &self.extract {
            let mut result = Ok(());
            extract.visit_fields(&mut |p, v| {
                if result.is_ok() {
                    result = native_main_row_visit_value(&format!("extract/{p}"), v, visitor)
                }
            });
            result?;
        }
        self.extract_io
            .visit_fields(&mut |p, v| visitor(&format!("extractIo/{p}"), v))?;
        visitor(
            "pending/present",
            &V::Bool(self.pending_descriptor().is_some()),
        )?;
        if let Some(pending) = self.pending_descriptor() {
            native_main_row_visit_value("pending/descriptor", &pending.value(), visitor)?;
        }
        visitor("caller/present", &V::Bool(self.caller.is_some()))?;
        if let Some(caller) = &self.caller {
            native_main_row_visit_value(
                "caller",
                &crate::source_contract_replay::SourceContractCommandOutcome::Dependency(
                    caller.clone(),
                )
                .value(),
                visitor,
            )?;
        }
        visitor("final/present", &V::Bool(self.final_result.is_some()))?;
        if let Some(result) = &self.final_result {
            match result {
                Ok(v) => native_main_row_visit_value(
                    "final/returned",
                    &crate::source_contract_replay::SourceContractCommandOutcome::Dependency(
                        v.clone(),
                    )
                    .value(),
                    visitor,
                )?,
                Err(e) => visitor("final/local", &s(format!("{e:?}")))?,
            }
        }
        visitor(
            "localStop",
            &self
                .local_stop
                .as_ref()
                .map_or(V::Null, |e| s(format!("{e:?}"))),
        )
    }
}
impl SourceContractRuntime {
    pub(crate) fn native_main_retained_caller_preflight(
        &self,
        command: &NativeMainRetainedCallerV1,
        documents: &V,
    ) -> Result<(), SourceRuleError> {
        self.native_main_service_preflight(documents)?;
        if let NativeMainRetainedCallerV1::SyncIndex { max } = command {
            native_main_retained_height(max)?;
        }
        Ok(())
    }
    pub(crate) fn native_main_retained_caller_begin(
        &mut self,
        index: &mut SourceCompactIndex,
        command: NativeMainRetainedCallerV1,
        scope: u64,
        generation: u64,
        documents: &V,
    ) -> Result<NativeMainRetainedCallerFrameV1, SourceRuleError> {
        if scope == 0 || scope > u64::MAX - 4096 {
            return Err(SourceRuleError::InvalidInput);
        }
        let (target, args, pc, max) = match &command {
            NativeMainRetainedCallerV1::Initialize => (
                "Main.initialize",
                vec![],
                NativeMainRetainedCallerPcV1::Delay,
                V::Undefined,
            ),
            NativeMainRetainedCallerV1::InitOrLoad => (
                "Main.initOrLoadTxIndex",
                vec![],
                NativeMainRetainedCallerPcV1::InitMax,
                V::Undefined,
            ),
            NativeMainRetainedCallerV1::SyncIndex { max } => (
                "Main.syncIndex",
                vec![max.clone()],
                NativeMainRetainedCallerPcV1::Sync,
                max.clone(),
            ),
        };
        let genesis = self
            .core
            .main_production
            .as_ref()
            .ok_or(SourceRuleError::InvalidInput)?
            .genesis
            .clone();
        let effect_start = self.core.effects.len();
        self.core
            .native_main_service_load_module(index, documents)?;
        let invocation = self.source_main_invocation_begin(&DependencyCall {
            target: target.into(),
            arguments: args,
        })?;
        Ok(NativeMainRetainedCallerFrameV1 {
            command,
            coercive_sync: false,
            null_group_member: false,
            invocation: Some(invocation),
            caller_scope: scope,
            pc,
            max,
            genesis,
            instance: V::Undefined,
            inline: None,
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
    fn native_main_retained_caller_finish(
        &mut self,
        f: &mut NativeMainRetainedCallerFrameV1,
        result: R<V>,
    ) {
        if f.null_group_member {
            let result = returned(result);
            if let Ok(value) = &result {
                f.caller.get_or_insert(value.clone());
            }
            f.final_result = Some(result);
            f.pc = NativeMainRetainedCallerPcV1::Done;
            return;
        }
        f.caller.get_or_insert_with(|| match &result {
            Ok(v) => DependencyReturn::Pending(v.clone()),
            Err(Fault::Thrown(e)) => DependencyReturn::PendingThrow(e.clone()),
            Err(Fault::Local(_)) => unreachable!("local stops do not finish source"),
        });
        f.final_result = Some(self.source_main_invocation_finish(
            f.invocation.take().expect("same caller invocation"),
            result,
        ));
        f.pc = NativeMainRetainedCallerPcV1::Done;
    }
    fn native_main_retained_delay(
        &mut self,
        f: &mut NativeMainRetainedCallerFrameV1,
        command: crate::source_main_step_kernel::MainStepCommandV1,
    ) -> Result<(), SourceRuleError> {
        let evaluation = f
            .kernel
            .apply(
                &mut NativeMainCallerDelayLoan {
                    core: &mut self.core,
                },
                command,
            )
            .map_err(|_| SourceRuleError::InvalidInput)?;
        for effect in evaluation.effects {
            self.core.emit(effect);
        }
        f.timer_done |= evaluation.completed.iter().any(|(_, result)| {
            matches!(
                result,
                crate::source_main_step_kernel::MainStepSourceResult::Fulfilled(_)
            )
        });
        Ok(())
    }
    pub(crate) fn native_main_retained_caller_fire(
        &mut self,
        f: &mut NativeMainRetainedCallerFrameV1,
        id: u64,
        generation: u64,
    ) -> Result<(), SourceRuleError> {
        if f.pc != NativeMainRetainedCallerPcV1::Delay || !f.timer_started || f.timer_done {
            return Err(SourceRuleError::InvalidInput);
        }
        self.native_main_retained_delay(
            f,
            crate::source_main_step_kernel::MainStepCommandV1::FireTimer { id, generation },
        )
    }
    pub(crate) fn native_main_retained_caller_advance(
        &mut self,
        index: &mut SourceCompactIndex,
        f: &mut NativeMainRetainedCallerFrameV1,
        documents: &V,
    ) -> NativeMainRetainedCallerStopV1 {
        match self.native_main_service_with_client_scope(
            NATIVE_MAIN_ACTIVE_SERVICE_CLIENT_BINDING,
            |runtime| runtime.native_main_retained_caller_advance_inner(index, f, documents),
        ) {
            Ok(Ok(stop)) => stop,
            Ok(Err(e)) | Err(e) => {
                NativeMainRetainedCallerStopV1::Native(NativeMainRowStopV1::SourceStopped(e))
            }
        }
    }
    fn native_main_retained_caller_advance_inner(
        &mut self,
        index: &mut SourceCompactIndex,
        f: &mut NativeMainRetainedCallerFrameV1,
        documents: &V,
    ) -> Result<NativeMainRetainedCallerStopV1, SourceRuleError> {
        if let Some(e) = &f.local_stop {
            return Ok(NativeMainRetainedCallerStopV1::Native(
                NativeMainRowStopV1::SourceStopped(e.clone()),
            ));
        }
        if f.pc == NativeMainRetainedCallerPcV1::Consensus && f.extract.is_some() {
            f.pc = NativeMainRetainedCallerPcV1::Detached;
        }
        let result = (|| -> R<NativeMainRetainedCallerStopV1> {
            loop {
                if self.core.effects_exhausted {
                    return Err(SourceRuleError::Capacity.into());
                }
                match f.pc {
                    NativeMainRetainedCallerPcV1::Delay => {
                        if !f.timer_started {
                            self.native_main_retained_delay(
                                f,
                                crate::source_main_step_kernel::MainStepCommandV1::Start {
                                    method: "Main.delay".into(),
                                    arguments: vec![n(1500.)],
                                },
                            )?;
                            f.timer_started = true;
                        }
                        if !f.timer_done {
                            return Ok(NativeMainRetainedCallerStopV1::PendingTimer);
                        }
                        f.pc = NativeMainRetainedCallerPcV1::InitializePrefix;
                    }
                    NativeMainRetainedCallerPcV1::InitializePrefix => {
                        let (instance, genesis) =
                            self.native_main_retained_initialize_prefix(index)?;
                        f.instance = instance;
                        f.genesis = genesis;
                        f.inline = Some(Box::new(self.core.native_main_inline_index_begin(
                            f.caller_scope + 1,
                            crate::txindex_source_owner::TxIndexInlineMethodV1::Initialize,
                            vec![f.genesis.clone()],
                        )?));
                        f.pc = NativeMainRetainedCallerPcV1::InstanceInit;
                    }
                    NativeMainRetainedCallerPcV1::InstanceInit
                    | NativeMainRetainedCallerPcV1::StaticInit => {
                        let child = f.inline.as_mut().expect("actual initializer source child");
                        let stop = self
                            .core
                            .native_main_inline_index_advance(index, child, documents);
                        if stop != NativeMainRowStopV1::Complete {
                            return Ok(NativeMainRetainedCallerStopV1::Native(stop));
                        }
                        let result = child
                            .result
                            .take()
                            .expect("actual completed source initializer");
                        f.inline = None;
                        if f.pc == NativeMainRetainedCallerPcV1::StaticInit {
                            result?;
                            f.max = f.genesis.clone();
                            f.pc = NativeMainRetainedCallerPcV1::Sync;
                        } else {
                            match result {
                                Ok(_) | Err(Fault::Thrown(_)) => (),
                                Err(e) => return Err(e),
                            }
                            f.pc = NativeMainRetainedCallerPcV1::Check;
                        }
                    }
                    NativeMainRetainedCallerPcV1::Check => {
                        if truth(
                            &self
                                .core
                                .native_main_index(index, "checkForIndex", vec![])?,
                        ) {
                            f.pc = NativeMainRetainedCallerPcV1::Consensus;
                        } else {
                            f.pc = NativeMainRetainedCallerPcV1::InitMax;
                        }
                    }
                    NativeMainRetainedCallerPcV1::InitMax => {
                        f.max =
                            self.core
                                .native_main_index(index, "findMaxIndexedBlock", vec![])?;
                        if same(&f.max, &n(0.)) || f.max == V::Null {
                            f.inline = Some(Box::new(self.core.native_main_inline_index_begin(
                                f.caller_scope + 2,
                                crate::txindex_source_owner::TxIndexInlineMethodV1::Initialize,
                                vec![f.genesis.clone()],
                            )?));
                            f.pc = NativeMainRetainedCallerPcV1::StaticInit;
                        } else {
                            f.pc = NativeMainRetainedCallerPcV1::Sync;
                        }
                    }
                    NativeMainRetainedCallerPcV1::Sync => {
                        if f.coercive_sync {
                            self.native_main_coercive_sync_step(f)?;
                            continue;
                        }
                        let max = native_main_retained_height(&f.max)?;
                        let tip = match self
                            .core
                            .source_main_production_io("Client.getBlockCount", &[])
                        {
                            Ok(v) => v,
                            Err(Fault::Thrown(_)) => {
                                self.native_main_retained_sync_return(f);
                                continue;
                            }
                            Err(e) => return Err(e),
                        };
                        let tip = native_main_retained_height(&tip)?;
                        let start = if tip > max && max != 0. {
                            Some(f.max.clone())
                        } else if max == 0. {
                            Some(f.genesis.clone())
                        } else {
                            None
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
                        } else if max == tip {
                            f.pc = NativeMainRetainedCallerPcV1::Consensus;
                        } else {
                            self.native_main_retained_sync_return(f);
                        }
                    }
                    NativeMainRetainedCallerPcV1::Extract
                    | NativeMainRetainedCallerPcV1::Detached => {
                        // The actual extractor alone consumes its child evaluation;
                        // shared IO borrows that SAME child without taking it.
                        let mut owner = std::mem::take(
                            &mut self
                                .core
                                .main_production
                                .as_mut()
                                .ok_or(SourceRuleError::InvalidInput)?
                                .index,
                        );
                        let extract = f.extract.as_mut().expect("actual extractor child");
                        let result = owner.advance_retained_extract(
                            f.caller_scope + 3,
                            extract,
                            &mut NativeMainServiceIndexBackend {
                                base: NativeMainIndexBackend {
                                    core: &mut self.core,
                                    index,
                                },
                                documents,
                            },
                        );
                        self.core
                            .main_production
                            .as_mut()
                            .expect("same source owner")
                            .index = owner;
                        match result? {
                            crate::txindex_source_owner::TxIndexExtractStopV1::AwaitDatabase => {
                                let descriptor = extract
                                    .pending_descriptor()
                                    .ok_or(SourceRuleError::InvalidInput)?;
                                let source = extract
                                    .pending_source_mut()
                                    .ok_or(SourceRuleError::InvalidInput)?;
                                let stop = self.core.native_main_inline_source_advance(
                                    index,
                                    f.extract_io.borrow(descriptor.method_scope(), source),
                                    documents,
                                );
                                if stop == NativeMainRowStopV1::Complete {
                                    continue;
                                }
                                return Ok(NativeMainRetainedCallerStopV1::Native(stop));
                            }
                            crate::txindex_source_owner::TxIndexExtractStopV1::CallerReturned => {
                                if !matches!(
                                    extract.caller_outcome(),
                                    Some(SourceOutcome::Returned(V::Undefined))
                                ) {
                                    return Err(SourceRuleError::InvalidInput.into());
                                }
                                self.native_main_retained_sync_return(f);
                                return Ok(if f.pc == NativeMainRetainedCallerPcV1::Consensus {
                                    NativeMainRetainedCallerStopV1::ConsensusReady
                                } else {
                                    NativeMainRetainedCallerStopV1::CallerReturned
                                });
                            }
                            crate::txindex_source_owner::TxIndexExtractStopV1::Complete => {
                                let evaluation = extract.take_evaluation()?;
                                self.core
                                    .main_height_rejections
                                    .extend(evaluation.deferred_rejections);
                                f.extract = None;
                                // Awaited extractor source errors are caught by Main.syncIndex.
                                self.native_main_retained_sync_return(f);
                            }
                        }
                    }
                    NativeMainRetainedCallerPcV1::Consensus => {
                        return Ok(NativeMainRetainedCallerStopV1::ConsensusReady)
                    }
                    NativeMainRetainedCallerPcV1::Done => {
                        return Ok(NativeMainRetainedCallerStopV1::Complete)
                    }
                }
            }
        })();
        match result {
            Ok(stop) => Ok(stop),
            Err(Fault::Thrown(e)) => {
                self.native_main_retained_caller_finish(f, Err(Fault::Thrown(e)));
                Ok(NativeMainRetainedCallerStopV1::Complete)
            }
            Err(Fault::Local(e)) => {
                f.local_stop = Some(e.clone());
                Ok(NativeMainRetainedCallerStopV1::Native(
                    NativeMainRowStopV1::SourceStopped(e),
                ))
            }
        }
    }
    fn native_main_retained_sync_return(&mut self, f: &mut NativeMainRetainedCallerFrameV1) {
        if f.command == NativeMainRetainedCallerV1::Initialize {
            f.pc = NativeMainRetainedCallerPcV1::Consensus;
        } else if f.extract.as_ref().is_some_and(|e| e.detached_pending()) {
            f.caller
                .get_or_insert(DependencyReturn::Pending(V::Undefined));
            f.pc = NativeMainRetainedCallerPcV1::Detached;
        } else {
            self.native_main_retained_caller_finish(f, Ok(V::Undefined));
        }
    }
    pub(crate) fn native_main_retained_caller_append(
        &mut self,
        index: &mut SourceCompactIndex,
        f: &mut NativeMainRetainedCallerFrameV1,
        descriptor: &NativeMainRetainedCallerPendingV1,
        permit: Option<&crate::source_compact_index_v2::SourceIndexGenerationEscrowPermitV1>,
    ) -> NativeMainRowStopV1 {
        if f.pending_descriptor().as_ref() != Some(descriptor) {
            return NativeMainRowStopV1::SourceStopped(SourceRuleError::InvalidInput);
        }
        if let Some(inline) = &mut f.inline {
            return self
                .core
                .native_main_inline_index_append(index, inline, permit);
        }
        let Some(source) = f.extract.as_mut().and_then(|e| e.pending_source_mut()) else {
            return NativeMainRowStopV1::SourceStopped(SourceRuleError::InvalidInput);
        };
        self.core.native_main_inline_source_append(
            index,
            f.extract_io.borrow(descriptor.method_scope, source),
            permit,
        )
    }
    pub(crate) fn native_main_retained_caller_resolve(
        &mut self,
        index: &SourceCompactIndex,
        f: &mut NativeMainRetainedCallerFrameV1,
        descriptor: &NativeMainRetainedCallerPendingV1,
    ) -> NativeMainRowStopV1 {
        if f.pending_descriptor().as_ref() != Some(descriptor) {
            return NativeMainRowStopV1::SourceStopped(SourceRuleError::InvalidInput);
        }
        if let Some(inline) = &mut f.inline {
            return self.core.native_main_inline_index_resolve(index, inline);
        }
        let Some(extract) = f.extract.as_mut() else {
            return NativeMainRowStopV1::SourceStopped(SourceRuleError::InvalidInput);
        };
        if !f.extract_io.attempted {
            return NativeMainRowStopV1::SourceStopped(SourceRuleError::InvalidInput);
        }
        let Some(write) = f.extract_io.write.as_deref() else {
            return NativeMainRowStopV1::SourceStopped(SourceRuleError::InvalidInput);
        };
        let returned = match index.resolve_inline_database_post(write) {
            Ok(returned) => returned,
            Err(e) => return NativeMainRowStopV1::StorageStopped(e.to_string()),
        };
        let Some(call) = extract.pending_call().cloned() else {
            return NativeMainRowStopV1::SourceStopped(SourceRuleError::InvalidInput);
        };
        let Some(actual_descriptor) = extract.pending_descriptor() else {
            return NativeMainRowStopV1::SourceStopped(SourceRuleError::InvalidInput);
        };
        let Some(production) = self.core.main_production.as_ref() else {
            return NativeMainRowStopV1::SourceStopped(SourceRuleError::InvalidInput);
        };
        if let Err(e) = production.index.resolve_retained_extract_source(
            f.caller_scope + 3,
            extract,
            &actual_descriptor,
            returned.clone(),
        ) {
            return NativeMainRowStopV1::SourceStopped(e);
        }
        native_main_inline_emit(&mut self.core, &call, &returned);
        f.extract_io.write = None;
        f.extract_io.attempted = false;
        f.extract_io.storage_stop = None;
        NativeMainRowStopV1::Complete
    }
}
fn native_main_retained_height(value: &V) -> Result<f64, SourceRuleError> {
    let V::Number(bits) = value else {
        return Err(SourceRuleError::UnsupportedNumericDomain);
    };
    let value = f64::from_bits(*bits);
    if !value.is_finite() || value < 0. || value.fract() != 0. || value > 9_007_199_254_740_991. {
        return Err(SourceRuleError::UnsupportedNumericDomain);
    }
    Ok(value)
}

impl SourceContractRuntime {
    fn native_main_retained_initialize_prefix(
        &mut self,
        index: &mut SourceCompactIndex,
    ) -> R<(V, V)> {
        let client = self
            .core
            .main_production
            .as_ref()
            .ok_or(SourceRuleError::InvalidInput)?
            .client
            .clone();
        if !truth(&client.get("chain"))
            && !truth(
                &self
                    .core
                    .source_main_service_get("startupModules")?
                    .get("database")
                    .get("initialized"),
            )
        {
            return Err(SourceRuleError::UnknownDependency.into());
        }
        self.native_main_retained_initialize_suffix(index, None, None)
    }
    fn native_main_retained_initialize_suffix(
        &mut self,
        index: &mut SourceCompactIndex,
        raw_client: Option<&V>,
        raw_sdk_fields: Option<&V>,
    ) -> R<(V, V)> {
        let instance = if let Some(fields) = raw_sdk_fields {
            self.core.source_main_raw_startup_sdk_index(index, fields)?
        } else {
            self.core.native_main_index(index, "getInstance", vec![])?
        };
        if let Some(raw_client @ (V::Null | V::Undefined)) = raw_client {
            return Err(type_error(format!(
                "Cannot read properties of {} (reading 'getTest')",
                if *raw_client == V::Null {
                    "null"
                } else {
                    "undefined"
                }
            )));
        }
        let test = self.core.source_main_production_io("Client.getTest", &[])?;
        let mut extra = self.core.source_main_service_get("extraOwn")?;
        set(&mut extra, "test", test.clone())?;
        self.core
            .source_main_service_set("extraOwn", extra.clone())?;
        let chain = self
            .core
            .source_main_production_io("Client.getChain", &[])?;
        set(&mut extra, "chain", chain.clone())?;
        self.core.source_main_service_set("extraOwn", extra)?;
        let genesis = if test == V::Undefined || chain == V::Undefined {
            None
        } else {
            match text(&chain)?.as_str() {
                "BTC" => Some(if truth(&test) { 3520000. } else { 880000. }),
                "DOGE" => Some(if truth(&test) { 6815000. } else { 5520000. }),
                "LTC" => Some(if truth(&test) { 3082500. } else { 2819000. }),
                _ => None,
            }
        };
        if let Some(h) = genesis {
            self.core
                .main_production
                .as_mut()
                .ok_or(SourceRuleError::InvalidInput)?
                .genesis = n(h);
        }
        let genesis = self
            .core
            .main_production
            .as_ref()
            .ok_or(SourceRuleError::InvalidInput)?
            .genesis
            .clone();
        Ok((instance, genesis))
    }
}
