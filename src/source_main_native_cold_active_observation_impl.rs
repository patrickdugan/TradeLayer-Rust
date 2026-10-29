impl NativeMainColdServiceStateV1 {
    pub fn visit_canonical_fields(
        &self,
        visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
    ) -> Result<(), SourceRuleError> {
        use crate::source_contract_runtime::native_main_row_visit_value as visit;
        visitor("profile", &s(self.profile()))?;
        visitor(
            "activeClientScope",
            &self
                .owner
                .financial
                .runtime()
                .native_main_service_client_scope_value(),
        )?;
        visitor("history/count", &s(&self.history.len().to_string()))?;
        visitor("previousHead/actions", &s(&self.head.actions.to_string()))?;
        visitor("previousHead/commitment", &s(&hex(&self.head.commitment)))?;
        visitor(
            "origin/commands",
            &s(&self.origin.session.commands.to_string()),
        )?;
        visitor("origin/head", &s(&hex(&self.origin.session.commitment)))?;
        visit(
            "origin/physical",
            &physical_value(&self.origin.physical),
            visitor,
        )?;
        visitor("frame/present", &V::Bool(self.frame.is_some()))?;
        if let Some(frame) = &self.frame {
            frame.visit_canonical_fields(&mut |path, value| {
                visitor(&format!("frame/{path}"), value)
            })?;
        }
        visitor(
            "sourceStartError",
            &self
                .source_start_error
                .as_ref()
                .map_or(V::Null, |e| s(&format!("{e:?}"))),
        )?;
        visitor(
            "attemptedWrite/present",
            &V::Bool(self.attempted_write.is_some()),
        )?;
        if let Some(write) = &self.attempted_write {
            visit("attemptedWrite", write, visitor)?;
        }
        visitor(
            "pendingObservation/present",
            &V::Bool(self.pending_observation.is_some()),
        )?;
        if let Some(pending) = &self.pending_observation {
            pending.action.visit_fields(&mut |path, value| {
                visitor(&format!("pendingObservation/action/{path}"), value)
            })?;
            pending.result.visit_fields(&mut |path, value| {
                visitor(&format!("pendingObservation/result/{path}"), value)
            })?;
            visit(
                "pendingObservation/before",
                &physical_value(&pending.before),
                visitor,
            )?;
        }
        visitor("observationLimit", &s(&self.observation_limit.to_string()))?;
        visitor("effects/count", &s(&self.effects().len().to_string()))?;
        for (at, effect) in self.effects().iter().enumerate() {
            visit(&format!("effects/{at}"), effect, visitor)?;
        }
        Ok(())
    }
    fn publish(
        &mut self,
        rows: RowStreamReceipt,
        after: NativePhysicalBinding,
    ) -> Result<&NativeMainActiveServiceReceiptV1, NativeMainActiveServiceErrorV1> {
        if self.pending_observation.is_none() {
            return Err(NativeMainActiveServiceErrorV1::NoObservationPending);
        }
        let financial = self
            .owner
            .financial
            .native_main_financial_commitment()
            .map_err(|e| NativeMainActiveServiceErrorV1::Backend(e.to_string()))?;
        let mut fields = NativeFieldCollector::new(
            "Native13/ActiveServiceObservation/v1",
            true,
            self.observation_limit,
        );
        self.visit_canonical_fields(&mut |path, value| fields.emit(path, value))
            .map_err(|e| NativeMainActiveServiceErrorV1::Backend(e.to_string()))?;
        fields
            .emit("financial", &s(&hex(&financial)))
            .map_err(|e| NativeMainActiveServiceErrorV1::Backend(e.to_string()))?;
        native_step_value_fields("afterRows", &logical_read(&rows), &mut fields)
            .map_err(|e| NativeMainActiveServiceErrorV1::Backend(e.to_string()))?;
        native_step_value_fields("afterPhysical", &physical_value(&after), &mut fields)
            .map_err(|e| NativeMainActiveServiceErrorV1::Backend(e.to_string()))?;
        let sequence = self
            .head
            .actions
            .checked_add(1)
            .ok_or(NativeMainActiveServiceErrorV1::Capacity)?;
        let mut bytes = NATIVE_ACTIVE_SERVICE_SESSION_PROFILE.as_bytes().to_vec();
        bytes.extend_from_slice(&self.head.commitment);
        bytes.extend_from_slice(&sequence.to_be_bytes());
        native_step_field_summary(&mut bytes, &fields.summary);
        let head = sha256(&bytes);
        let pending = self
            .pending_observation
            .take()
            .expect("checked actual pending observation");
        self.history.push(NativeMainActiveServiceReceiptV1 {
            sequence,
            action: pending.action,
            result: pending.result,
            fields: fields.summary,
            financial_commitment: financial,
            rows,
            before: pending.before,
            after,
            previous_head: self.head.commitment,
            head,
        });
        self.head = NativeMainActiveServiceHeadV1 {
            actions: sequence,
            commitment: head,
        };
        Ok(self.history.last().expect("published observation"))
    }
}
