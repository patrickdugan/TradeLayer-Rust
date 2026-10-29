// Explicit inline-await component. The existing Core owns its actual TxIndex
// receiver throughout. This frame holds source locals and one real native plan,
// never a financial Runtime clone, source result importer or retryable action.
struct NativeMainInlineIndexFrameV1 {
    scope: u64,
    source: crate::txindex_source_owner::TxIndexInlineContinuationV1,
    write: Option<Box<crate::source_compact_index_v2::NativePreparedInlineDatabaseV1>>,
    attempted: bool,
    storage_stop: Option<String>,
    local_stop: Option<SourceRuleError>,
    result: Option<R<V>>,
}
impl NativeMainInlineIndexFrameV1 {
    fn pending_write(
        &self,
    ) -> Option<&crate::source_compact_index_v2::NativePreparedInlineDatabaseV1> {
        self.write.as_deref()
    }
    fn visit_fields(
        &self,
        visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
    ) -> Result<(), SourceRuleError> {
        visitor("profile", &s("TL-MAIN-RETAINED-INLINE-INDEX-13-1"))?;
        let mut error = Ok(());
        self.source.visit_fields(&mut |path, value| {
            if error.is_ok() {
                error = native_main_row_visit_value(&format!("source/{path}"), value, visitor);
            }
        });
        error?;
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
            let mut error = Ok(());
            write
                .visit_canonical_fields(&mut |path, value| {
                    if error.is_ok() {
                        error =
                            native_main_row_visit_value(&format!("write/{path}"), value, visitor);
                    }
                })
                .map_err(|_| SourceRuleError::Backend)?;
            error?;
        }
        visitor("result/present", &V::Bool(self.result.is_some()))?;
        if let Some(result) = &self.result {
            match result {
                Ok(v) => native_main_row_visit_value("result/returned", v, visitor)?,
                Err(Fault::Thrown(e)) => {
                    visitor("result/name", &s(&e.name))?;
                    visitor("result/message", &s(&e.message))?;
                }
                Err(Fault::Local(e)) => visitor("result/local", &s(format!("{e:?}")))?,
            }
        }
        Ok(())
    }
}
fn native_main_inline_emit(
    core: &mut ContractCore,
    call: &DependencyCall,
    returned: &DependencyReturn,
) {
    let method = call
        .target
        .strip_prefix("db.")
        .expect("real source DB await");
    let mut effect = obj(&[
        ("kind", s("db")),
        ("collection", s("txIndex")),
        ("op", s(method.strip_suffix("Async").unwrap_or(method))),
        ("method", s(method)),
        ("args", V::Array(call.arguments[1..].to_vec())),
    ]);
    match returned {
        DependencyReturn::Immediate(v) | DependencyReturn::Pending(v) => {
            set(&mut effect, "returned", v.clone())
        }
        DependencyReturn::Throw(e) | DependencyReturn::PendingThrow(e) => set(
            &mut effect,
            "thrown",
            obj(&[("name", s(&e.name)), ("message", s(&e.message))]),
        ),
    }
    .expect("fresh fixed effect object");
    core.emit(effect);
}
impl ContractCore {
    fn native_main_inline_index_begin(
        &self,
        scope: u64,
        method: crate::txindex_source_owner::TxIndexInlineMethodV1,
        arguments: Vec<V>,
    ) -> Result<NativeMainInlineIndexFrameV1, SourceRuleError> {
        if self.native_main_binding.is_none() {
            return Err(SourceRuleError::InvalidInput);
        }
        let owner = &self
            .main_production
            .as_ref()
            .ok_or(SourceRuleError::InvalidInput)?
            .index;
        Ok(NativeMainInlineIndexFrameV1 {
            scope,
            source: owner.begin_inline_source(scope, method, arguments)?,
            write: None,
            attempted: false,
            storage_stop: None,
            local_stop: None,
            result: None,
        })
    }
    fn native_main_inline_index_advance(
        &mut self,
        index: &mut SourceCompactIndex,
        frame: &mut NativeMainInlineIndexFrameV1,
        documents: &V,
    ) -> NativeMainRowStopV1 {
        if let Some(error) = &frame.storage_stop {
            return NativeMainRowStopV1::StorageStopped(error.clone());
        }
        if let Some(error) = &frame.local_stop {
            return NativeMainRowStopV1::SourceStopped(error.clone());
        }
        if frame.result.is_some() {
            return NativeMainRowStopV1::Complete;
        }
        let stop = self.native_main_inline_source_advance(
            index,
            NativeMainInlineIoBorrowV1 {
                scope: frame.scope,
                source: &mut frame.source,
                write: &mut frame.write,
                attempted: &mut frame.attempted,
                storage_stop: &mut frame.storage_stop,
                local_stop: &mut frame.local_stop,
            },
            documents,
        );
        if stop == NativeMainRowStopV1::Complete {
            match frame.source.take_evaluation() {
                Ok(evaluation) => {
                    self.main_height_rejections
                        .extend(evaluation.deferred_rejections);
                    frame.result = Some(outcome(evaluation.outcome));
                }
                Err(error) => {
                    frame.local_stop = Some(error.clone());
                    return NativeMainRowStopV1::SourceStopped(error);
                }
            }
        }
        stop
    }
    fn native_main_inline_index_append(
        &mut self,
        index: &mut SourceCompactIndex,
        frame: &mut NativeMainInlineIndexFrameV1,
        permit: Option<&crate::source_compact_index_v2::SourceIndexGenerationEscrowPermitV1>,
    ) -> NativeMainRowStopV1 {
        self.native_main_inline_source_append(
            index,
            NativeMainInlineIoBorrowV1 {
                scope: frame.scope,
                source: &mut frame.source,
                write: &mut frame.write,
                attempted: &mut frame.attempted,
                storage_stop: &mut frame.storage_stop,
                local_stop: &mut frame.local_stop,
            },
            permit,
        )
    }
    fn native_main_inline_index_resolve(
        &mut self,
        index: &SourceCompactIndex,
        frame: &mut NativeMainInlineIndexFrameV1,
    ) -> NativeMainRowStopV1 {
        self.native_main_inline_source_resolve(
            index,
            NativeMainInlineIoBorrowV1 {
                scope: frame.scope,
                source: &mut frame.source,
                write: &mut frame.write,
                attempted: &mut frame.attempted,
                storage_stop: &mut frame.storage_stop,
                local_stop: &mut frame.local_stop,
            },
        )
    }
}
