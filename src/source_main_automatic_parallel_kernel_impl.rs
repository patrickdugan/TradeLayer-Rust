// Explicit automatic-profile scheduler metadata. The actual financial block
// remains in its existing Runtime frame; this is only its exact parked job.
impl MainStepKernelV1 {
    pub(crate) fn service_park_automatic_file_tail(
        &mut self,
        request: &MainServiceBlockRequest,
    ) -> Result<(), MainStepError> {
        if !self.native_file_blocks
            || self.native_file_tail_parked.is_some()
            || self.service_next_file_tail().as_ref() != Some(request)
        {
            return Err(MainStepError::InvalidInput);
        }
        assert_eq!(self.ready.pop_front(), Some(request.job));
        self.native_file_tail_parked = Some(request.clone());
        Ok(())
    }
    pub(crate) fn service_unpark_automatic_file_tail(
        &mut self,
        request: &MainServiceBlockRequest,
    ) -> Result<(), MainStepError> {
        if !self.native_file_blocks
            || self.owner_stopped.is_some()
            || self.terminal.is_some()
            || self.native_file_tail_parked.as_ref() != Some(request)
            || self.ready.contains(&request.job)
        {
            return Err(MainStepError::InvalidInput);
        }
        let job = self
            .jobs
            .iter()
            .find(|job| job.id == request.job)
            .ok_or(MainStepError::InvalidInput)?;
        if job.status != MainStepJobStatus::Ready {
            return Err(MainStepError::InvalidInput);
        }
        self.native_file_tail_parked = None;
        self.ready.push_front(request.job);
        Ok(())
    }
    fn visit_automatic_file_parked(
        &self,
        emit: &mut impl FnMut(&str, &V) -> FieldResult,
    ) -> FieldResult {
        if !self.native_file_blocks {
            return Ok(());
        }
        emit(
            "automaticFile/nativeTailParked/present",
            &V::Bool(self.native_file_tail_parked.is_some()),
        )?;
        if let Some(request) = &self.native_file_tail_parked {
            field_uint("automaticFile/nativeTailParked/job", request.job, emit)?;
            field_value("automaticFile/nativeTailParked/block", &request.block, emit)?;
            field_value(
                "automaticFile/nativeTailParked/height",
                &request.height,
                emit,
            )?;
        }
        Ok(())
    }
}
