// Complete ordered read-only continuation commitment. This is not an importer.
impl NativeMainServiceContinuationV1 {
    pub(crate) fn visit_canonical_fields(
        &self,
        visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
    ) -> Result<(), SourceRuleError> {
        visitor("profile", &s(NATIVE_MAIN_ACTIVE_SERVICE_PROFILE))?;
        if self.inline_profile {
            visitor(
                "inlineProfile",
                &s(NATIVE_MAIN_INLINE_ACTIVE_SERVICE_PROFILE),
            )?;
        }
        visitor("clientBinding", &s(self.client_binding))?;
        visitor("bootstrapStart", &s(self.bootstrap_start.to_string()))?;
        visitor(
            "invocationEffectStart",
            &s(self.invocation_effect_start.to_string()),
        )?;
        visitor("consumedRows", &s(self.consumed_rows().to_string()))?;
        self.catchup.visit_canonical_fields(&mut |path, value| {
            visitor(&format!("catchup/{path}"), value)
        })?;
        self.kernel
            .visit_canonical_fields(&mut |path, value| visitor(&format!("kernel/{path}"), value))?;
        native_main_row_visit_value("documents", &self.documents, visitor)?;
        visitor("tailStarted", &V::Bool(self.tail_started))?;
        visitor(
            "syncJob",
            &self.sync_job.map_or(V::Null, |v| s(v.to_string())),
        )?;
        visitor(
            "callerResult/present",
            &V::Bool(self.caller_result.is_some()),
        )?;
        if let Some(result) = &self.caller_result {
            match result {
                Ok(returned) => native_main_row_visit_return("callerResult", returned, visitor)?,
                Err(error) => {
                    visitor("callerResult/kind", &s("Local"))?;
                    visitor("callerResult/error", &s(format!("{error:?}")))?;
                }
            }
        }
        visitor("block/present", &V::Bool(self.block.is_some()))?;
        if let Some(block) = &self.block {
            block.visit_canonical_fields(&mut |path, value| {
                visitor(&format!("block/{path}"), value)
            })?;
        }
        visitor(
            "completedBlock/present",
            &V::Bool(self.completed_block.is_some()),
        )?;
        if let Some(block) = &self.completed_block {
            visitor("completedBlock/job", &s(block.request.job.to_string()))?;
            native_main_row_visit_value("completedBlock/block", &block.request.block, visitor)?;
            native_main_row_visit_value("completedBlock/height", &block.request.height, visitor)?;
            visitor(
                "completedBlock/effectStart",
                &s(block.effect_start.to_string()),
            )?;
            match &block.result {
                Ok(value) => {
                    visitor("completedBlock/result/kind", &s("Returned"))?;
                    native_main_row_visit_value("completedBlock/result/value", value, visitor)?;
                }
                Err(Fault::Thrown(error)) => {
                    visitor("completedBlock/result/kind", &s("Thrown"))?;
                    visitor("completedBlock/result/name", &s(&error.name))?;
                    visitor("completedBlock/result/message", &s(&error.message))?;
                }
                Err(Fault::Local(error)) => {
                    visitor("completedBlock/result/kind", &s("Local"))?;
                    visitor("completedBlock/result/error", &s(format!("{error:?}")))?;
                }
            }
        }
        visitor(
            "completedValidity/count",
            &s(self.completed_validity.len().to_string()),
        )?;
        for (at, validity) in self.completed_validity.iter().enumerate() {
            native_main_catchup_visit_validity(
                &format!("completedValidity/{at}"),
                validity,
                visitor,
            )?;
        }
        visitor("liveConsumed", &s(self.live_consumed.to_string()))?;
        visitor("events/count", &s(self.events.len().to_string()))?;
        for (at, event) in self.events.iter().enumerate() {
            event.visit_canonical_fields(&mut |path, value| {
                visitor(&format!("events/{at}/{path}"), value)
            })?;
        }
        visitor(
            "stopped",
            &self
                .stopped
                .as_ref()
                .map_or(V::Null, |e| s(format!("{e:?}"))),
        )?;
        visitor("terminal", &self.terminal.map_or(V::Null, |v| n(v.into())))
    }
}
