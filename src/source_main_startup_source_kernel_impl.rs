// The Promise domain is owned by this SAME Kernel, never a parallel financial
// session. Only a new typed controller can select it before source jobs start.
impl MainStepKernelV1 {
    pub(crate) fn startup_install_source_promises(&mut self) -> Result<(), MainStepError> {
        if self.startup_source_promises.is_some()
            || self.startup_source_nedb_jobs.is_some()
            || self.file_retries
            || self.native_file_blocks
            || self.native_file_tail_parked.is_some()
            || self.owner_stopped.is_some()
            || self.terminal.is_some()
            || !self.jobs.is_empty()
            || !self.timers.is_empty()
            || !self.ready.is_empty()
            || !self.pending_unhandled.is_empty()
        {
            return Err(MainStepError::InvalidInput);
        }
        let queue = startup_source_promise_queue::StartupPromiseQueue::new(self.generation)
            .map_err(|_| MainStepError::Capacity)?;
        let nedb = startup_source_nedb_jobs::StartupNedbQueue::new(self.generation)
            .map_err(|_| MainStepError::Capacity)?;
        self.startup_source_promises = Some(queue);
        self.startup_source_nedb_jobs = Some(nedb);
        Ok(())
    }
    pub(crate) fn startup_source_parts(
        &mut self,
    ) -> Result<
        (
            &mut startup_source_promise_queue::StartupPromiseQueue,
            &mut startup_source_nedb_jobs::StartupNedbQueue,
        ),
        MainStepError,
    > {
        if self.terminal.is_some() {
            return Err(MainStepError::AlreadyExited);
        }
        if self.owner_stopped.is_some() {
            return Err(MainStepError::OwnerStopped);
        }
        match (
            &mut self.startup_source_promises,
            &mut self.startup_source_nedb_jobs,
        ) {
            (Some(promises), Some(nedb)) => Ok((promises, nedb)),
            _ => Err(MainStepError::InvalidInput),
        }
    }
    pub(crate) fn startup_source_promises(
        &mut self,
    ) -> Result<&mut startup_source_promise_queue::StartupPromiseQueue, MainStepError> {
        if self.terminal.is_some() {
            return Err(MainStepError::AlreadyExited);
        }
        if self.owner_stopped.is_some() {
            return Err(MainStepError::OwnerStopped);
        }
        self.startup_source_promises
            .as_mut()
            .ok_or(MainStepError::InvalidInput)
    }
    fn startup_visit_source_promises(
        &self,
        visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
    ) -> Result<(), SourceRuleError> {
        // No absent/default discriminator is appended under old profiles.
        if let Some(queue) = &self.startup_source_promises {
            queue.visit_fields(&mut |path, value| {
                visitor(&format!("startupPromises/{path}"), value)
            })?;
        }
        if let Some(queue) = &self.startup_source_nedb_jobs {
            queue
                .visit_fields(&mut |path, value| visitor(&format!("startupNedb/{path}"), value))?;
        }
        Ok(())
    }
}
