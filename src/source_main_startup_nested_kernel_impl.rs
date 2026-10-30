// A real blocked source job is parked only while a separate original await
// continuation is runnable. This metadata token cannot execute financial work.
pub(crate) struct MainStartupParkedBlockV1 {
    generation: u64,
    request: MainServiceBlockRequest,
}
impl MainStartupParkedBlockV1 {
    pub(crate) fn value(&self) -> V {
        obj(&[
            ("generation", s(self.generation.to_string())),
            ("job", s(self.request.job.to_string())),
            ("block", self.request.block.clone()),
            ("height", self.request.height.clone()),
        ])
    }
}
impl MainStepKernelV1 {
    pub(crate) fn startup_job_is_detached_incoming(&self, id: u64) -> bool {
        self.jobs.iter().any(|job| {
            job.id == id
                && !job.caller_owned
                && job.parent.is_none()
                && matches!(job.frame, Frame::Incoming { .. })
        })
    }
    pub(crate) fn startup_park_block(
        &mut self,
        request: &MainServiceBlockRequest,
    ) -> Result<MainStartupParkedBlockV1, MainStepError> {
        if self.native_file_blocks
            || self.file_retries
            || self.service_next_block().as_ref() != Some(request)
        {
            return Err(MainStepError::InvalidInput);
        }
        assert_eq!(self.ready.pop_front(), Some(request.job));
        Ok(MainStartupParkedBlockV1 {
            generation: self.generation,
            request: request.clone(),
        })
    }
    pub(crate) fn startup_unpark_block(
        &mut self,
        token: &MainStartupParkedBlockV1,
    ) -> Result<(), MainStepError> {
        if self.generation != token.generation || self.ready.contains(&token.request.job) {
            return Err(MainStepError::InvalidInput);
        }
        let job = self
            .jobs
            .iter()
            .find(|job| job.id == token.request.job)
            .ok_or(MainStepError::InvalidInput)?;
        let Frame::Incoming {
            phase: 7,
            block,
            number,
            ..
        } = &job.frame
        else {
            return Err(MainStepError::InvalidInput);
        };
        if job.status != MainStepJobStatus::Ready
            || block != &token.request.block
            || number != &token.request.height
        {
            return Err(MainStepError::InvalidInput);
        }
        // The fulfilled source await queues its original continuation after
        // already-ready jobs, rather than rerunning or inventing a new job.
        self.ready.push_back(token.request.job);
        Ok(())
    }
}
