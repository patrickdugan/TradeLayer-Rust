// Exact V9 Main/raw22: validateTransfer deliberately does not await
// Channels.isValidChannel before awaiting Channels.getChannel. Both factories
// and both reads belong to the same actual Channels registry, not a verdict.
struct SourceTransferRead {
    call: SourceCall,
    factory: DependencyReturn,
    trace: usize,
}
struct SourceTransferValidityBackend<'a> {
    core: &'a mut ContractCore,
    pending: Option<SourceTransferRead>,
}
// The original load body is reused after its real factory/find suspension.
// These returns were captured by this call's actual database operations.
struct SourceTransferReadBackend {
    factory: Option<DependencyReturn>,
    rows: Option<DependencyReturn>,
}
impl ContractCore {
    // Logic.transfer awaits the channel save, but does not await its two Tally
    // updates. Each awaited channel-row write has already advanced these jobs.
    // The channel-save return adds one ready turn, rather than a fixed phase.
    fn source_transfer_balance_return(&mut self) -> R<()> {
        self.source_channel_balance_first_write()?;
        self.source_channel_balance_tick()?;
        Ok(())
    }
    fn source_transfer_balance_after_await(&mut self, collection: &str) -> R<()> {
        if !self.spot_lifecycle_enabled()
            || !self.source_spot_main_active
            || self.channel_balance_jobs.is_empty()
            || !matches!(collection, "consensus" | "txIndex")
        {
            return Ok(());
        }
        // These are the caller's real awaited processed/index operations. The
        // same jobs still own every actual Tally read, write and captured alias.
        self.source_channel_balance_tick()?;
        if self.channel_balance_jobs.iter().all(|job| job.phase == 6) {
            self.source_channel_balance_drain()?;
        }
        Ok(())
    }
    fn source_transfer_balance_finish(&mut self) -> R<()> {
        if self.spot_lifecycle_enabled() && !self.channel_balance_jobs.is_empty() {
            self.source_channel_balance_drain()?;
        }
        Ok(())
    }
}
impl SourceBackend for SourceTransferReadBackend {
    fn invoke(&mut self, call: &DependencyCall) -> Result<DependencyReturn, SourceRuleError> {
        match call.target.as_str() {
            "DB.getDatabase" if call.arguments == vec![s("channels")] => {
                self.factory.take().ok_or(SourceRuleError::Backend)
            }
            "DB.findAsync" if call.arguments == vec![s("channels"), obj(&[])] => {
                self.rows.take().ok_or(SourceRuleError::Backend)
            }
            _ => Err(SourceRuleError::UnknownDependency),
        }
    }
}
impl SourceTransferValidityBackend<'_> {
    fn new(core: &mut ContractCore) -> SourceTransferValidityBackend<'_> {
        SourceTransferValidityBackend {
            core,
            pending: None,
        }
    }
    fn begin(&mut self, call: &DependencyCall) -> Result<SourceTransferRead, SourceRuleError> {
        let trace = self.core.effects.len();
        self.core.emit(obj(&[
            ("kind", s("dependency")),
            ("target", s(&call.target)),
            ("args", V::Array(call.arguments.clone())),
        ]));
        let factory = returned(self.core.database("channels"))?;
        Ok(SourceTransferRead {
            call: SourceCall {
                method: call
                    .target
                    .strip_prefix("Channels.")
                    .ok_or(SourceRuleError::Backend)?
                    .into(),
                arguments: call.arguments.clone(),
            },
            factory,
            trace,
        })
    }
    fn read(
        &mut self,
        job: &SourceTransferRead,
    ) -> Result<Option<DependencyReturn>, SourceRuleError> {
        if matches!(
            job.factory,
            DependencyReturn::Throw(_) | DependencyReturn::PendingThrow(_)
        ) {
            Ok(None)
        } else {
            returned(self.core.db("channels", "findAsync", vec![obj(&[])])).map(Some)
        }
    }
    fn resume(
        &mut self,
        job: SourceTransferRead,
        rows: Option<DependencyReturn>,
    ) -> Result<SourceOutcome, SourceRuleError> {
        let outcome = self
            .core
            .channels
            .resume_channel_read_unverified(
                &job.call,
                &mut SourceTransferReadBackend {
                    factory: Some(job.factory),
                    rows,
                },
            )?
            .outcome;
        if job.trace < self.core.effects.len() {
            match &outcome {
                SourceOutcome::Returned(value) => {
                    set(&mut self.core.effects[job.trace], "returned", value.clone()).map_err(
                        |e| match e {
                            Fault::Local(e) => e,
                            Fault::Thrown(_) => SourceRuleError::Backend,
                        },
                    )?
                }
                SourceOutcome::Thrown(error) => set(
                    &mut self.core.effects[job.trace],
                    "thrown",
                    obj(&[("name", s(&error.name)), ("message", s(&error.message))]),
                )
                .map_err(|e| match e {
                    Fault::Local(e) => e,
                    Fault::Thrown(_) => SourceRuleError::Backend,
                })?,
            }
        }
        Ok(outcome)
    }
    fn finish(&mut self) -> Result<(), SourceRuleError> {
        if let Some(job) = self.pending.take() {
            let rows = self.read(&job)?;
            if let SourceOutcome::Thrown(error) = self.resume(job, rows)? {
                self.core
                    .source_supply_rejection(error)
                    .map_err(|e| match e {
                        Fault::Local(e) => e,
                        Fault::Thrown(_) => SourceRuleError::Backend,
                    })?;
            }
        }
        Ok(())
    }
}
impl SourceBackend for SourceTransferValidityBackend<'_> {
    fn invoke(&mut self, call: &DependencyCall) -> Result<DependencyReturn, SourceRuleError> {
        if call.target == "Channels.isValidChannel"
            && !truth(
                &self
                    .core
                    .channels
                    .cached_channel_read(&arg(&call.arguments, 0)),
            )
        {
            if self.pending.is_some() {
                return Err(SourceRuleError::Backend);
            }
            self.pending = Some(self.begin(call)?);
            return Ok(DependencyReturn::Pending(V::Undefined));
        }
        if call.target == "Channels.getChannel" {
            if let Some(first) = self.pending.take() {
                // getChannel looks up its cache before either suspended load
                // can commit. Start its second load now, even if load1 finds it.
                let second = if !truth(
                    &self
                        .core
                        .channels
                        .cached_channel_read(&arg(&call.arguments, 0)),
                ) {
                    Some(self.begin(call)?)
                } else {
                    None
                };
                let first_rows = self.read(&first)?;
                let second_rows = second
                    .as_ref()
                    .map(|job| self.read(job))
                    .transpose()?
                    .flatten();
                if let SourceOutcome::Thrown(error) = self.resume(first, first_rows)? {
                    self.core
                        .source_supply_rejection(error)
                        .map_err(|e| match e {
                            Fault::Local(e) => e,
                            Fault::Thrown(_) => SourceRuleError::Backend,
                        })?;
                }
                if let Some(second) = second {
                    return returned_source(self.resume(second, second_rows)?);
                }
            }
        }
        self.core.invoke(call)
    }
}
