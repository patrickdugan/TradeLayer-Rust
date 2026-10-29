// Scoped native seam. The live metadata Journal never enters cloned Core.
// The containing V13 session owns replay/admission; this is crate-private.
pub(crate) struct NativeMainRuntimeEvaluation {
    pub result: Result<DependencyReturn, SourceRuleError>,
    pub validity: Vec<crate::source_compact_index_v2::StreamedValidityEvaluation>,
    pub consumed_rows: u64,
    pub storage_stop: Option<String>,
    pub grouping: Option<crate::source_compact_index_v2::RowStreamReceipt>,
}
fn native_main_store_fault(
    error: crate::source_compact_index_v2::CompactIndexError,
    storage_stop: &mut Option<String>,
) -> Fault {
    Fault::Local(match error {
        crate::source_compact_index_v2::CompactIndexError::Source(rule) => rule,
        crate::source_compact_index_v2::CompactIndexError::Limit => SourceRuleError::Capacity,
        error => {
            *storage_stop = Some(error.to_string());
            SourceRuleError::Backend
        }
    })
}
struct NativeMainIndexBackend<'a> {
    core: &'a mut ContractCore,
    index: &'a mut crate::source_compact_index_v2::SourceCompactIndex,
}
impl SourceBackend for NativeMainIndexBackend<'_> {
    fn invoke(&mut self, call: &DependencyCall) -> Result<DependencyReturn, SourceRuleError> {
        if call.target == "db.getDatabase" {
            if call.arguments != [s("txIndex")] {
                return Err(SourceRuleError::InvalidInput);
            }
            return match self.core.database("txIndex") {
                Ok(_) => Ok(DependencyReturn::Pending(obj(&[(
                    "$database",
                    s("txIndex"),
                )]))),
                Err(Fault::Thrown(e)) => Ok(DependencyReturn::PendingThrow(e)),
                Err(Fault::Local(e)) => Err(e),
            };
        }
        if let Some(method) = call.target.strip_prefix("db.") {
            if call
                .arguments
                .first()
                .is_none_or(|v| v.get("$database") != s("txIndex"))
            {
                return Err(SourceRuleError::InvalidInput);
            }
            let args = &call.arguments[1..];
            let result = match self.core.check_failure("txIndex", method) {
                Ok(()) => {
                    self.index
                        .native_database(method, args)
                        .map_err(|error| match error {
                            crate::source_compact_index_v2::CompactIndexError::Source(rule) => rule,
                            crate::source_compact_index_v2::CompactIndexError::Limit => {
                                SourceRuleError::Capacity
                            }
                            _ => SourceRuleError::Backend,
                        })?
                }
                Err(Fault::Thrown(e)) => DependencyReturn::PendingThrow(e),
                Err(Fault::Local(e)) => return Err(e),
            };
            let mut effect = obj(&[
                ("kind", s("db")),
                ("collection", s("txIndex")),
                ("op", s(method.strip_suffix("Async").unwrap_or(method))),
                ("method", s(method)),
                ("args", V::Array(args.to_vec())),
            ]);
            match &result {
                DependencyReturn::Immediate(value) | DependencyReturn::Pending(value) => {
                    set(&mut effect, "returned", value.clone())
                }
                DependencyReturn::Throw(error) | DependencyReturn::PendingThrow(error) => set(
                    &mut effect,
                    "thrown",
                    obj(&[("name", s(&error.name)), ("message", s(&error.message))]),
                ),
            }
            .map_err(|_| SourceRuleError::InvalidInput)?;
            self.core.emit(effect);
            return Ok(result);
        }
        MainIndexBackend(self.core).invoke(call)
    }
}
impl ContractCore {
    fn native_main_index(
        &mut self,
        index: &mut crate::source_compact_index_v2::SourceCompactIndex,
        method: &str,
        args: Vec<V>,
    ) -> R<V> {
        self.native_main_index_scope(index, None, method, args)
    }
    fn native_main_index_scope(
        &mut self,
        index: &mut crate::source_compact_index_v2::SourceCompactIndex,
        instance: Option<u64>,
        method: &str,
        args: Vec<V>,
    ) -> R<V> {
        let mut owner = std::mem::take(
            &mut self
                .main_production
                .as_mut()
                .ok_or(SourceRuleError::InvalidInput)?
                .index,
        );
        let result = owner.evaluate(
            instance,
            &SourceCall {
                method: method.into(),
                arguments: args,
            },
            &mut NativeMainIndexBackend { core: self, index },
        );
        self.main_production
            .as_mut()
            .ok_or(SourceRuleError::InvalidInput)?
            .index = owner;
        let result = result?;
        for rejection in result.deferred_rejections {
            self.main_height_rejections.push(rejection);
        }
        outcome(result.outcome)
    }
}
impl SourceContractRuntime {
    pub(crate) fn native_main_state_value(&self) -> Result<V, SourceRuleError> {
        Ok(obj(&[
            ("runtime", self.state_value()?),
            (
                "borrowedQueue",
                self.core
                    .queue_context
                    .as_ref()
                    .map_or(V::Null, |queue| queue.state_value()),
            ),
        ]))
    }
    pub(crate) fn native_main_bind(&mut self, binding: V) -> Result<(), SourceRuleError> {
        binding.validate()?;
        if self.policy() != SourceContractRuntimePolicy::MainOrchestrationV12
            || self.core.main_production.is_none()
            || self.core.native_main_binding.is_some()
            || !self.collection("txIndex").is_empty()
        {
            return Err(SourceRuleError::InvalidInput);
        }
        self.core.native_main_binding = Some(binding);
        Ok(())
    }
    fn native_main_process_phase_inner(
        &mut self,
        index: &mut crate::source_compact_index_v2::SourceCompactIndex,
        cursor: &mut crate::source_compact_index_v2::MainIndexPhaseCursor,
        height: V,
        validity: &mut Vec<crate::source_compact_index_v2::StreamedValidityEvaluation>,
        consumed_rows: &mut u64,
        storage_stop: &mut Option<String>,
    ) -> R<V> {
        let mut cursor_stop = None;
        let rows = std::iter::from_fn(|| match cursor.next_row() {
            Ok(Some(row)) => {
                *consumed_rows += 1;
                Some(Ok(row))
            }
            Ok(None) => None,
            Err(error) => Some(Err(native_main_store_fault(error, &mut cursor_stop))),
        });
        let result =
            self.native_main_process_rows_with(index, rows, height, false, validity, storage_stop);
        if cursor_stop.is_some() {
            *storage_stop = cursor_stop;
        }
        result
    }
    fn native_main_process_rows_with(
        &mut self,
        index: &mut crate::source_compact_index_v2::SourceCompactIndex,
        rows: impl Iterator<Item = R<V>>,
        height: V,
        live: bool,
        validity: &mut Vec<crate::source_compact_index_v2::StreamedValidityEvaluation>,
        storage_stop: &mut Option<String>,
    ) -> R<V> {
        self.source_main_process_rows_iter(
            rows,
            height,
            live,
            &mut |core, id, kind, valid, reason| {
                core.database("txIndex")?;
                core.check_failure("txIndex", "findAsync")?;
                let before = index
                    .snapshot()
                    .map_err(|error| native_main_store_fault(error, storage_stop))?;
                let evaluated = index
                    .native_stream_upsert_validity(
                        &before,
                        id.clone(),
                        kind,
                        valid,
                        reason,
                        &mut |_arguments| match core.check_failure("txIndex", "updateAsync") {
                            Ok(()) => Ok(None),
                            Err(Fault::Thrown(error)) => {
                                Ok(Some(DependencyReturn::PendingThrow(error)))
                            }
                            Err(Fault::Local(error)) => Err(error.into()),
                        },
                    )
                    .map_err(|error| native_main_store_fault(error, storage_stop))?;
                let result = evaluated.outcome.clone();
                validity.push(evaluated);
                outcome(result).map(|_| ())
            },
        )
    }
    pub(crate) fn native_main_rows(
        &mut self,
        index: &mut crate::source_compact_index_v2::SourceCompactIndex,
        rows: V,
        height: V,
        live: bool,
    ) -> NativeMainRuntimeEvaluation {
        let mut validity = vec![];
        let mut consumed_rows = 0;
        let mut storage_stop = None;
        let result = if self.core.native_main_binding.is_none() {
            Err(SourceRuleError::Backend)
        } else if let V::Array(rows) = rows {
            if rows.len() > MAX_TABLE_ROWS {
                Err(SourceRuleError::Capacity)
            } else {
                let mut rows = Some(rows);
                self.source_main_invoke_with(
                    &DependencyCall {
                        target: if live {
                            "Main.processTx"
                        } else {
                            "Main.processTxSet"
                        }
                        .into(),
                        arguments: vec![],
                    },
                    false,
                    &mut |runtime, _| {
                        let rows = rows
                            .take()
                            .ok_or(SourceRuleError::Backend)?
                            .into_iter()
                            .map(|row| {
                                consumed_rows += 1;
                                Ok(row)
                            });
                        runtime.native_main_process_rows_with(
                            index,
                            rows,
                            height.clone(),
                            live,
                            &mut validity,
                            &mut storage_stop,
                        )
                    },
                )
            }
        } else {
            Err(SourceRuleError::InvalidInput)
        };
        NativeMainRuntimeEvaluation {
            result,
            validity,
            consumed_rows,
            storage_stop,
            grouping: None,
        }
    }
    pub(crate) fn native_main_construct(
        &mut self,
        index: &mut crate::source_compact_index_v2::SourceCompactIndex,
        start: V,
    ) -> NativeMainRuntimeEvaluation {
        let mut validity = vec![];
        let mut consumed_rows = 0u64;
        let mut storage_stop = None;
        let mut grouping = None;
        let result = if self.core.native_main_binding.is_none() {
            Err(SourceRuleError::Backend)
        } else {
            self.source_main_invoke_with(
                &DependencyCall {
                    target: "Main.constructConsensusFromIndex".into(),
                    arguments: vec![start.clone()],
                },
                false,
                &mut |runtime, _| {
                    runtime.native_main_catchup(
                        start.clone(),
                        index,
                        &mut grouping,
                        &mut validity,
                        &mut consumed_rows,
                        &mut storage_stop,
                    )
                },
            )
        };
        NativeMainRuntimeEvaluation {
            result,
            validity,
            consumed_rows,
            storage_stop,
            grouping,
        }
    }
}
include!("source_main_native_catchup_impl.rs");
