// Under the startup profile the legacy Incoming/Recovery height reads and
// writes are genuine NeDB 1.8 consensus requests (findOne by _id, upsert). The
// job parks at the end of its phase; the actual primitive runs at the sample
// delivery that reads or modifies the document; the job is ready again only at
// the callback, where its next phase consumes the retained result.
const INCOMING: &str = "Main.processIncomingBlocks";
const RECOVERY: &str = "Main.enterRecoveryMode";

#[cfg(test)]
#[derive(Debug, PartialEq, Eq)]
struct StartupLegacyParked {
    job: u64,
    request: startup_source_nedb_jobs::RequestKey,
    payload: startup_source_promise_queue::ActualPayloadRef,
    phase: String,
    operation: MainStepPrimitive,
    existence_sampled: bool,
    outcome: Option<Result<V, MainStepLegacyThrow>>,
}
#[cfg(test)]
#[derive(Debug, Default, PartialEq, Eq)]
struct StartupLegacyLane {
    // Legacy job, its source job in the Promise domain, its last await ordinal.
    source_jobs: Vec<(u64, startup_source_promise_queue::SourceJobKey, u64)>,
    parked: Vec<StartupLegacyParked>,
    results: Vec<(u64, Result<V, MainStepLegacyThrow>)>,
}

