pub const NATIVE_COMPLETED_PUBLICATION_PROFILE: &str = "TL-MAIN-ACTUAL-COMPLETED-PUBLICATION-13-1";
struct NativeMainCommandPublicationStartV1 {
    head: NativeMainSessionHead,
    history_len: usize,
    before: NativePhysicalBinding,
    source_head: SourceContractReplayHead,
    effects: Vec<V>,
}
struct NativeMainPreparedCompletedCommandV1 {
    receipt: Option<NativeMainSessionReceipt>,
    logical: Vec<u8>,
    after_control: Option<V>,
    accepted_blob: Option<crate::source_main_native_intent_store::NativeIntentBlobV1>,
}
/// Metadata only. Created by the actual constructor before source mutation,
/// moved with every owner loan, never restored from supplied financial state.
pub struct NativeMainCompletedPublicationV1 {
    start: NativeMainCommandPublicationStartV1,
    prepared: Option<NativeMainPreparedCompletedCommandV1>,
    installed: bool,
    final_accepted: Option<NativePreparedCompletedAcceptedV1>,
    final_command_ack: Option<NativePreparedCompletedCommandAckV1>,
}
impl SourceMainNativeSessionV13 {
    fn native_main_begin_publication(
        &mut self,
        expected: &NativeMainSessionExpectation,
    ) -> Result<NativeMainCompletedPublicationV1, NativeMainSessionError> {
        if self.halted {
            return Err(NativeMainSessionError::Halted);
        }
        if expected.session != self.head {
            return Err(NativeMainSessionError::WrongHead);
        }
        let before = physical(&self.index)?;
        if before != expected.physical {
            return Err(NativeMainSessionError::WrongPhysicalBinding);
        }
        if self.head.commands != self.history.len() as u64 {
            return Err(NativeMainSessionError::WrongHead);
        }
        if self.history.len() >= MAX_COMMANDS || self.head.commands.checked_add(1).is_none() {
            return Err(NativeMainSessionError::Capacity);
        }
        self.history
            .try_reserve(1)
            .map_err(|_| NativeMainSessionError::Capacity)?;
        Ok(NativeMainCompletedPublicationV1 {
            start: NativeMainCommandPublicationStartV1 {
                head: self.head.clone(),
                history_len: self.history.len(),
                before,
                source_head: self.financial.head(),
                effects: self.financial.runtime().native_main_row_effects().to_vec(),
            },
            prepared: None,
            installed: false,
            final_accepted: None,
            final_command_ack: None,
        })
    }
}
impl SourceMainNativeRowSessionV1 {
    fn prepare_actual_completed_publication(
        &self,
    ) -> Result<NativeMainPreparedCompletedCommandV1, NativeMainSessionError> {
        let start = &self.publication.start;
        if self.publication.installed || self.owner.halted {
            return Err(NativeMainSessionError::Halted);
        }
        if self.owner.head != start.head
            || self.owner.history.len() != start.history_len
            || self.owner.financial.head() != start.source_head
        {
            return Err(NativeMainSessionError::WrongHead);
        }
        if self.owner.history.len() >= MAX_COMMANDS
            || self.owner.history.capacity() <= self.owner.history.len()
        {
            return Err(NativeMainSessionError::Capacity);
        }
        if self.source_start_error.is_some() {
            return Err(NativeMainSessionError::Halted);
        }
        let frame = self.frame.as_ref().ok_or(NativeMainSessionError::Halted)?;
        if frame.pending_write().is_some() {
            return Err(NativeMainSessionError::Halted);
        }
        let Some(Ok(returned)) = frame.result() else {
            return Err(NativeMainSessionError::Halted);
        };
        let effects = self.owner.financial.runtime().native_main_row_effects();
        if !effects.starts_with(&start.effects) {
            return Err(NativeMainSessionError::Backend(
                "actual effect prefix changed".into(),
            ));
        }
        let after = physical(&self.owner.index)?;
        let after_rows = complete(&self.owner.index)?;
        let financial_commitment = self
            .owner
            .financial
            .native_main_financial_commitment()
            .map_err(|e| NativeMainSessionError::Backend(e.to_string()))?;
        let mut receipt = NativeMainSessionReceipt {
            profile: RECEIPT_PROFILE.into(),
            sequence: start.head.commands,
            command: self.command.clone(),
            status: NativeMainSessionStatus::Completed(returned.clone()),
            grouping: frame.grouping().cloned(),
            validity: frame.validity().to_vec(),
            effects: effects.to_vec(),
            financial_commitment,
            after_rows: Some(after_rows),
            before: start.before.clone(),
            after: Some(after),
            previous_head: start.head.commitment,
            head: [0; 32],
        };
        let logical = native_main_completed_chain(&mut receipt, &start.head, self.receipt_budget)?;
        Ok(NativeMainPreparedCompletedCommandV1 {
            receipt: Some(receipt),
            logical,
            after_control: None,
            accepted_blob: None,
        })
    }
    fn finish_completed_publication(&mut self) -> NativeMainRowStopV1 {
        if let Some(Err(error)) = self.frame.as_ref().and_then(|frame| frame.result()) {
            return NativeMainRowStopV1::SourceStopped(error.clone());
        }
        let stop = self.prepare_completed_acceptance();
        if stop != NativeMainRowStopV1::AwaitNativeWrite {
            return stop;
        }
        self.resume_completed_barriers()
    }
    fn prepare_completed_publication_context(&mut self) -> Result<NativeFieldCollector, String> {
        self.observe_control(false)?;
        if self.publication.prepared.is_none() {
            let prepared = self
                .prepare_actual_completed_publication()
                .map_err(|e| format!("completed publication: {e:?}"))?;
            self.publication.prepared = Some(prepared);
        }
        let collected = self.observe_control(true)?;
        let prepared = self
            .publication
            .prepared
            .as_ref()
            .expect("actual completed preparation");
        let receipt = prepared.receipt.as_ref().expect("not yet installed");
        let after = obj(&[
            ("profile", s("TL-MAIN-ACTUAL-ROW-CONTROL-13-1")),
            ("financial", s(&hex(&receipt.financial_commitment))),
            ("controlFields", s(&collected.summary.fields.to_string())),
            ("controlBytes", s(&collected.summary.bytes.to_string())),
            ("controlCommitment", s(&hex(&collected.summary.commitment))),
            (
                "nativePublicationProfile",
                s(NATIVE_COMPLETED_PUBLICATION_PROFILE),
            ),
            ("nativeSequence", s(&receipt.sequence.to_string())),
            ("nativePreviousHead", s(&hex(&receipt.previous_head))),
            ("nativeHead", s(&hex(&receipt.head))),
            ("nativeCommands", s(&(receipt.sequence + 1).to_string())),
            ("nativeLogicalBytes", s(&prepared.logical.len().to_string())),
            ("nativeLogicalSha256", s(&hex(&sha256(&prepared.logical)))),
            ("nativeOriginalBefore", physical_value(&receipt.before)),
        ]);
        self.publication.prepared.as_mut().unwrap().after_control = Some(after.clone());
        let raw = collected
            .frames
            .as_ref()
            .expect("retained actual control fields");
        self.publication.prepared.as_mut().unwrap().accepted_blob =
            Some(crate::source_main_native_intent_store::NativeIntentBlobV1 {
                bytes: raw.len() as u64,
                sha256: sha256(raw),
            });
        Ok(collected)
    }
}
