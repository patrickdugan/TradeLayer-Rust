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

use std::io::Write;
use tradelayer_core::compact_journal::{CatalogPublication, CompactingJournal, NativeAppendPin};
use tradelayer_core::source_contract_replay::source_durable::SourceDurableFilesystem as Fs;
use tradelayer_core::source_main_native_intent_store::{
    NativeIntentStoreError, NativeIntentStorePinV1, NativeIntentStoreV1,
    NativeObservedAppendBindingV1, NativeWriteIntentCandidatesV1,
};
use tradelayer_core::storage::{Mutation, Recovery};
fn store_new(temp: &Temp) -> NativeIntentStoreV1 {
    NativeIntentStoreV1::create(
        &temp.root,
        Namespace::new("LTC", false, "consensus").unwrap(),
        obj(&[
            ("sourceProfile", s("immediate")),
            ("purpose", s("actual one-owner source controls")),
        ]),
        V::Null,
        Fs::FileSyncOnly,
    )
    .unwrap()
}
fn raw_step(command: &Command) -> Vec<u8> {
    let mut out = vec![];
    command
        .visit_canonical_fields(&mut |path, value| {
            let raw = value.diagnostic_json()?;
            out.extend_from_slice(&(path.len() as u32).to_be_bytes());
            out.extend_from_slice(path.as_bytes());
            out.extend_from_slice(&(raw.len() as u32).to_be_bytes());
            out.extend_from_slice(raw.as_bytes());
            Ok(())
        })
        .unwrap();
    out
}
fn actual_receipt_bytes(
    receipt: &tradelayer_core::source_main_native_session::NativeStepReceiptV1,
) -> Vec<u8> {
    let mut out = b"actual-complete-step-observations-13".to_vec();
    for raw in [
        &receipt.result_frames,
        &receipt.kernel_frames,
        &receipt.native_frames,
    ] {
        out.extend_from_slice(&(raw.len() as u32).to_be_bytes());
        out.extend_from_slice(raw);
    }
    out.extend_from_slice(&receipt.financial_commitment);
    out.extend_from_slice(&receipt.head);
    out
}
fn before_value(
    session: &tradelayer_core::source_main_native_session::SourceMainNativeStepSessionV1,
) -> V {
    let expected = session.expectation().unwrap();
    obj(&[
        ("sourceCount", s(&expected.head.commands.to_string())),
        ("sourceHead", s(&hex_bytes(&expected.head.commitment))),
        ("nativeIdentity", s(&expected.physical.persistent_identity)),
        (
            "nativeCatalog",
            s(&expected.physical.canonical_catalog.to_string_lossy()),
        ),
        (
            "nativeSequence",
            s(&expected.physical.metadata.sequence.to_string()),
        ),
    ])
}
fn hex_bytes(raw: &[u8]) -> String {
    raw.iter().map(|v| format!("{v:02x}")).collect()
}
fn tree_bytes(root: &std::path::Path) -> Vec<(PathBuf, Vec<u8>)> {
    fn walk(root: &std::path::Path, out: &mut Vec<(PathBuf, Vec<u8>)>) {
        let mut paths = std::fs::read_dir(root)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect::<Vec<_>>();
        paths.sort();
        for path in paths {
            if path.is_dir() {
                walk(&path, out);
            } else {
                let raw = match std::fs::read(&path) {
                    Ok(raw) => raw,
                    Err(error) if matches!(error.raw_os_error(), Some(32 | 33)) => {
                        let meta = std::fs::metadata(&path).unwrap();
                        format!(
                            "exclusive native journal: {} {:?}",
                            meta.len(),
                            meta.modified().unwrap()
                        )
                        .into_bytes()
                    }
                    Err(error) => panic!("unexpected observer failure: {error}"),
                };
                out.push((path, raw));
            }
        }
    }
    let mut result = vec![];
    walk(root, &mut result);
    result
}
#[test]
fn actual_source_owner_runs_only_after_synced_command_intent_then_complete_acceptance_ack() {
    let store_temp = Temp::new();
    let source_temp = Temp::new();
    let mut store = store_new(&store_temp);
    let mut session = controller(&source_temp);
    let source_state = session.financial_runtime().state_value().unwrap();
    let command = Command::Start {
        method: "Main.setPause".into(),
        arguments: vec![],
    };
    let raw = raw_step(&command);
    let initial = store.pin().unwrap();
    let mut permit = store
        .sync_command_intent(&initial, &raw, before_value(&session), 1024 * 1024)
        .unwrap();
    assert_eq!(store.state().unwrap().get("kind"), s("CommandIntent"));
    assert_eq!(store.read_blob(permit.command()).unwrap(), raw);
    assert_eq!(
        session.financial_runtime().state_value().unwrap(),
        source_state
    );
    let receipt = apply(&mut session, command);
    assert_eq!(
        session
            .financial_runtime()
            .state_value()
            .unwrap()
            .get("mainProduction")
            .get("service")
            .get("pause"),
        V::Bool(true)
    );
    let accepted = actual_receipt_bytes(&receipt);
    let accepted_pin = store
        .sync_accepted(&mut permit, &accepted, before_value(&session))
        .unwrap();
    assert_eq!(store.state().unwrap().get("kind"), s("Accepted"));
    assert_eq!(accepted_pin.sequence, initial.sequence + 2);
    let ack = store.sync_ack(&mut permit).unwrap();
    assert_eq!(store.state().unwrap().get("kind"), s("Idle"));
    assert_eq!(
        store.state().unwrap().get("accepted"),
        before_value(&session)
    );
    assert_eq!(ack.sequence, initial.sequence + 3);
    let previous_ack = store.state().unwrap().get("ack");
    let command = Command::Start {
        method: "Main.setPause".into(),
        arguments: vec![],
    };
    let mut permit = store
        .sync_command_intent(
            &store.pin().unwrap(),
            &raw_step(&command),
            before_value(&session),
            1024 * 1024,
        )
        .unwrap();
    let receipt = apply(&mut session, command);
    store
        .sync_accepted(
            &mut permit,
            &actual_receipt_bytes(&receipt),
            before_value(&session),
        )
        .unwrap();
    store.sync_ack(&mut permit).unwrap();
    let latest = store.state().unwrap().get("ack");
    let latest_blob = tradelayer_core::source_main_native_intent_store::NativeIntentBlobV1 {
        bytes: u64_field(latest.get("bytes")),
        sha256: match latest.get("sha256") {
            V::String(s) => decode_hash(&s),
            _ => panic!(),
        },
    };
    let raw = store.read_blob(&latest_blob).unwrap();
    let record = tradelayer_core::property_catalog::parse_diagnostic_value(
        std::str::from_utf8(&raw).unwrap(),
    )
    .unwrap();
    assert_eq!(
        record.get("previousAck"),
        previous_ack,
        "Ack retains immutable accepted/native-prefix history"
    );
    assert_eq!(
        session
            .financial_runtime()
            .state_value()
            .unwrap()
            .get("mainProduction")
            .get("service")
            .get("pause"),
        V::Bool(false)
    );
}
#[test]
fn foreign_and_reopened_permits_reject_before_any_store_file_mutation() {
    let first = Temp::new();
    let second = Temp::new();
    let mut a = store_new(&first);
    let mut b = store_new(&second);
    let raw = raw_step(&Command::DrainReady { limit: 0 });
    let mut permit = a
        .sync_command_intent(&a.pin().unwrap(), &raw, V::Null, 1024)
        .unwrap();
    let before = tree_bytes(&second.root);
    assert!(matches!(
        b.sync_accepted(&mut permit, b"actual", V::Null),
        Err(NativeIntentStoreError::ForeignOrReopenedPermit)
    ));
    assert_eq!(tree_bytes(&second.root), before);
    let pin = a.pin().unwrap();
    drop(a);
    let mut reopened = NativeIntentStoreV1::open(
        &first.root,
        Namespace::new("LTC", false, "consensus").unwrap(),
        &pin,
        Fs::FileSyncOnly,
    )
    .unwrap();
    let before = tree_bytes(&first.root);
    assert!(matches!(
        reopened.sync_accepted(&mut permit, b"actual", V::Null),
        Err(NativeIntentStoreError::ForeignOrReopenedPermit)
    ));
    assert_eq!(tree_bytes(&first.root), before);
}
#[test]
fn accepted_capacity_stop_retains_actual_financial_prefix_and_pending_intent() {
    let store_temp = Temp::new();
    let source_temp = Temp::new();
    let mut store = store_new(&store_temp);
    let mut session = controller(&source_temp);
    let command = Command::Start {
        method: "Main.setPause".into(),
        arguments: vec![],
    };
    let mut permit = store
        .sync_command_intent(
            &store.pin().unwrap(),
            &raw_step(&command),
            before_value(&session),
            1,
        )
        .unwrap();
    let receipt = apply(&mut session, command);
    let pin = store.pin().unwrap();
    assert!(matches!(
        store.sync_accepted(
            &mut permit,
            &actual_receipt_bytes(&receipt),
            before_value(&session)
        ),
        Err(NativeIntentStoreError::Capacity)
    ));
    assert_eq!(store.pin().unwrap(), pin);
    assert_eq!(store.state().unwrap().get("kind"), s("CommandIntent"));
    assert_eq!(
        session
            .financial_runtime()
            .state_value()
            .unwrap()
            .get("mainProduction")
            .get("service")
            .get("pause"),
        V::Bool(true)
    );
    assert_eq!(session.head().unwrap().commands, 1);
}
fn native_new(root: &std::path::Path) -> CompactingJournal {
    std::fs::create_dir_all(root).unwrap();
    CompactingJournal::open(
        root,
        Namespace::new("LTC", false, "txIndex").unwrap(),
        Recovery::RejectIncompleteTail,
    )
    .unwrap()
    .0
}
#[test]
fn native_intent_retains_real_prepared_candidates_and_requires_same_store_observed_post() {
    let control = Temp::new();
    let native = Temp::new();
    let foreign = Temp::new();
    let mut store = store_new(&control);
    let mut journal = native_new(&native.root);
    let mut other = native_new(&foreign.root);
    let mut permit = store
        .sync_command_intent(
            &store.pin().unwrap(),
            &raw_step(&Command::DrainReady { limit: 0 }),
            V::Null,
            1024,
        )
        .unwrap();
    for ordinal in 0..2 {
        let plan = journal
            .prepare_native_append(vec![Mutation::Put {
                key: format!("row-{ordinal}"),
                value: b"actual metadata".to_vec(),
            }])
            .unwrap();
        let candidates = NativeWriteIntentCandidatesV1::from_prepared_append(
            &plan,
            obj(&[
                ("complete", V::Bool(true)),
                ("sequence", s(&journal.sequence().unwrap().to_string())),
            ]),
        )
        .unwrap();
        let observed_before = NativeObservedAppendBindingV1::observe(&journal).unwrap();
        let intent = store.sync_native_intent(&mut permit, &candidates).unwrap();
        assert_eq!(store.state().unwrap().get("kind"), s("NativeIntent"));
        assert!(matches!(
            store.sync_native_prefix_ack(&mut permit, &observed_before),
            Err(NativeIntentStoreError::WrongPin)
        ));
        assert_eq!(store.pin().unwrap(), intent);
        journal.commit_prepared_native_append(&plan).unwrap();
        let foreign_plan = other
            .prepare_native_append(vec![Mutation::Put {
                key: format!("row-{ordinal}"),
                value: b"actual metadata".to_vec(),
            }])
            .unwrap();
        other.commit_prepared_native_append(&foreign_plan).unwrap();
        let foreign_post = NativeObservedAppendBindingV1::observe(&other).unwrap();
        assert_eq!(
            foreign_post.value().get("append").get("logicalSequence"),
            candidates
                .expected_post()
                .get("append")
                .get("logicalSequence")
        );
        assert_ne!(
            foreign_post.value().get("catalog"),
            candidates.expected_post().get("catalog")
        );
        assert!(matches!(
            store.sync_native_prefix_ack(&mut permit, &foreign_post),
            Err(NativeIntentStoreError::WrongPin)
        ));
        assert_eq!(store.pin().unwrap(), intent);
        let actual_post = NativeObservedAppendBindingV1::observe(&journal).unwrap();
        assert_eq!(actual_post.value(), candidates.expected_post());
        store
            .sync_native_prefix_ack(&mut permit, &actual_post)
            .unwrap();
    }
    assert_ne!(store.state().unwrap().get("nativePrefix"), V::Undefined);
    store
        .sync_accepted(
            &mut permit,
            b"complete accepted source observations",
            V::Bool(true),
        )
        .unwrap();
    store.sync_ack(&mut permit).unwrap();
}
#[test]
fn actual_compaction_candidate_records_before_intermediate_and_post_before_commit() {
    let control = Temp::new();
    let native = Temp::new();
    let mut store = store_new(&control);
    let mut journal = native_new(&native.root);
    journal
        .configure(tradelayer_core::compact_journal::CompactionConfig {
            target_log_bytes: 65536,
            retained_ancestors: 1,
        })
        .unwrap();
    let value = vec![7u8; 32768];
    for _ in 0..20 {
        let plan = journal
            .prepare_native_append(vec![Mutation::Put {
                key: "same-live-key".into(),
                value: value.clone(),
            }])
            .unwrap();
        if let Some(intermediate) = plan.intermediate() {
            let before = NativeObservedAppendBindingV1::observe(&journal).unwrap();
            let candidates = NativeWriteIntentCandidatesV1::from_prepared_append(
                &plan,
                obj(&[("complete", V::Bool(true))]),
            )
            .unwrap();
            assert_ne!(
                candidates
                    .independently_retained_facts()
                    .get("intermediate"),
                V::Null
            );
            assert_eq!(
                intermediate.logical_sequence,
                plan.before().logical_sequence
            );
            assert_eq!(intermediate.publication, plan.after().publication);
            let mut permit = store
                .sync_command_intent(
                    &store.pin().unwrap(),
                    &raw_step(&Command::DrainReady { limit: 0 }),
                    V::Null,
                    1024,
                )
                .unwrap();
            store.sync_native_intent(&mut permit, &candidates).unwrap();
            assert_eq!(
                NativeObservedAppendBindingV1::observe(&journal).unwrap(),
                before,
                "sync native intent does not mutate native store"
            );
            journal.commit_prepared_native_append(&plan).unwrap();
            store
                .sync_native_prefix_ack(
                    &mut permit,
                    &NativeObservedAppendBindingV1::observe(&journal).unwrap(),
                )
                .unwrap();
            return;
        }
        journal.commit_prepared_native_append(&plan).unwrap();
    }
    panic!("test must exercise a real compaction candidate");
}

