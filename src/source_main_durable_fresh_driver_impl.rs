impl DurableFreshCoordinatorV1 {
    fn record(&self, stage: &str) -> DurableFreshResult<V> {
        let p = self
            .pending
            .as_ref()
            .ok_or(NativeMainDurableFreshErrorV1::NoAction)?;
        Ok(obj(&[
            ("profile", s(NATIVE_MAIN_DURABLE_FRESH_PROFILE)),
            ("stage", s(stage)),
            ("sequence", s(&(self.history.len() + 1).to_string())),
            ("previousHead", s(&hex(&self.head))),
            ("action", ActiveServiceWalBlobV1::of(&p.raw)?.value()),
            ("before", ActiveServiceWalBlobV1::of(&p.before.raw)?.value()),
            (
                "sourceBefore",
                ActiveServiceWalBlobV1::of(&p.before_binding.raw)?.value(),
            ),
            (
                "sourceAfter",
                p.after_binding
                    .as_ref()
                    .map(|v| ActiveServiceWalBlobV1::of(&v.raw).map(|p| p.value()))
                    .transpose()?
                    .unwrap_or(V::Null),
            ),
            (
                "after",
                p.after
                    .as_ref()
                    .map(|v| ActiveServiceWalBlobV1::of(&v.raw).map(|p| p.value()))
                    .transpose()?
                    .unwrap_or(V::Null),
            ),
            (
                "entry",
                p.entry
                    .as_ref()
                    .map(|v| ActiveServiceWalBlobV1::of(v).map(|p| p.value()))
                    .transpose()?
                    .unwrap_or(V::Null),
            ),
            (
                "head",
                p.receipt.as_ref().map_or(V::Null, |r| s(&hex(&r.head))),
            ),
        ]))
    }
    fn write_barrier(&mut self, stage: &str) -> DurableFreshResult<()> {
        #[cfg(test)]
        if self
            .barrier_fault
            .as_ref()
            .is_some_and(|(phase, _)| *phase == self.phase)
        {
            let (_, fault) = self.barrier_fault.take().expect("matching phase");
            self.wal.set_test_fault(fault);
        }
        if self
            .pending
            .as_ref()
            .ok_or(NativeMainDurableFreshErrorV1::NoAction)?
            .plan
            .is_none()
        {
            let p = self.pending.as_ref().expect("same pending action");
            let mut raw = vec![p.raw.clone(), p.before.raw.clone(), p.before.source.clone()];
            raw.extend(p.before_binding.blobs());
            if self.history.is_empty() {
                raw.push(self.origin_archive.clone());
                raw.extend(self.origin_binding.blobs());
                raw.push(self.origin_observation.raw.clone());
                raw.push(self.origin_observation.source.clone());
            }
            if let Some(after) = &p.after_binding {
                raw.extend(after.blobs());
            }
            if let Some(after) = &p.after {
                raw.push(after.raw.clone());
                raw.push(after.source.clone());
            }
            if let Some(entry) = &p.entry {
                raw.push(entry.clone());
            }
            let mut blobs = vec![];
            for bytes in raw {
                if !blobs.contains(&bytes) {
                    blobs.push(bytes);
                }
            }
            let mut references = blobs
                .iter()
                .map(|raw| ActiveServiceWalBlobV1::of(raw))
                .collect::<Result<Vec<_>, _>>()?;
            for raw in self.origin_binding.blobs().into_iter().chain([
                self.origin_archive.clone(),
                self.origin_observation.raw.clone(),
                self.origin_observation.source.clone(),
            ]) {
                let reference = ActiveServiceWalBlobV1::of(&raw)?;
                if !references.contains(&reference) {
                    references.push(reference);
                }
            }
            let plan =
                self.wal
                    .prepare(&self.wal.pin()?, self.record(stage)?, blobs, references)?;
            self.pending.as_mut().expect("same pending action").plan = Some(plan);
        }
        let plan = self
            .pending
            .as_mut()
            .expect("same pending action")
            .plan
            .as_mut()
            .expect("sealed barrier");
        if plan.attempted() {
            return Err(NativeMainDurableFreshErrorV1::NeedControlRecovery);
        }
        self.wal.attempt(plan)?;
        self.pending.as_mut().expect("same pending action").plan = None;
        Ok(())
    }
    fn after_barrier(&mut self) -> DurableFreshResult<()> {
        use NativeMainDurableFreshPhaseV1 as P;
        self.phase = match self.phase {
            P::Intent => P::Execute,
            P::SourceInputsCaptured => P::Observe,
            P::Accepted => P::Ack,
            P::Ack => {
                let pending = self
                    .pending
                    .take()
                    .ok_or(NativeMainDurableFreshErrorV1::NoAction)?;
                let receipt = pending
                    .receipt
                    .ok_or(NativeMainDurableFreshErrorV1::WrongContext)?;
                self.raw_bytes = self
                    .raw_bytes
                    .checked_add(pending.raw.len())
                    .ok_or(NativeMainDurableFreshErrorV1::Capacity)?;
                self.head = receipt.head;
                self.history.push(receipt);
                P::Idle
            }
            _ => return Err(NativeMainDurableFreshErrorV1::WrongContext),
        };
        Ok(())
    }
}
impl SourceMainNativeDurableFreshInputV1 {
    fn execute_feed(&mut self, input: NativeMainFreshRpcInputV1) -> DurableFreshResult<V> {
        let active = match self.source.inner.owner.as_mut() {
            Some(ActiveWalOwner::Active(active)) => active,
            _ => {
                return Err(NativeMainDurableFreshErrorV1::Service(
                    NativeMainActiveServiceErrorV1::NoFrame,
                ))
            }
        };
        let frame = active
            .frame
            .as_ref()
            .ok_or(NativeMainActiveServiceErrorV1::NoFrame)?;
        if let Some(code) = frame.terminal_code() {
            return Err(NativeMainActiveServiceErrorV1::AlreadyExited(code).into());
        }
        let boundary = fresh_input_boundary(active)?;
        let frame = active
            .frame
            .as_deref_mut()
            .expect("checked same source frame");
        // The identical integrated feed primitive preserves actual held plans,
        // current captured rows, queue, counters and all source jobs.
        let ordinal = self.control.feeds.len() as u64 + 1;
        let body = durable_fresh_bytes(&obj(&[
            ("ordinal", s(&ordinal.to_string())),
            ("raw", input.value()),
            ("actualBoundary", boundary.clone()),
        ]))?;
        let mut head = [0; 64];
        head[..32].copy_from_slice(&self.control.feed_head);
        head[32..].copy_from_slice(&sha256(&body));
        let head = sha256(&head);
        let outcome = obj(&[
            ("kind", s("ActualFeedPublished")),
            ("ordinal", s(&ordinal.to_string())),
            ("actualBoundary", boundary.clone()),
        ]);
        active
            .owner
            .financial
            .native_main_runtime_mut()
            .native_main_feed_fresh_rpc(frame, &input)
            .map_err(|e| NativeMainActiveServiceErrorV1::Backend(e.to_string()))?;
        self.control.feed_head = head;
        self.control.feeds.push(NativeMainFreshInputReceiptV1 {
            ordinal,
            raw: input,
            boundary,
        });
        Ok(outcome)
    }
    fn execute_once(&mut self) -> DurableFreshResult<()> {
        let action = self
            .control
            .pending
            .as_ref()
            .ok_or(NativeMainDurableFreshErrorV1::NoAction)?
            .action
            .clone();
        match action {
            NativeMainDurableFreshActionV1::Feed(input) => {
                let pending = self.control.pending.as_mut().expect("same action");
                if pending.executed {
                    return Err(NativeMainDurableFreshErrorV1::WrongContext);
                }
                pending.executed = true;
                // On any error the execution marker and the actual owner stay
                // retained. No observation/control retry may republish a feed.
                let result = self.execute_feed(input);
                let value = match result {
                    Ok(value) => value,
                    Err(error) => obj(&[
                        ("kind", s("ActualApiRejected")),
                        ("error", s(&error.to_string())),
                    ]),
                };
                self.control.pending.as_mut().expect("same action").outcome = Some(value);
                self.control.phase = NativeMainDurableFreshPhaseV1::SourceInputsCaptured;
                Ok(())
            }
            NativeMainDurableFreshActionV1::SourceAction(action) => {
                let pending = self.control.pending.as_mut().expect("same action");
                if !pending.source_prepared {
                    if pending.executed {
                        return Err(NativeMainDurableFreshErrorV1::WrongContext);
                    }
                    pending.executed = true;
                    if let Err(error) = self.source.prepare_action(action) {
                        pending.outcome = Some(obj(&[
                            ("kind", s("ActualApiRejected")),
                            ("error", s(&error.to_string())),
                        ]));
                        self.control.phase = NativeMainDurableFreshPhaseV1::SourceInputsCaptured;
                        return Ok(());
                    }
                    pending.source_prepared = true;
                }
                match self.source.advance() {
                    Ok(NativeMainActiveWalPhaseV1::Idle) => {
                        self.control.pending.as_mut().expect("same action").outcome = Some(obj(&[
                            ("kind", s("ActualSourceAcknowledged")),
                            ("sourceActions", s(&self.source.history().len().to_string())),
                            ("sourceHead", s(&hex(&self.source.head()))),
                        ]));
                        self.control.phase = NativeMainDurableFreshPhaseV1::SourceInputsCaptured;
                        Ok(())
                    }
                    Ok(_) => Err(NativeMainDurableFreshErrorV1::Busy),
                    Err(NativeMainActiveWalErrorV1::NativeRecoveryRequired) => {
                        self.control.phase = NativeMainDurableFreshPhaseV1::NativeRecoveryRequired;
                        Err(NativeMainDurableFreshErrorV1::NativeRecoveryRequired)
                    }
                    Err(error) => Err(error.into()),
                }
            }
        }
    }
    fn capture_after(&mut self) -> DurableFreshResult<()> {
        if self
            .control
            .pending
            .as_ref()
            .ok_or(NativeMainDurableFreshErrorV1::NoAction)?
            .after_binding
            .is_none()
        {
            let binding = durable_fresh_binding(&mut self.source)?;
            self.control
                .pending
                .as_mut()
                .expect("same action")
                .after_binding = Some(binding);
        }
        Ok(())
    }
    fn observe_after(&mut self) -> DurableFreshResult<()> {
        if self
            .control
            .pending
            .as_ref()
            .ok_or(NativeMainDurableFreshErrorV1::NoAction)?
            .after
            .is_none()
        {
            let after = durable_fresh_observation(
                &self.source,
                &self.control.feed_head,
                self.control.feeds.len(),
                self.control.observation_limit,
            )?;
            self.control.pending.as_mut().expect("same action").after = Some(after);
        }
        let p = self.control.pending.as_ref().expect("same action");
        if p.entry.is_some() {
            return Ok(());
        }
        let after = p.after.as_ref().expect("actual after observation");
        let binding = p.after_binding.as_ref().expect("synced actual input facts");
        let outcome = p
            .outcome
            .as_ref()
            .ok_or(NativeMainDurableFreshErrorV1::WrongContext)?
            .clone();
        let action = ActiveServiceWalBlobV1::of(&p.raw)?;
        let before = ActiveServiceWalBlobV1::of(&p.before.raw)?;
        let after = ActiveServiceWalBlobV1::of(&after.raw)?;
        let source_before = ActiveServiceWalBlobV1::of(&p.before_binding.raw)?;
        let source_after = ActiveServiceWalBlobV1::of(&binding.raw)?;
        let sequence = self.control.history.len() as u64 + 1;
        let entry = durable_fresh_bytes(&obj(&[
            ("profile", s(NATIVE_MAIN_DURABLE_FRESH_PROFILE)),
            ("sequence", s(&sequence.to_string())),
            ("previousHead", s(&hex(&self.control.head))),
            ("action", action.value()),
            ("before", before.value()),
            ("after", after.value()),
            ("sourceBefore", source_before.value()),
            ("sourceAfter", source_after.value()),
            ("outcome", outcome.clone()),
        ]))?;
        let mut head = self.control.head.to_vec();
        head.extend_from_slice(&sha256(&entry));
        let head = sha256(&head);
        let receipt = NativeMainDurableFreshReceiptV1 {
            sequence,
            action,
            before,
            after,
            source_before,
            source_after,
            entry: ActiveServiceWalBlobV1::of(&entry)?,
            outcome,
            previous_head: self.control.head,
            head,
        };
        let p = self.control.pending.as_mut().expect("same action");
        p.entry = Some(entry);
        p.receipt = Some(receipt);
        Ok(())
    }
    pub fn advance(&mut self) -> DurableFreshResult<NativeMainDurableFreshPhaseV1> {
        use NativeMainDurableFreshPhaseV1 as P;
        loop {
            match self.control.phase {
                P::Idle => return Ok(P::Idle),
                P::NativeRecoveryRequired => {
                    return Err(NativeMainDurableFreshErrorV1::NativeRecoveryRequired)
                }
                P::Intent => {
                    self.control.write_barrier("Intent")?;
                    self.control.after_barrier()?;
                }
                P::Execute => self.execute_once()?,
                P::SourceInputsCaptured => {
                    self.capture_after()?;
                    self.control.write_barrier("SourceInputsCaptured")?;
                    self.control.after_barrier()?;
                }
                P::Observe => {
                    self.observe_after()?;
                    self.control.phase = P::Accepted;
                }
                P::Accepted => {
                    self.control.write_barrier("Accepted")?;
                    self.control.after_barrier()?;
                }
                P::Ack => {
                    self.control.write_barrier("Ack")?;
                    self.control.after_barrier()?;
                }
            }
        }
    }
    pub fn resolve_control_write(&mut self) -> DurableFreshResult<ActiveServiceWalResolvedV1> {
        let plan = self
            .control
            .pending
            .as_ref()
            .and_then(|p| p.plan.as_ref())
            .ok_or(NativeMainDurableFreshErrorV1::NoAction)?;
        let resolved = self.control.wal.resolve(plan)?;
        match resolved {
            ActiveServiceWalResolvedV1::Before => {
                let plan = self.control.wal.rearm_before(plan)?;
                self.control.pending.as_mut().expect("same action").plan = Some(plan);
            }
            ActiveServiceWalResolvedV1::Post => {
                self.control.pending.as_mut().expect("same action").plan = None;
                self.control.after_barrier()?;
            }
        }
        Ok(resolved)
    }
    pub fn resolve_source_control_write(
        &mut self,
    ) -> DurableFreshResult<ActiveServiceWalResolvedV1> {
        if self.control.phase != NativeMainDurableFreshPhaseV1::Execute
            || !self
                .control
                .pending
                .as_ref()
                .is_some_and(|p| p.source_prepared)
        {
            return Err(NativeMainDurableFreshErrorV1::WrongContext);
        }
        Ok(self.source.resolve_control_write()?)
    }
}
