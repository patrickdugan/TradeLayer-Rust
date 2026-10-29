// Parent-buildable actual Core/private-bridge tests. Expected prefixes are
// independently executed unchanged Main controls, not callback answers.
use super::*;
use crate::source_compact_index_v2::SourceCompactIndex;
use crate::source_main_step_kernel::{
    MainStepCommandV1 as Command, MainStepEvaluationV1, MainStepJobStatus, MainStepKernelV1,
    MainStepSourceResult, MainStepStop,
};
use crate::storage::Namespace;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
static SERIAL: AtomicU64 = AtomicU64::new(0);
struct Temp {
    root: PathBuf,
    parent: PathBuf,
}
impl Temp {
    fn new() -> Self {
        let parent = std::env::var_os("MAIN_STEP_TMP")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        std::fs::create_dir_all(&parent).unwrap();
        let parent = parent.canonicalize().unwrap();
        let root = parent.join(format!(
            "actual-step-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        Self { root, parent }
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        if let Ok(root) = self.root.canonicalize() {
            assert_eq!(root.parent(), Some(self.parent.as_path()));
            let _ = std::fs::remove_dir_all(root);
        }
    }
}
struct Driver {
    runtime: SourceContractRuntime,
    index: SourceCompactIndex,
    kernel: MainStepKernelV1,
}
impl Driver {
    fn new(root: &Path, io: V, file_failures: Vec<V>) -> Self {
        let mut runtime =
            SourceContractRuntime::with_policy(SourceContractRuntimePolicy::MainOrchestrationV12);
        for (target, arguments) in [
            (
                "SourceActivation.configureClient",
                vec![obj(&[("chain", s("LTC")), ("test", V::Bool(false))])],
            ),
            ("Activation.init", vec![s("LTC")]),
            (
                "SourceMain.configureProduction",
                vec![obj(&[
                    (
                        "client",
                        obj(&[("chain", s("LTC")), ("test", V::Bool(false))]),
                    ),
                    ("io", io),
                    ("genesisBlock", n(10.)),
                ])],
            ),
            (
                "SourceMain.configureService",
                vec![obj(&[
                    ("fileInputs", V::Array(vec![])),
                    ("directoryInputs", V::Array(vec![])),
                    (
                        "fileIOInputs",
                        obj(&[
                            ("failures", V::Array(file_failures)),
                            ("mtimes", V::Array(vec![])),
                        ]),
                    ),
                ])],
            ),
        ] {
            assert!(
                matches!(
                    runtime
                        .invoke_dependency(&DependencyCall {
                            target: target.into(),
                            arguments
                        })
                        .unwrap(),
                    DependencyReturn::Immediate(_) | DependencyReturn::Pending(_)
                ),
                "{target}"
            );
            runtime.take_effects();
        }
        runtime.set_height(n(0.)).unwrap();
        let mut index = SourceCompactIndex::create(
            root,
            Namespace::new("LTC", false, "txIndex").unwrap(),
            "actual-suspended-main-component",
        )
        .unwrap();
        index
            .import_row(
                &index.snapshot().unwrap(),
                obj(&[("_id", s("MaxHeight")), ("value", n(0.))]),
            )
            .unwrap();
        runtime
            .native_main_bind(obj(&[("profile", s("test-only-actual-loan"))]))
            .unwrap();
        Self {
            runtime,
            index,
            kernel: MainStepKernelV1::new(17),
        }
    }
    fn apply(&mut self, command: Command) -> MainStepEvaluationV1 {
        let actual =
            self.runtime
                .native_main_step_apply(&mut self.index, &mut self.kernel, command);
        assert_eq!(actual.storage_stop, None);
        assert_eq!(
            actual.consumed_rows, 0,
            "these controls contain no indexed TL rows"
        );
        assert!(
            actual.validity.is_empty(),
            "these controls perform no TxIndex validity writes"
        );
        let evaluation = actual.evaluation.unwrap();
        assert_eq!(evaluation.stop, MainStepStop::Quiescent, "{evaluation:?}");
        assert!(
            self.runtime.collection("txIndex").is_empty(),
            "no mirrored metadata authority"
        );
        evaluation
    }
    fn start(&mut self, target: &str, arguments: Vec<V>) -> MainStepEvaluationV1 {
        self.apply(Command::Start {
            method: target.into(),
            arguments,
        })
    }
    fn fire(&mut self) -> MainStepEvaluationV1 {
        self.apply(Command::FireTimer {
            id: self.kernel.next_timer().unwrap(),
            generation: 17,
        })
    }
    fn track(&self) -> V {
        self.runtime
            .collection("consensus")
            .iter()
            .find(|row| row.get("_id") == s("TrackHeight"))
            .map_or(V::Undefined, |row| row.get("value"))
    }
}
fn io(height: f64, active: bool, blocks: Vec<V>) -> V {
    obj(&[
        ("height", n(height)),
        (
            "networkInfo",
            obj(&[
                ("networkactive", V::Bool(active)),
                ("connections", n(if active { 2. } else { 0. })),
            ]),
        ),
        ("blocks", V::Array(blocks)),
    ])
}
fn returned_for(e: &MainStepEvaluationV1, id: u64, value: V) {
    assert!(
        e.completed
            .iter()
            .any(|(job, result)| *job == id
                && result == &MainStepSourceResult::Fulfilled(value.clone())),
        "{e:?}"
    );
}
fn returned(e: &MainStepEvaluationV1, value: V) {
    returned_for(e, e.started.expect("caller-owned Start receipt"), value)
}
#[test]
fn unchanged_source_first_ten_pause_recovery_and_concurrent_height_controls() {
    // Original-source oracle: frozen-controls 38246d70, proof16 observations.
    let a = Temp::new();
    let mut d = Driver::new(&a.root, io(0., true, vec![]), vec![]);
    returned(&d.start("Main.setPause", vec![]), V::Bool(true)); // 1
    let sync = d.start("Main.syncIfNecessary", vec![]); // 2
    let sync_id = sync.started.unwrap();
    assert!(sync.completed.is_empty());
    assert!(matches!(
        d.kernel.jobs().find(|(id, _)| *id == sync_id).unwrap().1,
        MainStepJobStatus::WaitingTimer(_)
    ));
    assert!(d.fire().completed.is_empty());
    returned(&d.start("Main.setPause", vec![]), V::Bool(false)); // 3
    returned_for(&d.fire(), sync_id, V::Undefined);
    returned(&d.start("Main.saveTrackHeight", vec![n(5.)]), V::Undefined); // 4
    returned(
        &d.start("Main.checkBlockLag", vec![]),
        obj(&[("lag", n(-5.)), ("chainTip", n(0.)), ("maxTrack", n(5.))]),
    ); // 5

    let b = Temp::new();
    let mut d = Driver::new(&b.root, io(0., false, vec![]), vec![]);
    let recovery = d.start("Main.enterRecoveryMode", vec![n(0.), n(1.)]); // 6
    assert!(recovery.completed.is_empty());
    returned(&d.start("Main.saveTrackHeight", vec![n(7.)]), V::Undefined); // 7
    assert!(d
        .start("Main.processIncomingBlocks", vec![n(0.), n(7.), n(0.)])
        .completed
        .is_empty()); // 8
    returned(&d.start("Main.setPause", vec![]), V::Bool(true)); // 9
    d.apply(Command::UpdateRecordedIo(obj(&[(
        "networkInfo",
        obj(&[("networkactive", V::Bool(true)), ("connections", n(2.))]),
    )])));
    returned_for(&d.fire(), recovery.started.unwrap(), n(7.));
    returned(&d.start("Main.loadTrackHeight", vec![]), n(7.)); // 10
    assert!(d.kernel.next_timer().is_some());
}

#[test]
fn actual_empty_block_retains_stale_tip_and_cancelled_wait_stays_pending() {
    let t = Temp::new();
    let mut d = Driver::new(
        &t.root,
        io(
            1.,
            true,
            vec![obj(&[
                ("height", n(1.)),
                ("hash", s("h1")),
                (
                    "block",
                    obj(&[
                        ("hash", s("h1")),
                        ("previousblockhash", s("h0")),
                        ("tx", V::Array(vec![])),
                    ]),
                ),
            ])],
        ),
        vec![],
    );
    let incoming = d.start("Main.processIncomingBlocks", vec![n(1.), n(0.), n(1.)]);
    let caller = incoming.started.unwrap();
    assert_eq!(caller, 1);
    assert!(
        !incoming.completed.iter().any(|(id, _)| *id == caller),
        "caller promise stays pending; child completions remain visible"
    );
    assert!(matches!(
        d.kernel.jobs().find(|(id, _)| *id == caller).unwrap().1,
        MainStepJobStatus::WaitingTimer(_)
    ));
    let fetch_job = incoming
        .trace
        .iter()
        .find(|trace| trace.operation.get("operation") == s("FetchBlock"))
        .unwrap()
        .job;
    assert_eq!(fetch_job, 2);
    assert_eq!(
        incoming.completed,
        vec![(
            fetch_job,
            MainStepSourceResult::Fulfilled(obj(&[
                ("hash", s("h1")),
                ("previousblockhash", s("h0")),
                ("tx", V::Array(vec![]))
            ]))
        )],
        "retain the completed internal fetch and exact independently supplied block bytes"
    );
    assert_eq!(d.track(), n(1.));
    assert!(incoming
        .effects
        .iter()
        .any(|e| e.get("kind") == s("main-phase")
            && e.get("target") == s("Channels.processWithdrawals")));
    d.start("Main.saveTrackHeight", vec![n(9.)]);
    d.start("Main.setPause", vec![]);
    let first_wake = d.fire();
    assert!(!first_wake.completed.iter().any(|(id, _)| *id == caller));
    assert!(matches!(
        d.kernel.jobs().find(|(id, _)| *id == caller).unwrap().1,
        MainStepJobStatus::WaitingChild(_)
    ));
    assert_eq!(
        d.track(),
        n(1.),
        "actual source retained tip overwrites the concurrent write"
    );
    d.start("Main.setPause", vec![]);
    returned_for(&d.fire(), caller, V::Undefined);
    let timer = d.kernel.next_timer().unwrap();
    let waiting = d
        .kernel
        .jobs()
        .find_map(|(id, status)| match status {
            MainStepJobStatus::WaitingTimer(handle) if *handle == timer => Some(id),
            _ => None,
        })
        .unwrap();
    assert_ne!(waiting, caller);
    let cancel = d.apply(Command::ClearTimer {
        id: timer,
        generation: 17,
    });
    assert!(cancel.completed.is_empty());
    assert!(
        matches!(d.kernel.jobs().find(|(id,_)|*id==waiting).unwrap().1,MainStepJobStatus::WaitingTimer(handle) if *handle==timer)
    );
    assert!(
        d.kernel.jobs().all(|(id, _)| id != caller),
        "completed caller is not resurrected by callback cancellation"
    );
    assert_eq!(d.kernel.next_timer(), None);
}

#[test]
fn snapshot_retry_is_preflight_stop_before_financial_or_native_publication() {
    let t = Temp::new();
    let mut d = Driver::new(
        &t.root,
        io(1000., true, vec![]),
        vec![obj(&[
            ("method", s("copyFile")),
            ("ordinal", n(1.)),
            ("code", s("EBUSY")),
            ("message", s("recorded busy copy")),
        ])],
    );
    let before = d.runtime.native_main_state_value().unwrap();
    let pin = d.index.snapshot().unwrap();
    let mut validity = vec![];
    let mut consumed = 0;
    let mut storage_stop = None;
    let loan = NativeMainStepOwnerLoan {
        runtime: &mut d.runtime,
        index: &mut d.index,
        validity: &mut validity,
        consumed_rows: &mut consumed,
        storage_stop: &mut storage_stop,
    };
    use crate::source_main_step_kernel::MainStepOwner;
    assert_eq!(
        loan.preflight(
            &crate::source_main_step_kernel::MainStepPrimitive::ProcessBlock {
                block: obj(&[("hash", s("h1000")), ("tx", V::Array(vec![]))]),
                height: n(1000.)
            }
        ),
        Err(SourceRuleError::UnknownDependency)
    );
    assert_eq!(d.runtime.native_main_state_value().unwrap(), before);
    assert_eq!(d.index.snapshot().unwrap(), pin);
}
struct BudgetedActualLoan<'a> {
    actual: NativeMainStepOwnerLoan<'a>,
    remaining: usize,
    callbacks: std::cell::Cell<usize>,
}
impl crate::source_main_step_kernel::sealed::Sealed for BudgetedActualLoan<'_> {}
impl crate::source_main_step_kernel::MainStepOwner for BudgetedActualLoan<'_> {
    fn preflight(
        &self,
        op: &crate::source_main_step_kernel::MainStepPrimitive,
    ) -> Result<(), SourceRuleError> {
        self.callbacks.set(self.callbacks.get() + 1);
        let charged = matches!(
            op,
            crate::source_main_step_kernel::MainStepPrimitive::Atomic(_)
                | crate::source_main_step_kernel::MainStepPrimitive::BlockCount
        );
        if charged && self.remaining == 0 {
            return Err(SourceRuleError::Capacity);
        }
        crate::source_main_step_kernel::MainStepOwner::preflight(&self.actual, op)
    }
    fn execute(
        &mut self,
        op: &crate::source_main_step_kernel::MainStepPrimitive,
    ) -> crate::source_main_step_kernel::MainStepPrimitiveExecution {
        self.callbacks.set(self.callbacks.get() + 1);
        if matches!(
            op,
            crate::source_main_step_kernel::MainStepPrimitive::Atomic(_)
                | crate::source_main_step_kernel::MainStepPrimitive::BlockCount
        ) {
            self.remaining -= 1;
        }
        crate::source_main_step_kernel::MainStepOwner::execute(&mut self.actual, op)
    }
}
#[test]
fn unconditional_shutdown_after_a_real_prefix_and_sealed_local_budget_stop_never_drains_owner() {
    let temp = Temp::new();
    let mut d = Driver::new(&temp.root, io(0., true, vec![]), vec![]);
    let mut validity = vec![];
    let mut consumed_rows = 0;
    let mut storage_stop = None;
    {
        let mut loan = BudgetedActualLoan {
            actual: NativeMainStepOwnerLoan {
                runtime: &mut d.runtime,
                index: &mut d.index,
                validity: &mut validity,
                consumed_rows: &mut consumed_rows,
                storage_stop: &mut storage_stop,
            },
            remaining: 1,
            callbacks: std::cell::Cell::new(0),
        };
        let stopped = d
            .kernel
            .apply(
                &mut loan,
                Command::Start {
                    method: "Main.syncIfNecessary".into(),
                    arguments: vec![],
                },
            )
            .unwrap();
        assert_eq!(
            stopped.stop,
            MainStepStop::SourceOwnerStopped(SourceRuleError::Capacity)
        );
        assert!(stopped
            .trace
            .iter()
            .any(|t| t.phase == "Main.checkBlockLag" || t.phase == "Main.syncIfNecessary/0"));
    }
    assert!(matches!(d.kernel.value().get("ready"), V::Array(ref rows) if !rows.is_empty()));
    let before = d.runtime.native_main_state_value().unwrap();
    let pin = d.index.snapshot().unwrap();
    let shutdown = {
        let mut loan = BudgetedActualLoan {
            actual: NativeMainStepOwnerLoan {
                runtime: &mut d.runtime,
                index: &mut d.index,
                validity: &mut validity,
                consumed_rows: &mut consumed_rows,
                storage_stop: &mut storage_stop,
            },
            remaining: 0,
            callbacks: std::cell::Cell::new(0),
        };
        let shutdown = d
            .kernel
            .apply(&mut loan, Command::Shutdown(vec![]))
            .unwrap();
        assert_eq!(
            loan.callbacks.get(),
            0,
            "no primitive, post-call drain, or save after shutdown"
        );
        shutdown
    };
    assert_eq!(shutdown.stop, MainStepStop::ProcessExited(0));
    assert!(shutdown.trace.is_empty());
    assert!(shutdown.completed.is_empty());
    assert_eq!(shutdown.effects.len(), 3);
    assert_eq!(d.runtime.native_main_state_value().unwrap(), before);
    assert_eq!(d.index.snapshot().unwrap(), pin);
    assert!(matches!(d.kernel.value().get("ready"), V::Array(ref rows) if !rows.is_empty()));
}
