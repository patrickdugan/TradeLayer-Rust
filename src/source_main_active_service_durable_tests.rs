// Actual-owner candidate tests. Parent must compile these with the new bridge.
// Every raw row/RPC document and expected observation comes from the unchanged
// configured-service source corpus. Expected state is never imported.
use super::*;
use crate::source_contract_replay::{SourceContractCommand, SourceContractCommandOutcome};
use crate::source_main_native_owners::NativeMainExpectation;
use crate::storage::Namespace;
use std::sync::atomic::{AtomicU64, Ordering};
static SERIAL: AtomicU64 = AtomicU64::new(0);
struct Temp {
    root: PathBuf,
    parent: PathBuf,
}
impl Temp {
    fn new() -> Self {
        let parent = std::env::var_os("MAIN_SERVICE_TMP")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        std::fs::create_dir_all(&parent).unwrap();
        let parent = parent.canonicalize().unwrap();
        let root = parent.join(format!(
            "actual-main-active-wal-{}-{}",
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
fn arr(v: V) -> Vec<V> {
    let V::Array(rows) = v else {
        panic!("array: {v:?}")
    };
    rows
}
include!("source_main_native_service_test_transport.rs");
fn invoke(financial: &mut SourceContractReplay, target: &str, args: Vec<V>) {
    let result = financial
        .apply(SourceContractCommand::Invoke(DependencyCall {
            target: target.into(),
            arguments: args,
        }))
        .unwrap();
    assert!(
        matches!(
            result.outcome,
            SourceContractCommandOutcome::Dependency(
                DependencyReturn::Immediate(_) | DependencyReturn::Pending(_)
            )
        ),
        "{target}: {:?}",
        result.outcome
    );
}
fn setup(temp: &Temp, mode: &str) -> (SourceMainNativeSessionV13, V) {
    let input = record(mode, "input");
    let reset = untag(raw_get(input, "rawReset"));
    let mut financial =
        SourceContractReplay::with_policy(SourceContractRuntimePolicy::MainOrchestrationV12)
            .unwrap();
    // Reproduce DECLARED reference fixture construction followed by raw
    // reset(empty:true). No expected business table/state is read or imported.
    assert_eq!(
        untag(tagged_field(raw_get(input, "rawReset"), "empty")),
        V::Bool(true)
    );
    let constructor = untag(raw_get(input, "rawHarnessConstructorInput"));
    assert_eq!(
        constructor.get("profile"),
        s("TL-RAW-REFERENCE-FIXTURE-CONSTRUCTION-1")
    );
    for raw_collection in arr(constructor.get("collections")) {
        let V::String(collection) = raw_collection.get("collection") else {
            panic!("raw collection name")
        };
        financial
            .apply(SourceContractCommand::Import {
                collection,
                rows: vec![],
            })
            .unwrap();
    }
    let client = reset.get("clientProfile");
    let client = obj(&[("chain", client.get("chain")), ("test", client.get("test"))]);
    invoke(
        &mut financial,
        "SourceActivation.configureClient",
        vec![client.clone()],
    );
    invoke(&mut financial, "Activation.init", vec![reset.get("init")]);
    invoke(
        &mut financial,
        "SourceMain.configureProduction",
        vec![obj(&[
            ("client", client),
            ("io", reset.get("orchestration")),
            ("genesisBlock", reset.get("mainGenesis")),
        ])],
    );
    invoke(
        &mut financial,
        "SourceMain.configureService",
        vec![obj(&[
            ("fileInputs", V::Array(vec![])),
            ("directoryInputs", V::Array(vec![])),
            (
                "fileIOInputs",
                obj(&[("failures", V::Array(vec![])), ("mtimes", V::Array(vec![]))]),
            ),
        ])],
    );
    financial
        .apply(SourceContractCommand::SetHeight(
            reset.get("clientProfile").get("height"),
        ))
        .unwrap();
    let mut index = SourceCompactIndex::create(
        &temp.root,
        Namespace::new("LTC", false, "txIndex").unwrap(),
        "actual configured service raw inputs",
    )
    .unwrap();
    for row in tagged_items(raw_get(input, "nativeInput"))
        .into_iter()
        .map(untag)
    {
        index.import_row(&index.snapshot().unwrap(), row).unwrap();
    }
    let docs = untag(raw_get(input, "rawRPCDocuments"));
    let docs = if docs == V::Undefined {
        V::Array(vec![])
    } else {
        V::Array(vec![
            obj(&[
                ("id", docs.get("id")),
                ("raw", docs.get("raw")),
                ("document", docs.get("document")),
            ]),
            obj(&[
                ("id", docs.get("parent").get("txid")),
                ("document", docs.get("parent")),
            ]),
        ])
    };
    let expected = NativeMainExpectation {
        financial_head: financial.head(),
        namespace: Namespace::new("LTC", false, "txIndex").unwrap(),
        metadata: index.snapshot().unwrap(),
    };
    (
        SourceMainNativeSessionV13::adopt(
            NativeMainOwners::adopt(financial, index, &expected).unwrap(),
        )
        .unwrap(),
        docs,
    )
}

fn wal_setup(temp: &Temp, mode: &str) -> (SourceMainNativeDurableActiveServiceV1, V) {
    let (owner, docs) = setup(temp, mode);
    let controller = SourceMainNativeDurableActiveServiceV1::create(
        owner,
        &temp.root,
        Namespace::new("LTC", false, "consensus").unwrap(),
        crate::source_contract_replay::source_durable::SourceDurableFilesystem::FileSyncOnly,
        64 * 1024 * 1024,
    )
    .unwrap();
    (controller, docs)
}
fn run_action(
    owner: &mut SourceMainNativeDurableActiveServiceV1,
    action: NativeMainActiveServiceActionV1,
) {
    let count = owner.history().len();
    owner.prepare_action(action).unwrap();
    assert_eq!(owner.advance().unwrap(), NativeMainActiveWalPhaseV1::Idle);
    assert_eq!(owner.history().len(), count + 1);
}
fn wal_start(owner: &mut SourceMainNativeDurableActiveServiceV1, docs: V) {
    run_action(
        owner,
        NativeMainActiveServiceActionV1::Start {
            start: V::number(10.),
            generation: 71,
            documents: docs,
        },
    );
}
fn drive_to_live(owner: &mut SourceMainNativeDurableActiveServiceV1) {
    for _ in 0..12 {
        run_action(
            owner,
            NativeMainActiveServiceActionV1::Advance { limit: 4096 },
        );
        let service = owner.active_service().unwrap();
        if service.pending_write().is_some() {
            if service.caller_result().is_some() {
                return;
            }
            run_action(owner, NativeMainActiveServiceActionV1::AppendPending);
            run_action(owner, NativeMainActiveServiceActionV1::ResolvePost);
        }
    }
    panic!("source scene did not reach held live native await")
}
#[test]
fn raw_action_codec_roundtrips_exact_fields_and_refuses_legacy_magic() {
    let actions = vec![
        NativeMainActiveServiceActionV1::Start {
            start: V::number(-0.0),
            generation: u64::MAX,
            documents: V::Array(vec![obj(&[
                ("x", V::Undefined),
                ("n", V::number(f64::NAN)),
            ])]),
        },
        NativeMainActiveServiceActionV1::Advance { limit: 4096 },
        NativeMainActiveServiceActionV1::Pause,
        NativeMainActiveServiceActionV1::AppendPending,
        NativeMainActiveServiceActionV1::ResolvePost,
        NativeMainActiveServiceActionV1::FireTimer {
            id: 9,
            generation: 71,
        },
        NativeMainActiveServiceActionV1::ClearTimer {
            id: 9,
            generation: 71,
        },
        NativeMainActiveServiceActionV1::Shutdown(vec![V::Null, s("raw reason")]),
    ];
    for action in actions {
        let raw = action.canonical_action_bytes().unwrap();
        assert!(raw.starts_with(b"TLMASA\x01"));
        assert_eq!(
            NativeMainActiveServiceActionV1::from_canonical_action_bytes(&raw).unwrap(),
            action
        );
        let mut changed = raw.clone();
        changed.push(0);
        assert!(NativeMainActiveServiceActionV1::from_canonical_action_bytes(&changed).is_err());
        assert!(SourceContractReplay::from_checkpoint(
            &raw,
            SourceContractReplay::new().unwrap().head()
        )
        .is_err());
    }
}
#[test]
fn actual_start_is_preceded_by_synced_raw_intent_and_acked_history_is_replayable() {
    let temp = Temp::new();
    let (mut owner, docs) = wal_setup(&temp, "detached-live-rpc-failure");
    let before = owner.observation().unwrap();
    let prefix = owner.financial_runtime().native_main_row_effects().to_vec();
    owner
        .prepare_action(NativeMainActiveServiceActionV1::Start {
            start: V::number(10.),
            generation: 71,
            documents: docs,
        })
        .unwrap();
    assert_eq!(owner.observation().unwrap(), before);
    assert_eq!(owner.financial_runtime().native_main_row_effects(), prefix);
    owner.barrier_fault = Some((
        NativeMainActiveWalPhaseV1::Intent,
        crate::source_main_active_service_wal::TestFault::AfterCommit,
    ));
    assert!(owner.advance().is_err());
    assert!(owner.active_service().is_none());
    assert_eq!(owner.observation().unwrap(), before);
    assert_eq!(owner.history().len(), 0);
    assert_eq!(
        owner.resolve_control_write().unwrap(),
        ActiveServiceWalResolvedV1::Post
    );
    assert!(owner.active_service().is_some());
    owner.advance().unwrap();
    assert_eq!(owner.history().len(), 1);
    let receipt = &owner.history()[0];
    let raw = owner.read_history_blob(&receipt.action).unwrap();
    assert!(matches!(
        NativeMainActiveServiceActionV1::from_canonical_action_bytes(&raw).unwrap(),
        NativeMainActiveServiceActionV1::Start { generation: 71, .. }
    ));
    assert_eq!(owner.read_history_blob(&receipt.before).unwrap(), before);
    assert!(owner.read_history_blob(&receipt.accepted).unwrap().len() > before.len());
}
#[test]
fn accepted_and_ack_lost_returns_do_not_rerun_real_source_action() {
    for phase in [
        NativeMainActiveWalPhaseV1::Accepted,
        NativeMainActiveWalPhaseV1::Ack,
    ] {
        let temp = Temp::new();
        let (mut owner, docs) = wal_setup(&temp, "live-iteration-pause-boundary");
        wal_start(&mut owner, docs);
        owner
            .prepare_action(NativeMainActiveServiceActionV1::Advance { limit: 4096 })
            .unwrap();
        owner.barrier_fault = Some((
            phase,
            crate::source_main_active_service_wal::TestFault::AfterCommit,
        ));
        assert!(owner.advance().is_err());
        assert_eq!(owner.phase(), phase);
        let observation = owner.observation().unwrap();
        let effects = owner.active_service().unwrap().effects().to_vec();
        let actions = owner.active_service().unwrap().history().len();
        let history = owner.history().len();
        assert!(owner
            .prepare_action(NativeMainActiveServiceActionV1::Pause)
            .is_err());
        assert_eq!(
            owner.resolve_control_write().unwrap(),
            ActiveServiceWalResolvedV1::Post
        );
        owner.advance().unwrap();
        assert_eq!(owner.observation().unwrap(), observation);
        assert_eq!(owner.active_service().unwrap().effects(), effects);
        assert_eq!(owner.active_service().unwrap().history().len(), actions);
        assert_eq!(owner.history().len(), history + 1);
    }
}
#[test]
fn before_accepted_retry_writes_only_same_wal_bytes_without_decoding_again() {
    let temp = Temp::new();
    let (mut owner, docs) = wal_setup(&temp, "same-held-live-validity");
    wal_start(&mut owner, docs);
    owner
        .prepare_action(NativeMainActiveServiceActionV1::Advance { limit: 4096 })
        .unwrap();
    owner.barrier_fault = Some((
        NativeMainActiveWalPhaseV1::Accepted,
        crate::source_main_active_service_wal::TestFault::BeforeCommit,
    ));
    assert!(owner.advance().is_err());
    let before = owner.observation().unwrap();
    let effects = owner.active_service().unwrap().effects().to_vec();
    let actions = owner.active_service().unwrap().history().len();
    assert_eq!(
        owner.resolve_control_write().unwrap(),
        ActiveServiceWalResolvedV1::Before
    );
    owner.advance().unwrap();
    assert_eq!(owner.observation().unwrap(), before);
    assert_eq!(owner.active_service().unwrap().effects(), effects);
    assert_eq!(owner.active_service().unwrap().history().len(), actions);
}
#[test]
fn pause_and_shutdown_are_distinct_durable_actions_while_same_incoming_is_held() {
    let temp = Temp::new();
    let (mut owner, docs) = wal_setup(&temp, "same-held-live-validity");
    wal_start(&mut owner, docs);
    drive_to_live(&mut owner);
    let pending = owner
        .active_service()
        .unwrap()
        .pending_write()
        .unwrap()
        .durable_read_context();
    let physical = owner.index().snapshot().unwrap();
    let head = owner.head();
    run_action(&mut owner, NativeMainActiveServiceActionV1::Pause);
    assert_ne!(owner.head(), head);
    assert_eq!(owner.index().snapshot().unwrap(), physical);
    assert_eq!(
        owner
            .active_service()
            .unwrap()
            .pending_write()
            .unwrap()
            .durable_read_context(),
        pending
    );
    assert!(owner
        .prepare_action(NativeMainActiveServiceActionV1::ResolvePost)
        .is_err());
    run_action(
        &mut owner,
        NativeMainActiveServiceActionV1::Shutdown(vec![]),
    );
    assert_eq!(owner.index().snapshot().unwrap(), physical);
    assert_eq!(
        owner
            .active_service()
            .unwrap()
            .frame
            .as_ref()
            .unwrap()
            .terminal_code(),
        Some(0)
    );
    // A post-exit raw request may be recorded as an API rejection, never as a native write.
    run_action(&mut owner, NativeMainActiveServiceActionV1::AppendPending);
    assert_eq!(owner.index().snapshot().unwrap(), physical);
}
#[test]
fn native_append_intent_candidates_are_real_and_ack_precedes_same_plan_resolution() {
    let temp = Temp::new();
    let (mut owner, docs) = wal_setup(&temp, "same-held-live-validity");
    wal_start(&mut owner, docs);
    drive_to_live(&mut owner);
    let read = owner
        .active_service()
        .unwrap()
        .pending_write()
        .unwrap()
        .durable_read_context();
    owner
        .prepare_action(NativeMainActiveServiceActionV1::AppendPending)
        .unwrap();
    owner.barrier_fault = Some((
        NativeMainActiveWalPhaseV1::Intent,
        crate::source_main_active_service_wal::TestFault::AfterCommit,
    ));
    let physical = owner.index().snapshot().unwrap();
    assert!(owner.advance().is_err());
    assert_eq!(owner.index().snapshot().unwrap(), physical);
    let plan = owner.pending.as_ref().unwrap().plan.as_ref().unwrap();
    let record = plan.record().unwrap();
    assert_ne!(record.get("nativeCandidates"), V::Null);
    assert_ne!(record.get("nativeMutations"), V::Null);
    owner.resolve_control_write().unwrap();
    owner.advance().unwrap();
    assert_eq!(owner.acknowledged_append.as_ref(), Some(&read));
    let r = owner.history().last().unwrap();
    let raw = owner
        .read_history_blob(r.native_candidates.as_ref().unwrap())
        .unwrap();
    let facts = crate::property_catalog::parse_diagnostic_value(std::str::from_utf8(&raw).unwrap())
        .unwrap();
    assert_eq!(facts.get("readContext"), read);
    let raw = owner
        .read_history_blob(r.native_mutations.as_ref().unwrap())
        .unwrap();
    assert!(raw.starts_with(b"TLNM\x01"));
    run_action(&mut owner, NativeMainActiveServiceActionV1::ResolvePost);
    assert!(owner.acknowledged_append.is_none());
}
#[test]
fn synced_ack_with_missing_blob_keeps_actual_owner_private_and_unchanged() {
    let temp = Temp::new();
    let (mut owner, docs) = wal_setup(&temp, "detached-live-rpc-failure");
    wal_start(&mut owner, docs);
    owner
        .prepare_action(NativeMainActiveServiceActionV1::Advance { limit: 4096 })
        .unwrap();
    owner.barrier_fault = Some((
        NativeMainActiveWalPhaseV1::Ack,
        crate::source_main_active_service_wal::TestFault::AfterCommit,
    ));
    assert!(owner.advance().is_err());
    let p = owner.pending.as_ref().unwrap();
    let raw = p.entry.clone().unwrap();
    let pin = ActiveServiceWalBlobV1::of(&raw).unwrap();
    let path = owner.wal.pin();
    assert!(path.is_err());
    let root = p.plan.as_ref().unwrap().before().root.clone();
    let path = root
        .join("active-service-blobs-v1")
        .join(format!("{}.asw1", hex(&pin.sha256)));
    std::fs::remove_file(&path).unwrap();
    let effects = owner.active_service().unwrap().effects().to_vec();
    assert!(owner.resolve_control_write().is_err());
    assert_eq!(owner.phase(), NativeMainActiveWalPhaseV1::Ack);
    assert_eq!(owner.history().len(), 1);
    assert_eq!(owner.active_service().unwrap().effects(), effects);
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .unwrap();
    std::io::Write::write_all(&mut file, &raw).unwrap();
    file.sync_all().unwrap();
    drop(file);
    owner.resolve_control_write().unwrap();
    assert_eq!(owner.history().len(), 2);
    assert_eq!(owner.active_service().unwrap().effects(), effects);
}

#[test]
fn persisted_raw_actions_reexecute_on_fresh_raw_inputs_without_physical_head_equivalence() {
    let first = Temp::new();
    let (mut original, docs) = wal_setup(&first, "same-held-live-validity");
    wal_start(&mut original, docs);
    drive_to_live(&mut original);
    run_action(&mut original, NativeMainActiveServiceActionV1::Pause);
    run_action(
        &mut original,
        NativeMainActiveServiceActionV1::AppendPending,
    );
    run_action(&mut original, NativeMainActiveServiceActionV1::ResolvePost);
    run_action(
        &mut original,
        NativeMainActiveServiceActionV1::Advance { limit: 4096 },
    );
    let second = Temp::new();
    let (mut replacement, _) = wal_setup(&second, "same-held-live-validity");
    for receipt in original.history() {
        let bytes = original.read_history_blob(&receipt.action).unwrap();
        let action = NativeMainActiveServiceActionV1::from_canonical_action_bytes(&bytes).unwrap();
        run_action(&mut replacement, action);
        assert_eq!(
            replacement
                .read_history_blob(&replacement.history().last().unwrap().action)
                .unwrap(),
            bytes
        );
    }
    assert_eq!(
        original
            .active_service()
            .unwrap()
            .history()
            .iter()
            .map(|r| &r.result)
            .collect::<Vec<_>>(),
        replacement
            .active_service()
            .unwrap()
            .history()
            .iter()
            .map(|r| &r.result)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        original
            .financial_runtime()
            .state_value()
            .unwrap()
            .get("tables"),
        replacement
            .financial_runtime()
            .state_value()
            .unwrap()
            .get("tables")
    );
    let rows = |owner: &SourceMainNativeDurableActiveServiceV1| {
        let mut rows = vec![];
        owner
            .index()
            .visit_rows(|row| {
                rows.push(row.clone());
                Ok(true)
            })
            .unwrap();
        rows
    };
    assert_eq!(rows(&original), rows(&replacement));
    assert_eq!(
        original.active_service().unwrap().caller_result(),
        replacement.active_service().unwrap().caller_result()
    );
    assert_eq!(
        original.active_service().unwrap().next_timer(),
        replacement.active_service().unwrap().next_timer()
    );
    assert_ne!(
        original.index().snapshot().unwrap(),
        replacement.index().snapshot().unwrap(),
        "replacement physical binding remains distinct"
    );
    assert_ne!(
        original.head(),
        replacement.head(),
        "original-physical ACK hash is not a fresh-path logical head"
    );
}
#[test]
fn oversized_action_decode_never_mutates_the_retained_real_owner() {
    let temp = Temp::new();
    let (owner, _) = wal_setup(&temp, "same-held-live-validity");
    let before = owner.observation().unwrap();
    let physical = owner.index().snapshot().unwrap();
    assert!(
        NativeMainActiveServiceActionV1::from_canonical_action_bytes(&vec![
            0;
            MAX_ACTIVE_SERVICE_ACTION_BYTES
                + 1
        ])
        .is_err()
    );
    assert_eq!(owner.observation().unwrap(), before);
    assert_eq!(owner.index().snapshot().unwrap(), physical);
    assert!(owner.history().is_empty());
}

#[path = "source_main_active_service_recovery_tests.rs"]
mod native_recovery_tests;

#[path = "source_main_active_wal_exit_tests.rs"]
mod active_wal_exit_tests;

#[path = "source_main_active_service_cold_history_actual_tests.rs"]
mod cold_ack_actual_tests;

#[path = "source_main_active_history_pin_codec_tests.rs"]
mod active_history_pin_codec_tests;

#[path = "source_main_native_cold_active_reexecution_tests.rs"]
mod cold_active_reexecution_tests;
