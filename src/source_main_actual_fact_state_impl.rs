// Same actual raw action/body dispatcher, new independent fact adapter only.
impl NativeMainColdServiceStateV1 {
    fn actual_cold_fact_selection(&self) -> Option<V> {
        let pending = self
            .frame
            .as_ref()?
            .actual_historical_inline_fact_descriptor()?;
        Some(obj(&[
            ("profile", s("TL-MAIN-ACTUAL-SINGLE-INLINE-SELECTION-13-1")),
            ("sourceProfile", s(self.profile())),
            (
                "originCommands",
                s(&self.origin.session.commands.to_string()),
            ),
            ("originHead", s(&hex(&self.origin.session.commitment))),
            ("originPhysical", physical_value(&self.origin.physical)),
            ("serviceActions", s(&self.head.actions.to_string())),
            ("serviceHead", s(&hex(&self.head.commitment))),
            ("pending", pending),
        ]))
    }
    // The writer captures these independently produced selectors BEFORE the
    // action receipt is published. Compare the actual same source frame/head
    // at that exact boundary; never use a selector to construct source state.
    fn verify_actual_fact_selection(
        &self,
        transcript: &crate::source_main_active_service_wal::VerifiedActualSourceFactTraceV1,
        sequence: u64,
        after: bool,
    ) -> Result<(), String> {
        let input = transcript
            .source_pending_input(sequence, after)
            .map_err(|e| e.to_string())?;
        if self.actual_cold_fact_selection().as_ref() != input.as_ref().map(|v| v.selection()) {
            return Err("actual source pending selector mismatch".into());
        }
        Ok(())
    }
    fn actual_fact_observation(
        &self,
        pin: &SourceIndexHistoryPinV1,
        rows: RowStreamReceipt,
    ) -> Result<Vec<u8>, String> {
        let mut raw = self.observation(pin, rows)?;
        SourceMainNativeDurableActiveServiceV1::append_actual_facts_scope_observation(&mut raw)
            .map_err(|e| e.to_string())?;
        Ok(raw)
    }
    fn execute_actual_facts(
        &mut self,
        history: &mut NativeHistoricalValidityPlaybackV1<'_>,
        capability: &NativeColdIndexExecutionV1,
        action: NativeMainActiveServiceActionV1,
        mutations: Option<&[u8]>,
        facts: (
            &crate::source_main_active_service_wal::VerifiedActualSourceFactTraceV1,
            u64,
            &mut Option<String>,
        ),
    ) -> Result<(), NativeMainActiveServiceErrorV1> {
        let (transcript, sequence, verification) = facts;
        let before = cold_prefix_physical(history.source_history_pin());
        if let NativeMainActiveServiceActionV1::Start {
            start,
            generation,
            documents,
        } = &action
        {
            if self.started {
                return Err(NativeMainActiveServiceErrorV1::InvalidInput);
            }
            start
                .validate()
                .map_err(|_| NativeMainActiveServiceErrorV1::InvalidInput)?;
            self.owner
                .financial
                .runtime()
                .native_main_service_preflight(documents)
                .map_err(|e| NativeMainActiveServiceErrorV1::Backend(e.to_string()))?;
            self.history
                .try_reserve(1)
                .map_err(|_| NativeMainActiveServiceErrorV1::Capacity)?;
            let mut seed = self.profile().as_bytes().to_vec();
            seed.extend_from_slice(&self.origin.session.commands.to_be_bytes());
            seed.extend_from_slice(&self.origin.session.commitment);
            self.head = NativeMainActiveServiceHeadV1 {
                actions: 0,
                commitment: sha256(&seed),
            };
            let begun = self
                .owner
                .financial
                .native_main_runtime_mut()
                .native_main_service_begin_mode4(
                    history,
                    start.clone(),
                    *generation,
                    documents.clone(),
                );
            let result = match begun {
                Ok(frame) => {
                    self.frame = Some(Box::new(frame));
                    NativeMainActiveServiceActionResultV1::Started(Ok(()))
                }
                Err(error) => {
                    self.source_start_error = Some(error.clone());
                    NativeMainActiveServiceActionResultV1::Started(Err(error))
                }
            };
            self.started = true;
            self.pending_observation = Some(NativeMainActiveServicePendingV1 {
                action,
                result,
                before,
            });
        } else {
            if !self.started {
                return Err(NativeMainActiveServiceErrorV1::NoFrame);
            }
            if self.history.len() >= MAX_SERVICE_ACTIONS {
                return Err(NativeMainActiveServiceErrorV1::Capacity);
            }
            self.history
                .try_reserve(1)
                .map_err(|_| NativeMainActiveServiceErrorV1::Capacity)?;
            let frame = self
                .frame
                .as_ref()
                .ok_or(NativeMainActiveServiceErrorV1::NoFrame)?;
            if let Some(code) = frame.terminal_code() {
                return Err(NativeMainActiveServiceErrorV1::AlreadyExited(code));
            }
            match &action {
                NativeMainActiveServiceActionV1::Advance { limit }
                    if *limit == 0 || *limit > 4096 =>
                {
                    return Err(NativeMainActiveServiceErrorV1::InvalidInput)
                }
                NativeMainActiveServiceActionV1::Shutdown(args)
                    if args.len() > 8 || args.iter().any(|v| v.validate().is_err()) =>
                {
                    return Err(NativeMainActiveServiceErrorV1::InvalidInput)
                }
                NativeMainActiveServiceActionV1::AppendPending => {
                    let context = cold_service_pending_context(frame)
                        .ok_or(NativeMainActiveServiceErrorV1::NoAttemptedWrite)?;
                    if self.attempted_write.as_ref() == Some(&context) {
                        return Err(NativeMainActiveServiceErrorV1::WriteAlreadyAttempted);
                    }
                }
                NativeMainActiveServiceActionV1::ResolvePost => {
                    let context = cold_service_pending_context(frame)
                        .ok_or(NativeMainActiveServiceErrorV1::NoAttemptedWrite)?;
                    if self.attempted_write.as_ref() != Some(&context) {
                        return Err(NativeMainActiveServiceErrorV1::NoAttemptedWrite);
                    }
                }
                _ => (),
            }
            let actual_selection = self.actual_cold_fact_selection();
            let frame = self.frame.as_mut().expect("same actual cold source frame");
            let runtime = self.owner.financial.native_main_runtime_mut();
            use crate::source_main_step_kernel::MainStepCommandV1 as Command;
            let result = match &action {
                NativeMainActiveServiceActionV1::Advance { limit } => {
                    NativeMainActiveServiceActionResultV1::Stop(
                        runtime.native_main_service_advance_actual_facts(
                            history,
                            capability,
                            transcript,
                            sequence,
                            frame,
                            (*limit, &mut *verification),
                        ),
                    )
                }
                NativeMainActiveServiceActionV1::Pause => {
                    NativeMainActiveServiceActionResultV1::Pause(
                        runtime.native_main_service_pause(frame),
                    )
                }
                NativeMainActiveServiceActionV1::AppendPending => {
                    self.attempted_write = cold_service_pending_context(frame);
                    NativeMainActiveServiceActionResultV1::Stop(
                        runtime.native_main_service_append_actual_facts(
                            history,
                            capability,
                            transcript,
                            sequence,
                            frame,
                            (
                                mutations.unwrap_or(&[]),
                                actual_selection,
                                &mut *verification,
                            ),
                        ),
                    )
                }
                NativeMainActiveServiceActionV1::ResolvePost => {
                    NativeMainActiveServiceActionResultV1::Stop(
                        runtime.native_main_service_resolve_actual_facts(
                            history,
                            capability,
                            transcript,
                            sequence,
                            frame,
                            verification,
                        ),
                    )
                }
                NativeMainActiveServiceActionV1::FireTimer { id, generation } => {
                    NativeMainActiveServiceActionResultV1::Control(
                        runtime.native_main_service_control_historical(
                            history,
                            capability,
                            frame,
                            Command::FireTimer {
                                id: *id,
                                generation: *generation,
                            },
                        ),
                    )
                }
                NativeMainActiveServiceActionV1::ClearTimer { id, generation } => {
                    NativeMainActiveServiceActionResultV1::Control(
                        runtime.native_main_service_control_historical(
                            history,
                            capability,
                            frame,
                            Command::ClearTimer {
                                id: *id,
                                generation: *generation,
                            },
                        ),
                    )
                }
                NativeMainActiveServiceActionV1::Shutdown(args) => {
                    NativeMainActiveServiceActionResultV1::Control(
                        runtime.native_main_service_control_historical(
                            history,
                            capability,
                            frame,
                            Command::Shutdown(args.clone()),
                        ),
                    )
                }
                NativeMainActiveServiceActionV1::Start { .. } => {
                    unreachable!("handled constructor raw action")
                }
            };
            if matches!(action, NativeMainActiveServiceActionV1::AppendPending) {
                self.attempted_write = cold_service_pending_context(frame);
            }
            if cold_service_pending_context(frame).as_ref() != self.attempted_write.as_ref() {
                self.attempted_write = None;
            }
            self.pending_observation = Some(NativeMainActiveServicePendingV1 {
                action,
                result,
                before,
            });
        }
        if verification.is_none() {
            if let Err(error) = self.verify_actual_fact_selection(transcript, sequence, true) {
                *verification = Some(error);
            }
        }
        if verification.is_some() {
            return Err(NativeMainActiveServiceErrorV1::Backend(
                "actual fact verification failed; actor must abort".into(),
            ));
        }
        let post = history.source_history_pin();
        self.publish(
            history
                .source_actual_rows()
                .map_err(|e| NativeMainActiveServiceErrorV1::Backend(e.to_string()))?,
            cold_prefix_physical(post),
        )
        .map(|_| ())
    }
}
