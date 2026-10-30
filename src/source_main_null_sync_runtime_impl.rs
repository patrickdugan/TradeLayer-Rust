pub const NATIVE_MAIN_NULL_SYNC_PROFILE: &str = "TL-MAIN-NULL-SYNC-DETACHED-13-1";
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeMainNullJobV1 {
    Parent,
    DetachedInit,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NativeMainNullIoProfileV1 {
    pub hold_child_read: bool,
    pub hold_parent_tip: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeMainNullStopV1 {
    Quiescent,
    PendingSourceIo,
    PendingNativeWrites,
    PendingTimer,
    Complete,
    Stopped(SourceRuleError),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NullReady {
    ChildRead,
    ChildReadSettled,
    ParentTip,
    ChildRun,
    InitRead,
    InitReadSettled,
    ConsensusRead(NativeMainNullJobV1),
    ConsensusReadSettled(NativeMainNullJobV1),
    ConsensusIndexFactory(NativeMainNullJobV1),
    ConsensusInstanceFactories(NativeMainNullJobV1),
    ConsensusStateReads(NativeMainNullJobV1),
    ConsensusRun(NativeMainNullJobV1),
}
struct NullJob {
    caller: Box<NativeMainRetainedCallerFrameV1>,
    read: Option<crate::txindex_source_owner::TxIndexFindMaxReadV1>,
    service: Option<Box<NativeMainServiceContinuationV1>>,
    tally_pending: bool,
    consensus_factory: Option<R<V>>,
    bootstrap_done: bool,
}
pub(crate) struct NativeMainNullSyncFrameV1 {
    parent: NullJob,
    child: NullJob,
    parent_tip: Option<R<V>>,
    ready: std::collections::VecDeque<NullReady>,
    io: NativeMainNullIoProfileV1,
    child_read_waiting: bool,
    parent_tip_waiting: bool,
    child_rejection_notified: bool,
    header_closed: bool,
    stopped: Option<SourceRuleError>,
}
impl NativeMainNullSyncFrameV1 {
    fn job(&self, id: NativeMainNullJobV1) -> &NullJob {
        match id {
            NativeMainNullJobV1::Parent => &self.parent,
            NativeMainNullJobV1::DetachedInit => &self.child,
        }
    }
    fn job_mut(&mut self, id: NativeMainNullJobV1) -> &mut NullJob {
        match id {
            NativeMainNullJobV1::Parent => &mut self.parent,
            NativeMainNullJobV1::DetachedInit => &mut self.child,
        }
    }
    pub(crate) fn caller_result(&self) -> Option<&DependencyReturn> {
        self.parent.caller.caller.as_ref()
    }
    pub(crate) fn child_result(&self) -> Option<&DependencyReturn> {
        self.child.caller.caller.as_ref()
    }
    pub(crate) fn visit_fields(
        &self,
        visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
    ) -> Result<(), SourceRuleError> {
        visitor("profile", &s(NATIVE_MAIN_NULL_SYNC_PROFILE))?;
        for (id, job) in [
            (NativeMainNullJobV1::Parent, &self.parent),
            (NativeMainNullJobV1::DetachedInit, &self.child),
        ] {
            let prefix = format!("job/{id:?}");
            job.caller
                .visit_canonical_fields(&mut |p, v| visitor(&format!("{prefix}/caller/{p}"), v))?;
            visitor(
                &format!("{prefix}/read/present"),
                &V::Bool(job.read.is_some()),
            )?;
            if let Some(read) = &job.read {
                let mut error = Ok(());
                read.visit_fields(&mut |p, v| {
                    if error.is_ok() {
                        error =
                            native_main_row_visit_value(&format!("{prefix}/read/{p}"), v, visitor)
                    }
                });
                error?;
            }
            visitor(
                &format!("{prefix}/service/present"),
                &V::Bool(job.service.is_some()),
            )?;
            if let Some(service) = &job.service {
                service.visit_canonical_fields(&mut |p, v| {
                    visitor(&format!("{prefix}/service/{p}"), v)
                })?;
            }
            visitor(
                &format!("{prefix}/bootstrapDone"),
                &V::Bool(job.bootstrap_done),
            )?;
            visitor(
                &format!("{prefix}/tallyPending"),
                &V::Bool(job.tally_pending),
            )?;
            visitor(
                &format!("{prefix}/consensusFactory/present"),
                &V::Bool(job.consensus_factory.is_some()),
            )?;
            if let Some(result) = &job.consensus_factory {
                null_visit_result(&format!("{prefix}/consensusFactory"), result, visitor)?;
            }
        }
        visitor("ready/length", &s(self.ready.len().to_string()))?;
        for (at, ready) in self.ready.iter().enumerate() {
            visitor(&format!("ready/{at}"), &s(format!("{ready:?}")))?;
        }
        for (key, flag) in [
            ("io/holdChildRead", self.io.hold_child_read),
            ("io/holdParentTip", self.io.hold_parent_tip),
            ("childReadWaiting", self.child_read_waiting),
            ("parentTipWaiting", self.parent_tip_waiting),
            ("childRejectionNotified", self.child_rejection_notified),
            ("headerClosed", self.header_closed),
        ] {
            visitor(key, &V::Bool(flag))?;
        }
        visitor("parentTip/present", &V::Bool(self.parent_tip.is_some()))?;
        if let Some(result) = &self.parent_tip {
            null_visit_result("parentTip", result, visitor)?;
        }
        visitor(
            "stopped",
            &self
                .stopped
                .as_ref()
                .map_or(V::Null, |e| s(format!("{e:?}"))),
        )
    }
}
fn null_visit_result(
    prefix: &str,
    result: &R<V>,
    visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
) -> Result<(), SourceRuleError> {
    match result {
        Ok(value) => native_main_row_visit_value(&format!("{prefix}/returned"), value, visitor),
        Err(Fault::Thrown(error)) => {
            visitor(&format!("{prefix}/thrown/name"), &s(&error.name))?;
            visitor(&format!("{prefix}/thrown/message"), &s(&error.message))
        }
        Err(Fault::Local(error)) => visitor(&format!("{prefix}/local"), &s(format!("{error:?}"))),
    }
}
struct NullMetadataAccess<'a> {
    index: &'a mut SourceCompactIndex,
}
impl NativeMainSourceMetadataAccess for NullMetadataAccess<'_> {
    fn find_max(&mut self, core: &mut ContractCore) -> R<V> {
        NativeMainLiveSourceMetadataAccess { index: self.index }.find_max(core)
    }
    fn read_view(
        &mut self,
    ) -> Result<
        crate::source_compact_index_v2::MainIndexReadViewV1,
        crate::source_compact_index_v2::CompactIndexError,
    > {
        NativeMainLiveSourceMetadataAccess { index: self.index }.read_view()
    }
    fn prepare(
        &mut self,
        runtime: &mut SourceContractRuntime,
        start: V,
        grouping: &mut Option<crate::source_compact_index_v2::RowStreamReceipt>,
        storage_stop: &mut Option<String>,
    ) -> R<Option<NativeMainCatchupPreparedV1>> {
        runtime.native_main_catchup_prepare_after_bootstrap(start, self, grouping, storage_stop)
    }
    fn rows(
        &mut self,
        runtime: &mut SourceContractRuntime,
        row: &mut NativeMainRowContinuationV1,
    ) -> NativeMainRowStopV1 {
        NativeMainLiveSourceMetadataAccess { index: self.index }.rows(runtime, row)
    }
    fn block(
        &mut self,
        runtime: &mut SourceContractRuntime,
        block: &mut NativeMainServiceBlockFrame,
        documents: &V,
    ) -> NativeMainRowStopV1 {
        NativeMainLiveSourceMetadataAccess { index: self.index }.block(runtime, block, documents)
    }
    fn inline_block(
        &mut self,
        runtime: &mut SourceContractRuntime,
        block: &mut NativeMainServiceBlockFrame,
        documents: &V,
    ) -> NativeMainRowStopV1 {
        NativeMainLiveSourceMetadataAccess { index: self.index }
            .inline_block(runtime, block, documents)
    }
    fn block_prelude(
        &mut self,
        runtime: &mut SourceContractRuntime,
        block: V,
        height: V,
        documents: &V,
    ) -> R<V> {
        NativeMainLiveSourceMetadataAccess { index: self.index }
            .block_prelude(runtime, block, height, documents)
    }
    fn kernel(
        &mut self,
        runtime: &mut SourceContractRuntime,
        frame: &mut NativeMainServiceContinuationV1,
        command: crate::source_main_step_kernel::MainStepCommandV1,
        enqueue: bool,
    ) -> Result<
        crate::source_main_step_kernel::MainStepEvaluationV1,
        crate::source_main_step_kernel::MainStepError,
    > {
        NativeMainLiveSourceMetadataAccess { index: self.index }
            .kernel(runtime, frame, command, enqueue)
    }
}
impl SourceContractRuntime {
    pub(crate) fn native_main_null_preflight(&self, documents: &V) -> Result<(), SourceRuleError> {
        self.native_main_service_preflight(documents)?;
        if !native_main_sync_scalar(
            &self
                .core
                .main_production
                .as_ref()
                .ok_or(SourceRuleError::InvalidInput)?
                .io
                .get("height"),
        ) {
            return Err(SourceRuleError::UnknownDependency);
        }
        Ok(())
    }
    fn null_find_begin(
        &mut self,
        index: &mut SourceCompactIndex,
        scope: u64,
        documents: &V,
    ) -> Result<crate::txindex_source_owner::TxIndexFindMaxReadV1, SourceRuleError> {
        let owner = std::mem::take(
            &mut self
                .core
                .main_production
                .as_mut()
                .ok_or(SourceRuleError::InvalidInput)?
                .index,
        );
        let result = owner.begin_find_max_read(
            scope,
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
            .expect("same owner")
            .index = owner;
        result
    }
    fn null_find_enter(
        &mut self,
        index: &mut SourceCompactIndex,
        scope: u64,
        read: &mut crate::txindex_source_owner::TxIndexFindMaxReadV1,
        documents: &V,
    ) -> Result<(), SourceRuleError> {
        let owner = std::mem::take(
            &mut self
                .core
                .main_production
                .as_mut()
                .ok_or(SourceRuleError::InvalidInput)?
                .index,
        );
        let result = owner.enter_find_max_read(
            scope,
            read,
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
            .expect("same owner")
            .index = owner;
        result
    }
    fn null_find_settle(
        &mut self,
        scope: u64,
        read: &mut crate::txindex_source_owner::TxIndexFindMaxReadV1,
    ) -> R<V> {
        self.core
            .main_production
            .as_ref()
            .ok_or(SourceRuleError::InvalidInput)?
            .index
            .settle_find_max_read(scope, read)?;
        outcome(read.take_evaluation()?.outcome)
    }
    pub(crate) fn native_main_null_begin(
        &mut self,
        index: &mut SourceCompactIndex,
        scope: u64,
        generation: u64,
        documents: &V,
        io: NativeMainNullIoProfileV1,
    ) -> Result<NativeMainNullSyncFrameV1, SourceRuleError> {
        self.native_main_null_preflight(documents)?;
        if scope == 0 || scope > u64::MAX - 4096 {
            return Err(SourceRuleError::InvalidInput);
        }
        let mut parent = self.native_main_retained_caller_begin(
            index,
            NativeMainRetainedCallerV1::SyncIndex { max: V::Null },
            scope,
            generation,
            documents,
        )?;
        parent.null_group_member = true;
        let child = NativeMainRetainedCallerFrameV1 {
            command: NativeMainRetainedCallerV1::InitOrLoad,
            coercive_sync: true,
            null_group_member: true,
            invocation: None,
            caller_scope: scope + 32,
            pc: NativeMainRetainedCallerPcV1::InitMax,
            max: V::Undefined,
            genesis: parent.genesis.clone(),
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
            effect_start: parent.effect_start,
        };
        // Literal detached initOrLoad enters its first async factory BEFORE
        // the continuing parent invokes getBlockCountAsync.
        let read = self.null_find_begin(index, child.caller_scope, documents)?;
        let tip = self
            .core
            .source_main_production_io("Client.getBlockCount", &[]);
        if let Err(Fault::Local(error)) = &tip {
            return Err(error.clone());
        }
        Ok(NativeMainNullSyncFrameV1 {
            parent: NullJob {
                caller: Box::new(parent),
                read: None,
                service: None,
                tally_pending: false,
                consensus_factory: None,
                bootstrap_done: false,
            },
            child: NullJob {
                caller: Box::new(child),
                read: Some(read),
                service: None,
                tally_pending: false,
                consensus_factory: None,
                bootstrap_done: false,
            },
            parent_tip: Some(tip),
            ready: std::collections::VecDeque::from([NullReady::ChildRead, NullReady::ParentTip]),
            io,
            child_read_waiting: false,
            parent_tip_waiting: false,
            child_rejection_notified: false,
            header_closed: false,
            stopped: None,
        })
    }
    pub(crate) fn native_main_null_release_read(
        &mut self,
        f: &mut NativeMainNullSyncFrameV1,
    ) -> Result<(), SourceRuleError> {
        if !f.child_read_waiting {
            return Err(SourceRuleError::InvalidInput);
        }
        f.child_read_waiting = false;
        f.io.hold_child_read = false;
        f.ready.push_back(NullReady::ChildRead);
        Ok(())
    }
    pub(crate) fn native_main_null_release_tip(
        &mut self,
        f: &mut NativeMainNullSyncFrameV1,
    ) -> Result<(), SourceRuleError> {
        if !f.parent_tip_waiting {
            return Err(SourceRuleError::InvalidInput);
        }
        f.parent_tip_waiting = false;
        f.io.hold_parent_tip = false;
        f.ready.push_back(NullReady::ParentTip);
        Ok(())
    }
    fn null_consensus_begin(
        &mut self,
        index: &mut SourceCompactIndex,
        f: &mut NativeMainNullSyncFrameV1,
        id: NativeMainNullJobV1,
        documents: &V,
    ) -> Result<(), SourceRuleError> {
        let job = f.job_mut(id);
        if job.service.is_some() {
            return Err(SourceRuleError::InvalidInput);
        }
        job.service = Some(Box::new(self.native_main_retained_service_frame_fields(
            job.caller.genesis.clone(),
            job.caller.kernel.generation(),
            job.caller.effect_start,
            documents.clone(),
        )?));
        job.read = Some(self.null_find_begin(index, job.caller.caller_scope + 8, documents)?);
        f.ready.push_back(NullReady::ConsensusRead(id));
        Ok(())
    }
    fn null_fail_job(
        &mut self,
        f: &mut NativeMainNullSyncFrameV1,
        id: NativeMainNullJobV1,
        error: Fault,
    ) -> Result<(), SourceRuleError> {
        if let Fault::Local(error) = error {
            return Err(error);
        }
        let job = f.job_mut(id);
        self.native_main_retained_caller_finish(&mut job.caller, Err(error));
        Ok(())
    }
    fn null_initializer_prefix(
        &mut self,
        index: &mut SourceCompactIndex,
        f: &mut NativeMainNullSyncFrameV1,
        documents: &V,
        step: u8,
    ) -> Result<(), SourceRuleError> {
        let child = &mut f.child.caller;
        let source = &mut child
            .inline
            .as_mut()
            .ok_or(SourceRuleError::InvalidInput)?
            .source;
        let owner = std::mem::take(
            &mut self
                .core
                .main_production
                .as_mut()
                .ok_or(SourceRuleError::InvalidInput)?
                .index,
        );
        let scope = child.caller_scope + 2;
        let result = match step {
            0 => owner.null_initializer_factory(
                scope,
                source,
                &mut NativeMainServiceIndexBackend {
                    base: NativeMainIndexBackend {
                        core: &mut self.core,
                        index,
                    },
                    documents,
                },
            ),
            1 => owner
                .null_initializer_read(
                    scope,
                    source,
                    &mut NativeMainServiceIndexBackend {
                        base: NativeMainIndexBackend {
                            core: &mut self.core,
                            index,
                        },
                        documents,
                    },
                )
                .map(|_| ()),
            _ => owner.null_initializer_settle_read(scope, source),
        };
        self.core
            .main_production
            .as_mut()
            .expect("same source owner")
            .index = owner;
        result
    }
    pub(crate) fn native_main_null_drive(
        &mut self,
        index: &mut SourceCompactIndex,
        f: &mut NativeMainNullSyncFrameV1,
        documents: &V,
        limit: usize,
    ) -> NativeMainNullStopV1 {
        if let Some(error) = &f.stopped {
            return NativeMainNullStopV1::Stopped(error.clone());
        }
        if limit == 0 || limit > 4096 {
            return NativeMainNullStopV1::Stopped(SourceRuleError::InvalidInput);
        }
        for _ in 0..limit {
            let Some(ready) = f.ready.pop_front() else {
                break;
            };
            let result = self.null_step(index, f, documents, ready);
            if let Err(error) = result {
                f.stopped = Some(error.clone());
                return NativeMainNullStopV1::Stopped(error);
            }
        }
        if !f.ready.is_empty() {
            return NativeMainNullStopV1::Quiescent;
        }
        if !f.child_rejection_notified {
            if let Some(DependencyReturn::Throw(error) | DependencyReturn::PendingThrow(error)) =
                &f.child.caller.caller
            {
                self.core.main_height_rejections.push(error.clone());
                f.child_rejection_notified = true;
            }
        }
        if f.parent.caller.invocation.is_some() {
            if let Err(error) =
                self.native_main_retained_consensus_notifications(&mut f.parent.caller)
            {
                f.stopped = Some(error.clone());
                return NativeMainNullStopV1::Stopped(error);
            }
        }
        if !f.header_closed
            && f.parent.caller.caller.is_some()
            && f.child.caller.caller.is_some()
            && f.child.caller.extract.is_none()
            && [&f.parent, &f.child].iter().all(|job| {
                job.service.as_ref().is_none_or(|service| {
                    service.block.is_none() && service.completed_block.is_none()
                })
            })
        {
            let result = match f
                .parent
                .caller
                .caller
                .clone()
                .expect("actual parent outcome")
            {
                DependencyReturn::Pending(v) | DependencyReturn::Immediate(v) => Ok(v),
                DependencyReturn::PendingThrow(e) | DependencyReturn::Throw(e) => {
                    Err(Fault::Thrown(e))
                }
            };
            let invocation = f
                .parent
                .caller
                .invocation
                .take()
                .expect("one retained null-sync invocation");
            match self.source_main_invocation_finish(invocation, result) {
                Ok(_) => {
                    f.header_closed = true;
                    for job in [&mut f.parent, &mut f.child] {
                        if let Some(service) = &mut job.service {
                            service.borrowed_invocation = false;
                        }
                    }
                }
                Err(error) => {
                    f.stopped = Some(error.clone());
                    return NativeMainNullStopV1::Stopped(error);
                }
            }
        }
        if f.child_read_waiting || f.parent_tip_waiting {
            return NativeMainNullStopV1::PendingSourceIo;
        }
        if [&f.parent, &f.child].iter().any(|job| {
            job.caller.pending_write().is_some()
                || job
                    .service
                    .as_ref()
                    .is_some_and(|service| service.pending_native_context().is_some())
        }) {
            return NativeMainNullStopV1::PendingNativeWrites;
        }
        if [&f.parent, &f.child].iter().any(|job| {
            job.service
                .as_ref()
                .is_some_and(|service| service.next_timer().is_some())
        }) {
            return NativeMainNullStopV1::PendingTimer;
        }
        if f.header_closed {
            NativeMainNullStopV1::Complete
        } else {
            NativeMainNullStopV1::Quiescent
        }
    }
    fn null_step(
        &mut self,
        index: &mut SourceCompactIndex,
        f: &mut NativeMainNullSyncFrameV1,
        documents: &V,
        ready: NullReady,
    ) -> Result<(), SourceRuleError> {
        use NativeMainNullJobV1::{DetachedInit, Parent};
        match ready {
            NullReady::ChildRead => {
                if f.io.hold_child_read {
                    f.child_read_waiting = true;
                    return Ok(());
                }
                let job = &mut f.child;
                self.null_find_enter(
                    index,
                    job.caller.caller_scope,
                    job.read.as_mut().ok_or(SourceRuleError::InvalidInput)?,
                    documents,
                )?;
                f.ready.push_back(NullReady::ChildReadSettled);
            }
            NullReady::ChildReadSettled => {
                let job = &mut f.child;
                let result = self.null_find_settle(
                    job.caller.caller_scope,
                    job.read.as_mut().ok_or(SourceRuleError::InvalidInput)?,
                );
                job.read = None;
                match result {
                    Ok(max) => {
                        job.caller.max = max;
                        if same(&job.caller.max, &n(0.)) || job.caller.max == V::Null {
                            job.caller.inline =
                                Some(Box::new(self.core.native_main_inline_index_begin(
                                    job.caller.caller_scope + 2,
                                    crate::txindex_source_owner::TxIndexInlineMethodV1::Initialize,
                                    vec![job.caller.genesis.clone()],
                                )?));
                            job.caller.pc = NativeMainRetainedCallerPcV1::StaticInit;
                            self.null_initializer_prefix(index, f, documents, 0)?;
                            f.ready.push_back(NullReady::InitRead);
                        } else {
                            job.caller.pc = NativeMainRetainedCallerPcV1::Sync;
                            f.ready.push_back(NullReady::ChildRun);
                        }
                    }
                    Err(error) => self.null_fail_job(f, DetachedInit, error)?,
                };
            }
            NullReady::ParentTip => {
                if f.io.hold_parent_tip {
                    f.parent_tip_waiting = true;
                    return Ok(());
                }
                let tip = f.parent_tip.take().ok_or(SourceRuleError::InvalidInput)?;
                match tip {
                    Ok(V::Null | V::Undefined) => {
                        f.parent.caller.pc = NativeMainRetainedCallerPcV1::Consensus;
                        self.null_consensus_begin(index, f, Parent, documents)?;
                    }
                    Ok(_) | Err(Fault::Thrown(_)) => self
                        .native_main_retained_caller_finish(&mut f.parent.caller, Ok(V::Undefined)),
                    Err(Fault::Local(error)) => return Err(error),
                }
            }
            NullReady::InitRead => {
                self.null_initializer_prefix(index, f, documents, 1)?;
                f.ready.push_back(NullReady::InitReadSettled);
            }
            NullReady::InitReadSettled => {
                self.null_initializer_prefix(index, f, documents, 2)?;
                f.ready.push_back(NullReady::ChildRun);
            }
            NullReady::ChildRun => {
                let stop =
                    self.native_main_retained_caller_advance(index, &mut f.child.caller, documents);
                match stop {
                    NativeMainRetainedCallerStopV1::ConsensusReady => {
                        self.null_consensus_begin(index, f, DetachedInit, documents)?
                    }
                    NativeMainRetainedCallerStopV1::CallerReturned => {
                        f.ready.push_back(NullReady::ChildRun)
                    }
                    NativeMainRetainedCallerStopV1::Native(NativeMainRowStopV1::SourceStopped(
                        error,
                    )) => return Err(error),
                    _ => (),
                }
            }
            NullReady::ConsensusRead(id) => {
                let job = f.job_mut(id);
                self.null_find_enter(
                    index,
                    job.caller.caller_scope + 8,
                    job.read.as_mut().ok_or(SourceRuleError::InvalidInput)?,
                    documents,
                )?;
                f.ready.push_back(NullReady::ConsensusReadSettled(id));
            }
            NullReady::ConsensusReadSettled(id) => {
                let job = f.job_mut(id);
                let result = self.null_find_settle(
                    job.caller.caller_scope + 8,
                    job.read.as_mut().ok_or(SourceRuleError::InvalidInput)?,
                );
                job.read = None;
                match result {
                    Ok(_) => f.ready.push_back(NullReady::ConsensusIndexFactory(id)),
                    Err(error) => self.null_fail_job(f, id, error)?,
                }
            }
            NullReady::ConsensusIndexFactory(id) => match self.core.database("txIndex") {
                Ok(_) => f.ready.push_back(NullReady::ConsensusInstanceFactories(id)),
                Err(error) => self.null_fail_job(f, id, error)?,
            },
            NullReady::ConsensusInstanceFactories(id) => {
                self.core.source_send_tally_instance_initialized = true;
                let job = f.job_mut(id);
                job.tally_pending = match self.core.database("tallyMap") {
                    Ok(_) => true,
                    Err(Fault::Thrown(_)) => false,
                    Err(Fault::Local(e)) => return Err(e),
                };
                job.consensus_factory = Some(self.core.database("consensus"));
                f.ready.push_back(NullReady::ConsensusStateReads(id));
            }
            NullReady::ConsensusStateReads(id) => {
                let job = f.job_mut(id);
                if job.tally_pending {
                    let loaded = self.core.find("tallyMap", obj(&[("_id", s("tallyMap"))]));
                    match loaded.and_then(|row| ContractCore::tally_parsed_entries(&row)) {
                        Ok(entries) => {
                            self.core.tally = entries;
                            self.core.tally_static_initialized = true;
                        }
                        Err(Fault::Thrown(_)) => (),
                        Err(Fault::Local(e)) => return Err(e),
                    }
                }
                let result = job
                    .consensus_factory
                    .take()
                    .ok_or(SourceRuleError::InvalidInput)?
                    .and_then(|_| {
                        self.core
                            .find("consensus", obj(&[("_id", s("MaxProcessedHeight"))]))
                    });
                match result {
                    Ok(_) => {
                        job.bootstrap_done = true;
                        f.ready.push_back(NullReady::ConsensusRun(id));
                    }
                    Err(error) => self.null_fail_job(f, id, error)?,
                }
            }
            NullReady::ConsensusRun(id) => {
                let job = f.job_mut(id);
                if !job.bootstrap_done {
                    return Err(SourceRuleError::InvalidInput);
                }
                let service = job.service.as_mut().ok_or(SourceRuleError::InvalidInput)?;
                let stop = self.native_main_service_with_client_scope(
                    service.client_binding,
                    |runtime| {
                        runtime.native_main_service_advance_with_access(
                            service,
                            1,
                            &mut NullMetadataAccess { index },
                        )
                    },
                )?;
                if job.caller.caller.is_none() {
                    if let Some(result) = &service.caller_result {
                        let actual = match result {
                            Ok(DependencyReturn::Pending(v) | DependencyReturn::Immediate(v)) => {
                                Ok(if id == DetachedInit {
                                    V::Undefined
                                } else {
                                    v.clone()
                                })
                            }
                            Ok(DependencyReturn::PendingThrow(e) | DependencyReturn::Throw(e)) => {
                                Err(Fault::Thrown(e.clone()))
                            }
                            Err(e) => return Err(e.clone()),
                        };
                        self.native_main_retained_caller_finish(&mut job.caller, actual);
                    }
                }
                match stop {
                    NativeMainServiceStopV1::DrainBound => {
                        f.ready.push_back(NullReady::ConsensusRun(id))
                    }
                    NativeMainServiceStopV1::SourceStopped(error) => return Err(error),
                    _ => (),
                }
            }
        }
        Ok(())
    }
}
