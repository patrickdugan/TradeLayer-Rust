// The original source row stays at its await until a new actual intent syncs.
struct NativeMainPreparedRowSupersessionV1 {
    replacement: Option<Box<crate::source_compact_index_v2::NativePreparedValidityWriteV1>>,
    mapping: crate::source_compact_index_v2::NativePreparedValiditySupersessionV1,
    intent: NativePreparedSupersedingIntentV1,
    installed: bool,
}
impl NativeMainOwnedFrameV1 {
    fn install_supersession(
        &mut self,
        runtime: &mut SourceContractRuntime,
        replacement: Box<crate::source_compact_index_v2::NativePreparedValidityWriteV1>,
        mapping: &crate::source_compact_index_v2::NativePreparedValiditySupersessionV1,
    ) -> Result<
        (),
        (
            SourceRuleError,
            Box<crate::source_compact_index_v2::NativePreparedValidityWriteV1>,
        ),
    > {
        match self {
            Self::Rows(frame) => {
                runtime.native_main_row_install_supersession(frame, replacement, mapping)
            }
            Self::Catchup(frame) => {
                runtime.native_main_catchup_install_supersession(frame, replacement, mapping)
            }
        }
    }
}
impl SourceMainNativeRowSessionV1 {
    pub fn retained_superseding_intent(&self) -> Option<&NativePreparedSupersedingIntentV1> {
        Some(&self.supersession.as_ref()?.intent)
    }
    pub fn prepare_superseding(&mut self) -> NativeMainRowStopV1 {
        if !matches!(self.pc, NativeMainRowControllerPcV1::RecoveryBefore) {
            return NativeMainRowStopV1::StorageStopped(
                "verified Before/Intermediate reopen required".into(),
            );
        }
        let Some(original) = self.pending_write() else {
            return NativeMainRowStopV1::SourceStopped(SourceRuleError::InvalidInput);
        };
        let Some(observed) = self.reopened.as_ref() else {
            return NativeMainRowStopV1::SourceStopped(SourceRuleError::InvalidInput);
        };
        let (replacement, mapping) = match self
            .owner
            .index
            .supersede_prepared_validity(original, observed)
        {
            Ok(actual) => actual,
            Err(error) => return NativeMainRowStopV1::StorageStopped(error.to_string()),
        };
        let intent = match self.store.prepare_superseding_native_intent(
            &self.permit,
            original,
            &self.owner.index,
            observed,
            &replacement,
            &mapping,
        ) {
            Ok(intent) => intent,
            Err(error) => return NativeMainRowStopV1::StorageStopped(error.to_string()),
        };
        self.supersession = Some(NativeMainPreparedRowSupersessionV1 {
            replacement: Some(Box::new(replacement)),
            mapping,
            intent,
            installed: false,
        });
        self.pc = NativeMainRowControllerPcV1::SupersessionPrepared;
        NativeMainRowStopV1::AwaitNativeWrite
    }
    pub fn sync_prepared_superseding(&mut self) -> NativeMainRowStopV1 {
        if !matches!(
            self.pc,
            NativeMainRowControllerPcV1::SupersessionPrepared
                | NativeMainRowControllerPcV1::SupersessionStoreAttempted
        ) {
            return NativeMainRowStopV1::StorageStopped(
                "prepared superseding intent required; no blind sync retry".into(),
            );
        }
        if matches!(
            self.pc,
            NativeMainRowControllerPcV1::SupersessionStoreAttempted
        ) && self.store.pin().ok().as_ref()
            != Some(
                self.supersession
                    .as_ref()
                    .expect("retained intent")
                    .intent
                    .independently_expected_before(),
            )
        {
            return NativeMainRowStopV1::StorageStopped(
                "ambiguous control sync requires exact synced-Post reopen".into(),
            );
        }
        self.pc = NativeMainRowControllerPcV1::SupersessionStoreAttempted;
        let prepared = self.supersession.as_ref().expect("retained before sync");
        if let Err(error) = self
            .store
            .sync_prepared_superseding_native_intent(&mut self.permit, &prepared.intent)
        {
            return NativeMainRowStopV1::StorageStopped(error.to_string());
        }
        self.native_main_install_synced_supersession()
    }
    fn native_main_install_synced_supersession(&mut self) -> NativeMainRowStopV1 {
        let prepared = self
            .supersession
            .as_mut()
            .expect("retained actual replacement");
        let replacement = prepared
            .replacement
            .take()
            .expect("not installed before durable sync");
        let installed = self
            .frame
            .as_mut()
            .expect("actual source frame")
            .install_supersession(
                self.owner.financial.native_main_runtime_mut(),
                replacement,
                &prepared.mapping,
            );
        if let Err((error, replacement)) = installed {
            prepared.replacement = Some(replacement);
            return self.block(error.to_string());
        }
        prepared.installed = true;
        self.superseding_active = true;
        self.pc = NativeMainRowControllerPcV1::NativeIntent;
        NativeMainRowStopV1::AwaitNativeWrite
    }
}
fn native_main_supersession_store_pin(
    path: &str,
    pin: &NativeIntentStorePinV1,
    visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
) -> Result<(), SourceRuleError> {
    visitor(
        &format!("{path}/root"),
        &s(&pin.canonical_root.to_string_lossy()),
    )?;
    visitor(&format!("{path}/context"), &s(&hex(&pin.context)))?;
    visitor(&format!("{path}/sequence"), &s(&pin.sequence.to_string()))?;
    visitor(&format!("{path}/state"), &s(&hex(&pin.state_sha256)))
}
fn native_main_supersession_append_pin(
    path: &str,
    pin: &crate::compact_journal::NativeAppendPin,
    visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
) -> Result<(), SourceRuleError> {
    visitor(
        &format!("{path}/sequence"),
        &s(&pin.logical_sequence.to_string()),
    )?;
    let p = &pin.publication;
    visitor(&format!("{path}/generation"), &s(&p.generation.to_string()))?;
    visitor(
        &format!("{path}/parent"),
        &p.parent.map_or(V::Null, |n| s(&n.to_string())),
    )?;
    visitor(
        &format!("{path}/physicalCreation"),
        &s(&p.physical_creation_sequence.to_string()),
    )?;
    visitor(
        &format!("{path}/logicalCreation"),
        &s(&p.creation_sequence.to_string()),
    )?;
    visitor(&format!("{path}/manifest"), &s(&hex(&p.manifest_sha256d)))
}
fn native_main_supersession_fields(
    prepared: Option<&NativeMainPreparedRowSupersessionV1>,
    active: bool,
    visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
) -> Result<(), SourceRuleError> {
    visitor("supersession/active", &V::Bool(active))?;
    visitor("supersession/prepared", &V::Bool(prepared.is_some()))?;
    if let Some(prepared) = prepared {
        visitor("supersession/installed", &V::Bool(prepared.installed))?;
        visitor("supersession/profile", &s(prepared.intent.profile()))?;
        native_main_supersession_store_pin(
            "supersession/controlBefore",
            prepared.intent.independently_expected_before(),
            visitor,
        )?;
        native_main_supersession_store_pin(
            "supersession/controlAfter",
            prepared.intent.independently_expected_after(),
            visitor,
        )?;
        let original = prepared.intent.original_native_intent();
        visitor(
            "supersession/originalIntent/bytes",
            &s(&original.bytes.to_string()),
        )?;
        visitor(
            "supersession/originalIntent/sha256",
            &s(&hex(&original.sha256)),
        )?;
        let candidates = prepared.intent.replacement_candidates();
        crate::source_contract_runtime::native_main_row_visit_value(
            "supersession/candidates",
            candidates.independently_retained_facts(),
            visitor,
        )?;
        visitor(
            "supersession/mutations/bytes",
            &s(&candidates.exact_prepared_mutations().len().to_string()),
        )?;
        visitor(
            "supersession/mutations/sha256",
            &s(&hex(&sha256(candidates.exact_prepared_mutations()))),
        )?;
        for (name, pin) in [
            ("originalBefore", prepared.mapping.original_before()),
            ("originalAfter", prepared.mapping.original_after()),
            ("replacementBefore", prepared.mapping.replacement_before()),
            ("replacementAfter", prepared.mapping.replacement_after()),
        ] {
            native_main_supersession_append_pin(&format!("supersession/{name}"), pin, visitor)?;
        }
        visitor(
            "supersession/reopened/profile",
            &s(&prepared.mapping.reopened().profile),
        )?;
        visitor(
            "supersession/reopened/context",
            &s(&prepared.mapping.reopened().context),
        )?;
        visitor(
            "supersession/reopened/sequence",
            &s(&prepared.mapping.reopened().sequence.to_string()),
        )?;
        if let Some(write) = &prepared.replacement {
            crate::source_contract_runtime::native_main_row_visit_value(
                "supersession/uninstalledWrite",
                &write.durable_read_context(),
                visitor,
            )?;
        }
    }
    Ok(())
}
