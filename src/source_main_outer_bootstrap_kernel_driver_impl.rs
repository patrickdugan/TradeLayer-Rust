// The SAME Kernel's intrinsic Promise queue and NeDB executor choose every outer
// continuation. The frame owns every actual result; routes carry only keys.
use crate::source_main_step_kernel::startup_source_nedb_jobs::{
    Collection as NedbCollection, DeliveryKind as NedbDeliveryKind, QueryKind as NedbQueryKind,
    RequestKey as NedbRequestKey,
};
use crate::source_main_step_kernel::startup_source_promise_queue::{
    ActualPayloadBinding, ActualPayloadRef, ActualSettlement, PromiseKey, Realm,
    SourceContinuationKey, SourceJobKey, StartupPromiseQueue,
};
use crate::source_main_step_kernel::MainStepKernelV1;

struct NativeMainOuterKernelRouteV1 {
    await_: NativeMainOuterBootstrapAwaitV1,
    payload: ActualPayloadRef,
    promise: PromiseKey,
    continuation: SourceContinuationKey,
    request: Option<NedbRequestKey>,
}
// A parent await on an async call's still-pending Promise, subscribed when the
// call returned it. The frame's result descriptor is attached only later.
struct NativeMainOuterKernelEarlyRouteV1 {
    job: SourceJobKey,
    lane: NativeMainOuterBootstrapLaneV1,
    site: NativeMainOuterBootstrapSiteV1,
    call_ordinal: u64,
    promise: PromiseKey,
    continuation: SourceContinuationKey,
    attached: Option<u64>,
}
/// Raw request identity in actual issue order (collection, method, arguments).
pub(crate) struct NativeMainOuterKernelRequestV1 {
    pub(crate) collection: &'static str,
    pub(crate) method: &'static str,
    pub(crate) arguments: Vec<V>,
}
pub(crate) struct NativeMainOuterKernelDriverV1 {
    kernel: MainStepKernelV1,
    main_job: SourceJobKey,
    tally_job: Option<SourceJobKey>,
    routes: Vec<NativeMainOuterKernelRouteV1>,
    early: Vec<NativeMainOuterKernelEarlyRouteV1>,
    requests: Vec<NativeMainOuterKernelRequestV1>,
    resumed: Vec<NativeMainOuterBootstrapAwaitV1>,
}
// The async call whose first await is this factory, with its awaiting caller:
// Main awaits findMaxIndexedBlock and loadMaxProcessedHeight; Tally.getInstance
// awaits loadFromDB on the independent child job.
fn outer_kernel_parent_site(
    factory: NativeMainOuterBootstrapSiteV1,
) -> Option<(NativeMainOuterBootstrapSiteV1, &'static str, &'static str)> {
    use NativeMainOuterBootstrapSiteV1 as Site;
    match factory {
        Site::FindMaxFactory => Some((
            Site::FindMaxResult,
            "FindMaxResult",
            "Main.constructConsensusFromIndex",
        )),
        Site::TallyFactory => Some((
            Site::TallyLoadResult,
            "TallyLoadResult",
            "TallyMap.getInstance",
        )),
        Site::ConsensusFactory => Some((
            Site::MaxProcessedResult,
            "MaxProcessedResult",
            "Main.constructConsensusFromIndex",
        )),
        _ => None,
    }
}
fn outer_kernel_job(
    main_job: SourceJobKey,
    tally_job: Option<SourceJobKey>,
    await_: NativeMainOuterBootstrapAwaitV1,
) -> Result<SourceJobKey, SourceRuleError> {
    let job = match await_.lane() {
        NativeMainOuterBootstrapLaneV1::Main => main_job,
        NativeMainOuterBootstrapLaneV1::Tally => tally_job.ok_or(SourceRuleError::InvalidInput)?,
    };
    if job.serial() != await_.source_job() {
        return Err(SourceRuleError::InvalidInput);
    }
    Ok(job)
}
fn outer_kernel_binding(
    job: SourceJobKey,
    await_: NativeMainOuterBootstrapAwaitV1,
) -> ActualPayloadBinding {
    ActualPayloadBinding {
        job,
        method_scope: await_.method_scope(),
        source_site: await_.source_site_name(),
        lane: await_.lane_name(),
        await_ordinal: await_.ordinal(),
    }
}
// The awaiting source subscribes when the call returns its Promise; the result
// is attached only later, from the frame's retained actual completion.
fn outer_kernel_open(
    promises: &mut StartupPromiseQueue,
    routes: &mut Vec<NativeMainOuterKernelRouteV1>,
    job: SourceJobKey,
    await_: NativeMainOuterBootstrapAwaitV1,
    realm: Realm,
) -> Result<(), SourceRuleError> {
    if routes.iter().any(|route| route.await_ == await_) {
        return Err(SourceRuleError::InvalidInput);
    }
    let binding = outer_kernel_binding(job, await_);
    let payload = promises.reserve_actual_payload(binding)?;
    let promise = promises.create(realm)?;
    let continuation = promises.reserve_source_continuation(binding)?;
    promises.await_routed(Realm::SourceBusiness, promise, continuation)?;
    routes.push(NativeMainOuterKernelRouteV1 {
        await_,
        payload,
        promise,
        continuation,
        request: None,
    });
    Ok(())
}
fn outer_kernel_settle(
    promises: &mut StartupPromiseQueue,
    routes: &[NativeMainOuterKernelRouteV1],
    frame: &NativeMainOuterBootstrapV1,
    await_: NativeMainOuterBootstrapAwaitV1,
) -> Result<(), SourceRuleError> {
    let route = routes
        .iter()
        .find(|route| route.await_ == await_)
        .ok_or(SourceRuleError::InvalidInput)?;
    // Readiness comes only from the frame's retained true completion.
    if frame.actual_completion(await_)?.is_none() {
        return Err(SourceRuleError::InvalidInput);
    }
    promises.mark_actual_payload_ready(route.payload)?;
    // Each route's Promise is resolved exactly once; a locked resolver here
    // would mean a second settlement, which the binding refuses.
    if !promises.resolve_actual(route.promise, &ActualSettlement::Payload(route.payload))? {
        return Err(SourceRuleError::InvalidInput);
    }
    Ok(())
}
struct NativeMainOuterKernelHooksV1<'a> {
    promises: &'a mut StartupPromiseQueue,
    routes: &'a mut Vec<NativeMainOuterKernelRouteV1>,
    early: &'a mut Vec<NativeMainOuterKernelEarlyRouteV1>,
    main_job: SourceJobKey,
    tally_job: &'a mut Option<SourceJobKey>,
}
impl NativeMainOuterFactoryHooksV1 for NativeMainOuterKernelHooksV1<'_> {
    fn register_child(
        &mut self,
        parent: u64,
        method_scope: &'static str,
    ) -> Result<u64, SourceRuleError> {
        // The independent Tally job exists only from its true call onward.
        if parent != self.main_job.serial() || self.tally_job.is_some() {
            return Err(SourceRuleError::InvalidInput);
        }
        let job = self.promises.allocate_source_job(method_scope)?;
        *self.tally_job = Some(job);
        Ok(job.serial())
    }
    fn before_factory(
        &mut self,
        await_: NativeMainOuterBootstrapAwaitV1,
    ) -> Result<(), SourceRuleError> {
        let job = outer_kernel_job(self.main_job, *self.tally_job, await_)?;
        outer_kernel_open(
            self.promises,
            self.routes,
            job,
            await_,
            Realm::RecordedHostIo,
        )
    }
    fn after_factory_actual(
        &mut self,
        frame: &NativeMainOuterBootstrapV1,
        await_: NativeMainOuterBootstrapAwaitV1,
    ) -> Result<(), SourceRuleError> {
        outer_kernel_settle(self.promises, self.routes, frame, await_)?;
        // The async call awaited this factory and returned its still-pending
        // Promise; its caller subscribes now, before any later reaction runs.
        let Some((site, site_name, scope)) = outer_kernel_parent_site(await_.site()) else {
            return Ok(());
        };
        let job = outer_kernel_job(self.main_job, *self.tally_job, await_)?;
        if self
            .early
            .iter()
            .any(|early| early.job == job && early.site == site)
        {
            return Err(SourceRuleError::InvalidInput);
        }
        let continuation = self
            .promises
            .reserve_source_continuation(ActualPayloadBinding {
                job,
                method_scope: scope,
                source_site: site_name,
                lane: await_.lane_name(),
                await_ordinal: await_.ordinal(),
            })?;
        let promise = self.promises.create(Realm::SourceBusiness)?;
        self.promises
            .await_routed(Realm::SourceBusiness, promise, continuation)?;
        self.early.push(NativeMainOuterKernelEarlyRouteV1 {
            job,
            lane: await_.lane(),
            site,
            call_ordinal: await_.ordinal(),
            promise,
            continuation,
            attached: None,
        });
        Ok(())
    }
}
impl NativeMainOuterKernelDriverV1 {
    pub(crate) fn new(
        runtime: &SourceContractRuntime,
        start: V,
        index: &SourceCompactIndex,
        generation: u64,
    ) -> Result<(Self, NativeMainOuterBootstrapV1), SourceRuleError> {
        let mut kernel = MainStepKernelV1::new(generation);
        kernel
            .startup_install_source_promises()
            .map_err(|_| SourceRuleError::InvalidInput)?;
        let (promises, _) = kernel
            .startup_source_parts()
            .map_err(|_| SourceRuleError::InvalidInput)?;
        let main_job = promises.allocate_source_job("Main.constructConsensusFromIndex")?;
        let frame = runtime.native_main_outer_bootstrap_new(start, main_job.serial(), index)?;
        Ok((
            Self {
                kernel,
                main_job,
                tally_job: None,
                routes: vec![],
                early: vec![],
                requests: vec![],
                resumed: vec![],
            },
            frame,
        ))
    }
    pub(crate) fn kernel(&self) -> &MainStepKernelV1 {
        &self.kernel
    }
    pub(crate) fn requests(&self) -> &[NativeMainOuterKernelRequestV1] {
        &self.requests
    }
    pub(crate) fn resumed(&self) -> &[NativeMainOuterBootstrapAwaitV1] {
        &self.resumed
    }
    pub(crate) fn tally_job(&self) -> Option<u64> {
        self.tally_job.map(SourceJobKey::serial)
    }
    /// (result site, ordinal of the call that returned the pending Promise,
    /// ordinal of the result descriptor later attached), in subscription order.
    pub(crate) fn early_routes(&self) -> Vec<(NativeMainOuterBootstrapSiteV1, u64, Option<u64>)> {
        self.early
            .iter()
            .map(|early| (early.site, early.call_ordinal, early.attached))
            .collect()
    }
    /// Begin, then let only the Kernel's queues choose continuations until both
    /// are empty: every ready microtask, then exactly one NeDB immediate.
    pub(crate) fn run(
        &mut self,
        runtime: &mut SourceContractRuntime,
        frame: &mut NativeMainOuterBootstrapV1,
        index: &SourceCompactIndex,
    ) -> Result<(), SourceRuleError> {
        let Self {
            kernel,
            main_job,
            tally_job,
            routes,
            early,
            requests,
            resumed,
        } = self;
        let (promises, nedb) = kernel
            .startup_source_parts()
            .map_err(|_| SourceRuleError::InvalidInput)?;
        let mut hooks = NativeMainOuterKernelHooksV1 {
            promises,
            routes,
            early,
            main_job: *main_job,
            tally_job,
        };
        runtime.native_main_outer_bootstrap_begin_with_hooks(frame, &mut hooks)?;
        loop {
            outer_kernel_register(&mut hooks, nedb, requests, runtime, frame, index)?;
            while let Some(delivery) = hooks.promises.next_delivery()? {
                let route = hooks
                    .routes
                    .iter()
                    .find(|route| route.continuation.serial() == delivery.continuation)
                    .ok_or(SourceRuleError::InvalidInput)?;
                if delivery.settlement != ActualSettlement::Payload(route.payload) {
                    return Err(SourceRuleError::InvalidInput);
                }
                let await_ = route.await_;
                runtime.native_main_outer_bootstrap_resume_actual_with_hooks(
                    frame, await_, index, &mut hooks,
                )?;
                hooks.promises.acknowledge(delivery.ticket)?;
                resumed.push(await_);
                outer_kernel_register(&mut hooks, nedb, requests, runtime, frame, index)?;
            }
            if !hooks.promises.end_turn()?.is_empty() {
                return Err(SourceRuleError::InvalidInput);
            }
            let Some(delivery) = nedb.next_delivery()? else {
                return Ok(());
            };
            let await_ = hooks
                .routes
                .iter()
                .find(|route| {
                    route.request == Some(delivery.request) && route.payload == delivery.payload
                })
                .ok_or(SourceRuleError::InvalidInput)?
                .await_;
            match delivery.kind {
                NedbDeliveryKind::SampleCandidates => {
                    runtime
                        .native_main_outer_bootstrap_query_sample_actual(frame, await_, index)?;
                    nedb.acknowledge_actual(delivery)?;
                }
                NedbDeliveryKind::DeliverCallback => {
                    runtime
                        .native_main_outer_bootstrap_query_deliver_callback(frame, await_, index)?;
                    nedb.acknowledge_actual(delivery)?;
                    outer_kernel_settle(hooks.promises, hooks.routes, frame, await_)?;
                }
            }
        }
    }
}
// Route newly issued non-factory awaits. Factory awaits were routed by hooks
// around their actual call; a missing factory route is a binding error.
fn outer_kernel_register(
    hooks: &mut NativeMainOuterKernelHooksV1<'_>,
    nedb: &mut crate::source_main_step_kernel::startup_source_nedb_jobs::StartupNedbQueue,
    requests: &mut Vec<NativeMainOuterKernelRequestV1>,
    runtime: &mut SourceContractRuntime,
    frame: &mut NativeMainOuterBootstrapV1,
    index: &SourceCompactIndex,
) -> Result<(), SourceRuleError> {
    use NativeMainOuterBootstrapSiteV1 as Site;
    for lane in [
        NativeMainOuterBootstrapLaneV1::Main,
        NativeMainOuterBootstrapLaneV1::Tally,
    ] {
        let Some(await_) = frame.pending_site(lane) else {
            continue;
        };
        if hooks.routes.iter().any(|route| route.await_ == await_) {
            continue;
        }
        let job = outer_kernel_job(hooks.main_job, *hooks.tally_job, await_)?;
        match await_.site() {
            Site::FindMaxFactory
            | Site::TxIndexFactory
            | Site::TallyFactory
            | Site::ConsensusFactory => return Err(SourceRuleError::InvalidInput),
            Site::FindMaxResult | Site::TallyLoadResult | Site::MaxProcessedResult => {
                // Attach the actual result to the route its caller subscribed
                // at call return; never open a fresh late subscription.
                let early = hooks
                    .early
                    .iter_mut()
                    .find(|early| {
                        early.job == job
                            && early.lane == await_.lane()
                            && early.site == await_.site()
                            && early.attached.is_none()
                    })
                    .ok_or(SourceRuleError::InvalidInput)?;
                if early.call_ordinal >= await_.ordinal() {
                    return Err(SourceRuleError::InvalidInput);
                }
                let payload = hooks
                    .promises
                    .reserve_actual_payload(outer_kernel_binding(job, await_))?;
                early.attached = Some(await_.ordinal());
                hooks.routes.push(NativeMainOuterKernelRouteV1 {
                    await_,
                    payload,
                    promise: early.promise,
                    continuation: early.continuation,
                    request: None,
                });
                outer_kernel_settle(hooks.promises, hooks.routes, frame, await_)?;
            }
            Site::FindMaxQuery | Site::TallyQuery | Site::MaxProcessedQuery | Site::FixedRead => {
                outer_kernel_open(
                    hooks.promises,
                    hooks.routes,
                    job,
                    await_,
                    Realm::RecordedHostIo,
                )?;
                let view = frame.actual_query_request_view(await_)?;
                let collection = match view.collection {
                    "txIndex" => NedbCollection::TxIndex,
                    "tallyMap" => NedbCollection::TallyMap,
                    "consensus" => NedbCollection::Consensus,
                    _ => return Err(SourceRuleError::InvalidInput),
                };
                let kind = match view.method {
                    "findOneAsync" => NedbQueryKind::FindOneStringId,
                    "findAsync" => NedbQueryKind::FindAll,
                    _ => return Err(SourceRuleError::InvalidInput),
                };
                requests.push(NativeMainOuterKernelRequestV1 {
                    collection: view.collection,
                    method: view.method,
                    arguments: view.arguments.to_vec(),
                });
                if frame.query_status(await_)?.request_rejected {
                    // An actual request-time rejection has no executor job.
                    runtime
                        .native_main_outer_bootstrap_query_deliver_callback(frame, await_, index)?;
                    outer_kernel_settle(hooks.promises, hooks.routes, frame, await_)?;
                } else {
                    let route = hooks
                        .routes
                        .iter_mut()
                        .find(|route| route.await_ == await_)
                        .ok_or(SourceRuleError::InvalidInput)?;
                    route.request = Some(nedb.enqueue_actual_request(
                        hooks.promises,
                        route.payload,
                        collection,
                        kind,
                    )?);
                }
            }
        }
    }
    Ok(())
}
