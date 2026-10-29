use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use tradelayer_core::source_compact_index_v2::SourceCompactIndex;
use tradelayer_core::source_contract_replay::{
    SourceContractCommand as C, SourceContractCommandOutcome as O, SourceContractReplay,
};
use tradelayer_core::source_contract_runtime::SourceContractRuntimePolicy as P;
use tradelayer_core::source_main_native_owners::{NativeMainExpectation, NativeMainOwners};
use tradelayer_core::source_main_native_session::{
    NativeStepSessionError, SourceMainNativeSessionV13, SourceMainNativeStepSessionV1,
};
use tradelayer_core::source_main_step_kernel::{
    MainStepCommandV1 as Command, MainStepJobStatus, MainStepSourceResult, MainStepStop,
};
use tradelayer_core::storage::Namespace;
use tradelayer_core::validity_source::{DependencyCall, DependencyReturn, SourceValue as V};
static SERIAL: AtomicU64 = AtomicU64::new(0);
struct Temp {
    root: PathBuf,
    parent: PathBuf,
}
impl Temp {
    fn new() -> Self {
        let parent =
            std::env::var_os("MAIN_STEP_TMP").map_or_else(std::env::temp_dir, PathBuf::from);
        std::fs::create_dir_all(&parent).unwrap();
        let parent = parent.canonicalize().unwrap();
        let root = parent.join(format!(
            "step-session-{}-{}",
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
fn s(value: &str) -> V {
    V::String(value.into())
}
fn obj(fields: &[(&str, V)]) -> V {
    V::Object(
        fields
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
    )
}
fn command(replay: &mut SourceContractReplay, target: &str, args: Vec<V>) {
    let receipt = replay
        .apply(C::Invoke(DependencyCall {
            target: target.into(),
            arguments: args,
        }))
        .unwrap();
    assert!(
        !matches!(
            receipt.outcome,
            O::LocalError(_)
                | O::Dependency(DependencyReturn::Throw(_) | DependencyReturn::PendingThrow(_))
        ),
        "{target}: {:?}",
        receipt.outcome
    );
}
fn native(temp: &Temp) -> SourceMainNativeSessionV13 {
    native_with_service(
        temp,
        obj(&[
            ("fileInputs", V::Array(vec![])),
            ("directoryInputs", V::Array(vec![])),
            (
                "fileIOInputs",
                obj(&[("failures", V::Array(vec![])), ("mtimes", V::Array(vec![]))]),
            ),
        ]),
    )
}
fn native_with_service(temp: &Temp, service: V) -> SourceMainNativeSessionV13 {
    let mut financial = SourceContractReplay::with_policy(P::MainOrchestrationV12).unwrap();
    command(
        &mut financial,
        "SourceActivation.configureClient",
        vec![obj(&[("chain", s("LTC")), ("test", V::Bool(false))])],
    );
    command(&mut financial, "Activation.init", vec![s("LTC")]);
    command(
        &mut financial,
        "SourceMain.configureProduction",
        vec![obj(&[
            (
                "client",
                obj(&[("chain", s("LTC")), ("test", V::Bool(false))]),
            ),
            (
                "io",
                obj(&[
                    ("height", V::number(0.)),
                    (
                        "networkInfo",
                        obj(&[
                            ("networkactive", V::Bool(true)),
                            ("connections", V::number(2.)),
                        ]),
                    ),
                    ("blocks", V::Array(vec![])),
                ]),
            ),
            ("genesisBlock", V::number(10.)),
        ])],
    );
    command(&mut financial, "SourceMain.configureService", vec![service]);
    financial.apply(C::SetHeight(V::number(0.))).unwrap();
    let namespace = Namespace::new("LTC", false, "txIndex").unwrap();
    let index = SourceCompactIndex::create(
        &temp.root,
        namespace.clone(),
        "step-session/source-original-facts",
    )
    .unwrap();
    let expected = NativeMainExpectation {
        financial_head: financial.head(),
        namespace,
        metadata: index.snapshot().unwrap(),
    };
    SourceMainNativeSessionV13::adopt(NativeMainOwners::adopt(financial, index, &expected).unwrap())
        .unwrap()
}
fn controller(temp: &Temp) -> SourceMainNativeStepSessionV1 {
    let owner = native(temp);
    let expected = owner.expectation().unwrap();
    SourceMainNativeStepSessionV1::adopt(owner, &expected, 17).unwrap()
}
fn apply(
    session: &mut SourceMainNativeStepSessionV1,
    cmd: Command,
) -> tradelayer_core::source_main_native_session::NativeStepReceiptV1 {
    let expected = session.expectation().unwrap();
    session.apply_step(&expected, cmd).unwrap().clone()
}
fn start(
    session: &mut SourceMainNativeStepSessionV1,
    target: &str,
    args: Vec<V>,
) -> tradelayer_core::source_main_native_session::NativeStepReceiptV1 {
    apply(
        session,
        Command::Start {
            method: target.into(),
            arguments: args,
        },
    )
}
#[test]
fn public_native_step_session_executes_real_pause_timer_and_shared_height_prefix() {
    let temp = Temp::new();
    let mut session = controller(&temp);
    let original = session.head().unwrap();
    let pause = start(&mut session, "Main.setPause", vec![]);
    assert!(pause
        .evaluation
        .as_ref()
        .unwrap()
        .completed
        .iter()
        .any(|(_, r)| r == &MainStepSourceResult::Fulfilled(V::Bool(true))));
    let sync = start(&mut session, "Main.syncIfNecessary", vec![]);
    let id = sync.evaluation.as_ref().unwrap().started.unwrap();
    assert!(matches!(
        session
            .kernel()
            .jobs()
            .find(|(job, _)| *job == id)
            .unwrap()
            .1,
        MainStepJobStatus::WaitingTimer(_)
    ));
    let timer = session.kernel().next_timer().unwrap();
    apply(
        &mut session,
        Command::FireTimer {
            id: timer,
            generation: 17,
        },
    );
    start(&mut session, "Main.setPause", vec![]);
    let timer = session.kernel().next_timer().unwrap();
    let done = apply(
        &mut session,
        Command::FireTimer {
            id: timer,
            generation: 17,
        },
    );
    assert!(done
        .evaluation
        .as_ref()
        .unwrap()
        .completed
        .iter()
        .any(|(_, r)| r == &MainStepSourceResult::Fulfilled(V::Undefined)));
    start(&mut session, "Main.saveTrackHeight", vec![V::number(5.)]);
    assert_ne!(session.head().unwrap(), original);
    assert!(session
        .financial_runtime()
        .collection("consensus")
        .iter()
        .any(|row| row.get("_id") == s("TrackHeight") && row.get("value") == V::number(5.)));
    assert!(session.financial_runtime().collection("txIndex").is_empty());
    assert!(done.kernel_fields.fields > 0);
    assert!(!done.kernel_frames.is_empty());
    assert!(!done.result_frames.is_empty());
}
#[test]
fn complete_eight_large_arguments_pass_without_an_aggregate_diagnostic_or_truncation() {
    let temp = Temp::new();
    let mut session = controller(&temp);
    let args = (0..8)
        .map(|_| V::Array((0..1024).map(|n| V::number(n as f64)).collect()))
        .collect();
    let cmd = Command::Start {
        method: "Main.setPause".into(),
        arguments: args,
    };
    assert!(cmd.value().diagnostic_json().is_err());
    let receipt = apply(&mut session, cmd);
    assert!(receipt.command_fields.fields > 8192);
    assert!(receipt
        .evaluation
        .as_ref()
        .unwrap()
        .completed
        .iter()
        .any(|(_, r)| r == &MainStepSourceResult::Fulfilled(V::Bool(true))));
    assert!(session.command_field_frames(0).unwrap().len() > 500_000);
}
#[test]
fn wrong_binding_refuses_before_any_owner_effect_and_actual_shutdown_stops_followups() {
    let temp = Temp::new();
    let mut session = controller(&temp);
    let expected = session.expectation().unwrap();
    let state = session.financial_runtime().state_value().unwrap();
    let mut wrong = expected.clone();
    wrong.physical.canonical_catalog.push("foreign");
    assert_eq!(
        session
            .apply_step(
                &wrong,
                Command::Start {
                    method: "Main.setPause".into(),
                    arguments: vec![]
                }
            )
            .unwrap_err(),
        NativeStepSessionError::WrongPhysicalBinding
    );
    assert_eq!(session.financial_runtime().state_value().unwrap(), state);
    assert_eq!(session.head().unwrap(), expected.head);
    let before = session.index().snapshot().unwrap();
    let receipt = apply(&mut session, Command::Shutdown(vec![]));
    let result = receipt.evaluation.unwrap();
    assert_eq!(result.stop, MainStepStop::ProcessExited(0));
    assert_eq!(result.effects.len(), 3);
    assert_eq!(session.index().snapshot().unwrap(), before);
    assert_eq!(session.financial_runtime().state_value().unwrap(), state);
    let expected = session.expectation().unwrap();
    assert_eq!(
        session
            .apply_step(&expected, Command::DrainReady { limit: 1 })
            .unwrap_err(),
        NativeStepSessionError::Exited
    );
}
fn raw_origin(temp: &Temp) -> SourceCompactIndex {
    SourceCompactIndex::create(
        &temp.root,
        Namespace::new("LTC", false, "txIndex").unwrap(),
        "step-session/source-original-facts",
    )
    .unwrap()
}
fn physical(
    index: &SourceCompactIndex,
) -> tradelayer_core::source_main_native_session::NativePhysicalBinding {
    let (persistent_identity, canonical_catalog) = index.native_store_context().unwrap();
    tradelayer_core::source_main_native_session::NativePhysicalBinding {
        persistent_identity,
        canonical_catalog,
        metadata: index.snapshot().unwrap(),
    }
}
#[test]
fn step_command_checkpoint_reexecutes_real_pending_timer_on_fresh_owner_then_resumes() {
    let temp = Temp::new();
    let mut original = controller(&temp);
    start(&mut original, "Main.setPause", vec![]);
    start(&mut original, "Main.syncIfNecessary", vec![]);
    let (bytes, pin) = original.checkpoint().unwrap();
    assert_eq!(&bytes[..5], b"TLCR\x0d");
    assert!(SourceContractReplay::from_checkpoint(&bytes, pin.foundation.foundation).is_err());
    let fresh_temp = Temp::new();
    let fresh = raw_origin(&fresh_temp);
    let raw_pin = physical(&fresh);
    let mut replay =
        SourceMainNativeStepSessionV1::reexecute(&bytes, &pin, fresh, &raw_pin).unwrap();
    assert_eq!(replay.head().unwrap(), original.head().unwrap());
    assert_ne!(
        replay.expectation().unwrap().physical.canonical_catalog,
        pin.current.canonical_catalog
    );
    assert_eq!(
        replay.financial_runtime().state_value().unwrap(),
        original.financial_runtime().state_value().unwrap()
    );
    for session in [&mut original, &mut replay] {
        start(session, "Main.setPause", vec![]);
        let id = session.kernel().next_timer().unwrap();
        let generation = session.kernel().generation();
        apply(session, Command::FireTimer { id, generation });
    }
    assert_eq!(replay.head().unwrap(), original.head().unwrap());
    assert_eq!(
        replay.receipts().last().unwrap().kernel_frames,
        original.receipts().last().unwrap().kernel_frames
    );
}
#[test]
fn eight_large_original_arguments_reexecute_from_fields_without_aggregate_command_import() {
    let temp = Temp::new();
    let mut original = controller(&temp);
    let arguments = (0..8)
        .map(|_| V::Array((0..1024).map(|n| V::number(n as f64)).collect()))
        .collect();
    apply(
        &mut original,
        Command::Start {
            method: "Main.setPause".into(),
            arguments,
        },
    );
    let (bytes, pin) = original.checkpoint().unwrap();
    let fresh_temp = Temp::new();
    let fresh = raw_origin(&fresh_temp);
    let raw_pin = physical(&fresh);
    let replay = SourceMainNativeStepSessionV1::reexecute(&bytes, &pin, fresh, &raw_pin).unwrap();
    assert_eq!(replay.head().unwrap(), original.head().unwrap());
    assert_eq!(
        replay.command_field_frames(0),
        original.command_field_frames(0)
    );
    assert!(replay.receipts().next().unwrap().command_fields.fields > 8192);
}
#[test]
fn step_archive_tamper_refuses_before_any_native_or_financial_reexecution() {
    use tradelayer_core::source_main_native_session::NativeStepReexecutionOwnerV1;
    let temp = Temp::new();
    let mut original = controller(&temp);
    start(&mut original, "Main.setPause", vec![]);
    let (mut bytes, pin) = original.checkpoint().unwrap();
    bytes[8] ^= 1;
    let fresh_temp = Temp::new();
    let fresh = raw_origin(&fresh_temp);
    let raw_pin = physical(&fresh);
    let rejected =
        SourceMainNativeStepSessionV1::reexecute(&bytes, &pin, fresh, &raw_pin).unwrap_err();
    assert_eq!(rejected.error, NativeStepSessionError::WrongHead);
    let NativeStepReexecutionOwnerV1::Origin(index) = rejected.owner else {
        panic!("tamper must reject before source execution")
    };
    assert_eq!(physical(&index), raw_pin);
}
#[test]
fn immediate_archive_is_not_admitted_by_explicit_file_profile_reader() {
    use tradelayer_core::source_main_native_session::NativeStepReexecutionOwnerV1;
    let temp = Temp::new();
    let mut original = controller(&temp);
    start(&mut original, "Main.setPause", vec![]);
    let (bytes, pin) = original.checkpoint().unwrap();
    let fresh_temp = Temp::new();
    let fresh = raw_origin(&fresh_temp);
    let raw_pin = physical(&fresh);
    let rejected =
        SourceMainNativeStepSessionV1::reexecute_with_file_retries(&bytes, &pin, fresh, &raw_pin)
            .unwrap_err();
    let NativeStepReexecutionOwnerV1::Origin(index) = rejected.owner else {
        panic!("wrong source profile must reject before replay or native mutation")
    };
    assert_eq!(physical(&index), raw_pin);
}

// Appended to the existing public binding tests in this proposal. Facts are
// original source safeCopy inputs from main_file_retry_proof10.jsonl; no
// supplied financial answer or live-state import is used.
fn file_controller(temp: &Temp, failures: usize) -> SourceMainNativeStepSessionV1 {
    let service = obj(&[
        (
            "fileInputs",
            V::Array(vec![obj(&[
                ("path", s("/tl-data/ltc-main/a.db")),
                ("bytes", s("live a")),
            ])]),
        ),
        ("directoryInputs", V::Array(vec![])),
        (
            "fileIOInputs",
            obj(&[
                (
                    "failures",
                    V::Array(
                        (1..=failures)
                            .map(|ordinal| {
                                obj(&[
                                    ("method", s("copyFile")),
                                    ("ordinal", V::number(ordinal as f64)),
                                    ("code", s("EBUSY")),
                                    ("message", s(&format!("recorded copyFile EBUSY {ordinal}"))),
                                ])
                            })
                            .collect(),
                    ),
                ),
                ("mtimes", V::Array(vec![])),
            ]),
        ),
    ]);
    let owner = native_with_service(temp, service);
    let expected = owner.expectation().unwrap();
    SourceMainNativeStepSessionV1::adopt_with_file_retries(owner, &expected, 17).unwrap()
}
fn service_value(session: &SourceMainNativeStepSessionV1) -> V {
    session
        .financial_runtime()
        .state_value()
        .unwrap()
        .get("mainProduction")
        .get("service")
}
fn copy_start(session: &mut SourceMainNativeStepSessionV1) -> u64 {
    let copy = start(
        session,
        "Persistence.safeCopy",
        vec![s("/tl-data/ltc-main/a.db"), s("/tl-data/copied.db")],
    );
    assert_eq!(copy.consumed_rows, 0);
    assert!(copy.validity.is_empty());
    let id = copy.evaluation.unwrap().started.unwrap();
    assert!(matches!(
        session
            .kernel()
            .jobs()
            .find(|(job, _)| *job == id)
            .unwrap()
            .1,
        MainStepJobStatus::WaitingTimer(_)
    ));
    id
}
#[test]
fn explicit_file_profile_reexecutes_copy_wait_and_concurrent_main_prefix_on_one_owner() {
    use tradelayer_core::source_main_native_session::{
        NativeStepProfileV1, NativeStepReexecutionOwnerV1,
    };
    let temp = Temp::new();
    let mut original = file_controller(&temp, 1);
    assert_eq!(original.profile(), NativeStepProfileV1::FileRetries);
    assert_eq!(
        original.kernel().source_profile(),
        "TL-MAIN-EXPLICIT-TIMER-FILE-RETRY-1"
    );
    let job = copy_start(&mut original);
    assert_eq!(
        service_value(&original)
            .get("fileIO")
            .get("counts")
            .get("copyFile"),
        V::number(1.)
    );
    start(&mut original, "Main.setPause", vec![]);
    start(&mut original, "Main.saveTrackHeight", vec![V::number(5.)]);
    assert_eq!(service_value(&original).get("pause"), V::Bool(true));
    assert!(original
        .financial_runtime()
        .collection("consensus")
        .iter()
        .any(|r| r.get("_id") == s("TrackHeight") && r.get("value") == V::number(5.)));
    let (bytes, pin) = original.checkpoint().unwrap();
    let rejected_temp = Temp::new();
    let wrong = raw_origin(&rejected_temp);
    let wrong_pin = physical(&wrong);
    let rejected =
        SourceMainNativeStepSessionV1::reexecute(&bytes, &pin, wrong, &wrong_pin).unwrap_err();
    let NativeStepReexecutionOwnerV1::Origin(wrong) = rejected.owner else {
        panic!("ordinary reader must reject file profile before any source call")
    };
    assert_eq!(physical(&wrong), wrong_pin);
    let replay_temp = Temp::new();
    let raw = raw_origin(&replay_temp);
    let raw_pin = physical(&raw);
    let mut replay =
        SourceMainNativeStepSessionV1::reexecute_with_file_retries(&bytes, &pin, raw, &raw_pin)
            .unwrap();
    assert_eq!(replay.profile(), NativeStepProfileV1::FileRetries);
    assert_eq!(replay.head().unwrap(), original.head().unwrap());
    assert_ne!(
        replay.expectation().unwrap().physical.canonical_catalog,
        pin.current.canonical_catalog
    );
    for session in [&mut original, &mut replay] {
        let id = session.kernel().next_timer().unwrap();
        let result = apply(session, Command::FireTimer { id, generation: 17 });
        assert!(result
            .evaluation
            .unwrap()
            .completed
            .iter()
            .any(|(id, result)| *id == job
                && result == &MainStepSourceResult::Fulfilled(V::Undefined)));
        assert!(session.kernel().next_timer().is_none());
        assert_eq!(
            service_value(session)
                .get("fileIO")
                .get("counts")
                .get("copyFile"),
            V::number(2.)
        );
        let V::Array(files) = service_value(session).get("files") else {
            panic!()
        };
        assert!(files.contains(&V::Array(vec![s("/tl-data/copied.db"), s("live a")])));
        assert!(session.financial_runtime().collection("txIndex").is_empty());
    }
    assert_eq!(replay.head().unwrap(), original.head().unwrap());
    assert_eq!(
        replay.receipts().last().unwrap().kernel_frames,
        original.receipts().last().unwrap().kernel_frames
    );
    assert_eq!(
        replay.financial_runtime().state_value().unwrap(),
        original.financial_runtime().state_value().unwrap()
    );
}
#[test]
fn explicit_file_profile_retains_all_five_waits_and_shutdown_does_not_drain_pending_copy() {
    let temp = Temp::new();
    let mut session = file_controller(&temp, 5);
    let job = copy_start(&mut session);
    for wait in 1..=5 {
        let id = session
            .kernel()
            .next_timer()
            .expect("including the fifth failure's wait");
        let result = apply(&mut session, Command::FireTimer { id, generation: 17 });
        if wait < 5 {
            assert!(result
                .evaluation
                .unwrap()
                .completed
                .iter()
                .all(|(id, _)| *id != job));
        } else {
            assert!(result.evaluation.unwrap().completed.iter().any(
                |(id, result)| *id == job && matches!(result, MainStepSourceResult::Thrown(_))
            ));
        }
    }
    assert!(session.kernel().next_timer().is_none());
    assert_eq!(
        service_value(&session)
            .get("fileIO")
            .get("counts")
            .get("copyFile"),
        V::number(5.)
    );
    let other_temp = Temp::new();
    let mut pending = file_controller(&other_temp, 1);
    copy_start(&mut pending);
    let state = pending.financial_runtime().state_value().unwrap();
    let result = apply(&mut pending, Command::Shutdown(vec![]));
    assert_eq!(
        result.evaluation.unwrap().stop,
        MainStepStop::ProcessExited(0)
    );
    assert_eq!(pending.financial_runtime().state_value().unwrap(), state);
    let expected = pending.expectation().unwrap();
    assert_eq!(
        pending
            .apply_step(&expected, Command::DrainReady { limit: 1 })
            .unwrap_err(),
        NativeStepSessionError::Exited
    );
}

use std::collections::BTreeMap;
use tradelayer_core::source_main_native_session::{
    NativeStepHistoryBootstrapV1, NativeStepHistoryPublicationV1,
    NativeStepHistoryRejectionOwnerV1, NativeStepHistoryWriterV1, NativeStepProfileV1,
};
type SegmentStore = BTreeMap<[u8; 32], Vec<u8>>;
fn writer_apply(writer: &mut NativeStepHistoryWriterV1, command: Command) {
    let expected = writer.expectation().unwrap();
    writer.apply_step(&expected, command).unwrap();
}
fn writer_start(writer: &mut NativeStepHistoryWriterV1, method: &str, arguments: Vec<V>) {
    writer_apply(
        writer,
        Command::Start {
            method: method.into(),
            arguments,
        },
    );
}
fn writer_publish(
    writer: &mut NativeStepHistoryWriterV1,
    store: &mut SegmentStore,
) -> NativeStepHistoryPublicationV1 {
    let expected = writer.expectation().unwrap();
    writer
        .publish_segment(&expected, |segment| {
            store.insert(
                segment.publication().head.last_segment_sha256,
                segment.bytes().to_vec(),
            );
            Ok(())
        })
        .unwrap()
}
fn writer_new(temp: &Temp) -> NativeStepHistoryWriterV1 {
    let session = controller(temp);
    let expected = session.expectation().unwrap();
    NativeStepHistoryWriterV1::adopt(
        session,
        &expected,
        "source controls / segmented history",
        |_| Ok(()),
    )
    .unwrap()
}
fn short_history(
    temp: &Temp,
) -> (
    NativeStepHistoryWriterV1,
    NativeStepHistoryBootstrapV1,
    SegmentStore,
) {
    let mut writer = writer_new(temp);
    let bootstrap = writer.bootstrap().clone();
    let mut store = SegmentStore::new();
    writer_start(&mut writer, "Main.setPause", vec![]);
    writer_start(&mut writer, "Main.syncIfNecessary", vec![]);
    writer_publish(&mut writer, &mut store);
    writer_start(&mut writer, "Main.saveTrackHeight", vec![V::number(7.)]);
    writer_publish(&mut writer, &mut store);
    (writer, bootstrap, store)
}
#[test]
fn failed_bootstrap_sink_returns_same_owner_without_releasing_prior_commands() {
    let temp = Temp::new();
    let mut session = controller(&temp);
    start(&mut session, "Main.saveTrackHeight", vec![V::number(3.)]);
    let expected = session.expectation().unwrap();
    let state = session.financial_runtime().state_value().unwrap();
    let rejected = NativeStepHistoryWriterV1::adopt(session, &expected, "bootstrap retry", |_| {
        Err("unpublished bootstrap".into())
    })
    .unwrap_err();
    let NativeStepHistoryRejectionOwnerV1::Steps(session) = rejected.owner else {
        panic!()
    };
    assert_eq!(session.expectation().unwrap(), expected);
    assert_eq!(session.financial_runtime().state_value().unwrap(), state);
    assert_eq!(session.receipts().count(), 1);
    assert!(session.checkpoint().is_ok());
    let writer =
        NativeStepHistoryWriterV1::adopt(*session, &expected, "bootstrap retry", |_| Ok(()))
            .unwrap();
    assert_eq!(writer.session().head().unwrap(), expected.head);
    assert_eq!(writer.session().receipts().count(), 0);
    assert!(writer.session().checkpoint().is_err());
}
#[test]
fn segmented_publication_keeps_same_owner_timer_and_complete_reexecution() {
    let temp = Temp::new();
    let mut writer = writer_new(&temp);
    let bootstrap = writer.bootstrap().clone();
    let mut store = SegmentStore::new();
    writer_start(&mut writer, "Main.setPause", vec![]);
    writer_start(&mut writer, "Main.syncIfNecessary", vec![]);
    let expected = writer.expectation().unwrap();
    let owner_state = writer.session().financial_runtime().state_value().unwrap();
    assert!(matches!(
        writer.publish_segment(&expected, |_| Err("sink unavailable".into())),
        Err(NativeStepSessionError::PublicationBlocked(_))
    ));
    assert_eq!(writer.expectation().unwrap(), expected);
    assert_eq!(writer.session().receipts().count(), 2);
    let timer = writer.session().kernel().next_timer().unwrap();
    writer_publish(&mut writer, &mut store);
    assert_eq!(writer.session().head().unwrap(), expected.current.head);
    assert_eq!(
        writer.session().financial_runtime().state_value().unwrap(),
        owner_state
    );
    assert_eq!(writer.session().kernel().next_timer(), Some(timer));
    assert_eq!(writer.session().receipts().count(), 0);
    assert!(matches!(
        writer.session().checkpoint(),
        Err(NativeStepSessionError::PublicationBlocked(_))
    ));
    writer_start(&mut writer, "Main.setPause", vec![]);
    writer_apply(
        &mut writer,
        Command::FireTimer {
            id: timer,
            generation: 17,
        },
    );
    assert_eq!(writer.session().command_field_frames(0), None);
    assert!(writer.session().command_field_frames(2).is_some());
    let publication = writer_publish(&mut writer, &mut store);
    let fresh_temp = Temp::new();
    let fresh = raw_origin(&fresh_temp);
    let raw_pin = physical(&fresh);
    let replay = NativeStepHistoryWriterV1::reexecute(
        &bootstrap,
        &publication,
        |digest| store.get(&digest).cloned().ok_or("missing segment".into()),
        fresh,
        &raw_pin,
        NativeStepProfileV1::Immediate,
    )
    .unwrap();
    assert_eq!(replay.verified_publication(), &publication);
    assert_eq!(
        replay.session().head().unwrap(),
        writer.session().head().unwrap()
    );
    assert_eq!(
        replay.session().financial_runtime().state_value().unwrap(),
        writer.session().financial_runtime().state_value().unwrap()
    );
    assert_eq!(
        replay.session().kernel().next_timer(),
        writer.session().kernel().next_timer()
    );
    assert_eq!(replay.session().receipts().count(), 0);
    assert_ne!(
        replay
            .session()
            .expectation()
            .unwrap()
            .physical
            .canonical_catalog,
        publication.physical.canonical_catalog
    );
    assert!(replay.session().checkpoint().is_err());
}
#[test]
fn wrong_segment_or_physical_publication_rejects_before_owner_execution() {
    let temp = Temp::new();
    let (writer, bootstrap, store) = short_history(&temp);
    let publication = writer.expectation().unwrap().sealed;
    for tamper_physical in [false, true] {
        let fresh_temp = Temp::new();
        let fresh = raw_origin(&fresh_temp);
        let pin = physical(&fresh);
        let mut expected = publication.clone();
        if tamper_physical {
            expected.physical.canonical_catalog.push("foreign");
        }
        let rejected = NativeStepHistoryWriterV1::reexecute(
            &bootstrap,
            &expected,
            |digest| {
                let mut bytes = store[&digest].clone();
                if !tamper_physical {
                    bytes[6] ^= 1;
                }
                Ok(bytes)
            },
            fresh,
            &pin,
            NativeStepProfileV1::Immediate,
        )
        .unwrap_err();
        let NativeStepHistoryRejectionOwnerV1::Origin(index) = rejected.owner else {
            panic!("all chain checks must precede source execution")
        };
        assert_eq!(physical(&index), pin);
    }
    let fresh_temp = Temp::new();
    let fresh = raw_origin(&fresh_temp);
    let pin = physical(&fresh);
    let rejected = NativeStepHistoryWriterV1::reexecute(
        &bootstrap,
        &publication,
        |_| panic!("profile must reject before reading segments"),
        fresh,
        &pin,
        NativeStepProfileV1::FileRetries,
    )
    .unwrap_err();
    let NativeStepHistoryRejectionOwnerV1::Origin(index) = rejected.owner else {
        panic!()
    };
    assert_eq!(physical(&index), pin);
}
#[test]
fn reader_change_after_preflight_retains_actual_reexecuted_prefix() {
    let temp = Temp::new();
    let (writer, bootstrap, store) = short_history(&temp);
    let publication = writer.expectation().unwrap().sealed;
    let fresh_temp = Temp::new();
    let fresh = raw_origin(&fresh_temp);
    let pin = physical(&fresh);
    let mut reads = 0;
    let rejected = NativeStepHistoryWriterV1::reexecute(
        &bootstrap,
        &publication,
        |digest| {
            reads += 1;
            let mut bytes = store[&digest].clone();
            if reads > publication.head.segments {
                bytes[6] ^= 1;
            }
            Ok(bytes)
        },
        fresh,
        &pin,
        NativeStepProfileV1::Immediate,
    )
    .unwrap_err();
    let NativeStepHistoryRejectionOwnerV1::Steps(session) = rejected.owner else {
        panic!("changed source must return the actual retained owner")
    };
    assert_eq!(session.head().unwrap(), bootstrap.archive_pin.head);
    assert_eq!(session.receipts().count(), 0);
}
#[test]
fn full_segment_parks_before_more_calls_but_admits_real_terminal_suffix() {
    let temp = Temp::new();
    let mut writer = writer_new(&temp);
    let bootstrap = writer.bootstrap().clone();
    let mut store = SegmentStore::new();
    for _ in 0..256 {
        writer_apply(&mut writer, Command::DrainReady { limit: 0 });
    }
    let expected = writer.expectation().unwrap();
    assert_eq!(
        writer
            .apply_step(&expected, Command::DrainReady { limit: 0 })
            .unwrap_err(),
        NativeStepSessionError::Capacity
    );
    assert_eq!(writer.expectation().unwrap(), expected);
    writer_apply(&mut writer, Command::Shutdown(vec![]));
    let terminal = writer.expectation().unwrap();
    assert_eq!(
        writer
            .apply_step(&terminal, Command::DrainReady { limit: 0 })
            .unwrap_err(),
        NativeStepSessionError::Exited
    );
    let publication = writer_publish(&mut writer, &mut store);
    assert_eq!(publication.head.source.commands, 257);
    let fresh_temp = Temp::new();
    let fresh = raw_origin(&fresh_temp);
    let pin = physical(&fresh);
    let replay = NativeStepHistoryWriterV1::reexecute(
        &bootstrap,
        &publication,
        |digest| Ok(store[&digest].clone()),
        fresh,
        &pin,
        NativeStepProfileV1::Immediate,
    )
    .unwrap();
    let mut session = replay.into_session();
    let expected = session.expectation().unwrap();
    assert_eq!(
        session
            .apply_step(&expected, Command::DrainReady { limit: 0 })
            .unwrap_err(),
        NativeStepSessionError::Exited
    );
}

#[test]
fn more_than_4096_global_commands_reexecute_after_actual_bounded_proof_release() {
    let temp = Temp::new();
    let mut writer = writer_new(&temp);
    let bootstrap = writer.bootstrap().clone();
    let mut store = SegmentStore::new();
    writer_start(&mut writer, "Main.saveTrackHeight", vec![V::number(19.)]);
    writer_publish(&mut writer, &mut store);
    for _ in 0..17 {
        for _ in 0..256 {
            writer_apply(&mut writer, Command::DrainReady { limit: 0 });
        }
        writer_publish(&mut writer, &mut store);
        assert_eq!(writer.session().receipts().count(), 0);
    }
    let publication = writer.expectation().unwrap().sealed;
    assert_eq!(publication.head.source.commands, 4353);
    assert_eq!(publication.head.segments, 18);
    assert!(writer
        .session()
        .financial_runtime()
        .collection("consensus")
        .iter()
        .any(|r| r.get("_id") == s("TrackHeight") && r.get("value") == V::number(19.)));
    let fresh_temp = Temp::new();
    let fresh = raw_origin(&fresh_temp);
    let pin = physical(&fresh);
    let replay = NativeStepHistoryWriterV1::reexecute(
        &bootstrap,
        &publication,
        |digest| Ok(store[&digest].clone()),
        fresh,
        &pin,
        NativeStepProfileV1::Immediate,
    )
    .unwrap();
    assert_eq!(replay.session().head().unwrap(), publication.head.source);
    assert_eq!(
        replay.session().financial_runtime().state_value().unwrap(),
        writer.session().financial_runtime().state_value().unwrap()
    );
    assert_eq!(replay.session().receipts().count(), 0);
}

#[test]
fn file_retry_local_and_same_owner_main_calls_survive_segment_rotation_and_reexecution() {
    let temp = Temp::new();
    let session = file_controller(&temp, 1);
    let expected = session.expectation().unwrap();
    let mut writer = NativeStepHistoryWriterV1::adopt(
        session,
        &expected,
        "actual file-copy continuation segments",
        |_| Ok(()),
    )
    .unwrap();
    let bootstrap = writer.bootstrap().clone();
    let mut store = SegmentStore::new();
    writer_start(
        &mut writer,
        "Persistence.safeCopy",
        vec![s("/tl-data/ltc-main/a.db"), s("/tl-data/copied.db")],
    );
    let timer = writer.session().kernel().next_timer().unwrap();
    writer_publish(&mut writer, &mut store);
    writer_start(&mut writer, "Main.saveTrackHeight", vec![V::number(11.)]);
    let publication = writer_publish(&mut writer, &mut store);
    let fresh_temp = Temp::new();
    let fresh = raw_origin(&fresh_temp);
    let raw_pin = physical(&fresh);
    let restored = NativeStepHistoryWriterV1::reexecute(
        &bootstrap,
        &publication,
        |digest| Ok(store[&digest].clone()),
        fresh,
        &raw_pin,
        NativeStepProfileV1::FileRetries,
    )
    .unwrap();
    let mut replay = restored.into_session();
    assert_eq!(replay.kernel().next_timer(), Some(timer));
    assert_eq!(replay.head().unwrap(), writer.session().head().unwrap());
    writer_apply(
        &mut writer,
        Command::FireTimer {
            id: timer,
            generation: 17,
        },
    );
    apply(
        &mut replay,
        Command::FireTimer {
            id: timer,
            generation: 17,
        },
    );
    assert_eq!(replay.head().unwrap(), writer.session().head().unwrap());
    assert_eq!(service_value(&replay), service_value(writer.session()));
    assert_eq!(
        replay.financial_runtime().state_value().unwrap(),
        writer.session().financial_runtime().state_value().unwrap()
    );
    assert!(replay.financial_runtime().collection("txIndex").is_empty());
}

include!("support/source_main_native_step_byte_capacity.rs");

include!("support/native_segment_capacity_typed.rs");
