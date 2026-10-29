impl SourceMainNativeRowSessionV1 {
    fn install_committed_completed_publication(&mut self) -> Result<(), NativeMainSessionError> {
        #[cfg(test)]
        native_main_publication_guard_test_hook(self);
        if !matches!(self.pc, NativeMainRowControllerPcV1::Accepted) || self.publication.installed {
            return Err(NativeMainSessionError::Halted);
        }
        let start = &self.publication.start;
        let prepared = self
            .publication
            .prepared
            .as_ref()
            .ok_or(NativeMainSessionError::Halted)?;
        let receipt = prepared
            .receipt
            .as_ref()
            .ok_or(NativeMainSessionError::Halted)?;
        let after = prepared
            .after_control
            .as_ref()
            .ok_or(NativeMainSessionError::Halted)?;
        let accepted = prepared
            .accepted_blob
            .as_ref()
            .ok_or(NativeMainSessionError::Halted)?;
        self.store
            .require_actual_completed_command_ack(&self.permit, after, accepted)
            .map_err(|e| NativeMainSessionError::Backend(e.to_string()))?;
        if self.owner.head != start.head
            || self.owner.history.len() != start.history_len
            || self.owner.financial.head() != start.source_head
        {
            return Err(NativeMainSessionError::WrongHead);
        }
        if self.owner.halted
            || self.owner.history.len() >= MAX_COMMANDS
            || self.owner.history.capacity() <= self.owner.history.len()
        {
            return Err(NativeMainSessionError::Capacity);
        }
        if receipt.command != self.command
            || receipt.before != start.before
            || receipt.after.as_ref() != Some(&physical(&self.owner.index)?)
            || receipt.after_rows.as_ref() != Some(&complete(&self.owner.index)?)
            || receipt.financial_commitment
                != self
                    .owner
                    .financial
                    .native_main_financial_commitment()
                    .map_err(|e| NativeMainSessionError::Backend(e.to_string()))?
            || receipt.effects.as_slice()
                != self.owner.financial.runtime().native_main_row_effects()
            || logical_receipt_bytes(receipt)? != prepared.logical
        {
            return Err(NativeMainSessionError::Backend(
                "completed preparation no longer matches same actual owner".into(),
            ));
        }
        let frame = self.frame.as_ref().ok_or(NativeMainSessionError::Halted)?;
        let Some(Ok(returned)) = frame.result() else {
            return Err(NativeMainSessionError::Halted);
        };
        if receipt.status != NativeMainSessionStatus::Completed(returned.clone())
            || frame.pending_write().is_some()
            || receipt.grouping.as_ref() != frame.grouping()
            || receipt.validity.as_slice() != frame.validity()
        {
            return Err(NativeMainSessionError::Halted);
        }
        let mut chain = start.head.commitment.to_vec();
        chain.extend_from_slice(&prepared.logical);
        if receipt.sequence != start.head.commands
            || receipt.previous_head != start.head.commitment
            || sha256(&chain) != receipt.head
        {
            return Err(NativeMainSessionError::WrongHead);
        }
        let next = start
            .head
            .commands
            .checked_add(1)
            .ok_or(NativeMainSessionError::Capacity)?;
        // All fallible preconditions are complete; the same-owner drain and
        // reserved push have no source callback or financial runtime clone.
        let mut actual = self
            .publication
            .prepared
            .as_mut()
            .unwrap()
            .receipt
            .take()
            .unwrap();
        actual.effects = self
            .owner
            .financial
            .native_main_runtime_mut()
            .take_effects();
        self.owner.head = NativeMainSessionHead {
            commands: next,
            commitment: actual.head,
        };
        self.owner.history.push(actual);
        self.publication.installed = true;
        Ok(())
    }
    pub fn completed_native_receipt(&self) -> Option<&NativeMainSessionReceipt> {
        if self.publication.installed && matches!(self.pc, NativeMainRowControllerPcV1::Complete) {
            self.owner.history.last()
        } else {
            None
        }
    }
    pub fn completed_native_head(&self) -> Option<NativeMainSessionHead> {
        self.completed_native_receipt().map(|_| self.owner.head())
    }
    pub fn into_completed_owners(
        self,
    ) -> Result<
        (SourceMainNativeSessionV13, NativeIntentStoreV1),
        Box<RejectedNativeMainCompletedReleaseV1>,
    > {
        let checked = (|| {
            let receipt = self
                .completed_native_receipt()
                .ok_or(NativeMainSessionError::Halted)?;
            let prepared = self
                .publication
                .prepared
                .as_ref()
                .ok_or(NativeMainSessionError::Halted)?;
            let after = prepared
                .after_control
                .as_ref()
                .ok_or(NativeMainSessionError::Halted)?;
            let accepted = prepared
                .accepted_blob
                .as_ref()
                .ok_or(NativeMainSessionError::Halted)?;
            self.store
                .require_actual_completed_command_ack(&self.permit, after, accepted)
                .map_err(|e| NativeMainSessionError::Backend(e.to_string()))?;
            if self.owner.head.commands != self.publication.start.head.commands + 1
                || self.owner.head.commitment != receipt.head
                || self.owner.history.len() != self.publication.start.history_len + 1
                || prepared.receipt.is_some()
                || self.owner.halted
                || !self
                    .owner
                    .financial
                    .runtime()
                    .native_main_row_effects()
                    .is_empty()
                || receipt.after.as_ref() != Some(&physical(&self.owner.index)?)
                || logical_receipt_bytes(receipt)? != prepared.logical
                || receipt.financial_commitment
                    != self
                        .owner
                        .financial
                        .native_main_financial_commitment()
                        .map_err(|e| NativeMainSessionError::Backend(e.to_string()))?
            {
                return Err(NativeMainSessionError::WrongHead);
            }
            Ok(())
        })();
        if let Err(error) = checked {
            return Err(Box::new(RejectedNativeMainCompletedReleaseV1 {
                error,
                session: self,
            }));
        }
        let Self { owner, store, .. } = self;
        Ok((owner, store))
    }
}
pub struct RejectedNativeMainCompletedReleaseV1 {
    pub error: NativeMainSessionError,
    session: SourceMainNativeRowSessionV1,
}
impl RejectedNativeMainCompletedReleaseV1 {
    pub fn into_session(self) -> SourceMainNativeRowSessionV1 {
        self.session
    }
}
impl core::fmt::Debug for RejectedNativeMainCompletedReleaseV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RejectedNativeMainCompletedReleaseV1")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}
