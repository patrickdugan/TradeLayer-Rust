// The same service Kernel owns Incoming, its genuine FileStepFrame child and
// every retry timer. These private peeks cannot supply a source block outcome.
pub const AUTOMATIC_NATIVE_FILE_KERNEL_PROFILE: &str = "TL-MAIN-AUTOMATIC-NATIVE-FILE-KERNEL-13-1";
impl MainStepKernelV1 {
    pub(crate) fn enable_automatic_native_files(&mut self) -> Result<(), MainStepError> {
        if self.file_retries
            || self.native_file_blocks
            || self.next_job != 1
            || self.next_timer != 1
            || !self.jobs.is_empty()
            || !self.timers.is_empty()
            || !self.ready.is_empty()
            || self.owner_stopped.is_some()
            || self.terminal.is_some()
        {
            return Err(MainStepError::InvalidInput);
        }
        self.file_retries = true;
        self.native_file_blocks = true;
        Ok(())
    }
    fn automatic_file_block_at(&self, phase: u8) -> Option<MainServiceBlockRequest> {
        if !self.native_file_blocks || self.owner_stopped.is_some() || self.terminal.is_some() {
            return None;
        }
        let id = *self.ready.front()?;
        let job = self.jobs.iter().find(|j| j.id == id)?;
        if job.status != MainStepJobStatus::Ready {
            return None;
        }
        let Frame::File(file) = &job.frame else {
            return None;
        };
        let FileStepFrame::Block {
            block,
            height,
            phase: actual,
            ..
        } = file.as_ref()
        else {
            return None;
        };
        if *actual != phase {
            return None;
        }
        // Block phase4 has not consumed its checkpoint child yet. A thrown
        // child must propagate through the real FileStepFrame before ANY
        // financial/native tail is admitted.
        if phase == 4 && !matches!(job.child_result, Some(MainStepSourceResult::Fulfilled(_))) {
            return None;
        }
        let parent = self.jobs.iter().find(|p| Some(p.id) == job.parent)?;
        if parent.status != MainStepJobStatus::WaitingChild(id) {
            return None;
        }
        let Frame::Incoming {
            block: parent_block,
            number,
            phase: 12,
            ..
        } = &parent.frame
        else {
            return None;
        };
        if parent_block != block || number != height {
            return None;
        }
        Some(MainServiceBlockRequest {
            job: id,
            block: block.clone(),
            height: height.clone(),
        })
    }
    pub(crate) fn service_next_file_prefix(&self) -> Option<MainServiceBlockRequest> {
        self.automatic_file_block_at(0)
    }
    fn service_next_file_tail(&self) -> Option<MainServiceBlockRequest> {
        self.automatic_file_block_at(4)
    }
}
