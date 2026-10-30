// Ordered, lossless field stream. No aggregate SourceValue is constructed.
// Read-only visitors may incrementally hash/write frames under session13's
// own reservation/publication rules. This is not a kernel state importer.
pub const CANONICAL_FIELD_PROFILE: &str = "TL-MAIN-STEP-CANONICAL-FIELDS-1";
type FieldResult = Result<(), SourceRuleError>;

fn field_value(
    path: &str,
    value: &V,
    emit: &mut impl FnMut(&str, &V) -> FieldResult,
) -> FieldResult {
    // Container cardinality is part of the stream, even for empty values.
    // Object keys are separate values, not interpolated into a path.
    match value {
        V::Array(values) => {
            emit(&format!("{path}/kind"), &s("Array"))?;
            emit(&format!("{path}/length"), &s(values.len().to_string()))?;
            for (at, value) in values.iter().enumerate() {
                field_value(&format!("{path}/items/{at}"), value, emit)?;
            }
        }
        V::Object(fields) => {
            emit(&format!("{path}/kind"), &s("Object"))?;
            emit(&format!("{path}/length"), &s(fields.len().to_string()))?;
            for (at, (key, value)) in fields.iter().enumerate() {
                emit(&format!("{path}/fields/{at}/key"), &s(key))?;
                field_value(&format!("{path}/fields/{at}/value"), value, emit)?;
            }
        }
        _ => {
            value.validate()?;
            emit(&format!("{path}/atom"), value)?;
        }
    }
    Ok(())
}
fn field_uint(
    path: &str,
    value: u64,
    emit: &mut impl FnMut(&str, &V) -> FieldResult,
) -> FieldResult {
    emit(path, &s(value.to_string()))
}
fn field_throw(
    path: &str,
    value: &MainStepLegacyThrow,
    emit: &mut impl FnMut(&str, &V) -> FieldResult,
) -> FieldResult {
    emit(&format!("{path}/name"), &s(&value.exception.name))?;
    emit(&format!("{path}/message"), &s(&value.exception.message))?;
    field_value(&format!("{path}/code"), &value.code, emit)
}
fn field_result(
    path: &str,
    value: &MainStepSourceResult,
    emit: &mut impl FnMut(&str, &V) -> FieldResult,
) -> FieldResult {
    match value {
        MainStepSourceResult::Fulfilled(value) => {
            emit(&format!("{path}/kind"), &s("Fulfilled"))?;
            field_value(&format!("{path}/value"), value, emit)
        }
        MainStepSourceResult::Thrown(value) => {
            emit(&format!("{path}/kind"), &s("Thrown"))?;
            field_throw(&format!("{path}/thrown"), value, emit)
        }
    }
}
fn field_call(
    path: &str,
    value: &DependencyCall,
    emit: &mut impl FnMut(&str, &V) -> FieldResult,
) -> FieldResult {
    emit(&format!("{path}/target"), &s(&value.target))?;
    field_uint(
        &format!("{path}/arguments/length"),
        value.arguments.len() as u64,
        emit,
    )?;
    for (at, value) in value.arguments.iter().enumerate() {
        field_value(&format!("{path}/arguments/{at}"), value, emit)?;
    }
    Ok(())
}
impl Frame {
    fn visit_fields(
        &self,
        path: &str,
        emit: &mut impl FnMut(&str, &V) -> FieldResult,
    ) -> FieldResult {
        let kind = match self {
            Self::File(_) => "File",
            Self::Finish(_) => "Finish",
            Self::Atomic(_) => "Atomic",
            Self::Delay { .. } => "Delay",
            Self::Sync { .. } => "Sync",
            Self::Recovery { .. } => "Recovery",
            Self::Incoming { .. } => "Incoming",
            Self::Fetch { .. } => "Fetch",
        };
        emit(&format!("{path}/kind"), &s(kind))?;
        match self {
            Self::File(frame) => return frame.visit_fields(path, emit),
            Self::Finish(result) => field_result(&format!("{path}/result"), result, emit)?,
            Self::Atomic(call) => field_call(&format!("{path}/call"), call, emit)?,
            Self::Delay { millis, phase } => {
                field_value(&format!("{path}/millis"), millis, emit)?;
                field_uint(&format!("{path}/phase"), (*phase).into(), emit)?;
            }
            Self::Sync {
                restore,
                phase,
                lag,
            } => {
                field_value(&format!("{path}/restore"), restore, emit)?;
                field_uint(&format!("{path}/phase"), (*phase).into(), emit)?;
                field_value(&format!("{path}/lag"), lag, emit)?;
            }
            Self::Recovery {
                latest,
                track,
                phase,
                saved,
            } => {
                field_value(&format!("{path}/latest"), latest, emit)?;
                field_value(&format!("{path}/track"), track, emit)?;
                field_uint(&format!("{path}/phase"), (*phase).into(), emit)?;
                field_value(&format!("{path}/saved"), saved, emit)?;
            }
            Self::Incoming {
                latest,
                tip,
                block,
                number,
                phase,
            } => {
                for (key, value) in [
                    ("latest", latest),
                    ("tip", tip),
                    ("block", block),
                    ("number", number),
                ] {
                    field_value(&format!("{path}/{key}"), value, emit)?;
                }
                field_uint(&format!("{path}/phase"), (*phase).into(), emit)?;
            }
            Self::Fetch {
                height,
                retries,
                delay,
                attempt,
                phase,
            } => {
                for (key, value) in [("height", height), ("retries", retries), ("delay", delay)] {
                    field_value(&format!("{path}/{key}"), value, emit)?;
                }
                field_uint(&format!("{path}/attempt"), *attempt, emit)?;
                field_uint(&format!("{path}/phase"), (*phase).into(), emit)?;
            }
        }
        Ok(())
    }
}
impl MainStepKernelV1 {
    /// Complete ordered canonical fields; visitor refusal never mutates kernel
    /// or owners. All cardinalities, optional discriminators and source values
    /// are retained. There is deliberately no inverse snapshot constructor.
    pub fn visit_canonical_fields(
        &self,
        emit: &mut impl FnMut(&str, &V) -> FieldResult,
    ) -> FieldResult {
        emit("profile", &s(CANONICAL_FIELD_PROFILE))?;
        emit("sourceProfile", &s(self.source_profile()))?;
        self.visit_automatic_file_parked(emit)?;
        emit("source", &s(MAIN_SOURCE_SHA256))?;
        for (key, value) in [
            ("generation", self.generation),
            ("nextJob", self.next_job),
            ("nextTimer", self.next_timer),
        ] {
            field_uint(key, value, emit)?;
        }
        field_value("logicalTime", &self.logical_time, emit)?;
        field_uint("jobs/length", self.jobs.len() as u64, emit)?;
        for (at, job) in self.jobs.iter().enumerate() {
            let path = format!("jobs/{at}");
            field_uint(&format!("{path}/id"), job.id, emit)?;
            emit(&format!("{path}/callerOwned"), &V::Bool(job.caller_owned))?;
            emit(
                &format!("{path}/parent/present"),
                &V::Bool(job.parent.is_some()),
            )?;
            if let Some(parent) = job.parent {
                field_uint(&format!("{path}/parent/id"), parent, emit)?;
            }
            let kind = match job.status {
                MainStepJobStatus::Ready => "Ready",
                MainStepJobStatus::WaitingTimer(_) => "WaitingTimer",
                MainStepJobStatus::WaitingChild(_) => "WaitingChild",
                MainStepJobStatus::Completed(_) => "Completed",
            };
            emit(&format!("{path}/status/kind"), &s(kind))?;
            match &job.status {
                MainStepJobStatus::WaitingTimer(id) | MainStepJobStatus::WaitingChild(id) => {
                    field_uint(&format!("{path}/status/id"), *id, emit)?
                }
                MainStepJobStatus::Completed(result) => {
                    field_result(&format!("{path}/status/result"), result, emit)?
                }
                MainStepJobStatus::Ready => (),
            }
            job.frame.visit_fields(&format!("{path}/frame"), emit)?;
            emit(
                &format!("{path}/childResult/present"),
                &V::Bool(job.child_result.is_some()),
            )?;
            if let Some(result) = &job.child_result {
                field_result(&format!("{path}/childResult/value"), result, emit)?;
            }
        }
        field_uint("ready/length", self.ready.len() as u64, emit)?;
        for (at, id) in self.ready.iter().enumerate() {
            field_uint(&format!("ready/{at}"), *id, emit)?;
        }
        field_uint("timers/length", self.timers.len() as u64, emit)?;
        for (at, timer) in self.timers.iter().enumerate() {
            let path = format!("timers/{at}");
            for (key, id) in [
                ("id", timer.id),
                ("generation", timer.generation),
                ("job", timer.job),
            ] {
                field_uint(&format!("{path}/{key}"), id, emit)?;
            }
            field_value(&format!("{path}/due"), &timer.due, emit)?;
            field_value(&format!("{path}/delay"), &timer.delay, emit)?;
            emit(
                &format!("{path}/status"),
                &s(match timer.status {
                    TimerStatus::Scheduled => "Scheduled",
                    TimerStatus::Cancelled => "Cancelled",
                }),
            )?;
        }
        field_uint(
            "pendingUnhandled/length",
            self.pending_unhandled.len() as u64,
            emit,
        )?;
        for (at, (id, error)) in self.pending_unhandled.iter().enumerate() {
            field_uint(&format!("pendingUnhandled/{at}/job"), *id, emit)?;
            field_throw(&format!("pendingUnhandled/{at}/error"), error, emit)?;
        }
        emit(
            "ownerStopped/present",
            &V::Bool(self.owner_stopped.is_some()),
        )?;
        if let Some(error) = &self.owner_stopped {
            emit("ownerStopped/error", &s(error.to_string()))?;
        }
        emit("terminal/present", &V::Bool(self.terminal.is_some()))?;
        if let Some(code) = self.terminal {
            field_uint("terminal/code", code.into(), emit)?;
        }
        Ok(())
    }
}
impl MainStepCommandV1 {
    pub fn visit_canonical_fields(
        &self,
        emit: &mut impl FnMut(&str, &V) -> FieldResult,
    ) -> FieldResult {
        emit("profile", &s(CANONICAL_FIELD_PROFILE))?;
        let kind = match self {
            Self::Start { .. } => "Start",
            Self::DrainReady { .. } => "DrainReady",
            Self::FireTimer { .. } => "FireTimer",
            Self::ClearTimer { .. } => "ClearTimer",
            Self::UpdateRecordedIo(_) => "UpdateRecordedIo",
            Self::Shutdown(_) => "Shutdown",
        };
        emit("kind", &s(kind))?;
        match self {
            Self::Start { method, arguments } => field_call(
                "call",
                &DependencyCall {
                    target: method.clone(),
                    arguments: arguments.clone(),
                },
                emit,
            )?,
            Self::DrainReady { limit } => field_uint("limit", *limit as u64, emit)?,
            Self::FireTimer { id, generation } | Self::ClearTimer { id, generation } => {
                field_uint("id", *id, emit)?;
                field_uint("generation", *generation, emit)?;
            }
            Self::UpdateRecordedIo(value) => field_value("input", value, emit)?,
            Self::Shutdown(arguments) => {
                field_uint("arguments/length", arguments.len() as u64, emit)?;
                for (at, value) in arguments.iter().enumerate() {
                    field_value(&format!("arguments/{at}"), value, emit)?;
                }
            }
        }
        Ok(())
    }
}
impl MainStepEvaluationV1 {
    pub fn visit_canonical_fields(
        &self,
        emit: &mut impl FnMut(&str, &V) -> FieldResult,
    ) -> FieldResult {
        emit("profile", &s(CANONICAL_FIELD_PROFILE))?;
        emit("started/present", &V::Bool(self.started.is_some()))?;
        if let Some(id) = self.started {
            field_uint("started/id", id, emit)?;
        }
        let kind = match self.stop {
            MainStepStop::Quiescent => "Quiescent",
            MainStepStop::DrainBound => "DrainBound",
            MainStepStop::SourceOwnerStopped(_) => "SourceOwnerStopped",
            MainStepStop::ProcessExited(_) => "ProcessExited",
        };
        emit("stop/kind", &s(kind))?;
        match &self.stop {
            MainStepStop::SourceOwnerStopped(error) => emit("stop/error", &s(error.to_string()))?,
            MainStepStop::ProcessExited(code) => field_uint("stop/code", (*code).into(), emit)?,
            _ => (),
        }
        field_uint("trace/length", self.trace.len() as u64, emit)?;
        for (at, trace) in self.trace.iter().enumerate() {
            let path = format!("trace/{at}");
            field_uint(&format!("{path}/job"), trace.job, emit)?;
            emit(&format!("{path}/phase"), &s(&trace.phase))?;
            field_value(&format!("{path}/operation"), &trace.operation, emit)?;
            field_value(&format!("{path}/outcome"), &trace.outcome, emit)?;
            field_uint(
                &format!("{path}/effects/length"),
                trace.effects.len() as u64,
                emit,
            )?;
            for (at, effect) in trace.effects.iter().enumerate() {
                field_value(&format!("{path}/effects/{at}"), effect, emit)?;
            }
        }
        field_uint("effects/length", self.effects.len() as u64, emit)?;
        for (at, effect) in self.effects.iter().enumerate() {
            field_value(&format!("effects/{at}"), effect, emit)?;
        }
        field_uint("completed/length", self.completed.len() as u64, emit)?;
        for (at, (id, result)) in self.completed.iter().enumerate() {
            field_uint(&format!("completed/{at}/job"), *id, emit)?;
            field_result(&format!("completed/{at}/result"), result, emit)?;
        }
        Ok(())
    }
}
