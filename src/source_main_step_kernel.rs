//! Continuation metadata only. The financial Runtime/Journal is never stored
//! here. Only a crate-private sealed capability may execute actual primitives.
use crate::validity_source::{DependencyCall, SourceException, SourceRuleError, SourceValue as V};
use std::collections::VecDeque;

pub const SOURCE_PROFILE: &str = "TL-MAIN-EXPLICIT-TIMER-IMMEDIATE-DATA-1";
pub const MAIN_SOURCE_SHA256: &str =
    "006d59ba3d28d802b0e0c5be2d9d661b58caf1c919a704b3f0d40bc9613c4009";
const MAX_JOBS: usize = 64;
const MAX_TIMERS: usize = 1024;
const MAX_STEPS: usize = 4096;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MainStepCommandV1 {
    Start { method: String, arguments: Vec<V> },
    DrainReady { limit: usize },
    FireTimer { id: u64, generation: u64 },
    ClearTimer { id: u64, generation: u64 },
    UpdateRecordedIo(V),
    Shutdown(Vec<V>),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MainStepLegacyThrow {
    pub exception: SourceException,
    /// The actual observed code property, not inferred from an error label.
    pub code: V,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MainStepSourceResult {
    Fulfilled(V),
    Thrown(MainStepLegacyThrow),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MainStepJobStatus {
    Ready,
    WaitingTimer(u64),
    WaitingChild(u64),
    Completed(MainStepSourceResult),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MainStepStop {
    Quiescent,
    DrainBound,
    SourceOwnerStopped(SourceRuleError),
    ProcessExited(u8),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MainStepError {
    InvalidInput,
    Capacity,
    WrongGeneration,
    TimerNotScheduled,
    TimerNotNext,
    OwnerStopped,
    AlreadyExited,
    NotQuiescent,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MainStepPrimitive {
    BlockCount,
    HeightRead(&'static str),
    HeightWrite { name: &'static str, value: V },
    NetworkStatus,
    FetchBlock(V),
    ProcessBlock { block: V, height: V },
    PauseRead,
    Atomic(DependencyCall),
    UpdateIo(V),
    PublishUnhandled(MainStepLegacyThrow),
    Number(V),
    Add(V, V),
    Greater(V, V),
    LessEqual(V, V),
    Backoff { delay: V, attempt: u64 },
    File(Box<FileStepPrimitive>),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MainStepPrimitiveOutcome {
    Returned(V),
    Thrown(MainStepLegacyThrow),
    /// A real local/storage stop retains the accepted owner prefix. The
    /// containing native session must freeze/poison instead of replaying it.
    Stopped(SourceRuleError),
}
pub(crate) struct MainStepPrimitiveExecution {
    pub outcome: MainStepPrimitiveOutcome,
    pub effects: Vec<V>,
}
pub(crate) mod sealed {
    pub trait Sealed {}
}
pub(crate) trait MainStepOwner: sealed::Sealed {
    /// Must establish absence of hidden timers BEFORE running an atomic body.
    fn preflight(&self, operation: &MainStepPrimitive) -> Result<(), SourceRuleError>;
    fn execute(&mut self, operation: &MainStepPrimitive) -> MainStepPrimitiveExecution;
    // Only a sealed actual owner loan records its effect-origin ranges.
    fn record_owner_effect_range(&mut self, _trace: usize, _start: usize, _end: usize) {}
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Frame {
    File(Box<FileStepFrame>),
    Finish(MainStepSourceResult),
    Atomic(DependencyCall),
    Delay {
        millis: V,
        phase: u8,
    },
    Sync {
        restore: V,
        phase: u8,
        lag: V,
    },
    Recovery {
        latest: V,
        track: V,
        phase: u8,
        saved: V,
    },
    Incoming {
        latest: V,
        tip: V,
        block: V,
        number: V,
        phase: u8,
    },
    Fetch {
        height: V,
        retries: V,
        delay: V,
        attempt: u64,
        phase: u8,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Job {
    id: u64,
    caller_owned: bool,
    parent: Option<u64>,
    status: MainStepJobStatus,
    frame: Frame,
    child_result: Option<MainStepSourceResult>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum TimerStatus {
    Scheduled,
    Cancelled,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Timer {
    id: u64,
    generation: u64,
    job: u64,
    due: V,
    delay: V,
    status: TimerStatus,
}

/// No Clone: controllers cannot fork a live scheduler and execute both copies.
#[derive(Debug, PartialEq, Eq)]
pub struct MainStepKernelV1 {
    file_retries: bool,
    native_file_blocks: bool,
    native_file_tail_parked: Option<MainServiceBlockRequest>,
    #[cfg(test)]
    startup_source_promises: Option<startup_source_promise_queue::StartupPromiseQueue>,
    #[cfg(test)]
    startup_source_nedb_jobs: Option<startup_source_nedb_jobs::StartupNedbQueue>,
    generation: u64,
    next_job: u64,
    next_timer: u64,
    logical_time: V,
    jobs: Vec<Job>,
    timers: Vec<Timer>,
    ready: VecDeque<u64>,
    pending_unhandled: VecDeque<(u64, MainStepLegacyThrow)>,
    owner_stopped: Option<SourceRuleError>,
    terminal: Option<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MainStepTraceV1 {
    pub job: u64,
    pub phase: String,
    pub operation: V,
    pub outcome: V,
    pub effects: Vec<V>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MainStepEvaluationV1 {
    pub started: Option<u64>,
    pub stop: MainStepStop,
    pub trace: Vec<MainStepTraceV1>,
    pub effects: Vec<V>,
    pub completed: Vec<(u64, MainStepSourceResult)>,
}

fn s(value: impl Into<String>) -> V {
    V::String(value.into())
}
fn n(value: f64) -> V {
    V::number(value)
}
fn obj(fields: &[(&str, V)]) -> V {
    V::Object(
        fields
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
    )
}
fn truth(value: &V) -> bool {
    match value {
        V::Undefined | V::Null => false,
        V::Bool(v) => *v,
        V::Number(bits) => {
            let v = f64::from_bits(*bits);
            v != 0. && !v.is_nan()
        }
        V::String(v) => !v.is_empty(),
        _ => true,
    }
}
fn strict(a: &V, b: &V) -> bool {
    match (a, b) {
        (V::Number(a), V::Number(b)) => {
            let (a, b) = (f64::from_bits(*a), f64::from_bits(*b));
            !a.is_nan() && !b.is_nan() && a == b
        }
        (V::Undefined, V::Undefined) | (V::Null, V::Null) => true,
        (V::Bool(a), V::Bool(b)) => a == b,
        (V::String(a), V::String(b)) => a == b,
        _ => false,
    }
}
fn arg(arguments: &[V], index: usize) -> V {
    arguments.get(index).cloned().unwrap_or(V::Undefined)
}

enum AdvanceError {
    Throw(MainStepLegacyThrow),
    Stop(SourceRuleError),
}
type Advance<T> = Result<T, AdvanceError>;
fn raw_number(v: &V) -> f64 {
    if let V::Number(bits) = v {
        f64::from_bits(*bits)
    } else {
        unreachable!("validated numeric timer/internal result")
    }
}

fn throw_value(error: &MainStepLegacyThrow) -> V {
    obj(&[
        ("name", s(&error.exception.name)),
        ("message", s(&error.exception.message)),
        ("code", error.code.clone()),
    ])
}
fn result_value(result: &MainStepSourceResult) -> V {
    match result {
        MainStepSourceResult::Fulfilled(v) => obj(&[("returned", v.clone())]),
        MainStepSourceResult::Thrown(e) => obj(&[("thrown", throw_value(e))]),
    }
}
impl MainStepPrimitive {
    pub(crate) fn value(&self) -> V {
        use MainStepPrimitive::*;
        let (name, args) = match self {
            File(operation) => operation.descriptor(),
            BlockCount => ("BlockCount", vec![]),
            HeightRead(k) => ("HeightRead", vec![s(*k)]),
            HeightWrite { name, value } => ("HeightWrite", vec![s(*name), value.clone()]),
            NetworkStatus => ("NetworkStatus", vec![]),
            FetchBlock(h) => ("FetchBlock", vec![h.clone()]),
            ProcessBlock { block, height } => ("ProcessBlock", vec![block.clone(), height.clone()]),
            PauseRead => ("PauseRead", vec![]),
            Atomic(c) => ("Atomic", vec![s(&c.target), V::Array(c.arguments.clone())]),
            UpdateIo(v) => ("UpdateIo", vec![v.clone()]),
            PublishUnhandled(e) => ("PublishUnhandled", vec![throw_value(e)]),
            Number(v) => ("Number", vec![v.clone()]),
            Add(a, b) => ("Add", vec![a.clone(), b.clone()]),
            Greater(a, b) => ("Greater", vec![a.clone(), b.clone()]),
            LessEqual(a, b) => ("LessEqual", vec![a.clone(), b.clone()]),
            Backoff { delay, attempt } => ("Backoff", vec![delay.clone(), s(attempt.to_string())]),
        };
        obj(&[("operation", s(name)), ("arguments", V::Array(args))])
    }
}

impl MainStepKernelV1 {
    pub fn new(generation: u64) -> Self {
        Self {
            generation,
            file_retries: false,
            native_file_blocks: false,
            native_file_tail_parked: None,
            #[cfg(test)]
            startup_source_promises: None,
            #[cfg(test)]
            startup_source_nedb_jobs: None,
            next_job: 1,
            next_timer: 1,
            logical_time: n(0.),
            jobs: vec![],
            timers: vec![],
            ready: VecDeque::new(),
            pending_unhandled: VecDeque::new(),
            owner_stopped: None,
            terminal: None,
        }
    }
    pub fn with_file_retries(generation: u64) -> Self {
        let mut kernel = Self::new(generation);
        kernel.file_retries = true;
        kernel
    }
    pub fn source_profile(&self) -> &'static str {
        if self.startup_source_profile_active() {
            return "TL-MAIN-VM-BUSINESS-HOST-IO-13-1";
        }
        if self.native_file_blocks {
            return AUTOMATIC_NATIVE_FILE_KERNEL_PROFILE;
        }
        if self.file_retries {
            FILE_RETRY_PROFILE
        } else {
            SOURCE_PROFILE
        }
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn jobs(&self) -> impl Iterator<Item = (u64, &MainStepJobStatus)> {
        self.jobs.iter().map(|j| (j.id, &j.status))
    }
    pub fn next_timer(&self) -> Option<u64> {
        self.timers
            .iter()
            .filter(|t| t.status == TimerStatus::Scheduled)
            .min_by(|a, b| {
                raw_number(&a.due)
                    .total_cmp(&raw_number(&b.due))
                    .then(a.id.cmp(&b.id))
            })
            .map(|t| t.id)
    }
    fn position(&self, id: u64) -> usize {
        self.jobs
            .iter()
            .position(|j| j.id == id)
            .expect("internally admitted task reference")
    }
    fn spawn(
        &mut self,
        frame: Frame,
        parent: Option<u64>,
        caller_owned: bool,
    ) -> Result<u64, MainStepError> {
        if self.jobs.len() >= MAX_JOBS || self.next_job == u64::MAX {
            return Err(MainStepError::Capacity);
        }
        let id = self.next_job;
        self.next_job += 1;
        self.jobs.push(Job {
            id,
            caller_owned,
            parent,
            status: MainStepJobStatus::Ready,
            frame,
            child_result: None,
        });
        self.ready.push_back(id);
        Ok(id)
    }
    fn frame(&self, method: &str, a: &[V]) -> Result<Frame, MainStepError> {
        if a.len() > 8 || a.iter().any(|v| v.validate().is_err()) {
            return Err(MainStepError::InvalidInput);
        }
        if self.file_retries {
            if let Some(frame) = FileStepFrame::from_call(method, a)? {
                return Ok(Frame::File(Box::new(frame)));
            }
        }
        Ok(match method {
            "Main.delay" => Frame::Delay {
                millis: arg(a, 0),
                phase: 0,
            },
            "Main.syncIfNecessary" => Frame::Sync {
                restore: arg(a, 0),
                phase: 0,
                lag: V::Undefined,
            },
            "Main.enterRecoveryMode" => Frame::Recovery {
                latest: arg(a, 0),
                track: arg(a, 1),
                phase: 0,
                saved: V::Undefined,
            },
            "Main.processIncomingBlocks" => Frame::Incoming {
                latest: arg(a, 1),
                tip: arg(a, 2),
                block: V::Undefined,
                number: V::Undefined,
                phase: 0,
            },
            "Main.fetchWithRetry" => Frame::Fetch {
                height: arg(a, 0),
                retries: if arg(a, 1) == V::Undefined {
                    n(3.)
                } else {
                    arg(a, 1)
                },
                delay: if arg(a, 2) == V::Undefined {
                    n(1000.)
                } else {
                    arg(a, 2)
                },
                attempt: 0,
                phase: 0,
            },
            "Main.setPause"
            | "Main.saveTrackHeight"
            | "Main.loadTrackHeight"
            | "Main.loadMaxProcessedHeight"
            | "Main.checkBlockLag"
            | "Main.getCurrentBlockHeight"
            | "Main.checkNetworkStatus" => Frame::Atomic(DependencyCall {
                target: method.into(),
                arguments: a.to_vec(),
            }),
            _ => return Err(MainStepError::InvalidInput),
        })
    }

    fn operation(
        owner: &mut impl MainStepOwner,
        job: u64,
        phase: &str,
        operation: MainStepPrimitive,
        evaluation: &mut MainStepEvaluationV1,
    ) -> Advance<V> {
        if let Err(error) = owner.preflight(&operation) {
            evaluation.trace.push(MainStepTraceV1 {
                job,
                phase: phase.into(),
                operation: operation.value(),
                outcome: obj(&[("preflightStop", s(error.to_string()))]),
                effects: vec![],
            });
            return Err(AdvanceError::Stop(error));
        }
        let actual = owner.execute(&operation);
        let outcome = match &actual.outcome {
            MainStepPrimitiveOutcome::Returned(v) => obj(&[("returned", v.clone())]),
            MainStepPrimitiveOutcome::Thrown(e) => obj(&[("thrown", throw_value(e))]),
            MainStepPrimitiveOutcome::Stopped(e) => obj(&[("ownerStop", s(e.to_string()))]),
        };
        let owner_start = evaluation.effects.len();
        evaluation.effects.extend(actual.effects.iter().cloned());
        owner.record_owner_effect_range(
            evaluation.trace.len(),
            owner_start,
            evaluation.effects.len(),
        );
        evaluation.trace.push(MainStepTraceV1 {
            job,
            phase: phase.into(),
            operation: operation.value(),
            outcome,
            effects: actual.effects,
        });
        match actual.outcome {
            MainStepPrimitiveOutcome::Returned(v) => Ok(v),
            MainStepPrimitiveOutcome::Thrown(e) => Err(AdvanceError::Throw(e)),
            MainStepPrimitiveOutcome::Stopped(e) => Err(AdvanceError::Stop(e)),
        }
    }
    fn wait_timer(
        &mut self,
        job: u64,
        delay: V,
        evaluation: &mut MainStepEvaluationV1,
    ) -> Advance<()> {
        let V::Number(bits) = delay else {
            return Err(AdvanceError::Stop(
                SourceRuleError::UnsupportedNumericDomain,
            ));
        };
        let delay = f64::from_bits(bits);
        let due = raw_number(&self.logical_time) + delay;
        if !delay.is_finite() || delay < 0. || !due.is_finite() {
            return Err(AdvanceError::Stop(
                SourceRuleError::UnsupportedNumericDomain,
            ));
        }
        if self.timers.len() >= MAX_TIMERS || self.next_timer == u64::MAX {
            return Err(AdvanceError::Stop(SourceRuleError::Capacity));
        }
        let id = self.next_timer;
        self.next_timer += 1;
        self.timers.push(Timer {
            id,
            generation: self.generation,
            job,
            due: n(due),
            delay: n(delay),
            status: TimerStatus::Scheduled,
        });
        let at = self.position(job);
        self.jobs[at].status = MainStepJobStatus::WaitingTimer(id);
        evaluation.effects.push(obj(&[
            ("kind", s("timer-scheduled")),
            ("id", n(id as f64)),
            ("delay", n(delay)),
            ("due", n(due)),
        ]));
        Ok(())
    }
    fn child(&mut self, parent: u64, frame: Frame) -> Advance<()> {
        let id = self
            .spawn(frame, Some(parent), false)
            .map_err(|_| AdvanceError::Stop(SourceRuleError::Capacity))?;
        let at = self.position(parent);
        self.jobs[at].status = MainStepJobStatus::WaitingChild(id);
        Ok(())
    }
    fn child_value(&mut self, parent: u64) -> Advance<V> {
        let at = self.position(parent);
        match self.jobs[at]
            .child_result
            .take()
            .ok_or(AdvanceError::Stop(SourceRuleError::Backend))?
        {
            MainStepSourceResult::Fulfilled(v) => Ok(v),
            MainStepSourceResult::Thrown(e) => Err(AdvanceError::Throw(e)),
        }
    }
    fn complete(
        &mut self,
        id: u64,
        result: MainStepSourceResult,
        evaluation: &mut MainStepEvaluationV1,
    ) {
        let at = self.position(id);
        let parent = self.jobs[at].parent;
        let caller = self.jobs[at].caller_owned;
        self.jobs[at].status = MainStepJobStatus::Completed(result.clone());
        evaluation.completed.push((id, result.clone()));
        if let Some(parent) = parent {
            let at = self.position(parent);
            assert_eq!(self.jobs[at].status, MainStepJobStatus::WaitingChild(id));
            self.jobs[at].child_result = Some(result);
            self.jobs[at].status = MainStepJobStatus::Ready;
            self.ready.push_back(parent);
        } else if !caller {
            if let MainStepSourceResult::Thrown(error) = result {
                self.pending_unhandled.push_back((id, error));
            }
        }
    }
    fn advance(
        &mut self,
        id: u64,
        frame: &mut Frame,
        owner: &mut impl MainStepOwner,
        evaluation: &mut MainStepEvaluationV1,
    ) -> Advance<Option<V>> {
        use MainStepPrimitive as Op;
        let phase = frame.phase();
        let mut call = |operation| Self::operation(owner, id, &phase, operation, evaluation);
        match frame {
            Frame::Finish(_) => Err(AdvanceError::Stop(SourceRuleError::Backend)),
            Frame::File(frame) => frame.advance(self, id, owner, evaluation),
            Frame::Atomic(c) => Ok(Some(call(Op::Atomic(c.clone()))?)),
            Frame::Delay { millis, phase } => {
                if *phase == 0 {
                    *phase = 1;
                    self.wait_timer(id, millis.clone(), evaluation)?;
                    Ok(None)
                } else {
                    Ok(Some(V::Undefined))
                }
            }
            Frame::Sync {
                restore,
                phase,
                lag,
            } => match *phase {
                0 => {
                    *lag = call(Op::Atomic(DependencyCall {
                        target: "Main.checkBlockLag".into(),
                        arguments: vec![],
                    }))?;
                    if truth(restore) {
                        lag.insert("maxTrack", restore.clone())
                            .map_err(AdvanceError::Stop)?;
                    }
                    *phase = 1;
                    Ok(None)
                }
                1 => {
                    if truth(&call(Op::PauseRead)?) {
                        *phase = 2;
                        self.wait_timer(id, n(1000.), evaluation)?;
                        Ok(None)
                    } else {
                        self.spawn(
                            Frame::Incoming {
                                latest: lag.get("maxTrack"),
                                tip: lag.get("chainTip"),
                                block: V::Undefined,
                                number: V::Undefined,
                                phase: 0,
                            },
                            None,
                            false,
                        )
                        .map_err(|_| AdvanceError::Stop(SourceRuleError::Capacity))?;
                        Ok(Some(V::Undefined))
                    }
                }
                2 => {
                    if truth(&call(Op::PauseRead)?) {
                        self.wait_timer(id, n(1000.), evaluation)?;
                    } else {
                        *phase = 3;
                        self.child(
                            id,
                            Frame::Sync {
                                restore: V::Undefined,
                                phase: 0,
                                lag: V::Undefined,
                            },
                        )?;
                    }
                    Ok(None)
                }
                3 => Ok(Some(self.child_value(id)?)),
                _ => Err(AdvanceError::Stop(SourceRuleError::Backend)),
            },
            Frame::Recovery { phase, saved, .. } => match *phase {
                0 => {
                    if truth(&call(Op::NetworkStatus)?.get("status")) {
                        *phase = 1;
                    } else {
                        self.wait_timer(id, n(10000.), evaluation)?;
                    }
                    Ok(None)
                }
                1 => {
                    *saved = call(Op::HeightRead("TrackHeight"))?;
                    *phase = 2;
                    Ok(None)
                }
                2 => {
                    call(Op::HeightRead("MaxProcessedHeight"))?;
                    Ok(Some(saved.clone()))
                }
                _ => Err(AdvanceError::Stop(SourceRuleError::Backend)),
            },
            Frame::Fetch {
                height,
                retries,
                delay,
                attempt,
                phase,
            } => {
                if *phase == 1 {
                    *attempt = attempt
                        .checked_add(1)
                        .ok_or(AdvanceError::Stop(SourceRuleError::Capacity))?;
                    *phase = 0;
                }
                if !truth(&call(Op::LessEqual(n(*attempt as f64), retries.clone()))?) {
                    return Ok(Some(V::Undefined));
                }
                match call(Op::FetchBlock(height.clone())) {
                    Ok(block) => Ok(Some(block)),
                    Err(AdvanceError::Throw(error)) => {
                        if !strict(&error.code, &s("ETIMEDOUT"))
                            || strict(retries, &n(*attempt as f64))
                        {
                            return Err(AdvanceError::Throw(error));
                        }
                        let backoff = call(Op::Backoff {
                            delay: delay.clone(),
                            attempt: *attempt,
                        })?;
                        *phase = 1;
                        self.wait_timer(id, backoff, evaluation)?;
                        Ok(None)
                    }
                    Err(e) => Err(e),
                }
            }
            Frame::Incoming {
                latest,
                tip,
                block,
                number,
                phase,
            } => match *phase {
                0 => {
                    *tip = call(Op::BlockCount)?;
                    *phase = 1;
                    Ok(None)
                }
                1 => {
                    let track = call(Op::HeightRead("TrackHeight"))?;
                    if truth(&call(Op::Greater(track.clone(), latest.clone()))?) {
                        *latest = track;
                    }
                    *number = call(Op::Add(latest.clone(), n(1.)))?;
                    *phase = 2;
                    Ok(None)
                }
                2 => {
                    *phase = if truth(&call(Op::LessEqual(number.clone(), tip.clone()))?) {
                        3
                    } else {
                        8
                    };
                    Ok(None)
                }
                3 => {
                    if truth(&call(Op::NetworkStatus)?.get("status")) {
                        *phase = 5;
                    } else {
                        *phase = 4;
                        self.child(
                            id,
                            Frame::Recovery {
                                latest: latest.clone(),
                                track: number.clone(),
                                phase: 0,
                                saved: V::Undefined,
                            },
                        )?;
                    }
                    Ok(None)
                }
                4 => {
                    *number = self.child_value(id)?;
                    *phase = 5;
                    Ok(None)
                }
                5 => {
                    *phase = 6;
                    self.child(
                        id,
                        Frame::Fetch {
                            height: number.clone(),
                            retries: n(3.),
                            delay: n(1000.),
                            attempt: 0,
                            phase: 0,
                        },
                    )?;
                    Ok(None)
                }
                6 => {
                    *block = self.child_value(id)?;
                    *phase = 7;
                    Ok(None)
                }
                7 if self.file_retries => {
                    *phase = 12;
                    self.child(
                        id,
                        Frame::File(Box::new(FileStepFrame::block(
                            block.clone(),
                            number.clone(),
                        ))),
                    )?;
                    Ok(None)
                }
                12 => {
                    let result = self.child_value(id)?;
                    if strict(&result.get("reOrg"), &V::Bool(true)) {
                        *phase = 11;
                        self.child(
                            id,
                            Frame::Sync {
                                restore: result.get("restore"),
                                phase: 0,
                                lag: V::Undefined,
                            },
                        )?;
                    } else {
                        *phase = 9;
                    }
                    Ok(None)
                }
                7 => {
                    let result = call(Op::ProcessBlock {
                        block: block.clone(),
                        height: number.clone(),
                    })?;
                    if strict(&result.get("reOrg"), &V::Bool(true)) {
                        *phase = 11;
                        self.child(
                            id,
                            Frame::Sync {
                                restore: result.get("restore"),
                                phase: 0,
                                lag: V::Undefined,
                            },
                        )?;
                    } else {
                        *phase = 9;
                    }
                    Ok(None)
                }
                8 => {
                    if truth(&call(Op::PauseRead)?) {
                        *phase = 11;
                        self.child(
                            id,
                            Frame::Sync {
                                restore: V::Undefined,
                                phase: 0,
                                lag: V::Undefined,
                            },
                        )?;
                    } else {
                        *phase = 10;
                        self.wait_timer(id, n(10000.), evaluation)?;
                    }
                    Ok(None)
                }
                9 => {
                    call(Op::HeightWrite {
                        name: "TrackHeight",
                        value: number.clone(),
                    })?;
                    let numeric = call(Op::Number(number.clone()))?;
                    *number = call(Op::Add(numeric, n(1.)))?;
                    *phase = 2;
                    Ok(None)
                }
                10 => {
                    call(Op::HeightWrite {
                        name: "TrackHeight",
                        value: tip.clone(),
                    })?;
                    *phase = 0;
                    Ok(None)
                }
                11 => Ok(Some(self.child_value(id)?)),
                _ => Err(AdvanceError::Stop(SourceRuleError::Backend)),
            },
        }
    }

    pub(crate) fn apply(
        &mut self,
        owner: &mut impl MainStepOwner,
        command: MainStepCommandV1,
    ) -> Result<MainStepEvaluationV1, MainStepError> {
        self.apply_with_drain_bound(owner, command, None)
    }
    fn apply_with_drain_bound(
        &mut self,
        owner: &mut impl MainStepOwner,
        command: MainStepCommandV1,
        drain_bound: Option<usize>,
    ) -> Result<MainStepEvaluationV1, MainStepError> {
        // A typed source-await controller is the sole driver of its new
        // Promise domain. Old generic commands cannot silently reinterpret it.
        // Literal shutdown retains its unconditional no-drain behavior.
        if self.startup_source_profile_active()
            && !matches!(command, MainStepCommandV1::Shutdown(_))
        {
            return Err(MainStepError::InvalidInput);
        }
        if drain_bound.is_some_and(|bound| bound > MAX_STEPS) {
            return Err(MainStepError::InvalidInput);
        }
        if self.terminal.is_some() {
            return Err(MainStepError::AlreadyExited);
        }
        if self.owner_stopped.is_some() && !matches!(command, MainStepCommandV1::Shutdown(_)) {
            return Err(MainStepError::OwnerStopped);
        }
        if (!self.ready.is_empty() || !self.pending_unhandled.is_empty())
            && !matches!(
                command,
                MainStepCommandV1::DrainReady { .. } | MainStepCommandV1::Shutdown(_)
            )
        {
            return Err(MainStepError::NotQuiescent);
        }
        let mut evaluation = MainStepEvaluationV1 {
            started: None,
            stop: MainStepStop::Quiescent,
            trace: vec![],
            effects: vec![],
            completed: vec![],
        };
        let mut limit = MAX_STEPS;
        match command {
            MainStepCommandV1::Start { method, arguments } => {
                let frame = self.frame(&method, &arguments)?;
                evaluation.started = Some(self.spawn(frame, None, true)?);
            }
            MainStepCommandV1::DrainReady { limit: bound } => {
                if bound == 0 || bound > MAX_STEPS {
                    return Err(MainStepError::InvalidInput);
                }
                limit = bound;
            }
            MainStepCommandV1::FireTimer { id, generation } => {
                if generation != self.generation {
                    return Err(MainStepError::WrongGeneration);
                }
                let at = self
                    .timers
                    .iter()
                    .position(|t| t.id == id && t.status == TimerStatus::Scheduled)
                    .ok_or(MainStepError::TimerNotScheduled)?;
                if self.next_timer() != Some(id) {
                    return Err(MainStepError::TimerNotNext);
                }
                let job = self.timers[at].job;
                let timer = self.timers.remove(at);
                self.logical_time = timer.due;
                let at = self.position(job);
                assert_eq!(self.jobs[at].status, MainStepJobStatus::WaitingTimer(id));
                self.jobs[at].status = MainStepJobStatus::Ready;
                self.ready.push_back(job);
                evaluation.effects.push(obj(&[
                    ("kind", s("timer-fired")),
                    ("id", n(id as f64)),
                    ("logicalTime", self.logical_time.clone()),
                ]));
            }
            MainStepCommandV1::ClearTimer { id, generation } => {
                if generation != self.generation {
                    return Err(MainStepError::WrongGeneration);
                }
                if let Some(timer) = self
                    .timers
                    .iter_mut()
                    .find(|t| t.id == id && t.status == TimerStatus::Scheduled)
                {
                    timer.status = TimerStatus::Cancelled;
                    evaluation
                        .effects
                        .push(obj(&[("kind", s("timer-cancelled")), ("id", n(id as f64))]));
                }
            }
            MainStepCommandV1::UpdateRecordedIo(value) => {
                value.validate().map_err(|_| MainStepError::InvalidInput)?;
                if let Err(fault) = Self::operation(
                    owner,
                    0,
                    "RecordedIOFactsUpdate",
                    MainStepPrimitive::UpdateIo(value),
                    &mut evaluation,
                ) {
                    return Ok(self.stop_on_fault(fault, evaluation));
                }
            }
            MainStepCommandV1::Shutdown(arguments) => {
                if arguments.len() > 8 || arguments.iter().any(|a| a.validate().is_err()) {
                    return Err(MainStepError::InvalidInput);
                }
                // Literal pinned shutdown has no receiver use, await or DB save.
                for message in ["Saving state to database...", "Shutdown completed."] {
                    evaluation.effects.push(obj(&[
                        ("kind", s("console")),
                        ("level", s("log")),
                        ("values", V::Array(vec![s(message)])),
                    ]));
                }
                evaluation
                    .effects
                    .push(obj(&[("kind", s("process-exit")), ("code", n(0.))]));
                self.terminal = Some(0);
                evaluation.stop = MainStepStop::ProcessExited(0);
                return Ok(evaluation);
            }
        }
        if let Some(bound) = drain_bound {
            limit = limit.min(bound);
        }
        for _ in 0..limit {
            let Some(id) = self.ready.pop_front() else {
                if let Some((job, error)) = self.pending_unhandled.pop_front() {
                    if let Err(fault) = Self::operation(
                        owner,
                        job,
                        "UnhandledRejectionCheckpoint",
                        MainStepPrimitive::PublishUnhandled(error),
                        &mut evaluation,
                    ) {
                        return Ok(self.stop_on_fault(fault, evaluation));
                    }
                    continue;
                }
                self.prune_completed();
                return Ok(evaluation);
            };
            let at = self.position(id);
            if self.jobs[at].status != MainStepJobStatus::Ready {
                self.owner_stopped = Some(SourceRuleError::Backend);
                evaluation.stop = MainStepStop::SourceOwnerStopped(SourceRuleError::Backend);
                return Ok(evaluation);
            }
            let mut frame = self.jobs[at].frame.clone();
            if let Frame::Finish(result) = frame {
                self.complete(id, result, &mut evaluation);
                continue;
            }
            match self.advance(id, &mut frame, owner, &mut evaluation) {
                Ok(Some(value)) => {
                    self.jobs[at].frame = Frame::Finish(MainStepSourceResult::Fulfilled(value));
                    self.ready.push_back(id);
                }
                Ok(None) => {
                    self.jobs[at].frame = frame;
                    if self.jobs[at].status == MainStepJobStatus::Ready {
                        self.ready.push_back(id);
                    }
                }
                Err(AdvanceError::Throw(error)) => {
                    self.jobs[at].frame = Frame::Finish(MainStepSourceResult::Thrown(error));
                    self.ready.push_back(id);
                }
                Err(fault) => {
                    self.jobs[at].frame = frame;
                    return Ok(self.stop_on_fault(fault, evaluation));
                }
            }
        }
        self.prune_completed();
        evaluation.stop = if self.ready.is_empty() && self.pending_unhandled.is_empty() {
            MainStepStop::Quiescent
        } else {
            MainStepStop::DrainBound
        };
        Ok(evaluation)
    }
    fn stop_on_fault(
        &mut self,
        fault: AdvanceError,
        mut evaluation: MainStepEvaluationV1,
    ) -> MainStepEvaluationV1 {
        let error = match fault {
            AdvanceError::Stop(error) => error,
            AdvanceError::Throw(_) => SourceRuleError::Backend,
        };
        self.owner_stopped = Some(error.clone());
        evaluation.stop = MainStepStop::SourceOwnerStopped(error);
        evaluation
    }
    fn prune_completed(&mut self) {
        // Completed results live in immutable step receipts. Awaiting parents
        // already hold their actual child value. Do not accumulate dead jobs
        // or fired timers as an artificial long-running service ceiling.
        self.jobs
            .retain(|j| !matches!(j.status, MainStepJobStatus::Completed(_)));
    }
}

impl Frame {
    fn phase(&self) -> String {
        match self {
            Self::File(frame) => frame.phase_label(),
            Self::Finish(_) => "PromiseResolution".into(),
            Self::Atomic(c) => c.target.clone(),
            Self::Delay { phase, .. } => format!("Main.delay/{phase}"),
            Self::Sync { phase, .. } => format!("Main.syncIfNecessary/{phase}"),
            Self::Recovery { phase, .. } => format!("Main.enterRecoveryMode/{phase}"),
            Self::Incoming { phase, .. } => format!("Main.processIncomingBlocks/{phase}"),
            Self::Fetch { phase, .. } => format!("Main.fetchWithRetry/{phase}"),
        }
    }
    fn value(&self) -> V {
        let fields = match self {
            Self::File(frame) => return frame.value(),
            Self::Finish(result) => vec![("kind", s("Finish")), ("result", result_value(result))],
            Self::Atomic(c) => vec![
                ("kind", s("Atomic")),
                ("target", s(&c.target)),
                ("arguments", V::Array(c.arguments.clone())),
            ],
            Self::Delay { millis, phase } => vec![
                ("kind", s("Delay")),
                ("millis", millis.clone()),
                ("phase", n(*phase as f64)),
            ],
            Self::Sync {
                restore,
                phase,
                lag,
            } => vec![
                ("kind", s("Sync")),
                ("restore", restore.clone()),
                ("phase", n(*phase as f64)),
                ("lag", lag.clone()),
            ],
            Self::Recovery {
                latest,
                track,
                phase,
                saved,
            } => vec![
                ("kind", s("Recovery")),
                ("latest", latest.clone()),
                ("track", track.clone()),
                ("phase", n(*phase as f64)),
                ("saved", saved.clone()),
            ],
            Self::Incoming {
                latest,
                tip,
                block,
                number,
                phase,
            } => vec![
                ("kind", s("Incoming")),
                ("latest", latest.clone()),
                ("tip", tip.clone()),
                ("block", block.clone()),
                ("number", number.clone()),
                ("phase", n(*phase as f64)),
            ],
            Self::Fetch {
                height,
                retries,
                delay,
                attempt,
                phase,
            } => vec![
                ("kind", s("Fetch")),
                ("height", height.clone()),
                ("retries", retries.clone()),
                ("delay", delay.clone()),
                ("attempt", s(attempt.to_string())),
                ("phase", n(*phase as f64)),
            ],
        };
        obj(&fields)
    }
}

impl MainStepKernelV1 {
    /// Diagnostic output only. No state-import API exists: restart must
    /// reexecute independently bound source/control commands from its foundation.
    pub fn value(&self) -> V {
        let mut fields = vec![
            ("profile", s(self.source_profile())),
            ("source", s(MAIN_SOURCE_SHA256)),
            ("generation", s(self.generation.to_string())),
            ("nextJob", s(self.next_job.to_string())),
            ("nextTimer", s(self.next_timer.to_string())),
            ("logicalTime", self.logical_time.clone()),
            (
                "jobs",
                V::Array(
                    self.jobs
                        .iter()
                        .map(|j| {
                            obj(&[
                                ("id", s(j.id.to_string())),
                                ("callerOwned", V::Bool(j.caller_owned)),
                                ("parent", j.parent.map_or(V::Null, |p| s(p.to_string()))),
                                (
                                    "status",
                                    match &j.status {
                                        MainStepJobStatus::Ready => s("Ready"),
                                        MainStepJobStatus::WaitingTimer(t) => {
                                            obj(&[("waitingTimer", s(t.to_string()))])
                                        }
                                        MainStepJobStatus::WaitingChild(c) => {
                                            obj(&[("waitingChild", s(c.to_string()))])
                                        }
                                        MainStepJobStatus::Completed(r) => result_value(r),
                                    },
                                ),
                                ("frame", j.frame.value()),
                                (
                                    "childResult",
                                    j.child_result.as_ref().map_or(V::Null, result_value),
                                ),
                            ])
                        })
                        .collect(),
                ),
            ),
            (
                "ready",
                V::Array(self.ready.iter().map(|id| s(id.to_string())).collect()),
            ),
            (
                "timers",
                V::Array(
                    self.timers
                        .iter()
                        .map(|t| {
                            obj(&[
                                ("id", s(t.id.to_string())),
                                ("generation", s(t.generation.to_string())),
                                ("job", s(t.job.to_string())),
                                ("due", t.due.clone()),
                                ("delay", t.delay.clone()),
                                (
                                    "status",
                                    s(match t.status {
                                        TimerStatus::Scheduled => "Scheduled",
                                        TimerStatus::Cancelled => "Cancelled",
                                    }),
                                ),
                            ])
                        })
                        .collect(),
                ),
            ),
            (
                "pendingUnhandled",
                V::Array(
                    self.pending_unhandled
                        .iter()
                        .map(|(id, e)| {
                            obj(&[("job", s(id.to_string())), ("error", throw_value(e))])
                        })
                        .collect(),
                ),
            ),
            (
                "ownerStopped",
                self.owner_stopped
                    .as_ref()
                    .map_or(V::Null, |e| s(e.to_string())),
            ),
            (
                "terminal",
                self.terminal.map_or(V::Null, |code| n(code as f64)),
            ),
        ];
        if self.startup_source_profile_active() {
            fields.push(("startupPromisesRequireCanonicalFields", V::Bool(true)));
        }
        obj(&fields)
    }
}
impl MainStepEvaluationV1 {
    pub fn value(&self) -> V {
        obj(&[
            (
                "started",
                self.started.map_or(V::Null, |id| s(id.to_string())),
            ),
            (
                "stop",
                match &self.stop {
                    MainStepStop::Quiescent => s("Quiescent"),
                    MainStepStop::DrainBound => s("DrainBound"),
                    MainStepStop::SourceOwnerStopped(e) => {
                        obj(&[("ownerStopped", s(e.to_string()))])
                    }
                    MainStepStop::ProcessExited(code) => obj(&[("processExited", n(*code as f64))]),
                },
            ),
            (
                "trace",
                V::Array(
                    self.trace
                        .iter()
                        .map(|t| {
                            obj(&[
                                ("job", s(t.job.to_string())),
                                ("phase", s(&t.phase)),
                                ("operation", t.operation.clone()),
                                ("outcome", t.outcome.clone()),
                                ("effects", V::Array(t.effects.clone())),
                            ])
                        })
                        .collect(),
                ),
            ),
            ("effects", V::Array(self.effects.clone())),
            (
                "completed",
                V::Array(
                    self.completed
                        .iter()
                        .map(|(id, r)| {
                            obj(&[("job", s(id.to_string())), ("result", result_value(r))])
                        })
                        .collect(),
                ),
            ),
        ])
    }
}
impl MainStepCommandV1 {
    pub fn value(&self) -> V {
        match self {
            Self::Start { method, arguments } => obj(&[
                ("kind", s("Start")),
                ("method", s(method)),
                ("arguments", V::Array(arguments.clone())),
            ]),
            Self::DrainReady { limit } => {
                obj(&[("kind", s("DrainReady")), ("limit", s(limit.to_string()))])
            }
            Self::FireTimer { id, generation } => obj(&[
                ("kind", s("FireTimer")),
                ("id", s(id.to_string())),
                ("generation", s(generation.to_string())),
            ]),
            Self::ClearTimer { id, generation } => obj(&[
                ("kind", s("ClearTimer")),
                ("id", s(id.to_string())),
                ("generation", s(generation.to_string())),
            ]),
            Self::UpdateRecordedIo(v) => {
                obj(&[("kind", s("UpdateRecordedIo")), ("input", v.clone())])
            }
            Self::Shutdown(a) => {
                obj(&[("kind", s("Shutdown")), ("arguments", V::Array(a.clone()))])
            }
        }
    }
    pub fn from_value(v: &V) -> Result<Self, MainStepError> {
        v.validate().map_err(|_| MainStepError::InvalidInput)?;
        fn text(v: V) -> Result<String, MainStepError> {
            if let V::String(s) = v {
                Ok(s)
            } else {
                Err(MainStepError::InvalidInput)
            }
        }
        fn uint(v: V) -> Result<u64, MainStepError> {
            text(v)?.parse().map_err(|_| MainStepError::InvalidInput)
        }
        fn array(v: V) -> Result<Vec<V>, MainStepError> {
            if let V::Array(a) = v {
                Ok(a)
            } else {
                Err(MainStepError::InvalidInput)
            }
        }
        let command = match text(v.get("kind"))?.as_str() {
            "Start" => Self::Start {
                method: text(v.get("method"))?,
                arguments: array(v.get("arguments"))?,
            },
            "DrainReady" => Self::DrainReady {
                limit: usize::try_from(uint(v.get("limit"))?)
                    .map_err(|_| MainStepError::InvalidInput)?,
            },
            "FireTimer" => Self::FireTimer {
                id: uint(v.get("id"))?,
                generation: uint(v.get("generation"))?,
            },
            "ClearTimer" => Self::ClearTimer {
                id: uint(v.get("id"))?,
                generation: uint(v.get("generation"))?,
            },
            "UpdateRecordedIo" => Self::UpdateRecordedIo(v.get("input")),
            "Shutdown" => Self::Shutdown(array(v.get("arguments"))?),
            _ => return Err(MainStepError::InvalidInput),
        };
        if command.value() != *v {
            return Err(MainStepError::InvalidInput);
        }
        Ok(command)
    }
}

include!("source_main_step_fields_impl.rs");
include!("source_main_step_file_kernel_impl.rs");

include!("source_main_service_kernel_impl.rs");
include!("source_main_automatic_file_kernel_impl.rs");
include!("source_main_automatic_parallel_kernel_impl.rs");
#[cfg(test)]
#[path = "source_main_automatic_file_kernel_tests.rs"]
mod automatic_file_kernel_tests;
#[cfg(test)]
#[path = "source_main_automatic_parallel_kernel_tests.rs"]
mod automatic_parallel_kernel_tests;

include!("source_main_step_startup_clock_impl.rs");

include!("source_main_startup_nested_kernel_impl.rs");

#[cfg(test)]
#[path = "source_main_startup_nested_kernel_tests.rs"]
mod startup_nested_kernel_tests;

#[cfg(test)]
#[path = "source_main_startup_nedb_jobs.rs"]
pub(crate) mod startup_source_nedb_jobs;
#[cfg(test)]
#[path = "source_main_startup_promise_queue.rs"]
pub(crate) mod startup_source_promise_queue;
#[cfg(test)]
include!("source_main_startup_source_kernel_impl.rs");
include!("source_main_startup_source_kernel_gate_impl.rs");
