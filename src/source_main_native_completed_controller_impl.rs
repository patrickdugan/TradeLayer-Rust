// Final control barriers operate only on the retained, completed source frame.
// Sealed Before/Post pins are captured before IO; no source callback is retried.
impl SourceMainNativeRowSessionV1 {
    pub fn retained_completed_acceptance(&self) -> Option<&NativePreparedCompletedAcceptedV1> {
        self.publication.final_accepted.as_ref()
    }
    pub fn retained_completed_acknowledgement(
        &self,
    ) -> Option<&NativePreparedCompletedCommandAckV1> {
        self.publication.final_command_ack.as_ref()
    }
    pub fn prepare_completed_acceptance(&mut self) -> NativeMainRowStopV1 {
        if !matches!(self.pc, NativeMainRowControllerPcV1::Source)
            || self.publication.final_accepted.is_some()
        {
            return NativeMainRowStopV1::StorageStopped(
                "completed source at original acceptance boundary required".into(),
            );
        }
        let collected = match self.prepare_completed_publication_context() {
            Ok(collected) => collected,
            Err(error) => return self.block(error),
        };
        let prepared = self
            .publication
            .prepared
            .as_ref()
            .expect("actual completed receipt");
        let after = prepared
            .after_control
            .as_ref()
            .expect("sealed publication context");
        let operation = match self.store.prepare_completed_accepted(
            &self.permit,
            collected
                .frames
                .as_ref()
                .expect("retained actual control fields"),
            after,
        ) {
            Ok(operation) => operation,
            Err(error) => return NativeMainRowStopV1::StorageStopped(error.to_string()),
        };
        if Some(operation.accepted_blob()) != prepared.accepted_blob.as_ref()
            || operation.after_context() != after
        {
            return self.block("sealed acceptance differs from actual publication".into());
        }
        self.publication.final_accepted = Some(operation);
        self.pc = NativeMainRowControllerPcV1::FinalAcceptedPrepared;
        NativeMainRowStopV1::AwaitNativeWrite
    }
    pub fn sync_prepared_completed_acceptance(&mut self) -> NativeMainRowStopV1 {
        if !matches!(
            self.pc,
            NativeMainRowControllerPcV1::FinalAcceptedPrepared
                | NativeMainRowControllerPcV1::FinalAcceptedAttempted
        ) {
            return NativeMainRowStopV1::StorageStopped(
                "retained final acceptance required".into(),
            );
        }
        let operation = self
            .publication
            .final_accepted
            .as_ref()
            .expect("retained acceptance");
        if matches!(self.pc, NativeMainRowControllerPcV1::FinalAcceptedAttempted)
            && self.store.pin().ok().as_ref() != Some(operation.independently_expected_before())
        {
            return NativeMainRowStopV1::StorageStopped(
                "final acceptance ambiguity requires exact synced-Post reopen".into(),
            );
        }
        self.pc = NativeMainRowControllerPcV1::FinalAcceptedAttempted;
        if let Err(error) = self
            .store
            .sync_prepared_completed_accepted(&mut self.permit, operation)
        {
            return NativeMainRowStopV1::StorageStopped(error.to_string());
        }
        self.pc = NativeMainRowControllerPcV1::FinalAcceptedSynced;
        NativeMainRowStopV1::AwaitNativeWrite
    }
    pub fn prepare_completed_acknowledgement(&mut self) -> NativeMainRowStopV1 {
        if !matches!(self.pc, NativeMainRowControllerPcV1::FinalAcceptedSynced)
            || self.publication.final_command_ack.is_some()
        {
            return NativeMainRowStopV1::StorageStopped(
                "verified final acceptance required".into(),
            );
        }
        let operation = match self.store.prepare_completed_command_ack(&self.permit) {
            Ok(operation) => operation,
            Err(error) => return NativeMainRowStopV1::StorageStopped(error.to_string()),
        };
        let prepared = self
            .publication
            .prepared
            .as_ref()
            .expect("actual completed receipt");
        if Some(operation.accepted_blob()) != prepared.accepted_blob.as_ref()
            || Some(operation.after_context()) != prepared.after_control.as_ref()
        {
            return self
                .block("sealed command acknowledgement differs from actual publication".into());
        }
        self.publication.final_command_ack = Some(operation);
        self.pc = NativeMainRowControllerPcV1::FinalCommandAckPrepared;
        NativeMainRowStopV1::AwaitNativeWrite
    }
    pub fn sync_prepared_completed_acknowledgement(&mut self) -> NativeMainRowStopV1 {
        if !matches!(
            self.pc,
            NativeMainRowControllerPcV1::FinalCommandAckPrepared
                | NativeMainRowControllerPcV1::FinalCommandAckAttempted
        ) {
            return NativeMainRowStopV1::StorageStopped(
                "retained final command acknowledgement required".into(),
            );
        }
        let operation = self
            .publication
            .final_command_ack
            .as_ref()
            .expect("retained command acknowledgement");
        if matches!(
            self.pc,
            NativeMainRowControllerPcV1::FinalCommandAckAttempted
        ) && self.store.pin().ok().as_ref() != Some(operation.independently_expected_before())
        {
            return NativeMainRowStopV1::StorageStopped(
                "final command acknowledgement ambiguity requires exact synced-Post reopen".into(),
            );
        }
        self.pc = NativeMainRowControllerPcV1::FinalCommandAckAttempted;
        if let Err(error) = self
            .store
            .sync_prepared_completed_command_ack(&mut self.permit, operation)
        {
            return NativeMainRowStopV1::StorageStopped(error.to_string());
        }
        self.pc = NativeMainRowControllerPcV1::FinalCommandAckSynced;
        NativeMainRowStopV1::AwaitNativeWrite
    }
    fn resume_completed_barriers(&mut self) -> NativeMainRowStopV1 {
        if matches!(
            self.pc,
            NativeMainRowControllerPcV1::FinalAcceptedPrepared
                | NativeMainRowControllerPcV1::FinalAcceptedAttempted
        ) {
            let stop = self.sync_prepared_completed_acceptance();
            if stop != NativeMainRowStopV1::AwaitNativeWrite {
                return stop;
            }
        }
        if matches!(self.pc, NativeMainRowControllerPcV1::FinalAcceptedSynced) {
            let stop = self.prepare_completed_acknowledgement();
            if stop != NativeMainRowStopV1::AwaitNativeWrite {
                return stop;
            }
        }
        if matches!(
            self.pc,
            NativeMainRowControllerPcV1::FinalCommandAckPrepared
                | NativeMainRowControllerPcV1::FinalCommandAckAttempted
        ) {
            let stop = self.sync_prepared_completed_acknowledgement();
            if stop != NativeMainRowStopV1::AwaitNativeWrite {
                return stop;
            }
        }
        if !matches!(self.pc, NativeMainRowControllerPcV1::FinalCommandAckSynced) {
            return NativeMainRowStopV1::StorageStopped(
                "verified final command acknowledgement required".into(),
            );
        }
        self.pc = NativeMainRowControllerPcV1::Accepted;
        if let Err(error) = self.install_committed_completed_publication() {
            return self.block(format!("{error:?}"));
        }
        self.pc = NativeMainRowControllerPcV1::Complete;
        NativeMainRowStopV1::Complete
    }
}
fn native_main_completed_barrier_fields(
    publication: &NativeMainCompletedPublicationV1,
    visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
) -> Result<(), SourceRuleError> {
    // Preserve original accepted-frame bytes when both operations are absent.
    // Once sealed, emit their retained facts without recapturing an acceptance.
    let accepted = publication.final_accepted.as_ref();
    let ack = publication.final_command_ack.as_ref();
    if accepted.is_none() && ack.is_none() {
        return Ok(());
    }
    for (name, profile, before, after, command, prefix, previous, blob, context) in [
        accepted.map(|op| {
            (
                "completedAccepted",
                op.profile(),
                op.independently_expected_before(),
                op.independently_expected_after(),
                op.command(),
                op.native_prefix(),
                op.previous_ack(),
                op.accepted_blob(),
                op.after_context(),
            )
        }),
        ack.map(|op| {
            (
                "completedCommandAck",
                op.profile(),
                op.independently_expected_before(),
                op.independently_expected_after(),
                op.command(),
                op.native_prefix(),
                op.previous_ack(),
                op.accepted_blob(),
                op.after_context(),
            )
        }),
    ]
    .into_iter()
    .flatten()
    {
        visitor(&format!("publication/{name}/profile"), &s(profile))?;
        native_main_supersession_store_pin(&format!("publication/{name}/before"), before, visitor)?;
        native_main_supersession_store_pin(&format!("publication/{name}/after"), after, visitor)?;
        for (label, value) in [
            ("command", Some(command)),
            ("nativePrefix", prefix),
            ("accepted", Some(blob)),
        ] {
            visitor(
                &format!("publication/{name}/{label}/present"),
                &V::Bool(value.is_some()),
            )?;
            if let Some(value) = value {
                visitor(
                    &format!("publication/{name}/{label}/bytes"),
                    &s(&value.bytes.to_string()),
                )?;
                visitor(
                    &format!("publication/{name}/{label}/sha256"),
                    &s(&hex(&value.sha256)),
                )?;
            }
        }
        crate::source_contract_runtime::native_main_row_visit_value(
            &format!("publication/{name}/previousAck"),
            previous,
            visitor,
        )?;
        crate::source_contract_runtime::native_main_row_visit_value(
            &format!("publication/{name}/afterContext"),
            context,
            visitor,
        )?;
    }
    if let Some(ack) = ack {
        let blob = ack.acknowledgement();
        visitor(
            "publication/completedCommandAck/ack/bytes",
            &s(&blob.bytes.to_string()),
        )?;
        visitor(
            "publication/completedCommandAck/ack/sha256",
            &s(&hex(&blob.sha256)),
        )?;
    }
    Ok(())
}
