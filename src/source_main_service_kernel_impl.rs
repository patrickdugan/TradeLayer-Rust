// Explicit running-service control seam. No new legacy Kernel fields, frame
// tags, bytes or input readers are introduced. Only the same actual loop runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MainServiceBlockRequest {
    pub job: u64,
    pub block: V,
    pub height: V,
}
impl MainStepKernelV1 {
    pub(crate) fn service_enqueue(
        &mut self,
        owner: &mut impl MainStepOwner,
        command: MainStepCommandV1,
    ) -> Result<MainStepEvaluationV1, MainStepError> {
        self.apply_with_drain_bound(owner, command, Some(0))
    }
    pub(crate) fn service_drain_one(
        &mut self,
        owner: &mut impl MainStepOwner,
    ) -> Result<MainStepEvaluationV1, MainStepError> {
        self.apply_with_drain_bound(owner, MainStepCommandV1::DrainReady { limit: 1 }, Some(1))
    }
    pub(crate) fn service_next_block(&self) -> Option<MainServiceBlockRequest> {
        if self.native_file_blocks {
            return self.service_next_file_tail();
        }
        if self.file_retries || self.owner_stopped.is_some() || self.terminal.is_some() {
            return None;
        }
        let id = *self.ready.front()?;
        let job = self.jobs.iter().find(|job| job.id == id)?;
        if job.status != MainStepJobStatus::Ready {
            return None;
        }
        let Frame::Incoming {
            block,
            number,
            phase: 7,
            ..
        } = &job.frame
        else {
            return None;
        };
        Some(MainServiceBlockRequest {
            job: id,
            block: block.clone(),
            height: number.clone(),
        })
    }
    pub(crate) fn service_has_ready(&self) -> bool {
        !self.ready.is_empty() || !self.pending_unhandled.is_empty()
    }
}
