// Actual row source execution on one existing financial owner. Metadata awaits
// are checked against actual cold historical views; no past batch is appended.
impl SourceContractRuntime {
    pub(crate) fn native_main_historical_rows(
        &mut self,
        history: &mut crate::source_compact_index_v2::NativeHistoricalValidityPlaybackV1<'_>,
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
                        runtime.source_main_process_rows_iter(
                            rows,
                            height.clone(),
                            live,
                            &mut |core, id, kind, valid, reason| {
                                core.database("txIndex")?;
                                core.check_failure("txIndex", "findAsync")?;
                                let evaluated = history
                                    .verify_source_update(
                                        id.clone(),
                                        kind,
                                        valid,
                                        reason,
                                        &mut |_arguments| match core
                                            .check_failure("txIndex", "updateAsync")
                                        {
                                            Ok(()) => Ok(None),
                                            Err(Fault::Thrown(error)) => {
                                                Ok(Some(DependencyReturn::PendingThrow(error)))
                                            }
                                            Err(Fault::Local(error)) => Err(error.into()),
                                        },
                                    )
                                    .map_err(|error| {
                                        native_main_store_fault(error, &mut storage_stop)
                                    })?;
                                let result = evaluated.outcome.clone();
                                validity.push(evaluated);
                                outcome(result).map(|_| ())
                            },
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
}