fn emit_pin(pin: &NativeIntentStorePinV1) {
    println!("PIN {} {}", pin.sequence, hex_bytes(&pin.state_sha256));
    std::io::stdout().flush().unwrap();
}
fn decode_hash(raw: &str) -> [u8; 32] {
    let v = (0..raw.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&raw[i..i + 2], 16).unwrap())
        .collect::<Vec<_>>();
    v.try_into().unwrap()
}
#[test]
#[ignore]
fn native_intent_child_entry() {
    let Ok(mode) = std::env::var("NATIVE_INTENT_CHILD") else {
        return;
    };
    let root = PathBuf::from(std::env::var_os("NATIVE_INTENT_ROOT").unwrap());
    let mut expected = NativeIntentStorePinV1 {
        canonical_root: root.canonicalize().unwrap(),
        context: std::fs::read(root.join("external-context.bin")).unwrap(),
        sequence: std::env::var("NATIVE_INTENT_SEQ").unwrap().parse().unwrap(),
        state_sha256: decode_hash(&std::env::var("NATIVE_INTENT_SHA").unwrap()),
    };
    let mut store = NativeIntentStoreV1::open(
        &root,
        Namespace::new("LTC", false, "consensus").unwrap(),
        &expected,
        Fs::FileSyncOnly,
    )
    .unwrap();
    let command = Command::Start {
        method: "Main.setPause".into(),
        arguments: vec![],
    };
    if mode == "orphan_blob" {
        let path = root.join("native13-control-blobs/pending-actual-torn-child.tmp");
        let mut file = std::fs::File::create(path).unwrap();
        file.write_all(&raw_step(&command)[..17]).unwrap();
        file.sync_all().unwrap();
        emit_pin(&expected);
        std::process::exit(76);
    }
    let mut permit = store
        .sync_command_intent(&expected, &raw_step(&command), V::Null, 1024 * 1024)
        .unwrap();
    if mode == "intent" {
        emit_pin(permit.independently_retained_pin());
        std::process::exit(76);
    }
    if mode == "torn_tail" {
        expected = store.pin().unwrap();
        let path = store.journal_path().to_path_buf();
        emit_pin(&expected);
        drop(store);
        let mut file = std::fs::OpenOptions::new().append(true).open(path).unwrap();
        file.write_all(b"bad").unwrap();
        file.sync_all().unwrap();
        std::process::exit(76);
    }
    if mode.starts_with("native_") {
        let mut journal = native_new(&root.join("actual-native"));
        let plan = journal
            .prepare_native_append(vec![Mutation::Put {
                key: "original-source-metadata".into(),
                value: b"actual child write".to_vec(),
            }])
            .unwrap();
        let candidates = NativeWriteIntentCandidatesV1::from_prepared_append(
            &plan,
            obj(&[("complete", V::Bool(true))]),
        )
        .unwrap();
        store.sync_native_intent(&mut permit, &candidates).unwrap();
        println!(
            "CANDIDATES {}",
            candidates
                .independently_retained_facts()
                .diagnostic_json()
                .unwrap()
        );
        std::io::stdout().flush().unwrap();
        if mode == "native_intent" {
            emit_pin(permit.independently_retained_pin());
            std::process::exit(76);
        }
        journal.commit_prepared_native_append(&plan).unwrap();
        if mode == "native_synced" {
            emit_pin(permit.independently_retained_pin());
            std::process::exit(76);
        }
        let actual = NativeObservedAppendBindingV1::observe(&journal).unwrap();
        store.sync_native_prefix_ack(&mut permit, &actual).unwrap();
        emit_pin(permit.independently_retained_pin());
        std::process::exit(76);
    }
    let source = Temp::new();
    let mut session = controller(&source);
    let receipt = apply(&mut session, command);
    store
        .sync_accepted(
            &mut permit,
            &actual_receipt_bytes(&receipt),
            before_value(&session),
        )
        .unwrap();
    if mode == "ack" {
        store.sync_ack(&mut permit).unwrap();
    }
    emit_pin(permit.independently_retained_pin());
    std::process::exit(76);
}
fn u64_field(v: V) -> u64 {
    let V::String(s) = v else { panic!() };
    s.parse().unwrap()
}
fn pin_from_facts(v: &V) -> NativeAppendPin {
    NativeAppendPin {
        logical_sequence: u64_field(v.get("logicalSequence")),
        publication: CatalogPublication {
            generation: u64_field(v.get("generation")),
            parent: match v.get("parent") {
                V::Null => None,
                v => Some(u64_field(v)),
            },
            physical_creation_sequence: u64_field(v.get("physicalCreation")),
            creation_sequence: u64_field(v.get("logicalCreation")),
            manifest_sha256d: match v.get("manifest") {
                V::String(s) => decode_hash(&s),
                _ => panic!(),
            },
        },
    }
}
#[test]
fn eight_real_process_exits_preserve_phase_and_never_repair_an_unknown_tail() {
    for mode in [
        "orphan_blob",
        "intent",
        "native_intent",
        "native_synced",
        "native_ack",
        "accepted",
        "ack",
        "torn_tail",
    ] {
        let temp = Temp::new();
        let store = store_new(&temp);
        let initial = store.pin().unwrap();
        let journal_path = store.journal_path().to_path_buf();
        drop(store);
        std::fs::write(temp.root.join("external-context.bin"), &initial.context).unwrap();
        let source_temp = temp.root.join("source-children");
        std::fs::create_dir(&source_temp).unwrap();
        let target = format!("{}::native_intent_child_entry", module_path!());
        let target = target
            .split_once("::")
            .map_or("native_intent_child_entry", |(_, rest)| rest);
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", target, "--nocapture"])
            .env("NATIVE_INTENT_CHILD", mode)
            .env("NATIVE_INTENT_ROOT", &temp.root)
            .env("NATIVE_INTENT_SEQ", initial.sequence.to_string())
            .env("NATIVE_INTENT_SHA", hex_bytes(&initial.state_sha256))
            .env("MAIN_STEP_TMP", &source_temp)
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(76),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout).unwrap();
        let line = stdout.lines().find(|l| l.starts_with("PIN ")).unwrap();
        let parts = line.split_whitespace().collect::<Vec<_>>();
        let expected = NativeIntentStorePinV1 {
            sequence: parts[1].parse().unwrap(),
            state_sha256: decode_hash(parts[2]),
            ..initial.clone()
        };
        let length = std::fs::metadata(&journal_path).unwrap().len();
        let opened = NativeIntentStoreV1::open(
            &temp.root,
            Namespace::new("LTC", false, "consensus").unwrap(),
            &expected,
            Fs::FileSyncOnly,
        );
        if mode == "torn_tail" {
            assert!(matches!(
                opened,
                Err(NativeIntentStoreError::Storage(
                    tradelayer_core::storage::StorageError::IncompleteTail { .. }
                ))
            ));
            assert_eq!(std::fs::metadata(&journal_path).unwrap().len(), length);
            continue;
        }
        let opened = opened.unwrap();
        let kind = opened.state().unwrap().get("kind");
        let expected_kind = match mode {
            "orphan_blob" | "ack" => "Idle",
            "intent" | "native_ack" => "CommandIntent",
            "native_intent" | "native_synced" => "NativeIntent",
            "accepted" => "Accepted",
            _ => unreachable!(),
        };
        assert_eq!(kind, s(expected_kind), "{mode}");
        if mode == "native_synced" {
            let line = stdout
                .lines()
                .find(|l| l.starts_with("CANDIDATES "))
                .unwrap();
            let facts =
                tradelayer_core::property_catalog::parse_diagnostic_value(&line[11..]).unwrap();
            let expected_post = pin_from_facts(&facts.get("post").get("append"));
            let (native, _) = CompactingJournal::open_pinned(
                &temp.root.join("actual-native"),
                Namespace::new("LTC", false, "txIndex").unwrap(),
                Recovery::RejectIncompleteTail,
                &expected_post.publication,
                expected_post.logical_sequence,
            )
            .unwrap();
            assert_eq!(
                native.get("original-source-metadata").unwrap(),
                Some(b"actual child write".as_slice())
            );
            assert_eq!(
                opened.state().unwrap().get("kind"),
                s("NativeIntent"),
                "known native post is parked until exact source continuation exists"
            );
        }
    }
}

#[test]
fn namespace_substitution_rejects_before_store_open() {
    let temp = Temp::new();
    let first = store_new(&temp);
    let pin = first.pin().unwrap();
    drop(first);
    let other = NativeIntentStoreV1::create(
        &temp.root,
        Namespace::new("LTC", false, "txIndex").unwrap(),
        obj(&[
            ("sourceProfile", s("immediate")),
            ("purpose", s("actual one-owner source controls")),
        ]),
        V::Null,
        Fs::FileSyncOnly,
    )
    .unwrap();
    let other_pin = other.pin().unwrap();
    drop(other);
    eprintln!("old control pin collision={}", pin == other_pin);
    let before = tree_bytes(&temp.root);
    let reopened = NativeIntentStoreV1::open(
        &temp.root,
        Namespace::new("LTC", false, "txIndex").unwrap(),
        &pin,
        Fs::FileSyncOnly,
    );
    assert!(
        matches!(reopened, Err(NativeIntentStoreError::WrongContext)),
        "namespace substitution must reject before any store mutation"
    );
    assert_eq!(tree_bytes(&temp.root), before);
}