#[cfg(test)]
impl MainStepKernelV1 {
    fn startup_park(
        &mut self,
        job: u64,
        scope: &'static str,
        origin: u8,
        operation: MainStepPrimitive,
    ) -> Advance<()> {
        use startup_source_nedb_jobs::{Collection, QueryKind};
        let stop = AdvanceError::Stop;
        let (site, kind) = match &operation {
            MainStepPrimitive::HeightRead("TrackHeight") => {
                ("loadTrackHeight", QueryKind::FindOneStringId)
            }
            MainStepPrimitive::HeightRead("MaxProcessedHeight") => {
                ("loadMaxProcessedHeight", QueryKind::FindOneStringId)
            }
            MainStepPrimitive::HeightWrite {
                name: "TrackHeight",
                ..
            } => ("saveTrackHeight", QueryKind::Upsert),
            _ => return Err(stop(SourceRuleError::Backend)),
        };
        let (Some(promises), Some(nedb)) = (
            self.startup_source_promises.as_mut(),
            self.startup_source_nedb_jobs.as_mut(),
        ) else {
            return Err(stop(SourceRuleError::Backend));
        };
        let lane = &mut self.startup_legacy;
        let at = match lane.source_jobs.iter().position(|(id, ..)| *id == job) {
            Some(at) => at,
            None => {
                let key = promises.allocate_source_job(scope).map_err(stop)?;
                lane.source_jobs.push((job, key, 0));
                lane.source_jobs.len() - 1
            }
        };
        let ordinal = lane.source_jobs[at]
            .2
            .checked_add(1)
            .ok_or(stop(SourceRuleError::Capacity))?;
        lane.source_jobs[at].2 = ordinal;
        let payload = promises
            .reserve_actual_payload(startup_source_promise_queue::ActualPayloadBinding {
                job: lane.source_jobs[at].1,
                method_scope: scope,
                source_site: site,
                lane: "Incoming",
                await_ordinal: ordinal,
            })
            .map_err(stop)?;
        let request = nedb
            .enqueue_actual_request(promises, payload, Collection::Consensus, kind)
            .map_err(stop)?;
        lane.parked.push(StartupLegacyParked {
            job,
            request,
            payload,
            phase: format!("{scope}/{origin}"),
            operation,
            existence_sampled: false,
            outcome: None,
        });
        let at = self.position(job);
        self.jobs[at].status = MainStepJobStatus::WaitingDatabase(request.serial());
        Ok(())
    }
    fn startup_take(&mut self, job: u64) -> Advance<V> {
        let results = &mut self.startup_legacy.results;
        let at = results
            .iter()
            .position(|(id, _)| *id == job)
            .ok_or(AdvanceError::Stop(SourceRuleError::Backend))?;
        results.remove(at).1.map_err(AdvanceError::Throw)
    }
    /// Route one NeDB delivery to its parked legacy job. `Ok(false)`: the
    /// request is not a legacy one; its own route owns the delivery.
    fn startup_legacy_deliver(
        &mut self,
        owner: &mut impl MainStepOwner,
        delivery: startup_source_nedb_jobs::Delivery,
        evaluation: &mut MainStepEvaluationV1,
    ) -> Advance<bool> {
        use startup_source_nedb_jobs::DeliveryKind;
        let stop = AdvanceError::Stop;
        let Some(at) = self
            .startup_legacy
            .parked
            .iter()
            .position(|parked| parked.request == delivery.request)
        else {
            return Ok(false);
        };
        let job = self.startup_legacy.parked[at].job;
        match delivery.kind {
            DeliveryKind::SampleCandidates => {
                let parked = &mut self.startup_legacy.parked[at];
                if parked.outcome.is_some() {
                    return Err(stop(SourceRuleError::Backend));
                }
                let existence = match parked.operation {
                    MainStepPrimitive::HeightWrite { name, .. } if !parked.existence_sampled => {
                        Some(name)
                    }
                    _ => None,
                };
                if let Some(name) = existence {
                    parked.existence_sampled = true;
                    // A found document is modified by the second candidate
                    // pass; only that sample runs the actual write.
                    if owner.startup_height_present(name).map_err(stop)? {
                        return self
                            .startup_nedb()?
                            .acknowledge_upsert_sample(delivery, true)
                            .map(|()| true)
                            .map_err(stop);
                    }
                }
                let (phase, operation) = (parked.phase.clone(), parked.operation.clone());
                let outcome = match Self::operation(owner, job, &phase, operation, evaluation) {
                    Ok(value) => Ok(value),
                    Err(AdvanceError::Throw(error)) => Err(error),
                    Err(fault) => return Err(fault),
                };
                self.startup_legacy.parked[at].outcome = Some(outcome);
                let nedb = self.startup_nedb()?;
                if existence.is_some() {
                    nedb.acknowledge_upsert_sample(delivery, false)
                } else {
                    nedb.acknowledge_actual(delivery)
                }
                .map_err(stop)?;
            }
            DeliveryKind::DeliverCallback => {
                self.startup_nedb()?
                    .acknowledge_actual(delivery)
                    .map_err(stop)?;
                let parked = self.startup_legacy.parked.remove(at);
                let outcome = parked.outcome.ok_or(stop(SourceRuleError::Backend))?;
                self.startup_source_promises
                    .as_mut()
                    .ok_or(stop(SourceRuleError::Backend))?
                    .mark_actual_payload_ready(parked.payload)
                    .map_err(stop)?;
                let at = self.position(job);
                if self.jobs[at].status
                    != MainStepJobStatus::WaitingDatabase(parked.request.serial())
                {
                    return Err(stop(SourceRuleError::Backend));
                }
                self.startup_legacy.results.push((job, outcome));
                self.jobs[at].status = MainStepJobStatus::Ready;
                self.ready.push_back(job);
            }
        }
        Ok(true)
    }
    fn startup_nedb(&mut self) -> Advance<&mut startup_source_nedb_jobs::StartupNedbQueue> {
        self.startup_source_nedb_jobs
            .as_mut()
            .ok_or(AdvanceError::Stop(SourceRuleError::Backend))
    }
    fn startup_legacy_start(
        &mut self,
        method: &str,
        arguments: &[V],
    ) -> Result<u64, MainStepError> {
        if !self.startup_source_profile_active() {
            return Err(MainStepError::InvalidInput);
        }
        if self.terminal.is_some() {
            return Err(MainStepError::AlreadyExited);
        }
        if self.owner_stopped.is_some() {
            return Err(MainStepError::OwnerStopped);
        }
        let frame = self.frame(method, arguments)?;
        if !matches!(frame, Frame::Incoming { .. }) {
            return Err(MainStepError::InvalidInput);
        }
        self.spawn(frame, None, true)
    }
    /// Every ready legacy job, then exactly one NeDB immediate, until both are
    /// empty. Pending timers stay pending: no timer is fired here.
    fn startup_legacy_run(
        &mut self,
        owner: &mut impl MainStepOwner,
    ) -> Result<MainStepEvaluationV1, MainStepError> {
        if !self.startup_source_profile_active() {
            return Err(MainStepError::InvalidInput);
        }
        if self.terminal.is_some() {
            return Err(MainStepError::AlreadyExited);
        }
        if self.owner_stopped.is_some() {
            return Err(MainStepError::OwnerStopped);
        }
        let mut evaluation = MainStepEvaluationV1 {
            started: None,
            stop: MainStepStop::Quiescent,
            trace: vec![],
            effects: vec![],
            completed: vec![],
        };
        for _ in 0..MAX_STEPS {
            if let Some(id) = self.ready.pop_front() {
                if let Err(fault) = self.step_ready(id, owner, &mut evaluation) {
                    return Ok(self.startup_stop(fault, evaluation));
                }
                continue;
            }
            if let Some((job, error)) = self.pending_unhandled.pop_front() {
                if let Err(fault) = Self::operation(
                    owner,
                    job,
                    "UnhandledRejectionCheckpoint",
                    MainStepPrimitive::PublishUnhandled(error),
                    &mut evaluation,
                ) {
                    return Ok(self.startup_stop(fault, evaluation));
                }
                continue;
            }
            let delivery = match self.startup_nedb().map(|nedb| nedb.next_delivery()) {
                Ok(Ok(Some(delivery))) => delivery,
                Ok(Ok(None)) => {
                    self.prune_completed();
                    return Ok(evaluation);
                }
                Ok(Err(error)) => {
                    return Ok(self.startup_stop(AdvanceError::Stop(error), evaluation))
                }
                Err(fault) => return Ok(self.startup_stop(fault, evaluation)),
            };
            match self.startup_legacy_deliver(owner, delivery, &mut evaluation) {
                Ok(true) => {}
                Ok(false) => {
                    let fault = AdvanceError::Stop(SourceRuleError::InvalidInput);
                    return Ok(self.startup_stop(fault, evaluation));
                }
                Err(fault) => return Ok(self.startup_stop(fault, evaluation)),
            }
        }
        self.prune_completed();
        evaluation.stop = MainStepStop::DrainBound;
        Ok(evaluation)
    }
    // A stop freezes the whole same-owner domain at its accepted prefix.
    fn startup_stop(
        &mut self,
        fault: AdvanceError,
        evaluation: MainStepEvaluationV1,
    ) -> MainStepEvaluationV1 {
        let evaluation = self.stop_on_fault(fault, evaluation);
        if let Some(error) = self.owner_stopped.clone() {
            if let Some(nedb) = self.startup_source_nedb_jobs.as_mut() {
                nedb.stop_after_owner_prefix(error.clone());
            }
            if let Some(promises) = self.startup_source_promises.as_mut() {
                promises.stop_after_owner_prefix(error);
            }
        }
        evaluation
    }
    fn startup_visit_legacy(
        &self,
        visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
    ) -> Result<(), SourceRuleError> {
        let lane = &self.startup_legacy;
        visitor(
            "startupLegacy/parked/count",
            &s(lane.parked.len().to_string()),
        )?;
        for (at, parked) in lane.parked.iter().enumerate() {
            let p = format!("startupLegacy/parked/{at}");
            visitor(&format!("{p}/job"), &s(parked.job.to_string()))?;
            visitor(
                &format!("{p}/request"),
                &s(parked.request.serial().to_string()),
            )?;
            visitor(&format!("{p}/phase"), &s(&parked.phase))?;
            visitor(&format!("{p}/operation"), &parked.operation.value())?;
            visitor(
                &format!("{p}/existenceSampled"),
                &V::Bool(parked.existence_sampled),
            )?;
            visitor(
                &format!("{p}/outcomePresent"),
                &V::Bool(parked.outcome.is_some()),
            )?;
        }
        visitor(
            "startupLegacy/results/count",
            &s(lane.results.len().to_string()),
        )
    }
}
#[cfg(not(test))]
impl MainStepKernelV1 {
    fn startup_park(
        &mut self,
        _job: u64,
        _scope: &'static str,
        _origin: u8,
        _operation: MainStepPrimitive,
    ) -> Advance<()> {
        Err(AdvanceError::Stop(SourceRuleError::Backend))
    }
    fn startup_take(&mut self, _job: u64) -> Advance<V> {
        Err(AdvanceError::Stop(SourceRuleError::Backend))
    }
}
