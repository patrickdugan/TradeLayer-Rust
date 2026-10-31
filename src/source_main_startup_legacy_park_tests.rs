// Startup-profile height parking. The recorded owner is a declared scene: RPC
// facts and stored heights are inputs, and each primitive runs only when the
// Kernel executes it. Turn numbers are compared with the frozen genuine NeDB
// 1.8 per-kind profile, never used as schedule input.
use super::startup_source_nedb_jobs::{Collection, DeliveryKind, QueryKind};
use super::startup_source_promise_queue::ActualPayloadBinding;
use super::*;

struct Recorded {
    heights: Vec<(&'static str, V)>,
    network: VecDeque<bool>,
    throw_write: bool,
}
impl Recorded {
    fn new(track: Option<f64>) -> Self {
        Self {
            heights: track.map_or(vec![], |t| vec![("TrackHeight", n(t))]),
            network: VecDeque::new(),
            throw_write: false,
        }
    }
}
fn num(v: &V) -> f64 {
    match v {
        V::Number(bits) => f64::from_bits(*bits),
        V::Null => 0.,
        _ => unreachable!("scene arithmetic is numeric"),
    }
}
impl sealed::Sealed for Recorded {}
impl MainStepOwner for Recorded {
    fn preflight(&self, _: &MainStepPrimitive) -> Result<(), SourceRuleError> {
        Ok(())
    }
    fn execute(&mut self, operation: &MainStepPrimitive) -> MainStepPrimitiveExecution {
        use MainStepPrimitive as Op;
        let returned = match operation {
            Op::BlockCount => n(102.),
            Op::HeightRead(name) => self
                .heights
                .iter()
                .find(|(k, _)| k == name)
                .map_or(V::Null, |(_, v)| v.clone()),
            Op::HeightWrite { name, value } => {
                if self.throw_write {
                    return MainStepPrimitiveExecution {
                        outcome: MainStepPrimitiveOutcome::Thrown(MainStepLegacyThrow {
                            exception: SourceException {
                                name: "Error".into(),
                                message: "write refused".into(),
                            },
                            code: V::Undefined,
                        }),
                        effects: vec![],
                    };
                }
                self.heights.retain(|(k, _)| k != name);
                self.heights.push((name, value.clone()));
                V::Undefined
            }
            Op::NetworkStatus => {
                obj(&[("status", V::Bool(self.network.pop_front().unwrap_or(true)))])
            }
            Op::FetchBlock(height) => obj(&[("height", height.clone())]),
            Op::ProcessBlock { .. } => obj(&[("reOrg", V::Bool(false))]),
            Op::PauseRead => V::Bool(false),
            Op::Number(v) => v.clone(),
            Op::Add(a, b) => n(num(a) + num(b)),
            Op::Greater(a, b) => V::Bool(num(a) > num(b)),
            Op::LessEqual(a, b) => V::Bool(num(a) <= num(b)),
            other => unreachable!("not reached by the Incoming scene: {other:?}"),
        };
        MainStepPrimitiveExecution {
            outcome: MainStepPrimitiveOutcome::Returned(returned),
            effects: vec![],
        }
    }
    fn startup_height_present(&self, name: &str) -> Result<bool, SourceRuleError> {
        Ok(self.heights.iter().any(|(k, _)| *k == name))
    }
}
const ARGS: [f64; 3] = [2., 100., 102.];
fn args() -> Vec<V> {
    ARGS.iter().map(|v| n(*v)).collect()
}
fn synchronous(owner: &mut Recorded) -> (MainStepKernelV1, MainStepEvaluationV1) {
    let mut kernel = MainStepKernelV1::new(7);
    let evaluation = kernel
        .apply(
            owner,
            MainStepCommandV1::Start {
                method: "Main.processIncomingBlocks".into(),
                arguments: args(),
            },
        )
        .unwrap();
    (kernel, evaluation)
}
fn startup() -> MainStepKernelV1 {
    let mut kernel = MainStepKernelV1::new(7);
    kernel.startup_install_source_promises().unwrap();
    kernel
        .startup_legacy_start("Main.processIncomingBlocks", &args())
        .unwrap();
    kernel
}
fn empty() -> MainStepEvaluationV1 {
    MainStepEvaluationV1 {
        started: None,
        stop: MainStepStop::Quiescent,
        trace: vec![],
        effects: vec![],
        completed: vec![],
    }
}
fn immediates(kernel: &MainStepKernelV1) -> u64 {
    let mut count = None;
    kernel
        .startup_source_nedb_jobs
        .as_ref()
        .unwrap()
        .visit_fields(&mut |path, value| {
            if path == "completedImmediates" {
                let V::String(text) = value else {
                    unreachable!()
                };
                count = Some(text.parse().unwrap());
            }
            Ok(())
        })
        .unwrap();
    count.unwrap()
}
// Parked continuation phases map back to the phase whose DB call they await.
fn origin(label: &str) -> String {
    for (parked, from) in [
        ("processIncomingBlocks/13", "processIncomingBlocks/1"),
        ("processIncomingBlocks/14", "processIncomingBlocks/9"),
        ("processIncomingBlocks/15", "processIncomingBlocks/10"),
        ("enterRecoveryMode/3", "enterRecoveryMode/1"),
        ("enterRecoveryMode/4", "enterRecoveryMode/2"),
    ] {
        if label.ends_with(parked) {
            return label.replace(parked, from);
        }
    }
    label.to_owned()
}
fn ops(trace: &[MainStepTraceV1]) -> Vec<(u64, String, V, V)> {
    trace
        .iter()
        .map(|t| {
            (
                t.job,
                origin(&t.phase),
                t.operation.clone(),
                t.outcome.clone(),
            )
        })
        .collect()
}
fn op_name(t: &MainStepTraceV1) -> String {
    match t.operation.get("operation") {
        V::String(name) => name,
        _ => unreachable!(),
    }
}

#[test]
fn parked_heights_run_the_same_source_prefix_as_the_synchronous_kernel() {
    // TrackHeight present (both writes modify), absent (first write inserts),
    // and a network flap that enters Recovery and reads both heights.
    for (track, network) in [
        (Some(100.), vec![]),
        (None, vec![]),
        (Some(99.), vec![false]),
    ] {
        let mut sync_owner = Recorded::new(track);
        sync_owner.network = network.clone().into();
        let (sync_kernel, sync) = synchronous(&mut sync_owner);
        let mut owner = Recorded::new(track);
        owner.network = network.into();
        let mut kernel = startup();
        let parked = kernel.startup_legacy_run(&mut owner).unwrap();
        assert_eq!(parked.stop, MainStepStop::Quiescent);
        assert_eq!(sync.stop, MainStepStop::Quiescent);
        assert_eq!(ops(&parked.trace), ops(&sync.trace));
        assert_eq!(parked.effects, sync.effects);
        assert_eq!(parked.completed, sync.completed);
        assert_eq!(owner.heights, sync_owner.heights);
        assert_eq!(owner.heights, vec![("TrackHeight", n(102.))]);
        // Both stop at the same pending 10 s timer; no parked request remains.
        let statuses = |k: &MainStepKernelV1| {
            k.jobs()
                .map(|(id, status)| (id, status.clone()))
                .collect::<Vec<_>>()
        };
        assert_eq!(statuses(&kernel), statuses(&sync_kernel));
        assert!(matches!(
            statuses(&kernel)[0].1,
            MainStepJobStatus::WaitingTimer(_)
        ));
        assert!(kernel.startup_legacy.parked.is_empty());
        assert!(kernel.startup_legacy.results.is_empty());
        let reads = parked
            .trace
            .iter()
            .filter(|t| op_name(t) == "HeightRead")
            .count();
        assert_eq!(reads, if track == Some(99.) { 3 } else { 1 });
    }
}

#[test]
fn each_height_primitive_runs_at_its_nedb_sample_and_resumes_at_callback() {
    let genuine =
        include_str!("../fixtures/reference_fidelity/main-startup-nedb-kinds-integration.json");
    let profile = |kind: &str| {
        let at = genuine.find(&format!("\"{kind}\": \"")).unwrap() + kind.len() + 5;
        genuine[at..at + genuine[at..].find('"').unwrap()].to_owned()
    };
    for (track, writes) in [
        (Some(100.), ["modify", "modify"]),
        (None, ["insert", "modify"]),
    ] {
        let mut owner = Recorded::new(track);
        let mut kernel = startup();
        let mut evaluation = empty();
        // (request, event, completed immediates, trace length at the event)
        let mut events: Vec<(u64, &str, u64, usize)> = vec![];
        loop {
            while let Some(id) = kernel.ready.pop_front() {
                assert!(kernel.step_ready(id, &mut owner, &mut evaluation).is_ok());
            }
            for parked in &kernel.startup_legacy.parked {
                let serial = parked.request.serial();
                if !events.iter().any(|(r, ..)| *r == serial) {
                    events.push((
                        serial,
                        "enqueue",
                        immediates(&kernel),
                        evaluation.trace.len(),
                    ));
                    assert_eq!(
                        kernel.jobs[kernel.position(parked.job)].status,
                        MainStepJobStatus::WaitingDatabase(serial)
                    );
                }
            }
            let Some(delivery) = kernel.startup_nedb().ok().unwrap().next_delivery().unwrap()
            else {
                break;
            };
            let before = evaluation.trace.len();
            let kind = match delivery.kind {
                DeliveryKind::SampleCandidates => "sample",
                DeliveryKind::DeliverCallback => "callback",
            };
            assert!(matches!(
                kernel.startup_legacy_deliver(&mut owner, delivery, &mut evaluation),
                Ok(true)
            ));
            events.push((delivery.request.serial(), kind, immediates(&kernel), before));
            // A callback runs no primitive; it only readies the parked job.
            if kind == "callback" {
                assert_eq!(evaluation.trace.len(), before);
            }
        }
        let per = |serial: u64| {
            events
                .iter()
                .filter(|(r, ..)| *r == serial)
                .copied()
                .collect::<Vec<_>>()
        };
        // Request 1 is the TrackHeight findOne on an idle database: its turns
        // equal the genuine idle profile, counted from its q.process turn.
        let read = per(1);
        let turn0 = read[0].2 + 1;
        let turns = read[1..]
            .iter()
            .map(|(_, kind, at, _)| format!("{kind}@{}", at - turn0))
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(turns, profile("findOne/find"));
        // The read runs exactly at its sample; the next primitive only after
        // the callback readied the job.
        let sample_trace = read[1].3;
        assert_eq!(op_name(&evaluation.trace[sample_trace]), "HeightRead");
        assert_eq!(read[2].3, sample_trace + 1);
        for (serial, write) in [2u64, 3].into_iter().zip(writes) {
            let events = per(serial);
            let shape = events[1..]
                .iter()
                .map(|(_, kind, ..)| *kind)
                .collect::<Vec<_>>();
            let writing_sample = if write == "insert" {
                assert_eq!(shape, ["sample", "callback"]);
                events[1]
            } else {
                // The existence sample does not write; the second pass does.
                assert_eq!(shape, ["sample", "sample", "callback"]);
                assert_eq!(events[2].3, events[1].3);
                events[2]
            };
            assert_eq!(op_name(&evaluation.trace[writing_sample.3]), "HeightWrite");
        }
        // Idle-database insert/modify sample counts match the genuine kinds.
        let samples = |kind: &str| profile(kind).matches("sample").count();
        assert_eq!(samples("upsertInsert"), 1);
        assert_eq!(samples("upsertModify"), 2);
        assert_eq!(owner.heights, vec![("TrackHeight", n(102.))]);
    }
}

#[test]
fn a_foreign_request_on_another_database_samples_before_the_lanes_first_write() {
    let mut owner = Recorded::new(Some(100.));
    let mut kernel = startup();
    let mut evaluation = empty();
    // Run the lane until its TrackHeight read is parked.
    while let Some(id) = kernel.ready.pop_front() {
        assert!(kernel.step_ready(id, &mut owner, &mut evaluation).is_ok());
    }
    assert_eq!(kernel.startup_legacy.parked.len(), 1);
    // An outer-style txIndex read issued at that point, on its own source job.
    let (promises, nedb) = kernel.startup_source_parts().unwrap();
    let job = promises
        .allocate_source_job("Main.constructConsensusFromIndex")
        .unwrap();
    let payload = promises
        .reserve_actual_payload(ActualPayloadBinding {
            job,
            method_scope: "Main.constructConsensusFromIndex",
            source_site: "FixedRead",
            lane: "Main",
            await_ordinal: 1,
        })
        .unwrap();
    let foreign = nedb
        .enqueue_actual_request(promises, payload, Collection::TxIndex, QueryKind::FindAll)
        .unwrap();
    let mut foreign_sampled_at = None;
    loop {
        while let Some(id) = kernel.ready.pop_front() {
            assert!(kernel.step_ready(id, &mut owner, &mut evaluation).is_ok());
        }
        let Some(delivery) = kernel.startup_nedb().ok().unwrap().next_delivery().unwrap() else {
            break;
        };
        if delivery.request == foreign {
            // Its own route would act here; the lane does not own it.
            assert!(matches!(
                kernel.startup_legacy_deliver(&mut owner, delivery, &mut evaluation),
                Ok(false)
            ));
            if delivery.kind == DeliveryKind::SampleCandidates {
                foreign_sampled_at = Some(evaluation.trace.len());
            }
            kernel
                .startup_nedb()
                .ok()
                .unwrap()
                .acknowledge_actual(delivery)
                .unwrap();
            continue;
        }
        assert!(matches!(
            kernel.startup_legacy_deliver(&mut owner, delivery, &mut evaluation),
            Ok(true)
        ));
    }
    let first_write = evaluation
        .trace
        .iter()
        .position(|t| op_name(t) == "HeightWrite")
        .unwrap();
    assert!(foreign_sampled_at.unwrap() < first_write);
    // The synchronous Kernel runs that write within the lane's first turn.
    let (_, sync) = synchronous(&mut Recorded::new(Some(100.)));
    assert!(sync.trace.iter().any(|t| op_name(t) == "HeightWrite"));
}

#[test]
fn a_thrown_parked_write_completes_the_job_as_the_synchronous_kernel_does() {
    let mut sync_owner = Recorded::new(None);
    sync_owner.throw_write = true;
    let (_, sync) = synchronous(&mut sync_owner);
    let mut owner = Recorded::new(None);
    owner.throw_write = true;
    let mut kernel = startup();
    let parked = kernel.startup_legacy_run(&mut owner).unwrap();
    assert_eq!(ops(&parked.trace), ops(&sync.trace));
    assert_eq!(parked.completed, sync.completed);
    assert!(matches!(
        parked.completed.last(),
        Some((1, MainStepSourceResult::Thrown(_)))
    ));
    assert!(kernel.startup_legacy.parked.is_empty());
}

#[test]
fn the_parked_lane_refuses_outside_the_startup_profile_and_after_a_stop() {
    let mut kernel = MainStepKernelV1::new(7);
    assert_eq!(
        kernel.startup_legacy_start("Main.processIncomingBlocks", &args()),
        Err(MainStepError::InvalidInput)
    );
    let mut kernel = startup();
    assert_eq!(
        kernel.startup_legacy_start("Main.delay", &[n(5.)]),
        Err(MainStepError::InvalidInput)
    );
    // A delivery for a request no route owns stops the whole domain.
    let mut owner = Recorded::new(Some(100.));
    let mut evaluation = empty();
    while let Some(id) = kernel.ready.pop_front() {
        assert!(kernel.step_ready(id, &mut owner, &mut evaluation).is_ok());
    }
    let (promises, nedb) = kernel.startup_source_parts().unwrap();
    let job = promises
        .allocate_source_job("Main.constructConsensusFromIndex")
        .unwrap();
    let payload = promises
        .reserve_actual_payload(ActualPayloadBinding {
            job,
            method_scope: "Main.constructConsensusFromIndex",
            source_site: "FixedRead",
            lane: "Main",
            await_ordinal: 1,
        })
        .unwrap();
    nedb.enqueue_actual_request(promises, payload, Collection::TxIndex, QueryKind::FindAll)
        .unwrap();
    let stopped = kernel.startup_legacy_run(&mut owner).unwrap();
    assert_eq!(
        stopped.stop,
        MainStepStop::SourceOwnerStopped(SourceRuleError::InvalidInput)
    );
    assert_eq!(
        kernel.startup_legacy_run(&mut owner),
        Err(MainStepError::OwnerStopped)
    );
    assert!(kernel.startup_source_parts().is_err());
}
