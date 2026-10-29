// Actual source locals, outside Clone/Eq Core. No params/state constructor.
// The containing owner lends its SAME runtime and index on every operation.
// Only genuine source evaluation can create an await; callers cannot supply a
// validity outcome. Native append and verified-post resolution consume the
// actual sealed metadata plan. Effects remain in Core until real finish.
use crate::source_compact_index_v2::{
    NativePreparedValidityOperationV1, NativePreparedValidityQueryV1,
    NativePreparedValidityWriteV1, SourceCompactIndex, StreamedValidityEvaluation,
};
#[cfg(test)]
std::thread_local! {
    static NATIVE_MAIN_ACTUAL_ROW_DECODE_COUNT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeMainRowStopV1 {
    AwaitNativeWrite,
    StorageStopped(String),
    SourceStopped(SourceRuleError),
    Complete,
}

enum NativeMainRowPcV1 {
    Prefix,
    PrimaryValidity,
    FatalMark,
    FatalValidity,
    Done,
    Stopped(SourceRuleError),
}
struct NativeMainActualRowV1 {
    row: V,
    id: V,
    kind: V,
    params: V,
    pc: NativeMainRowPcV1,
    validity_started: bool,
    write: Option<Box<NativePreparedValidityWriteV1>>,
    ambiguous: Option<String>,
}
pub(crate) struct NativeMainRowContinuationV1 {
    invocation: Option<SourceMainInvocationV1>,
    rows: std::collections::VecDeque<V>,
    height: V,
    live: bool,
    skips: usize,
    processed_any: bool,
    consumed_rows: u64,
    current: Option<NativeMainActualRowV1>,
    validity: Vec<StreamedValidityEvaluation>,
    result: Option<Result<DependencyReturn, SourceRuleError>>,
}
impl NativeMainRowContinuationV1 {
    pub(crate) fn pending_write(&self) -> Option<&NativePreparedValidityWriteV1> {
        self.current.as_ref()?.write.as_deref()
    }
    pub(crate) fn result(&self) -> Option<&Result<DependencyReturn, SourceRuleError>> {
        self.result.as_ref()
    }
    pub(crate) fn validity(&self) -> &[StreamedValidityEvaluation] {
        &self.validity
    }
    pub(crate) fn consumed_rows(&self) -> u64 {
        self.consumed_rows
    }
    pub(crate) fn visit_canonical_fields(
        &self,
        visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
    ) -> Result<(), SourceRuleError> {
        visitor("profile", &s("TL-MAIN-ACTUAL-ROW-CONTINUATION-13-1"))?;
        native_main_row_visit_value("height", &self.height, visitor)?;
        visitor("live", &V::Bool(self.live))?;
        visitor("skips", &s(self.skips.to_string()))?;
        visitor("processedAny", &V::Bool(self.processed_any))?;
        visitor("consumedRows", &s(self.consumed_rows.to_string()))?;
        visitor("remainingRows/count", &s(self.rows.len().to_string()))?;
        for (at, row) in self.rows.iter().enumerate() {
            native_main_row_visit_value(&format!("remainingRows/{at}"), row, visitor)?;
        }
        if let Some(invocation) = &self.invocation {
            visitor("invocation/target", &s(&invocation.target))?;
            visitor("invocation/spotMain", &V::Bool(invocation.spot_main))?;
            visitor(
                "invocation/rejectedBefore",
                &s(invocation.rejected_before.to_string()),
            )?;
            visitor(
                "invocation/transferRejectedBefore",
                &s(invocation.transfer_rejected_before.to_string()),
            )?;
        }
        visitor("hasCurrent", &V::Bool(self.current.is_some()))?;
        if let Some(row) = &self.current {
            native_main_row_visit_value("row/value", &row.row, visitor)?;
            native_main_row_visit_value("row/id", &row.id, visitor)?;
            native_main_row_visit_value("row/kind", &row.kind, visitor)?;
            native_main_row_visit_value("row/params", &row.params, visitor)?;
            visitor(
                "row/pc",
                &s(match &row.pc {
                    NativeMainRowPcV1::Prefix => "Prefix",
                    NativeMainRowPcV1::PrimaryValidity => "PrimaryValidity",
                    NativeMainRowPcV1::FatalMark => "FatalMark",
                    NativeMainRowPcV1::FatalValidity => "FatalValidity",
                    NativeMainRowPcV1::Done => "Done",
                    NativeMainRowPcV1::Stopped(_) => "Stopped",
                }),
            )?;
            if let NativeMainRowPcV1::Stopped(error) = &row.pc {
                visitor("row/stopped", &s(format!("{error:?}")))?;
            }
            visitor("row/validityStarted", &V::Bool(row.validity_started))?;
            visitor("row/ambiguous", &row.ambiguous.as_ref().map_or(V::Null, s))?;
            native_main_row_visit_value(
                "row/write",
                &row.write
                    .as_ref()
                    .map_or(V::Null, |write| write.durable_read_context()),
                visitor,
            )?;
        }
        visitor("validity/count", &s(self.validity.len().to_string()))?;
        for (at, evaluation) in self.validity.iter().enumerate() {
            let path = format!("validity/{at}");
            visitor(&format!("{path}/profile"), &s(&evaluation.profile))?;
            let query = &evaluation.query;
            visitor(&format!("{path}/query/profile"), &s(&query.profile))?;
            visitor(&format!("{path}/query/predicate"), &s(&query.predicate))?;
            visitor(
                &format!("{path}/query/visitedRows"),
                &s(query.visited_rows.to_string()),
            )?;
            visitor(
                &format!("{path}/query/matchedRows"),
                &s(query.matched_rows.to_string()),
            )?;
            visitor(
                &format!("{path}/query/chain"),
                &s(native_main_row_hex(&query.chain_sha256d)),
            )?;
            visitor(&format!("{path}/query/complete"), &V::Bool(query.complete))?;
            native_main_row_visit_snapshot(
                &format!("{path}/query/snapshot"),
                &query.snapshot,
                visitor,
            )?;
            native_main_row_visit_snapshot(
                &format!("{path}/postState"),
                &evaluation.post_state,
                visitor,
            )?;
            match &evaluation.outcome {
                SourceOutcome::Returned(value) => {
                    visitor(&format!("{path}/outcome/kind"), &s("Returned"))?;
                    native_main_row_visit_value(&format!("{path}/outcome/value"), value, visitor)?;
                }
                SourceOutcome::Thrown(error) => {
                    visitor(&format!("{path}/outcome/kind"), &s("Thrown"))?;
                    visitor(&format!("{path}/outcome/name"), &s(&error.name))?;
                    visitor(&format!("{path}/outcome/message"), &s(&error.message))?;
                }
            }
            visitor(
                &format!("{path}/hasUpdate"),
                &V::Bool(evaluation.update.is_some()),
            )?;
            if let Some(update) = &evaluation.update {
                visitor(&format!("{path}/update/target"), &s(&update.call.target))?;
                visitor(
                    &format!("{path}/update/arguments/count"),
                    &s(update.call.arguments.len().to_string()),
                )?;
                for (at, value) in update.call.arguments.iter().enumerate() {
                    native_main_row_visit_value(
                        &format!("{path}/update/arguments/{at}"),
                        value,
                        visitor,
                    )?;
                }
                native_main_row_visit_return(
                    &format!("{path}/update/returned"),
                    &update.returned,
                    visitor,
                )?;
            }
        }
        visitor("complete", &V::Bool(self.result.is_some()))?;
        if let Some(result) = &self.result {
            match result {
                Ok(returned) => native_main_row_visit_return("result", returned, visitor)?,
                Err(error) => {
                    visitor("result/kind", &s("Local"))?;
                    visitor("result/error", &s(format!("{error:?}")))?;
                }
            }
        }
        Ok(())
    }
}
pub(crate) fn native_main_row_visit_value(
    path: &str,
    value: &V,
    visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
) -> Result<(), SourceRuleError> {
    match value {
        V::Array(values) => {
            visitor(&format!("{path}/kind"), &s("Array"))?;
            visitor(&format!("{path}/count"), &s(values.len().to_string()))?;
            for (at, value) in values.iter().enumerate() {
                native_main_row_visit_value(&format!("{path}/{at}"), value, visitor)?;
            }
        }
        V::Object(fields) => {
            visitor(&format!("{path}/kind"), &s("Object"))?;
            visitor(&format!("{path}/count"), &s(fields.len().to_string()))?;
            for (at, (key, value)) in fields.iter().enumerate() {
                visitor(&format!("{path}/{at}/key"), &s(key))?;
                native_main_row_visit_value(&format!("{path}/{at}/value"), value, visitor)?;
            }
        }
        value => visitor(path, value)?,
    }
    Ok(())
}
fn native_main_row_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn native_main_row_visit_snapshot(
    path: &str,
    pin: &crate::source_compact_index_v2::MetadataSnapshot,
    visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
) -> Result<(), SourceRuleError> {
    visitor(&format!("{path}/profile"), &s(&pin.profile))?;
    visitor(&format!("{path}/context"), &s(&pin.context))?;
    visitor(&format!("{path}/sequence"), &s(pin.sequence.to_string()))?;
    let p = &pin.publication;
    visitor(&format!("{path}/generation"), &s(p.generation.to_string()))?;
    visitor(
        &format!("{path}/parent"),
        &p.parent.map_or(V::Null, |n| s(n.to_string())),
    )?;
    visitor(
        &format!("{path}/physicalCreation"),
        &s(p.physical_creation_sequence.to_string()),
    )?;
    visitor(
        &format!("{path}/logicalCreation"),
        &s(p.creation_sequence.to_string()),
    )?;
    visitor(
        &format!("{path}/manifest"),
        &s(native_main_row_hex(&p.manifest_sha256d)),
    )
}
fn native_main_row_visit_return(
    path: &str,
    returned: &DependencyReturn,
    visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
) -> Result<(), SourceRuleError> {
    let (kind, value) = match returned {
        DependencyReturn::Immediate(v) => ("Immediate", Ok(v)),
        DependencyReturn::Pending(v) => ("Pending", Ok(v)),
        DependencyReturn::Throw(e) => ("Throw", Err(e)),
        DependencyReturn::PendingThrow(e) => ("PendingThrow", Err(e)),
    };
    visitor(&format!("{path}/kind"), &s(kind))?;
    match value {
        Ok(value) => native_main_row_visit_value(&format!("{path}/value"), value, visitor),
        Err(error) => {
            visitor(&format!("{path}/name"), &s(&error.name))?;
            visitor(&format!("{path}/message"), &s(&error.message))
        }
    }
}
impl SourceContractRuntime {
    pub(crate) fn native_main_rows_begin(
        &mut self,
        rows: V,
        height: V,
        live: bool,
    ) -> Result<NativeMainRowContinuationV1, SourceRuleError> {
        if self.core.native_main_binding.is_none() {
            return Err(SourceRuleError::Backend);
        }
        let V::Array(rows) = rows else {
            return Err(SourceRuleError::InvalidInput);
        };
        if rows.len() > MAX_TABLE_ROWS {
            return Err(SourceRuleError::Capacity);
        }
        let invocation = self.source_main_invocation_begin(&DependencyCall {
            target: if live {
                "Main.processTx"
            } else {
                "Main.processTxSet"
            }
            .into(),
            arguments: vec![],
        })?;
        Ok(NativeMainRowContinuationV1 {
            invocation: Some(invocation),
            rows: rows.into(),
            height,
            live,
            skips: 0,
            processed_any: false,
            consumed_rows: 0,
            current: None,
            validity: vec![],
            result: None,
        })
    }
    fn native_main_row_fatal(
        row: &mut NativeMainActualRowV1,
        height: &V,
        error: crate::validity_source::SourceException,
    ) {
        row.params = obj(&[
            ("valid", V::Bool(false)),
            (
                "reason",
                s(format!("Fatal tx processing error: {}", error.message)),
            ),
            ("block", height.clone()),
            ("txid", row.id.clone()),
        ]);
        row.validity_started = false;
        row.pc = NativeMainRowPcV1::FatalMark;
    }
    fn native_main_rows_finish(&mut self, frame: &mut NativeMainRowContinuationV1, result: R<V>) {
        frame.result = Some(self.source_main_invocation_finish(
            frame.invocation.take().expect("linear invocation"),
            result,
        ));
    }
    fn native_main_row_validity_result(
        &mut self,
        frame: &mut NativeMainRowContinuationV1,
        evaluation: StreamedValidityEvaluation,
    ) {
        let result = outcome(evaluation.outcome.clone());
        frame.validity.push(evaluation);
        let row = frame.current.as_mut().expect("actual row");
        row.write = None;
        row.ambiguous = None;
        match result {
            Ok(_) => row.pc = NativeMainRowPcV1::Done,
            Err(Fault::Thrown(error)) if matches!(row.pc, NativeMainRowPcV1::PrimaryValidity) => {
                Self::native_main_row_fatal(row, &frame.height, error);
            }
            Err(error) => self.native_main_rows_finish(frame, Err(error)),
        }
    }
    pub(crate) fn native_main_rows_advance(
        &mut self,
        index: &SourceCompactIndex,
        frame: &mut NativeMainRowContinuationV1,
    ) -> NativeMainRowStopV1 {
        loop {
            if frame.result.is_some() {
                return NativeMainRowStopV1::Complete;
            }
            if frame.current.is_none() {
                let Some(row) = frame.rows.pop_front() else {
                    let value = if frame.live {
                        V::Bool(frame.processed_any)
                    } else {
                        n(frame.skips as f64)
                    };
                    self.native_main_rows_finish(frame, Ok(value));
                    return NativeMainRowStopV1::Complete;
                };
                frame.consumed_rows += 1;
                frame.current = Some(NativeMainActualRowV1 {
                    row,
                    id: s("unknown"),
                    kind: n(-1.),
                    params: V::Undefined,
                    pc: NativeMainRowPcV1::Prefix,
                    validity_started: false,
                    write: None,
                    ambiguous: None,
                });
            }
            let row = frame.current.as_mut().expect("actual row");
            if let Some(error) = &row.ambiguous {
                return NativeMainRowStopV1::StorageStopped(error.clone());
            }
            if row.write.is_some() {
                return NativeMainRowStopV1::AwaitNativeWrite;
            }
            match &row.pc {
                NativeMainRowPcV1::Stopped(error) => {
                    return NativeMainRowStopV1::SourceStopped(error.clone())
                }
                NativeMainRowPcV1::Done => {
                    frame.current = None;
                    continue;
                }
                NativeMainRowPcV1::Prefix => {
                    match self.source_main_prepare_row_prefix(
                        &row.row,
                        (&frame.height, frame.live),
                        &mut row.id,
                        &mut row.kind,
                        &mut frame.processed_any,
                    ) {
                        Ok(SourceMainPreparedRowPrefix::AlreadyProcessed) => {
                            frame.skips += 1;
                            row.pc = NativeMainRowPcV1::Done;
                        }
                        Ok(SourceMainPreparedRowPrefix::AwaitValidity(params)) => {
                            row.params = params;
                            row.pc = NativeMainRowPcV1::PrimaryValidity;
                        }
                        Err(Fault::Thrown(error)) => {
                            Self::native_main_row_fatal(row, &frame.height, error)
                        }
                        Err(Fault::Local(error)) => row.pc = NativeMainRowPcV1::Stopped(error),
                    }
                }
                NativeMainRowPcV1::FatalMark => {
                    if row.id == s("unknown") {
                        row.pc = NativeMainRowPcV1::Done;
                        continue;
                    }
                    let result = self
                        .core
                        .source_main_mark_processed(row.id.clone(), row.params.clone())
                        .and_then(|()| number(&row.kind));
                    match result {
                        Ok(kind) => {
                            row.pc = if kind >= 0. {
                                NativeMainRowPcV1::FatalValidity
                            } else {
                                NativeMainRowPcV1::Done
                            }
                        }
                        Err(Fault::Local(error)) => row.pc = NativeMainRowPcV1::Stopped(error),
                        Err(error) => self.native_main_rows_finish(frame, Err(error)),
                    }
                }
                NativeMainRowPcV1::PrimaryValidity | NativeMainRowPcV1::FatalValidity => {
                    // These actual source failure facts execute once, before the
                    // complete query. A native read stop cannot consume them twice.
                    if !row.validity_started {
                        row.validity_started = true;
                        match self
                            .core
                            .database("txIndex")
                            .and_then(|_| self.core.check_failure("txIndex", "findAsync"))
                        {
                            Ok(()) => (),
                            Err(Fault::Thrown(error))
                                if matches!(row.pc, NativeMainRowPcV1::PrimaryValidity) =>
                            {
                                Self::native_main_row_fatal(row, &frame.height, error);
                                continue;
                            }
                            Err(Fault::Local(error)) => {
                                row.pc = NativeMainRowPcV1::Stopped(error);
                                continue;
                            }
                            Err(error) => {
                                self.native_main_rows_finish(frame, Err(error));
                                continue;
                            }
                        }
                    }
                    let query = index.snapshot().and_then(|pin| {
                        index.prepare_validity_read(
                            &pin,
                            row.id.clone(),
                            row.kind.clone(),
                            row.params.get("valid"),
                            row.params.get("reason"),
                        )
                    });
                    let query = match query {
                        Ok(query) => query,
                        Err(error) => {
                            row.ambiguous = Some(error.to_string());
                            continue;
                        }
                    };
                    let selected = match query {
                        NativePreparedValidityQueryV1::Complete(evaluation) => {
                            self.native_main_row_validity_result(frame, *evaluation);
                            continue;
                        }
                        NativePreparedValidityQueryV1::Selected(selected) => selected,
                    };
                    let prepared = match self.core.check_failure("txIndex", "updateAsync") {
                        Ok(()) => index
                            .snapshot()
                            .and_then(|pin| selected.prepare_write(index, &pin)),
                        Err(Fault::Thrown(error)) => {
                            selected.source_rejected(index, error).map(|evaluation| {
                                NativePreparedValidityOperationV1::Complete(Box::new(evaluation))
                            })
                        }
                        Err(Fault::Local(error)) => {
                            row.pc = NativeMainRowPcV1::Stopped(error);
                            continue;
                        }
                    };
                    match prepared {
                        Ok(NativePreparedValidityOperationV1::Complete(evaluation)) => {
                            self.native_main_row_validity_result(frame, *evaluation)
                        }
                        Ok(NativePreparedValidityOperationV1::Write(write)) => {
                            row.write = Some(write)
                        }
                        Err(error) => row.ambiguous = Some(error.to_string()),
                    }
                }
            }
        }
    }
    pub(crate) fn native_main_rows_append(
        &mut self,
        index: &mut SourceCompactIndex,
        frame: &mut NativeMainRowContinuationV1,
    ) -> NativeMainRowStopV1 {
        let Some(row) = frame.current.as_mut() else {
            return NativeMainRowStopV1::SourceStopped(SourceRuleError::InvalidInput);
        };
        if let Some(error) = &row.ambiguous {
            return NativeMainRowStopV1::StorageStopped(error.clone());
        }
        let Some(write) = &row.write else {
            return NativeMainRowStopV1::SourceStopped(SourceRuleError::InvalidInput);
        };
        match index.commit_prepared_validity(write) {
            // Keep the source await unresolved until its real durable prefix
            // acknowledgement. The evaluation is regenerated from exact post.
            Ok(_) => return NativeMainRowStopV1::AwaitNativeWrite,
            Err(error) => row.ambiguous = Some(error.to_string()),
        }
        self.native_main_rows_advance(index, frame)
    }
    #[cfg(test)]
    fn native_main_rows_commit(
        &mut self,
        index: &mut SourceCompactIndex,
        frame: &mut NativeMainRowContinuationV1,
    ) -> NativeMainRowStopV1 {
        let stop = self.native_main_rows_append(index, frame);
        if stop == NativeMainRowStopV1::AwaitNativeWrite {
            self.native_main_rows_resolve_post(index, frame)
        } else {
            stop
        }
    }
    pub(crate) fn native_main_row_effects(&self) -> &[V] {
        &self.core.effects
    }
    pub(crate) fn native_main_rows_resolve_post(
        &mut self,
        index: &SourceCompactIndex,
        frame: &mut NativeMainRowContinuationV1,
    ) -> NativeMainRowStopV1 {
        let Some(write) = frame.pending_write() else {
            return NativeMainRowStopV1::SourceStopped(SourceRuleError::InvalidInput);
        };
        match index.resolve_prepared_validity_post(write) {
            Ok(evaluation) => {
                self.native_main_row_validity_result(frame, evaluation);
                self.native_main_rows_advance(index, frame)
            }
            Err(error) => NativeMainRowStopV1::StorageStopped(error.to_string()),
        }
    }
}
