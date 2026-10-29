// Borrow the SAME actual source continuation and native IO fields. Existing
// live-block Frame layout/visitor/signatures stay unchanged for Mode4.
struct NativeMainInlineIoBorrowV1<'a> {
    scope: u64,
    source: &'a mut crate::txindex_source_owner::TxIndexInlineContinuationV1,
    write: &'a mut Option<Box<crate::source_compact_index_v2::NativePreparedInlineDatabaseV1>>,
    attempted: &'a mut bool,
    storage_stop: &'a mut Option<String>,
    local_stop: &'a mut Option<SourceRuleError>,
}
#[derive(Default)]
struct NativeMainRetainedInlineIoV1 {
    write: Option<Box<crate::source_compact_index_v2::NativePreparedInlineDatabaseV1>>,
    attempted: bool,
    storage_stop: Option<String>,
    local_stop: Option<SourceRuleError>,
}
impl NativeMainRetainedInlineIoV1 {
    fn borrow<'a>(
        &'a mut self,
        scope: u64,
        source: &'a mut crate::txindex_source_owner::TxIndexInlineContinuationV1,
    ) -> NativeMainInlineIoBorrowV1<'a> {
        NativeMainInlineIoBorrowV1 {
            scope,
            source,
            write: &mut self.write,
            attempted: &mut self.attempted,
            storage_stop: &mut self.storage_stop,
            local_stop: &mut self.local_stop,
        }
    }
    fn visit_fields(
        &self,
        visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
    ) -> Result<(), SourceRuleError> {
        visitor("attempted", &V::Bool(self.attempted))?;
        visitor(
            "storageStop",
            &self.storage_stop.as_ref().map_or(V::Null, s),
        )?;
        visitor(
            "localStop",
            &self
                .local_stop
                .as_ref()
                .map_or(V::Null, |e| s(format!("{e:?}"))),
        )?;
        visitor("write/present", &V::Bool(self.write.is_some()))?;
        if let Some(write) = &self.write {
            let mut result = Ok(());
            write
                .visit_canonical_fields(&mut |p, v| {
                    if result.is_ok() {
                        result = native_main_row_visit_value(&format!("write/{p}"), v, visitor)
                    }
                })
                .map_err(|_| SourceRuleError::Backend)?;
            result?;
        }
        Ok(())
    }
}

