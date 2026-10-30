use super::*;
use crate::source_contract_replay::{
    SourceContractCommand as C, SourceContractCommandOutcome as O,
};
use crate::source_main_native_owners::NativeMainExpectation;
use crate::source_main_step_kernel::{
    MainStepCommandV1 as Step, MainStepSourceResult, MainStepStop,
};
use crate::storage::Namespace;
use std::sync::atomic::{AtomicU64, Ordering};
static SERIAL: AtomicU64 = AtomicU64::new(0);
include!("source_main_retained_file_test_transport.rs");
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
            "actual-retained-file-{}-{}",
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
fn raw(name: &str) -> V {
    let line = include_str!("../fixtures/native_main_retained_file_raw_inputs.jsonl")
        .lines()
        .find(|line| json_string(raw_get(line, "name")) == name)
        .unwrap();
    untag(raw_get(line, "rawCommand"))
}
fn invoke(replay: &mut SourceContractReplay, target: &str, arguments: Vec<V>) {
    let receipt = replay
        .apply(C::Invoke(DependencyCall {
            target: target.into(),
            arguments,
        }))
        .unwrap();
    assert!(
        !matches!(receipt.outcome, O::LocalError(_)),
        "{target}:{:?}",
        receipt.outcome
    );
}
fn setup(t: &Temp, name: &str) -> SourceMainNativeSessionV13 {
    let reset = raw(name).get("reset");
    let options = reset.get("options");
    let profile = options.get("clientProfile");
    assert_eq!(options.get("empty"), V::Bool(true));
    let mut replay =
        SourceContractReplay::with_policy(SourceContractRuntimePolicy::MainOrchestrationV12)
            .unwrap();
    invoke(
        &mut replay,
        "SourceActivation.configureClient",
        vec![obj(&[
            ("chain", profile.get("chain")),
            ("test", profile.get("test")),
        ])],
    );
    replay.apply(C::SetHeight(profile.get("height"))).unwrap();
    invoke(&mut replay, "Activation.init", vec![options.get("init")]);
    invoke(
        &mut replay,
        "SourceMain.configureProduction",
        vec![obj(&[
            (
                "client",
                obj(&[
                    ("chain", profile.get("chain")),
                    ("test", profile.get("test")),
                ]),
            ),
            ("io", options.get("orchestration")),
            ("genesisBlock", options.get("mainGenesis")),
        ])],
    );
    invoke(
        &mut replay,
        "SourceMain.configureService",
        vec![obj(&[
            ("fileInputs", options.get("fileInputs")),
            ("fileIOInputs", options.get("fileIOInputs")),
            (
                "directoryInputs",
                if options.get("directoryInputs") == V::Undefined {
                    V::Array(vec![])
                } else {
                    options.get("directoryInputs")
                },
            ),
        ])],
    );
    let mut index = SourceCompactIndex::create(
        &t.root,
        Namespace::new("LTC", false, "txIndex").unwrap(),
        "actual retained file raw inputs",
    )
    .unwrap();
    let V::Array(rows) = reset.get("indexRows") else {
        panic!("raw metadata inputs")
    };
    for row in rows {
        index.import_row(&index.snapshot().unwrap(), row).unwrap();
    }
    let expected = NativeMainExpectation {
        financial_head: replay.head(),
        namespace: Namespace::new("LTC", false, "txIndex").unwrap(),
        metadata: index.snapshot().unwrap(),
    };
    SourceMainNativeSessionV13::adopt(NativeMainOwners::adopt(replay, index, &expected).unwrap())
        .unwrap()
}
fn file_owner(t: &Temp, name: &str) -> SourceMainNativeCallerFileRetriesV1 {
    let owner = setup(t, name);
    let expected = owner.expectation().unwrap();
    owner
        .into_retained_caller_with_file_retries(
            &expected,
            NativeMainRetainedCallerV1::Initialize,
            901,
            91,
            V::Array(vec![]),
        )
        .unwrap()
}
fn start(
    owner: &mut SourceMainNativeCallerFileRetriesV1,
    target: &str,
    args: Vec<V>,
) -> crate::source_main_step_kernel::MainStepEvaluationV1 {
    owner
        .apply_file(Step::Start {
            method: target.into(),
            arguments: args,
        })
        .unwrap()
        .clone()
}
fn service(owner: &SourceMainNativeCallerFileRetriesV1) -> V {
    owner
        .financial_state()
        .unwrap()
        .get("mainProduction")
        .get("service")
}
fn file_bytes(owner: &SourceMainNativeCallerFileRetriesV1, path: &str) -> V {
    let V::Array(files) = service(owner).get("files") else {
        panic!("actual complete file rows")
    };
    files
        .into_iter()
        .find_map(|row| {
            let V::Array(fields) = row else {
                panic!("actual file row")
            };
            if fields[0] == s(path) {
                Some(fields[1].clone())
            } else {
                None
            }
        })
        .unwrap_or(V::Undefined)
}
fn copy_count(owner: &SourceMainNativeCallerFileRetriesV1, method: &str) -> usize {
    owner
        .effects()
        .iter()
        .filter(|e| e.get("kind") == s("filesystem") && e.get("method") == s(method))
        .count()
}
fn fire(
    owner: &mut SourceMainNativeCallerFileRetriesV1,
) -> crate::source_main_step_kernel::MainStepEvaluationV1 {
    let id = owner.next_file_timer().unwrap();
    owner
        .apply_file(Step::FireTimer { id, generation: 91 })
        .unwrap()
        .clone()
}
fn caller_still_borrowed(owner: &SourceMainNativeCallerFileRetriesV1) {
    let mut fields = vec![];
    owner
        .visit_canonical_fields(&mut |p, v| {
            fields.push((p.to_owned(), v.clone()));
            Ok(())
        })
        .unwrap();
    assert!(fields.contains(&(
        "retainedCaller/caller/invocation/present".into(),
        V::Bool(true)
    )));
}
#[test]
fn file_retry_new_profile_keeps_old_busy_refusal_and_retained_caller_queue() {
    let t = Temp::new();
    let owner = setup(&t, "file-retry/first10/copy/input");
    let expected = owner.expectation().unwrap();
    let rejected = owner
        .into_retained_main_caller(
            &expected,
            NativeMainRetainedCallerV1::Initialize,
            901,
            91,
            V::Array(vec![]),
        )
        .err()
        .expect("old immediate-file entry refuses retries");
    let owner = rejected.into_owner();
    assert_eq!(owner.expectation().unwrap(), expected);
    let mut owner = owner
        .into_retained_caller_with_file_retries(
            &expected,
            NativeMainRetainedCallerV1::Initialize,
            901,
            91,
            V::Array(vec![]),
        )
        .unwrap();
    let physical = owner.caller.owner.index.snapshot().unwrap();
    let prefix = owner.effects().to_vec();
    assert!(matches!(
        owner.apply_file(Step::Start {
            method: "Main.processBlock".into(),
            arguments: vec![V::Object(vec![]), V::number(1000.)]
        }),
        Err(crate::source_main_step_kernel::MainStepError::InvalidInput)
    ));
    assert_eq!(owner.caller.owner.index.snapshot().unwrap(), physical);
    assert_eq!(owner.effects(), prefix);

    let first = start(
        &mut owner,
        "Persistence.safeCopy",
        vec![s("/tl-data/ltc-main/a.db"), s("/tl-data/copied.db")],
    );
    assert!(
        first.completed.is_empty(),
        "snapshot completed={:?}, stop={:?}",
        first.completed,
        first.stop
    );
    assert!(owner.next_file_timer().is_some());
    caller_still_borrowed(&owner);
    start(&mut owner, "Main.setPause", vec![]);
    start(&mut owner, "Main.saveTrackHeight", vec![V::number(9.)]);
    let done = fire(&mut owner);
    assert!(done
        .completed
        .iter()
        .any(|(_, r)| r == &MainStepSourceResult::Fulfilled(V::Undefined)));
    assert!(owner.next_file_timer().is_none());
    assert_eq!(copy_count(&owner, "copyFile"), 2);
    assert!(owner.effects().starts_with(&prefix));
    assert_eq!(owner.caller.owner.index.snapshot().unwrap(), physical);
    caller_still_borrowed(&owner);
    let loaded = start(&mut owner, "Main.loadTrackHeight", vec![]);
    assert!(loaded
        .completed
        .iter()
        .any(|(_, r)| r == &MainStepSourceResult::Fulfilled(V::number(9.))));
}
#[test]
fn file_retry_retains_all_five_attempts_and_waits_after_the_fifth_failure() {
    for (name, target, args, method) in [
        (
            "file-retry/first10/copy-exhausted/input",
            "Persistence.safeCopy",
            vec![s("/tl-data/ltc-main/a.db"), s("/tl-data/never.db")],
            "copyFile",
        ),
        (
            "file-retry/first10/delete-exhausted/input",
            "Persistence.safeDelete",
            vec![s("/tl-data/ltc-main/a.db")],
            "unlink",
        ),
    ] {
        let t = Temp::new();
        let mut owner = file_owner(&t, name);
        let first = start(&mut owner, target, args);
        assert!(first.completed.is_empty());
        for at in 1..=5 {
            assert!(owner.next_file_timer().is_some());
            let evaluation = fire(&mut owner);
            if at < 5 {
                assert!(evaluation.completed.is_empty());
            } else {
                assert!(evaluation
                    .completed
                    .iter()
                    .any(|(_, r)| matches!(r, MainStepSourceResult::Thrown(_))));
            }
        }
        assert!(owner.next_file_timer().is_none());
        assert_eq!(copy_count(&owner, method), 5);
        caller_still_borrowed(&owner);
        assert_eq!(
            owner
                .effects()
                .iter()
                .filter(
                    |e| e.get("kind") == s("timer-scheduled") && e.get("delay") == V::number(50.)
                )
                .count(),
            5
        );
    }
}
#[test]
fn file_retry_enoent_and_nonbusy_errors_follow_actual_source_catch_scopes() {
    let t = Temp::new();
    let mut owner = file_owner(&t, "file-retry/first10/delete-missing/input");
    let result = start(
        &mut owner,
        "Persistence.safeDelete",
        vec![s("/tl-data/ltc-main/absent.db")],
    );
    assert!(result
        .completed
        .iter()
        .any(|(_, r)| r == &MainStepSourceResult::Fulfilled(V::Undefined)));
    assert!(owner.next_file_timer().is_none());
    let t = Temp::new();
    let mut owner = file_owner(&t, "file-retry/enoent-after-permission/input");
    let result = start(
        &mut owner,
        "Persistence.safeDelete",
        vec![s("/tl-data/ltc-main/a.db")],
    );
    assert!(result.completed.is_empty());
    let result = fire(&mut owner);
    assert!(result
        .completed
        .iter()
        .any(|(_, r)| r == &MainStepSourceResult::Fulfilled(V::Undefined)));
    assert_eq!(file_bytes(&owner, "/tl-data/ltc-main/a.db"), s("live a"));
    assert!(owner.next_file_timer().is_none());
    let t = Temp::new();
    let mut owner = file_owner(&t, "file-retry/nonbusy/input");
    let result = start(
        &mut owner,
        "Persistence.safeCopy",
        vec![s("/tl-data/ltc-main/a.db"), s("/tl-data/failed.db")],
    );
    assert!(result.completed.iter().any(|(_,r)|matches!(r,MainStepSourceResult::Thrown(e)if e.exception.message=="recorded copyFile EIO 1")));
    assert!(owner.next_file_timer().is_none());
}
#[test]
fn file_retry_snapshot_partial_copy_is_not_restarted_by_concurrent_source_file_change() {
    let t = Temp::new();
    let mut owner = file_owner(&t, "file-retry/snapshot-prefix/input");
    let first = start(
        &mut owner,
        "Persistence.createSnapshot",
        vec![V::number(1000.), s("recorded source hash")],
    );
    let parent = first.started.unwrap();
    assert!(
        first.completed.iter().all(|(id, _)| *id != parent),
        "{first:?}"
    );
    assert!(owner.next_file_timer().is_some(), "{first:?}");
    let before = service(&owner);
    assert_ne!(before, V::Undefined);
    let count = copy_count(&owner, "copyFile");
    assert_eq!(count, 2);
    start(
        &mut owner,
        "Persistence.safeCopy",
        vec![s("/tl-data/new-a"), s("/tl-data/ltc-main/a.db")],
    );
    let done = fire(&mut owner);
    assert!(
        done.completed
            .iter()
            .any(|(id, r)| *id == parent && r == &MainStepSourceResult::Fulfilled(V::Undefined)),
        "{done:?}"
    );
    assert_eq!(copy_count(&owner, "copyFile"), 4);
    caller_still_borrowed(&owner);
    // Successful first file, busy second, concurrent copy, retry second: no
    // replay of CreateBegin or first file. Complete file bytes remain in Core.
    let copies = owner
        .effects()
        .iter()
        .filter(|e| e.get("kind") == s("filesystem") && e.get("method") == s("copyFile"))
        .collect::<Vec<_>>();
    let V::Array(args) = copies[0].get("args") else {
        panic!("actual copy args")
    };
    assert_eq!(args[0], s("/tl-data/ltc-main/a.db"));
    assert_eq!(
        file_bytes(&owner, "/tl-data/ltc-main/a.db"),
        s("replacement a")
    );
    let V::Array(files) = service(&owner).get("files") else {
        panic!("actual file rows")
    };
    let snapshot = files
        .into_iter()
        .filter_map(|row| {
            let V::Array(parts) = row else {
                panic!("actual file row")
            };
            let V::String(path) = &parts[0] else {
                panic!("actual file path")
            };
            if path.starts_with("/tl-data/ltc-main-snapshots/") && path.ends_with(".db") {
                Some(parts[1].clone())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(snapshot, vec![s("live a"), s("live b")]);
}
#[test]
fn file_retry_shutdown_retains_pending_timer_and_refuses_post_exit_callbacks() {
    let t = Temp::new();
    let mut owner = file_owner(&t, "file-retry/first10/copy/input");
    start(
        &mut owner,
        "Persistence.safeCopy",
        vec![s("/tl-data/ltc-main/a.db"), s("/tl-data/copied.db")],
    );
    let timer = owner.next_file_timer().unwrap();
    let prefix = owner.effects().to_vec();
    let exited = owner.apply_file(Step::Shutdown(vec![])).unwrap();
    assert_eq!(exited.stop, MainStepStop::ProcessExited(0));
    assert_eq!(owner.next_file_timer(), Some(timer));
    assert!(owner.effects().starts_with(&prefix));
    assert!(matches!(
        owner.apply_file(Step::FireTimer {
            id: timer,
            generation: 91
        }),
        Err(crate::source_main_step_kernel::MainStepError::AlreadyExited)
    ));
    caller_still_borrowed(&owner);
}

#[test]
fn file_retry_restore_keeps_backup_delete_prefix_and_actual_height_return_order() {
    let t = Temp::new();
    let mut owner = file_owner(&t, "file-retry/main-restore/input");
    let metadata = owner.caller.owner.index.snapshot().unwrap();
    let balances = owner
        .caller
        .owner
        .financial
        .runtime()
        .collection("tallyMap")
        .to_vec();
    // Raw original snapshot metadata and directory from the admitted input;
    // no financial rows or source returned result are supplied.
    let result = start(
        &mut owner,
        "Persistence.restoreSnapshot",
        vec![obj(&[
            ("blockHeight", V::number(1.)),
            ("dir", s("one")),
            ("files", V::Array(vec![s("a.db")])),
        ])],
    );
    let parent = result.started.unwrap();
    assert!(
        result.completed.iter().all(|(id, _)| *id != parent),
        "{result:?}"
    );
    assert!(owner.next_file_timer().is_some(), "{result:?}");
    assert_eq!(file_bytes(&owner, "/tl-data/ltc-main/a.db"), s("live a"));
    assert_eq!(copy_count(&owner, "copyFile"), 3);
    let prefix = owner.effects().to_vec();
    start(&mut owner, "Main.saveTrackHeight", vec![V::number(9.)]);
    start(&mut owner, "Main.setPause", vec![]);
    let result = fire(&mut owner);
    assert!(
        result
            .completed
            .iter()
            .any(|(id, r)| *id == parent && r == &MainStepSourceResult::Fulfilled(V::Undefined)),
        "{result:?}"
    );
    assert_eq!(
        file_bytes(&owner, "/tl-data/ltc-main/a.db"),
        s("historical a")
    );
    assert_eq!(file_bytes(&owner, "/tl-data/ltc-main/b.db"), V::Undefined);
    assert_eq!(
        file_bytes(&owner, "/tl-data/ltc-main/note.txt"),
        s("keep note")
    );
    assert_eq!(copy_count(&owner, "copyFile"), 4);
    assert!(owner.effects().starts_with(&prefix));
    let result = start(&mut owner, "Main.loadTrackHeight", vec![]);
    assert!(result
        .completed
        .iter()
        .any(|(_, r)| r == &MainStepSourceResult::Fulfilled(V::number(1.))));
    assert_eq!(
        owner
            .caller
            .owner
            .financial
            .runtime()
            .collection("tallyMap"),
        balances
    );
    assert_eq!(owner.caller.owner.index.snapshot().unwrap(), metadata);
    caller_still_borrowed(&owner);
}
