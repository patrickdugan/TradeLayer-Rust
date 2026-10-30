use crate::validity_source::SourceException as NativeOuterSourceException;

// Source prefix only. The containing Kernel owns Promise adoption/jobs/header.
// This frame owns real results and immutable native views, never another Core.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NativeMainOuterBootstrapLaneV1 {
    Main,
    Tally,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NativeMainOuterBootstrapSiteV1 {
    FindMaxFactory,
    FindMaxQuery,
    FindMaxResult,
    TxIndexFactory,
    TallyFactory,
    TallyQuery,
    TallyLoadResult,
    ConsensusFactory,
    MaxProcessedQuery,
    MaxProcessedResult,
    FixedRead,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct NativeMainOuterBootstrapAwaitV1 {
    source_job: u64,
    lane: NativeMainOuterBootstrapLaneV1,
    site: NativeMainOuterBootstrapSiteV1,
    ordinal: u64,
}
impl NativeMainOuterBootstrapAwaitV1 {
    pub(crate) fn source_job(&self) -> u64 {
        self.source_job
    }
    pub(crate) fn lane(&self) -> NativeMainOuterBootstrapLaneV1 {
        self.lane
    }
    pub(crate) fn site(&self) -> NativeMainOuterBootstrapSiteV1 {
        self.site
    }
    pub(crate) fn ordinal(&self) -> u64 {
        self.ordinal
    }
    pub(crate) fn method_scope(&self) -> &'static str {
        use NativeMainOuterBootstrapSiteV1 as Site;
        match self.site {
            Site::FindMaxFactory | Site::FindMaxQuery => "TxIndex.findMaxIndexedBlock",
            Site::TallyFactory | Site::TallyQuery => "TallyMap.loadFromDB",
            Site::TallyLoadResult => "TallyMap.getInstance",
            Site::ConsensusFactory | Site::MaxProcessedQuery => "Main.loadMaxProcessedHeight",
            _ => "Main.constructConsensusFromIndex",
        }
    }
    pub(crate) fn is_host_query(&self) -> bool {
        matches!(
            self.site,
            NativeMainOuterBootstrapSiteV1::FindMaxQuery
                | NativeMainOuterBootstrapSiteV1::TallyQuery
                | NativeMainOuterBootstrapSiteV1::MaxProcessedQuery
                | NativeMainOuterBootstrapSiteV1::FixedRead
        )
    }
}
// Returned by borrowing the actual frame. There is no public completion value
// constructor. Map/fixed-query results cannot be disguised as Undefined/Array.
pub(crate) enum NativeMainOuterActualCompletionV1<'a> {
    Value(&'a V),
    TallyMap(&'a [(V, V)]),
    FixedRead(&'a crate::source_compact_index_v2::NativeMainStartupIndexReadV1),
    Thrown(&'a NativeOuterSourceException),
}
pub(crate) struct NativeMainOuterBootstrapV1 {
    source_job: u64,
    native_scope: crate::source_compact_index_v2::NativeMainStartupIndexScopeV1,
    tally_job: Option<u64>,
    next_ordinal: u64,
    issued: Vec<NativeMainOuterBootstrapAwaitV1>,
    started: bool,
    start: V,
    main_await: Option<NativeMainOuterBootstrapAwaitV1>,
    tally_await: Option<NativeMainOuterBootstrapAwaitV1>,
    find_max_factory: Option<R<V>>,
    tx_index_factory: Option<R<V>>,
    tally_factory: Option<R<V>>,
    consensus_factory: Option<R<V>>,
    find_max_query: Option<crate::source_compact_index_v2::NativeMainStartupIndexReadV1>,
    fixed_query: Option<crate::source_compact_index_v2::NativeMainStartupIndexReadV1>,
    find_max_failure: Option<Fault>,
    fixed_failure: Option<Fault>,
    find_max_callback: bool,
    fixed_callback: bool,
    tally_query: Option<NativeMainStartupDbQueryV1>,
    consensus_query: Option<NativeMainStartupDbQueryV1>,
    find_max_return: Option<R<V>>,
    max_processed_return: Option<R<V>>,
    tally_query_return: Option<R<V>>,
    consensus_query_return: Option<R<V>>,
    last_indexed_block: Option<V>,
    last_consensus_height: Option<V>,
    tally_returned_map: Option<Vec<(V, V)>>,
    tally_caught_undefined: Option<V>,
    tally_swallowed: Option<NativeOuterSourceException>,
    tally_load_complete: bool,
    tally_instance: Option<V>,
    prepared: Option<R<Option<NativeMainCatchupPreparedV1>>>,
    prepared_taken: bool,
    grouping: Option<crate::source_compact_index_v2::RowStreamReceipt>,
    storage_stop: Option<String>,
    stopped: Option<SourceRuleError>,
}
fn outer_bootstrap_clone_result(result: &R<V>) -> R<V> {
    match result {
        Ok(value) => Ok(value.clone()),
        Err(Fault::Thrown(error)) => Err(Fault::Thrown(error.clone())),
        Err(Fault::Local(error)) => Err(Fault::Local(error.clone())),
    }
}
fn outer_bootstrap_completion(
    result: &R<V>,
) -> Result<NativeMainOuterActualCompletionV1<'_>, SourceRuleError> {
    match result {
        Ok(value) => Ok(NativeMainOuterActualCompletionV1::Value(value)),
        Err(Fault::Thrown(error)) => Ok(NativeMainOuterActualCompletionV1::Thrown(error)),
        Err(Fault::Local(error)) => Err(error.clone()),
    }
}
impl NativeMainOuterBootstrapV1 {
    pub(crate) fn pending_site(
        &self,
        lane: NativeMainOuterBootstrapLaneV1,
    ) -> Option<NativeMainOuterBootstrapAwaitV1> {
        match lane {
            NativeMainOuterBootstrapLaneV1::Main => self.main_await,
            NativeMainOuterBootstrapLaneV1::Tally => self.tally_await,
        }
    }
    pub(crate) fn stopped(&self) -> Option<&SourceRuleError> {
        self.stopped.as_ref()
    }
    pub(crate) fn storage_stop(&self) -> Option<&str> {
        self.storage_stop.as_deref()
    }
    pub(crate) fn grouping(&self) -> Option<&crate::source_compact_index_v2::RowStreamReceipt> {
        self.grouping.as_ref()
    }
    pub(crate) fn tally_child_complete(&self) -> bool {
        self.tally_instance.is_some()
    }
    pub(crate) fn prepared_ready(&self) -> bool {
        self.prepared.is_some()
    }
    fn take_prepared(&mut self) -> Result<R<Option<NativeMainCatchupPreparedV1>>, SourceRuleError> {
        if let Some(error) = &self.stopped {
            return Err(error.clone());
        }
        if self.prepared_taken {
            return Err(SourceRuleError::InvalidInput);
        }
        let result = self.prepared.take().ok_or(SourceRuleError::InvalidInput)?;
        self.prepared_taken = true;
        // The Tally child, its actual results and pending job remain retained.
        Ok(result)
    }
    fn require_issued(
        &self,
        await_: NativeMainOuterBootstrapAwaitV1,
    ) -> Result<(), SourceRuleError> {
        if !self.issued.contains(&await_) {
            return Err(SourceRuleError::InvalidInput);
        }
        Ok(())
    }
    fn require(&self, await_: NativeMainOuterBootstrapAwaitV1) -> Result<(), SourceRuleError> {
        if let Some(error) = &self.stopped {
            return Err(error.clone());
        }
        if self.pending_site(await_.lane) != Some(await_) {
            return Err(SourceRuleError::InvalidInput);
        }
        Ok(())
    }
    fn install(
        &mut self,
        lane: NativeMainOuterBootstrapLaneV1,
        site: NativeMainOuterBootstrapSiteV1,
    ) -> Result<(), SourceRuleError> {
        let source_job = match lane {
            NativeMainOuterBootstrapLaneV1::Main => self.source_job,
            NativeMainOuterBootstrapLaneV1::Tally => {
                self.tally_job.ok_or(SourceRuleError::InvalidInput)?
            }
        };
        if self.issued.len() >= 32 {
            return Err(self.stop(SourceRuleError::Capacity));
        }
        let ordinal = self
            .next_ordinal
            .checked_add(1)
            .ok_or(SourceRuleError::Capacity)?;
        self.next_ordinal = ordinal;
        let descriptor = NativeMainOuterBootstrapAwaitV1 {
            source_job,
            lane,
            site,
            ordinal,
        };
        self.issued.push(descriptor);
        let await_ = Some(descriptor);
        match lane {
            NativeMainOuterBootstrapLaneV1::Main => self.main_await = await_,
            NativeMainOuterBootstrapLaneV1::Tally => self.tally_await = await_,
        }
        Ok(())
    }
    fn require_index(&mut self, index: &SourceCompactIndex) -> Result<(), SourceRuleError> {
        index
            .require_startup_source_index_scope(&self.native_scope)
            .map_err(|error| {
                self.storage_stop = Some(error.to_string());
                self.stop(SourceRuleError::Backend)
            })
    }
    fn stop(&mut self, error: SourceRuleError) -> SourceRuleError {
        self.stopped = Some(error.clone());
        error
    }
    fn retain_factory(
        &mut self,
        result: R<V>,
        site: NativeMainOuterBootstrapSiteV1,
    ) -> Result<(), SourceRuleError> {
        let local = match &result {
            Err(Fault::Local(error)) => Some(error.clone()),
            _ => None,
        };
        match site {
            NativeMainOuterBootstrapSiteV1::FindMaxFactory => self.find_max_factory = Some(result),
            NativeMainOuterBootstrapSiteV1::TxIndexFactory => self.tx_index_factory = Some(result),
            NativeMainOuterBootstrapSiteV1::TallyFactory => self.tally_factory = Some(result),
            NativeMainOuterBootstrapSiteV1::ConsensusFactory => {
                self.consensus_factory = Some(result)
            }
            _ => return Err(SourceRuleError::InvalidInput),
        }
        local.map_or(Ok(()), |error| Err(self.stop(error)))
    }
    fn finish_main(
        &mut self,
        result: R<Option<NativeMainCatchupPreparedV1>>,
    ) -> Result<(), SourceRuleError> {
        let local = match &result {
            Err(Fault::Local(error)) => Some(error.clone()),
            _ => None,
        };
        self.prepared = Some(result);
        self.main_await = None;
        local.map_or(Ok(()), |error| Err(self.stop(error)))
    }
    fn actual_factory(
        &self,
        site: NativeMainOuterBootstrapSiteV1,
    ) -> Result<R<V>, SourceRuleError> {
        let result = match site {
            NativeMainOuterBootstrapSiteV1::FindMaxFactory => &self.find_max_factory,
            NativeMainOuterBootstrapSiteV1::TxIndexFactory => &self.tx_index_factory,
            NativeMainOuterBootstrapSiteV1::TallyFactory => &self.tally_factory,
            NativeMainOuterBootstrapSiteV1::ConsensusFactory => &self.consensus_factory,
            _ => return Err(SourceRuleError::InvalidInput),
        };
        Ok(outer_bootstrap_clone_result(
            result.as_ref().ok_or(SourceRuleError::InvalidInput)?,
        ))
    }
    pub(crate) fn actual_query_receipt(
        &self,
        await_: NativeMainOuterBootstrapAwaitV1,
    ) -> Result<
        Option<&crate::source_compact_index_v2::NativeMainStartupIndexReadV1>,
        SourceRuleError,
    > {
        self.require_issued(await_)?;
        let query = match await_.site {
            NativeMainOuterBootstrapSiteV1::FindMaxQuery => self.find_max_query.as_ref(),
            NativeMainOuterBootstrapSiteV1::FixedRead => self.fixed_query.as_ref(),
            _ => return Err(SourceRuleError::InvalidInput),
        };
        Ok(query.filter(|q| q.sampled()))
    }
    pub(crate) fn actual_completion(
        &self,
        await_: NativeMainOuterBootstrapAwaitV1,
    ) -> Result<Option<NativeMainOuterActualCompletionV1<'_>>, SourceRuleError> {
        self.require_issued(await_)?;
        use NativeMainOuterBootstrapSiteV1 as Site;
        let result = match await_.site {
            Site::FindMaxFactory => self.find_max_factory.as_ref(),
            Site::TxIndexFactory => self.tx_index_factory.as_ref(),
            Site::TallyFactory => self.tally_factory.as_ref(),
            Site::ConsensusFactory => self.consensus_factory.as_ref(),
            Site::FindMaxResult => self.find_max_return.as_ref(),
            Site::MaxProcessedResult => self.max_processed_return.as_ref(),
            Site::TallyQuery => self
                .tally_query_return
                .as_ref()
                .or_else(|| self.tally_query.as_ref().and_then(|q| q.actual_result())),
            Site::MaxProcessedQuery => self.consensus_query_return.as_ref().or_else(|| {
                self.consensus_query
                    .as_ref()
                    .and_then(|q| q.actual_result())
            }),
            Site::TallyLoadResult => {
                if !self.tally_load_complete {
                    return Ok(None);
                }
                return Ok(Some(match &self.tally_returned_map {
                    Some(entries) => NativeMainOuterActualCompletionV1::TallyMap(entries),
                    None => NativeMainOuterActualCompletionV1::Value(
                        self.tally_caught_undefined
                            .as_ref()
                            .ok_or(SourceRuleError::InvalidInput)?,
                    ),
                }));
            }
            Site::FindMaxQuery | Site::FixedRead => {
                let (callback, failure, query) = if await_.site == Site::FindMaxQuery {
                    (
                        self.find_max_callback,
                        &self.find_max_failure,
                        &self.find_max_query,
                    )
                } else {
                    (self.fixed_callback, &self.fixed_failure, &self.fixed_query)
                };
                if !callback {
                    return Ok(None);
                }
                if let Some(error) = failure {
                    return match error {
                        Fault::Thrown(error) => {
                            Ok(Some(NativeMainOuterActualCompletionV1::Thrown(error)))
                        }
                        Fault::Local(error) => Err(error.clone()),
                    };
                }
                let query = query.as_ref().ok_or(SourceRuleError::InvalidInput)?;
                if await_.site == Site::FixedRead {
                    return Ok(Some(NativeMainOuterActualCompletionV1::FixedRead(query)));
                }
                return Ok(Some(
                    match query
                        .actual_returned()
                        .ok_or(SourceRuleError::InvalidInput)?
                    {
                        DependencyReturn::Immediate(v) | DependencyReturn::Pending(v) => {
                            NativeMainOuterActualCompletionV1::Value(v)
                        }
                        DependencyReturn::Throw(error) | DependencyReturn::PendingThrow(error) => {
                            NativeMainOuterActualCompletionV1::Thrown(error)
                        }
                    },
                ));
            }
        };
        result.map(outer_bootstrap_completion).transpose()
    }
}
impl SourceContractRuntime {
    pub(crate) fn native_main_outer_bootstrap_new(
        &self,
        start: V,
        source_job: u64,
        index: &SourceCompactIndex,
    ) -> Result<NativeMainOuterBootstrapV1, SourceRuleError> {
        if self.policy() != SourceContractRuntimePolicy::MainOrchestrationV12 || source_job == 0 {
            return Err(SourceRuleError::InvalidInput);
        }
        if !matches!(&start, V::Number(bits) if f64::from_bits(*bits).is_finite() && f64::from_bits(*bits)>=0. && f64::from_bits(*bits).fract()==0.)
        {
            return Err(SourceRuleError::UnsupportedNumericDomain);
        }
        let native_scope = index
            .capture_startup_source_index_scope()
            .map_err(|_| SourceRuleError::Backend)?;
        Ok(NativeMainOuterBootstrapV1 {
            source_job,
            native_scope,
            tally_job: None,
            next_ordinal: 0,
            issued: vec![],
            started: false,
            start,
            main_await: None,
            tally_await: None,
            find_max_factory: None,
            tx_index_factory: None,
            tally_factory: None,
            consensus_factory: None,
            find_max_query: None,
            fixed_query: None,
            find_max_failure: None,
            fixed_failure: None,
            find_max_callback: false,
            fixed_callback: false,
            tally_query: None,
            consensus_query: None,
            find_max_return: None,
            max_processed_return: None,
            tally_query_return: None,
            consensus_query_return: None,
            last_indexed_block: None,
            last_consensus_height: None,
            tally_returned_map: None,
            tally_caught_undefined: None,
            tally_swallowed: None,
            tally_load_complete: false,
            tally_instance: None,
            prepared: None,
            prepared_taken: false,
            grouping: None,
            storage_stop: None,
            stopped: None,
        })
    }
    pub(crate) fn native_main_outer_bootstrap_begin(
        &mut self,
        frame: &mut NativeMainOuterBootstrapV1,
    ) -> Result<(), SourceRuleError> {
        if frame.started || frame.stopped.is_some() {
            return Err(SourceRuleError::InvalidInput);
        }
        frame.install(
            NativeMainOuterBootstrapLaneV1::Main,
            NativeMainOuterBootstrapSiteV1::FindMaxFactory,
        )?;
        frame.started = true;
        self.native_main_outer_bootstrap_capture_factory(
            frame,
            "txIndex",
            NativeMainOuterBootstrapSiteV1::FindMaxFactory,
        )?;
        if self.core.effects_exhausted {
            return Err(frame.stop(SourceRuleError::Capacity));
        }
        Ok(())
    }
    fn native_main_outer_bootstrap_request_native(
        &mut self,
        frame: &mut NativeMainOuterBootstrapV1,
        index: &SourceCompactIndex,
        fixed: bool,
    ) -> Result<(), SourceRuleError> {
        use crate::source_compact_index_v2::NativeMainStartupIndexReadKindV1 as Kind;
        let query = index
            .startup_source_index_read_request(if fixed { Kind::Fixed } else { Kind::FindMax })
            .map_err(|error| {
                frame.storage_stop = Some(error.to_string());
                frame.stop(SourceRuleError::Backend)
            })?;
        let failure = self
            .core
            .check_failure("txIndex", if fixed { "findAsync" } else { "findOneAsync" })
            .err();
        let local = match &failure {
            Some(Fault::Local(error)) => Some(error.clone()),
            _ => None,
        };
        if fixed {
            frame.fixed_query = Some(query);
            frame.fixed_failure = failure;
        } else {
            frame.find_max_query = Some(query);
            frame.find_max_failure = failure;
        }
        local.map_or(Ok(()), |error| Err(frame.stop(error)))
    }
    pub(crate) fn native_main_outer_bootstrap_query_sample_actual(
        &mut self,
        frame: &mut NativeMainOuterBootstrapV1,
        await_: NativeMainOuterBootstrapAwaitV1,
        index: &SourceCompactIndex,
    ) -> Result<(), SourceRuleError> {
        frame.require(await_)?;
        frame.require_index(index)?;
        use NativeMainOuterBootstrapSiteV1 as Site;
        let result = match await_.site {
            Site::FindMaxQuery | Site::FixedRead => {
                let (failure, query) = if await_.site == Site::FindMaxQuery {
                    (&frame.find_max_failure, &mut frame.find_max_query)
                } else {
                    (&frame.fixed_failure, &mut frame.fixed_query)
                };
                if failure.is_some() {
                    return Err(SourceRuleError::InvalidInput);
                }
                index
                    .startup_source_index_read_sample_actual(
                        query.as_mut().ok_or(SourceRuleError::InvalidInput)?,
                    )
                    .map_err(|error| {
                        frame.storage_stop = Some(error.to_string());
                        SourceRuleError::Backend
                    })
            }
            Site::TallyQuery => self.native_main_startup_db_query_sample_actual(
                frame
                    .tally_query
                    .as_mut()
                    .ok_or(SourceRuleError::InvalidInput)?,
            ),
            Site::MaxProcessedQuery => self.native_main_startup_db_query_sample_actual(
                frame
                    .consensus_query
                    .as_mut()
                    .ok_or(SourceRuleError::InvalidInput)?,
            ),
            _ => return Err(SourceRuleError::InvalidInput),
        };
        result.map_err(|error| frame.stop(error))
    }
    pub(crate) fn native_main_outer_bootstrap_query_deliver_callback(
        &mut self,
        frame: &mut NativeMainOuterBootstrapV1,
        await_: NativeMainOuterBootstrapAwaitV1,
        index: &SourceCompactIndex,
    ) -> Result<(), SourceRuleError> {
        frame.require(await_)?;
        frame.require_index(index)?;
        self.native_main_outer_bootstrap_query_deliver_callback_inner(frame, await_, index)
            .map_err(|error| frame.stop(error))
    }
    fn native_main_outer_bootstrap_query_deliver_callback_inner(
        &mut self,
        frame: &mut NativeMainOuterBootstrapV1,
        await_: NativeMainOuterBootstrapAwaitV1,
        index: &SourceCompactIndex,
    ) -> Result<(), SourceRuleError> {
        frame.require(await_)?;
        frame.require_index(index)?;
        use NativeMainOuterBootstrapSiteV1 as Site;
        let result = match await_.site {
            Site::FindMaxQuery | Site::FixedRead => {
                let (failure, query, callback) = if await_.site == Site::FindMaxQuery {
                    (
                        &frame.find_max_failure,
                        &mut frame.find_max_query,
                        &mut frame.find_max_callback,
                    )
                } else {
                    (
                        &frame.fixed_failure,
                        &mut frame.fixed_query,
                        &mut frame.fixed_callback,
                    )
                };
                if *callback {
                    return Err(SourceRuleError::InvalidInput);
                }
                if failure.is_none() {
                    index
                        .startup_source_index_read_deliver_callback(
                            query.as_mut().ok_or(SourceRuleError::InvalidInput)?,
                        )
                        .map_err(|error| {
                            frame.storage_stop = Some(error.to_string());
                            SourceRuleError::Backend
                        })?;
                }
                *callback = true; // actual result retained before observer faults
                if await_.site == Site::FindMaxQuery {
                    let result = match failure {
                        Some(Fault::Thrown(e)) => DependencyReturn::PendingThrow(e.clone()),
                        Some(Fault::Local(e)) => return Err(e.clone()),
                        None => query
                            .as_ref()
                            .and_then(|q| q.actual_returned())
                            .cloned()
                            .ok_or(SourceRuleError::InvalidInput)?,
                    };
                    let mut event = obj(&[
                        ("kind", s("db")),
                        ("collection", s("txIndex")),
                        ("op", s("findOne")),
                        ("method", s("findOneAsync")),
                        ("args", V::Array(vec![obj(&[("_id", s("MaxHeight"))])])),
                    ]);
                    match result {
                        DependencyReturn::Immediate(v) | DependencyReturn::Pending(v) => {
                            set(&mut event, "returned", v)
                        }
                        DependencyReturn::Throw(e) | DependencyReturn::PendingThrow(e) => set(
                            &mut event,
                            "thrown",
                            obj(&[("name", s(e.name)), ("message", s(e.message))]),
                        ),
                    }
                    .map_err(|_| SourceRuleError::InvalidInput)?;
                    self.core.emit(event);
                    if self.core.effects_exhausted {
                        return Err(frame.stop(SourceRuleError::Capacity));
                    }
                }
                // Fixed query retains streamed canonical rows, as the existing
                // native grouping path does; it adds no synthetic db event.
                Ok(())
            }
            Site::TallyQuery => self.native_main_startup_db_query_deliver_callback(
                frame
                    .tally_query
                    .as_mut()
                    .ok_or(SourceRuleError::InvalidInput)?,
            ),
            Site::MaxProcessedQuery => self.native_main_startup_db_query_deliver_callback(
                frame
                    .consensus_query
                    .as_mut()
                    .ok_or(SourceRuleError::InvalidInput)?,
            ),
            _ => return Err(SourceRuleError::InvalidInput),
        };
        result.map_err(|error| frame.stop(error))
    }
    pub(crate) fn native_main_outer_bootstrap_resume_actual(
        &mut self,
        frame: &mut NativeMainOuterBootstrapV1,
        await_: NativeMainOuterBootstrapAwaitV1,
        index: &SourceCompactIndex,
        register_child: &mut impl FnMut(u64, &'static str) -> Result<u64, SourceRuleError>,
    ) -> Result<(), SourceRuleError> {
        frame.require(await_)?;
        frame.require_index(index)?;
        if frame.actual_completion(await_)?.is_none() {
            return Err(SourceRuleError::InvalidInput);
        }
        self.native_main_outer_bootstrap_resume_actual_inner(frame, await_, index, register_child)
            .map_err(|error| frame.stop(error))?;
        if self.core.effects_exhausted {
            return Err(frame.stop(SourceRuleError::Capacity));
        }
        Ok(())
    }
    fn native_main_outer_bootstrap_resume_actual_inner(
        &mut self,
        frame: &mut NativeMainOuterBootstrapV1,
        await_: NativeMainOuterBootstrapAwaitV1,
        index: &SourceCompactIndex,
        register_child: &mut impl FnMut(u64, &'static str) -> Result<u64, SourceRuleError>,
    ) -> Result<(), SourceRuleError> {
        frame.require(await_)?;
        frame.require_index(index)?;
        if frame.actual_completion(await_)?.is_none() {
            return Err(SourceRuleError::InvalidInput);
        }
        use NativeMainOuterBootstrapLaneV1 as Lane;
        use NativeMainOuterBootstrapSiteV1 as Site;
        match await_.site {
            Site::FindMaxFactory | Site::ConsensusFactory => {
                if let Err(error) = frame.actual_factory(await_.site)? {
                    return match error {
                        Fault::Local(error) => Err(frame.stop(error)),
                        error => {
                            if await_.site == Site::FindMaxFactory {
                                frame.find_max_return = Some(Err(error));
                                frame.install(Lane::Main, Site::FindMaxResult)
                            } else {
                                frame.max_processed_return = Some(Err(error));
                                frame.install(Lane::Main, Site::MaxProcessedResult)
                            }
                        }
                    };
                }
                if await_.site == Site::FindMaxFactory {
                    frame.install(Lane::Main, Site::FindMaxQuery)?;
                    self.native_main_outer_bootstrap_request_native(frame, index, false)
                } else {
                    frame.install(Lane::Main, Site::MaxProcessedQuery)?;
                    let query = self.native_main_startup_db_query_request(
                        "consensus",
                        "findOneAsync",
                        vec![obj(&[("_id", s("MaxProcessedHeight"))])],
                    )?;
                    let stop = query.stopped().cloned();
                    frame.consensus_query = Some(query);
                    stop.map_or(Ok(()), Err)
                }
            }
            Site::FindMaxQuery => {
                let result =
                    if let Some(error) = &frame.find_max_failure {
                        Err(match error {
                            Fault::Thrown(e) => Fault::Thrown(e.clone()),
                            Fault::Local(e) => Fault::Local(e.clone()),
                        })
                    } else {
                        let query = frame
                            .find_max_query
                            .as_mut()
                            .ok_or(SourceRuleError::InvalidInput)?;
                        query
                            .consume_callback()
                            .map_err(|_| SourceRuleError::InvalidInput)?;
                        match query
                            .actual_returned()
                            .cloned()
                            .ok_or(SourceRuleError::InvalidInput)?
                        {
                            DependencyReturn::Immediate(value)
                            | DependencyReturn::Pending(value) => Ok(value),
                            DependencyReturn::Throw(error)
                            | DependencyReturn::PendingThrow(error) => Err(Fault::Thrown(error)),
                        }
                    };
                match result {
                    Ok(doc) => {
                        frame.find_max_return = Some(Ok(if truth(&doc) {
                            doc.get("value")
                        } else {
                            V::Null
                        }))
                    }
                    Err(Fault::Local(error)) => return Err(frame.stop(error)),
                    Err(error) => frame.find_max_return = Some(Err(error)),
                }
                frame.install(Lane::Main, Site::FindMaxResult)
            }
            Site::FindMaxResult => {
                match outer_bootstrap_clone_result(
                    frame
                        .find_max_return
                        .as_ref()
                        .ok_or(SourceRuleError::InvalidInput)?,
                ) {
                    Ok(value) => frame.last_indexed_block = Some(value),
                    Err(error) => return frame.finish_main(Err(error)),
                }
                frame.install(Lane::Main, Site::TxIndexFactory)?;
                self.native_main_outer_bootstrap_capture_factory(
                    frame,
                    "txIndex",
                    Site::TxIndexFactory,
                )
            }
            Site::TxIndexFactory => {
                if let Err(error) = frame.actual_factory(Site::TxIndexFactory)? {
                    return frame.finish_main(Err(error));
                }
                // Actual synchronous new-instance/loadFromDB entry. This child
                // is reserved NOW, not at outer begin or as its parent's job.
                let job = register_child(frame.source_job, "TallyMap.getInstance/loadFromDB")
                    .map_err(|error| frame.stop(error))?;
                if job == 0 || job == frame.source_job {
                    return Err(frame.stop(SourceRuleError::InvalidInput));
                }
                frame.tally_job = Some(job);
                self.core.source_send_tally_instance_initialized = true;
                frame.install(Lane::Tally, Site::TallyFactory)?;
                self.native_main_outer_bootstrap_capture_factory(
                    frame,
                    "tallyMap",
                    Site::TallyFactory,
                )?;
                // No reaction/job drain between these two original factories.
                frame.install(Lane::Main, Site::ConsensusFactory)?;
                self.native_main_outer_bootstrap_capture_factory(
                    frame,
                    "consensus",
                    Site::ConsensusFactory,
                )
            }
            Site::TallyFactory => match frame.actual_factory(Site::TallyFactory)? {
                Ok(_) => {
                    frame.install(Lane::Tally, Site::TallyQuery)?;
                    let query = self.native_main_startup_db_query_request(
                        "tallyMap",
                        "findOneAsync",
                        vec![obj(&[("_id", s("tallyMap"))])],
                    )?;
                    let stop = query.stopped().cloned();
                    frame.tally_query = Some(query);
                    stop.map_or(Ok(()), Err)
                }
                Err(Fault::Thrown(error)) => {
                    frame.tally_swallowed = Some(error);
                    frame.tally_caught_undefined = Some(V::Undefined);
                    frame.tally_load_complete = true;
                    frame.install(Lane::Tally, Site::TallyLoadResult)
                }
                Err(Fault::Local(error)) => Err(frame.stop(error)),
            },
            Site::TallyQuery => {
                let result = frame
                    .tally_query
                    .as_mut()
                    .ok_or(SourceRuleError::InvalidInput)?
                    .take_actual_result()?;
                frame.tally_query_return = Some(outer_bootstrap_clone_result(&result));
                match result.and_then(|row| ContractCore::tally_parsed_entries(&row)) {
                    Ok(entries) => {
                        self.core.tally = entries.clone();
                        self.core.tally_static_initialized = true;
                        frame.tally_returned_map = Some(entries);
                    }
                    Err(Fault::Thrown(error)) => {
                        frame.tally_swallowed = Some(error);
                        frame.tally_caught_undefined = Some(V::Undefined);
                    }
                    Err(Fault::Local(error)) => return Err(frame.stop(error)),
                }
                frame.tally_load_complete = true;
                frame.install(Lane::Tally, Site::TallyLoadResult)
            }
            Site::TallyLoadResult => {
                frame.tally_instance = Some(obj(&[("sourceTallyInstance", V::Bool(true))]));
                frame.tally_await = None;
                Ok(())
            }
            Site::MaxProcessedQuery => {
                let result = frame
                    .consensus_query
                    .as_mut()
                    .ok_or(SourceRuleError::InvalidInput)?
                    .take_actual_result()?;
                frame.consensus_query_return = Some(outer_bootstrap_clone_result(&result));
                match result {
                    Ok(doc) => {
                        frame.max_processed_return = Some(Ok(if truth(&doc) {
                            doc.get("value")
                        } else {
                            V::Null
                        }))
                    }
                    Err(Fault::Local(error)) => return Err(frame.stop(error)),
                    Err(error) => frame.max_processed_return = Some(Err(error)),
                }
                frame.install(Lane::Main, Site::MaxProcessedResult)
            }
            Site::MaxProcessedResult => {
                match outer_bootstrap_clone_result(
                    frame
                        .max_processed_return
                        .as_ref()
                        .ok_or(SourceRuleError::InvalidInput)?,
                ) {
                    Ok(value) => frame.last_consensus_height = Some(value),
                    Err(error) => return frame.finish_main(Err(error)),
                }
                frame.install(Lane::Main, Site::FixedRead)?;
                self.native_main_outer_bootstrap_request_native(frame, index, true)
            }
            Site::FixedRead => {
                if let Some(error) = &frame.fixed_failure {
                    return frame.finish_main(Err(match error {
                        Fault::Thrown(e) => Fault::Thrown(e.clone()),
                        Fault::Local(e) => Fault::Local(e.clone()),
                    }));
                }
                let query = frame
                    .fixed_query
                    .as_mut()
                    .ok_or(SourceRuleError::InvalidInput)?;
                query
                    .consume_callback()
                    .map_err(|_| SourceRuleError::InvalidInput)?;
                let read = query.actual_view().ok_or(SourceRuleError::InvalidInput)?;
                let result = native_main_outer_bootstrap_plan(
                    &frame.start,
                    read,
                    &mut frame.grouping,
                    &mut frame.storage_stop,
                );
                frame.finish_main(result)
            }
        }
    }
}
fn native_main_outer_bootstrap_plan(
    start: &V,
    read: &crate::source_compact_index_v2::MainIndexReadViewV1,
    grouping: &mut Option<crate::source_compact_index_v2::RowStreamReceipt>,
    storage_stop: &mut Option<String>,
) -> R<Option<NativeMainCatchupPreparedV1>> {
    let plan = match read
        .plan_main_groups()
        .map_err(|error| native_main_store_fault(error, storage_stop))?
    {
        crate::source_compact_index_v2::MainIndexPlanningOutcome::Ready(plan) => {
            *grouping = Some(plan.receipt().clone());
            plan
        }
        crate::source_compact_index_v2::MainIndexPlanningOutcome::Thrown { exception, receipt } => {
            *grouping = Some(*receipt);
            return Err(Fault::Thrown(exception));
        }
    };
    let last_value = plan.max_index_block();
    if last_value == V::Null {
        return Ok(None);
    }
    let last = number(&last_value)?;
    if last == 0. || last.is_nan() {
        return Ok(None);
    }
    let h = number(start)?;
    const MAX_FINITE_MAIN_HEIGHT: f64 = 9_007_199_254_740_991.;
    if h > MAX_FINITE_MAIN_HEIGHT || last > MAX_FINITE_MAIN_HEIGHT {
        return Err(SourceRuleError::UnsupportedNumericDomain.into());
    }
    if !h.is_finite() || h.fract() != 0. || h < 0. || !last.is_finite() || last - h > 1024. {
        return Err(SourceRuleError::Capacity.into());
    }
    Ok(Some(NativeMainCatchupPreparedV1 { plan, h, last }))
}

include!("source_main_outer_bootstrap_factory_stop_impl.rs");