impl ContractCore {
    fn native_main_inline_source_advance(
        &mut self,
        index: &mut SourceCompactIndex,
        frame: NativeMainInlineIoBorrowV1<'_>,
        documents: &V,
    ) -> NativeMainRowStopV1 {
        use crate::txindex_source_owner::TxIndexInlineStopV1 as Stop;
        if let Some(error) = &frame.storage_stop {
            return NativeMainRowStopV1::StorageStopped(error.clone());
        }
        if let Some(error) = &frame.local_stop {
            return NativeMainRowStopV1::SourceStopped(error.clone());
        }
        if frame.write.is_some() {
            return NativeMainRowStopV1::AwaitNativeWrite;
        }
        let Some(production) = self.main_production.as_mut() else {
            return NativeMainRowStopV1::SourceStopped(SourceRuleError::InvalidInput);
        };
        let mut owner = std::mem::take(&mut production.index);
        let result = (|| loop {
            match owner.advance_inline_source(
                frame.scope,
                &mut *frame.source,
                &mut NativeMainServiceIndexBackend {
                    base: NativeMainIndexBackend { core: self, index },
                    documents,
                },
            )? {
                Stop::Complete => {
                    return Ok(NativeMainRowStopV1::Complete);
                }
                Stop::AwaitDatabase => {
                    if self.effects_exhausted {
                        return Err(SourceRuleError::Capacity);
                    }
                    let call = frame
                        .source
                        .pending_call()
                        .ok_or(SourceRuleError::InvalidInput)?
                        .clone();
                    let method = call
                        .target
                        .strip_prefix("db.")
                        .ok_or(SourceRuleError::InvalidInput)?;
                    match self.check_failure("txIndex", method) {
                        Err(Fault::Thrown(error)) => {
                            let returned = DependencyReturn::PendingThrow(error);
                            native_main_inline_emit(self, &call, &returned);
                            owner.resolve_inline_source(
                                frame.scope,
                                &mut *frame.source,
                                returned,
                            )?;
                        }
                        Err(Fault::Local(error)) => return Err(error),
                        Ok(()) => {
                            let prepared = index.snapshot().and_then(|before| {
                                index.prepare_inline_database(
                                    &before,
                                    method,
                                    call.arguments[1..].to_vec(),
                                )
                            });
                            match prepared {
                                Ok(plan) => {
                                    *frame.write = Some(Box::new(plan));
                                    return Ok(NativeMainRowStopV1::AwaitNativeWrite);
                                }
                                Err(error) => {
                                    let error = error.to_string();
                                    *frame.storage_stop = Some(error.clone());
                                    return Ok(NativeMainRowStopV1::StorageStopped(error));
                                }
                            }
                        }
                    }
                }
            }
        })();
        self.main_production
            .as_mut()
            .expect("same actual production owner")
            .index = owner;
        match result {
            Ok(stop) => stop,
            Err(error) => {
                *frame.local_stop = Some(error.clone());
                NativeMainRowStopV1::SourceStopped(error)
            }
        }
    }
    fn native_main_inline_source_append(
        &mut self,
        index: &mut SourceCompactIndex,
        frame: NativeMainInlineIoBorrowV1<'_>,
        permit: Option<&crate::source_compact_index_v2::SourceIndexGenerationEscrowPermitV1>,
    ) -> NativeMainRowStopV1 {
        if let Some(error) = &frame.storage_stop {
            return NativeMainRowStopV1::StorageStopped(error.clone());
        }
        if *frame.attempted {
            return NativeMainRowStopV1::SourceStopped(SourceRuleError::InvalidInput);
        }
        let Some(write) = frame.write.as_deref() else {
            return NativeMainRowStopV1::SourceStopped(SourceRuleError::InvalidInput);
        };
        *frame.attempted = true; // Actual attempt is retained even when return is lost.
        match index.append_inline_database(write, permit) {
            Ok(()) => NativeMainRowStopV1::AwaitNativeWrite,
            Err(error) => {
                let error = error.to_string();
                *frame.storage_stop = Some(error.clone());
                NativeMainRowStopV1::StorageStopped(error)
            }
        }
    }
    fn native_main_inline_source_resolve(
        &mut self,
        index: &SourceCompactIndex,
        frame: NativeMainInlineIoBorrowV1<'_>,
    ) -> NativeMainRowStopV1 {
        if !*frame.attempted {
            return NativeMainRowStopV1::SourceStopped(SourceRuleError::InvalidInput);
        }
        let Some(write) = frame.write.as_deref() else {
            return NativeMainRowStopV1::SourceStopped(SourceRuleError::InvalidInput);
        };
        let returned = match index.resolve_inline_database_post(write) {
            Ok(v) => v,
            Err(e) => return NativeMainRowStopV1::StorageStopped(e.to_string()),
        };
        let Some(production) = self.main_production.as_ref() else {
            return NativeMainRowStopV1::SourceStopped(SourceRuleError::InvalidInput);
        };
        let Some(call) = frame.source.pending_call().cloned() else {
            return NativeMainRowStopV1::SourceStopped(SourceRuleError::InvalidInput);
        };
        if let Err(e) = production.index.resolve_inline_source(
            frame.scope,
            &mut *frame.source,
            returned.clone(),
        ) {
            return NativeMainRowStopV1::SourceStopped(e);
        }
        native_main_inline_emit(self, &call, &returned);
        *frame.write = None;
        *frame.attempted = false;
        *frame.storage_stop = None;
        NativeMainRowStopV1::Complete
    }
}
