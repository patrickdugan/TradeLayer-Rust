// Child of durable tests: reuse raw source fixture inputs, with unique paths.
use super::*;
use crate::source_compact_index_v2::NativePreparedValidityCandidateV1 as Candidate;
use crate::storage::Recovery;
fn recovery_temp() -> Temp {
    let parent = std::env::temp_dir().canonicalize().unwrap();
    let root = parent.join(format!(
        "actual-active-service-native-recovery-{}-{}",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&root).unwrap();
    Temp { root, parent }
}
fn make_held(temp: &Temp) -> SourceMainNativeDurableActiveServiceV1 {
    let (owner, docs) = setup(temp, "same-held-live-validity");
    let mut owner = SourceMainNativeDurableActiveServiceV1::create_with_verified_history(
        owner,
        &temp.root,
        Namespace::new("LTC", false, "consensus").unwrap(),
        crate::source_contract_replay::source_durable::SourceDurableFilesystem::FileSyncOnly,
        64 * 1024 * 1024,
    )
    .unwrap();
    wal_start(&mut owner, docs);
    drive_to_live(&mut owner);
    owner
}
fn active_mut(
    owner: &mut SourceMainNativeDurableActiveServiceV1,
) -> &mut SourceMainNativeActiveServiceV1 {
    let Some(ActiveWalOwner::Active(service)) = &mut owner.owner else {
        panic!("active actual owner")
    };
    service
}
fn failed_append(owner: &mut SourceMainNativeDurableActiveServiceV1, post: bool) {
    if post {
        active_mut(owner).owner.index.test_fail_after_sync();
    } else {
        active_mut(owner)
            .owner
            .index
            .test_fail_after_partial_write();
    }
    owner
        .prepare_action(NativeMainActiveServiceActionV1::AppendPending)
        .unwrap();
    assert!(matches!(
        owner.advance(),
        Err(NativeMainActiveWalErrorV1::NativeRecoveryRequired)
    ));
    assert_eq!(
        owner.phase(),
        NativeMainActiveWalPhaseV1::NativeRecoveryRequired
    );
    assert!(owner
        .prepare_action(NativeMainActiveServiceActionV1::Pause)
        .is_err());
    assert!(owner
        .prepare_action(NativeMainActiveServiceActionV1::Shutdown(vec![]))
        .is_err());
}
#[test]
fn actual_lost_native_return_reopens_post_same_owner_without_reappend_or_source_retry() {
    let temp = recovery_temp();
    let mut owner = make_held(&temp);
    let effects = owner.active_service().unwrap().effects().to_vec();
    let count = owner
        .financial_runtime()
        .native_main_test_row_decode_count();
    let read = owner
        .active_service()
        .unwrap()
        .pending_write()
        .unwrap()
        .durable_read_context();
    failed_append(&mut owner, true);
    let no_index = owner.into_no_native_index().unwrap();
    assert_eq!(no_index.effects(), effects);
    let no_index = no_index
        .try_reopen(
            &temp.root,
            Candidate::Before,
            Recovery::RejectIncompleteTail,
        )
        .err()
        .expect("current native files must refuse this candidate")
        .into_owner();
    let mut owner = no_index
        .try_reopen(&temp.root, Candidate::Post, Recovery::RejectIncompleteTail)
        .unwrap();
    assert_eq!(owner.phase(), NativeMainActiveWalPhaseV1::Observe);
    owner.advance().unwrap();
    assert_eq!(owner.active_service().unwrap().effects(), effects);
    assert_eq!(
        owner
            .financial_runtime()
            .native_main_test_row_decode_count(),
        count
    );
    assert_eq!(owner.acknowledged_append.as_ref(), Some(&read));
    assert!(
        matches!(
            owner
                .active_service()
                .unwrap()
                .history()
                .last()
                .unwrap()
                .result,
            NativeMainActiveServiceActionResultV1::Stop(NativeMainServiceStopV1::StorageStopped(_))
        ),
        "retain actual original failure observation"
    );
    run_action(&mut owner, NativeMainActiveServiceActionV1::ResolvePost);
}
#[test]
fn actual_before_reopen_requires_new_durable_native_mapping_before_retry() {
    let temp = recovery_temp();
    let mut owner = make_held(&temp);
    let count = owner
        .financial_runtime()
        .native_main_test_row_decode_count();
    let effects = owner.active_service().unwrap().effects().to_vec();
    failed_append(&mut owner, false);
    let no_index = owner.into_no_native_index().unwrap();
    let no_index = no_index
        .try_reopen(&temp.root, Candidate::Post, Recovery::DiscardIncompleteTail)
        .err()
        .expect("current native files must refuse this candidate")
        .into_owner();
    let mut owner = no_index
        .try_reopen(
            &temp.root,
            Candidate::Before,
            Recovery::DiscardIncompleteTail,
        )
        .unwrap();
    let before = owner.index().snapshot().unwrap();
    owner.barrier_fault = Some((
        NativeMainActiveWalPhaseV1::NativeSupersessionIntent,
        crate::source_main_active_service_wal::TestFault::AfterCommit,
    ));
    assert!(owner.advance().is_err());
    assert_eq!(owner.index().snapshot().unwrap(), before);
    assert!(owner
        .prepare_action(NativeMainActiveServiceActionV1::Shutdown(vec![]))
        .is_err());
    owner.resolve_control_write().unwrap();
    assert_eq!(
        owner.phase(),
        NativeMainActiveWalPhaseV1::NativeSupersessionAppend
    );
    assert_eq!(
        owner.index().snapshot().unwrap(),
        before,
        "synced replacement install is not a native append"
    );
    owner.advance().unwrap();
    assert_eq!(
        owner
            .financial_runtime()
            .native_main_test_row_decode_count(),
        count
    );
    assert_eq!(owner.active_service().unwrap().effects(), effects);
    run_action(&mut owner, NativeMainActiveServiceActionV1::ResolvePost);
}
#[test]
fn active_native_before_recovery_rejects_foreign_root_stale_suffix_and_corrupt_current() {
    for bad in ["foreign", "stale", "corrupt"] {
        let temp = recovery_temp();
        let mut owner = make_held(&temp);
        failed_append(&mut owner, false);
        let foreign = recovery_temp();
        let no_index = owner.into_no_native_index().unwrap();
        let before = no_index.effects().to_vec();
        let no_index = if bad == "foreign" {
            no_index
                .try_reopen(
                    &foreign.root,
                    Candidate::Before,
                    Recovery::DiscardIncompleteTail,
                )
                .err()
                .expect("current native files must refuse this candidate")
                .into_owner()
        } else {
            let plan = no_index.pending_write();
            let (mut extra, _) = SourceCompactIndex::reopen_prepared_validity(
                &temp.root,
                Namespace::new("LTC", false, "txIndex").unwrap(),
                plan,
                Candidate::Before,
                Recovery::DiscardIncompleteTail,
            )
            .unwrap();
            if bad == "stale" {
                extra
                    .import_row(
                        &extra.snapshot().unwrap(),
                        obj(&[("_id", s("tx-99-foreign")), ("value", V::object())]),
                    )
                    .unwrap();
                drop(extra);
            } else {
                let (_, catalog) = extra.native_store_context().unwrap();
                drop(extra);
                let path = catalog.join("CURRENT");
                let mut bytes = std::fs::read(&path).unwrap();
                bytes[0] ^= 1;
                std::fs::write(&path, bytes).unwrap();
            }
            no_index
                .try_reopen(
                    &temp.root,
                    Candidate::Before,
                    Recovery::DiscardIncompleteTail,
                )
                .err()
                .expect("current native files must refuse this candidate")
                .into_owner()
        };
        assert_eq!(no_index.effects(), before);
    }
}
#[test]
fn actual_compaction_midpoint_reopens_then_supersedes_without_another_logic_call() {
    let temp = recovery_temp();
    let (raw_owner, docs) = setup_compacting(&temp, "same-held-live-validity");
    let mut owner = SourceMainNativeDurableActiveServiceV1::create_with_verified_history(
        raw_owner,
        &temp.root,
        Namespace::new("LTC", false, "consensus").unwrap(),
        crate::source_contract_replay::source_durable::SourceDurableFilesystem::FileSyncOnly,
        64 * 1024 * 1024,
    )
    .unwrap();
    wal_start(&mut owner, docs);
    // The raw storage fixture primes the FIRST catchup write. Consuming that
    // write while driving to live would already publish/prune its compaction.
    run_action(
        &mut owner,
        NativeMainActiveServiceActionV1::Advance { limit: 4096 },
    );
    assert!(
        owner.active_service().unwrap().caller_result().is_none(),
        "original outer catchup remains pending"
    );
    assert!(
        owner.active_service().unwrap().pending_write().is_some(),
        "actual first source validity await"
    );
    let effects = owner.active_service().unwrap().effects().to_vec();
    let decoded = owner
        .financial_runtime()
        .native_main_test_row_decode_count();
    assert!(
        owner
            .active_service()
            .unwrap()
            .pending_write()
            .unwrap()
            .append_plan()
            .intermediate()
            .is_some(),
        "real prepared compaction midpoint required"
    );
    active_mut(&mut owner)
        .owner
        .index
        .native_main_test_compaction_fault("after-current-sync");
    owner
        .prepare_action(NativeMainActiveServiceActionV1::AppendPending)
        .unwrap();
    assert!(owner.advance().is_err());
    let no_index = owner.into_no_native_index().unwrap();
    let mut owner = no_index
        .try_reopen(
            &temp.root,
            Candidate::Intermediate,
            Recovery::RejectIncompleteTail,
        )
        .unwrap();
    owner.advance().unwrap();
    assert_eq!(owner.active_service().unwrap().effects(), effects);
    assert_eq!(
        owner
            .financial_runtime()
            .native_main_test_row_decode_count(),
        decoded
    );
    run_action(&mut owner, NativeMainActiveServiceActionV1::ResolvePost);
}
#[test]
fn verified_history_pins_are_actual_before_after_blobs_and_old_entry_keeps_mode_off() {
    let temp = recovery_temp();
    let owner = make_held(&temp);
    let r = owner.history().last().unwrap();
    let raw = owner.read_history_blob(&r.entry).unwrap();
    let entry = crate::property_catalog::parse_diagnostic_value(std::str::from_utf8(&raw).unwrap())
        .unwrap();
    assert_ne!(entry.get("verifiedHistoryBefore"), V::Undefined);
    assert_ne!(entry.get("verifiedHistoryPost"), V::Undefined);
    let other = recovery_temp();
    let (mut legacy, docs) = wal_setup(&other, "detached-live-rpc-failure");
    wal_start(&mut legacy, docs);
    let raw = legacy
        .read_history_blob(&legacy.history()[0].entry)
        .unwrap();
    let entry = crate::property_catalog::parse_diagnostic_value(std::str::from_utf8(&raw).unwrap())
        .unwrap();
    assert_eq!(entry.get("verifiedHistoryBefore"), V::Undefined);
    assert_eq!(entry.get("nativeRecovery"), V::Undefined);
}

fn setup_compacting(temp: &Temp, mode: &str) -> (SourceMainNativeSessionV13, V) {
    let input = record(mode, "input");
    let reset = untag(raw_get(input, "rawReset"));
    let mut financial =
        SourceContractReplay::with_policy(SourceContractRuntimePolicy::MainOrchestrationV12)
            .unwrap();
    // Reproduce DECLARED reference fixture construction followed by raw
    // reset(empty:true). No expected business table/state is read or imported.
    assert_eq!(reset.get("empty"), V::Bool(true));
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
    for row in arr(untag(raw_get(input, "nativeInput"))) {
        index.import_row(&index.snapshot().unwrap(), row).unwrap();
    }
    index.native_main_test_compaction_fixture().unwrap();
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

#[test]
fn enhanced_history_mode_is_context_bound_and_missing_required_blob_cannot_downgrade() {
    let temp = recovery_temp();
    let owner = make_held(&temp);
    assert_eq!(owner.profile(), "TL-MAIN-ACTIVE-SERVICE-HISTORY-WAL-13-1");
    let context = owner.wal.origin_context().unwrap();
    assert_eq!(context.get("origin").get("profile"), s(owner.profile()));
    assert_ne!(
        context.get("origin").get("requiredVerifiedHistory"),
        V::Undefined
    );
    let receipt = owner.history().last().unwrap();
    let entry = owner.read_checked_history_entry(receipt).unwrap();
    let V::String(hash) = entry.get("verifiedHistoryBefore").get("sha256") else {
        panic!("captured actual pin")
    };
    let root = owner.wal_pin().unwrap().root;
    std::fs::remove_file(
        root.join("active-service-blobs-v1")
            .join(format!("{hash}.asw1")),
    )
    .unwrap();
    assert!(owner.read_checked_history_entry(receipt).is_err());
    assert_eq!(owner.profile(), "TL-MAIN-ACTIVE-SERVICE-HISTORY-WAL-13-1");
}
