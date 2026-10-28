use std::sync::atomic::AtomicUsize;
static TEMP: AtomicUsize = AtomicUsize::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "tl-source-durable-{}-{}",
            std::process::id(),
            TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn context() -> SourceDurableContext {
    SourceDurableContext {
        chain: "TL-DURABLE-TEST".into(),
        test: true,
        policy: SourceContractRuntimePolicy::SpotLifecycleV9,
        filesystem: SourceDurableFilesystem::FileSyncOnly,
    }
}
fn n(v: f64) -> V {
    V::number(v)
}
fn call(target: &str, args: Vec<V>) -> SourceContractCommand {
    SourceContractCommand::Invoke(DependencyCall {
        target: target.into(),
        arguments: args,
    })
}
fn fee() -> SourceContractCommand {
    call(
        "Tally.queueFeeAccrual",
        vec![n(7.), n(0.00000001), n(1.), n(10.)],
    )
}
fn update(available: f64, reserved: f64) -> SourceContractCommand {
    call(
        "Tally.updateBalance",
        vec![
            s("alice"),
            n(7.),
            n(available),
            n(reserved),
            n(0.),
            n(0.),
            s("durable-source"),
            n(10.),
        ],
    )
}
fn initial_inputs() -> Vec<SourceContractCommand> {
    let properties = r#"[[7,{"ticker":"DURABLE","totalInCirculation":10,"type":1,"whitelistId":0,"issuer":"alice"}]]"#;
    let tally = r#"[["alice",{"7":{"amount":10,"available":10,"reserved":0,"margin":0,"vesting":0,"channelBalance":0}}]]"#;
    vec![
        SourceContractCommand::Import {
            collection: "propertyList".into(),
            rows: vec![obj(&[
                ("_id", s("propertyIndex")),
                ("value", s(properties)),
            ])],
        },
        SourceContractCommand::Import {
            collection: "tallyMap".into(),
            rows: vec![obj(&[("_id", s("tallyMap")), ("data", s(tally))])],
        },
        call("Tally.loadFromDB", vec![]),
        call("Property.getPropertyData", vec![n(7.)]),
    ]
}
// Independent continuous reference owner uses the existing public runtime API;
// it has no TLCR command-history cap and receives exactly the original inputs.
fn direct(
    runtime: &mut SourceContractRuntime,
    command: &SourceContractCommand,
) -> (SourceContractCommandOutcome, Vec<V>) {
    let before = runtime.clone();
    let result = match command {
        SourceContractCommand::Import { collection, rows } => runtime
            .import_collection(collection, rows.clone())
            .map(|()| SourceContractCommandOutcome::Void),
        SourceContractCommand::Invoke(call) => runtime
            .invoke_dependency(call)
            .map(SourceContractCommandOutcome::Dependency),
        SourceContractCommand::FailStorage(f) => runtime
            .fail_storage(f.clone())
            .map(|()| SourceContractCommandOutcome::Void),
        SourceContractCommand::SetHeight(v) => runtime
            .set_height(v.clone())
            .map(|()| SourceContractCommandOutcome::Void),
        SourceContractCommand::SetEntry(v) => runtime
            .set_entry(v)
            .map(|()| SourceContractCommandOutcome::Void),
        SourceContractCommand::SetActivation(v) => {
            runtime.set_activation_answer(*v);
            Ok(SourceContractCommandOutcome::Void)
        }
        SourceContractCommand::ExecuteContract(v) => runtime
            .execute_contract_unverified(v)
            .map(SourceContractCommandOutcome::Source),
        SourceContractCommand::EvaluateQueue(v) => runtime
            .evaluate_queue(v)
            .map(SourceContractCommandOutcome::Source),
    };
    let outcome = match result {
        Ok(v) => v,
        Err(e) => {
            *runtime = before;
            SourceContractCommandOutcome::LocalError(e)
        }
    };
    (outcome, runtime.take_effects())
}
fn commit(
    node: &mut SourceDurableReplay,
    reference: &mut SourceContractRuntime,
    commands: Vec<SourceContractCommand>,
) -> Vec<SourceDurableReceipt> {
    let expected = commands
        .iter()
        .map(|c| direct(reference, c))
        .collect::<Vec<_>>();
    let before = node.publication().unwrap();
    let receipts = node
        .commit_batch(before, "unverified-test-source-batch", commands)
        .unwrap();
    for (i, (receipt, (outcome, effects))) in receipts.iter().zip(expected).enumerate() {
        assert_eq!(
            receipt.absolute_sequence,
            before.history.commands + i as u64
        );
        assert_eq!(receipt.source.sequence, i as u64);
        assert_eq!(receipt.source.outcome, outcome);
        assert_eq!(receipt.source.effects, effects);
    }
    assert_eq!(
        node.runtime().unwrap(),
        reference,
        "every complete Core field/arena/counter remains the same authoritative owner"
    );
    receipts
}
fn planned(
    node: &SourceDurableReplay,
    commands: &[SourceContractCommand],
) -> SourceDurablePublication {
    let mut replay = node.replay.clone();
    for c in commands {
        replay.apply(c.clone()).unwrap();
    }
    let segment = DurableSegment {
        previous: node.publication.history,
        label: "crash-test".into(),
        entries: replay.journal,
    };
    let digest = sha256(&durable_encode_segment(node.context_hash, &segment).unwrap());
    SourceDurablePublication {
        revision: node.publication.revision + 1,
        history: durable_after(node.context_hash, &segment, digest).unwrap(),
    }
}
#[test]
fn more_than_4096_real_owner_commands_and_retained_financial_prefix_recover_exactly() {
    let t = Temp::new();
    let ctx = context();
    let mut node = SourceDurableReplay::initialize(&t.0, ctx.clone()).unwrap();
    let mut reference = SourceContractRuntime::with_policy(ctx.policy);
    let mut start = initial_inputs();
    start.push(update(1., -100.));
    let receipts = commit(&mut node, &mut reference, start);
    assert!(
        matches!(&receipts.last().unwrap().source.outcome,SourceContractCommandOutcome::Dependency(DependencyReturn::PendingThrow(e))if e.message.contains("Reserved balance cannot go negative"))
    );
    let retained = node.publication().unwrap();
    let retained_state = reference.clone();
    let V::Array(entries) = reference.tally_cache() else {
        panic!()
    };
    let V::Array(pair) = &entries[0] else {
        panic!()
    };
    assert_eq!(pair[1].get("7").get("available"), n(11.));
    assert_eq!(
        pair[1].get("7").get("amount"),
        n(10.),
        "literal partial write stays nonconserving"
    );
    drop(node);
    let mut node =
        SourceDurableReplay::open(&t.0, ctx.clone(), retained, Recovery::RejectIncompleteTail)
            .unwrap();
    assert_eq!(node.runtime().unwrap(), &retained_state);
    let retry = commit(&mut node, &mut reference, vec![update(1., -100.)]);
    assert!(matches!(
        retry[0].source.outcome,
        SourceContractCommandOutcome::Dependency(DependencyReturn::PendingThrow(_))
    ));
    let mut financial = vec![];
    for _ in 0..16 {
        financial.push(update(2., 0.));
        financial.push(update(-1., 0.));
    }
    commit(&mut node, &mut reference, financial);
    for chunk in (0..5000)
        .collect::<Vec<_>>()
        .chunks(MAX_DURABLE_SEGMENT_COMMANDS)
    {
        commit(
            &mut node,
            &mut reference,
            chunk.iter().map(|_| fee()).collect(),
        );
    }
    let expected = node.publication().unwrap();
    assert_eq!(expected.history.commands, 5038);
    assert_eq!(expected.history.segments, 23);
    assert_eq!(MAX_COMMANDS, 4096);
    let expected_state = reference.clone();
    drop(node);
    let mut node =
        SourceDurableReplay::open(&t.0, ctx.clone(), expected, Recovery::RejectIncompleteTail)
            .unwrap();
    assert_eq!(node.runtime().unwrap(), &expected_state);
    commit(&mut node, &mut reference, vec![fee(), update(1., 0.)]);
    let newest = node.publication().unwrap();
    assert_eq!(newest.history.commands, 5040);
    assert_eq!(
        node.require_authenticated_input_and_state(),
        Err(SourceRuleError::UnauthenticatedInputAndState)
    );
    node.rewind_to(newest, retained.history).unwrap();
    assert_eq!(node.runtime().unwrap(), &retained_state);
    let rewound = node.publication().unwrap();
    assert!(rewound.revision > newest.revision);
    assert_eq!(rewound.history, retained.history);
    let mut branch_reference = retained_state;
    commit(
        &mut node,
        &mut branch_reference,
        vec![update(3., 0.), fee()],
    );
    let branch = node.publication().unwrap();
    assert_eq!(branch.history.commands, 7);
    eprintln!(
        "verified 5038 commands / 23 segments, 5040 after recovered retry; exact 7-command retained-prefix branch"
    );
    drop(node);
    assert!(matches!(
        SourceDurableReplay::open(&t.0, ctx.clone(), expected, Recovery::RejectIncompleteTail),
        Err(SourceDurableError::WrongHead)
    ));
    let node =
        SourceDurableReplay::open(&t.0, ctx, branch, Recovery::RejectIncompleteTail).unwrap();
    assert_eq!(node.runtime().unwrap(), &branch_reference);
}

#[test]
fn torn_missing_corrupt_and_substituted_files_are_rejected() {
    for variant in ["missing", "truncated", "corrupt", "duplicated", "reordered"] {
        let t = Temp::new();
        let ctx = context();
        let mut node = SourceDurableReplay::initialize(&t.0, ctx.clone()).unwrap();
        let mut reference = SourceContractRuntime::with_policy(ctx.policy);
        commit(&mut node, &mut reference, vec![fee()]);
        let first = node.publication().unwrap();
        commit(&mut node, &mut reference, vec![fee(), fee()]);
        let expected = node.publication().unwrap();
        drop(node);
        let a = durable_segment_path(&t.0, first.history.last_segment);
        let b = durable_segment_path(&t.0, expected.history.last_segment);
        let original = fs::read(&b).unwrap();
        match variant {
            "missing" => fs::remove_file(&a).unwrap(),
            "truncated" => fs::write(&b, &original[..original.len() / 2]).unwrap(),
            "corrupt" => {
                let mut v = original;
                v[0] ^= 1;
                fs::write(&b, v).unwrap();
            }
            "duplicated" => fs::copy(&a, &b).map(|_| ()).unwrap(),
            "reordered" => {
                let old = fs::read(&a).unwrap();
                fs::write(&a, original).unwrap();
                fs::write(&b, old).unwrap();
            }
            _ => unreachable!(),
        }
        assert!(
            SourceDurableReplay::open(&t.0, ctx, expected, Recovery::RejectIncompleteTail).is_err(),
            "{variant}"
        );
    }
}

#[test]
fn repaired_hashes_counts_and_publication_cannot_supply_source_receipts() {
    for variant in [
        "changed-command",
        "omitted-ancestor",
        "duplicated-command",
        "reordered-commands",
    ] {
        let t = Temp::new();
        let ctx = context();
        let mut node = SourceDurableReplay::initialize(&t.0, ctx.clone()).unwrap();
        let bootstrap = node.publication().unwrap();
        let hash = node.context_hash;
        let mut reference = SourceContractRuntime::with_policy(ctx.policy);
        commit(&mut node, &mut reference, vec![fee()]);
        commit(
            &mut node,
            &mut reference,
            vec![fee(), SourceContractCommand::SetHeight(n(11.))],
        );
        let old = node.publication().unwrap();
        drop(node);
        let mut segment = durable_read_segment(&t.0, hash, old.history.last_segment).unwrap();
        match variant {
            "changed-command" => {
                segment.entries[0].command = SourceContractCommand::SetHeight(n(99.))
            }
            "omitted-ancestor" => segment.previous = bootstrap.history,
            "duplicated-command" => segment.entries.insert(0, segment.entries[0].clone()),
            "reordered-commands" => segment.entries.swap(0, 1),
            _ => unreachable!(),
        }
        let bytes = durable_encode_segment(hash, &segment).unwrap();
        let digest = sha256(&bytes);
        fs::write(durable_segment_path(&t.0, digest), bytes).unwrap();
        let forged = SourceDurablePublication {
            revision: old.revision + 1,
            history: durable_after(hash, &segment, digest).unwrap(),
        };
        let namespace = Namespace::new(&ctx.chain, ctx.test, "consensus").unwrap();
        let (mut journal, _) =
            Journal::open_existing(&t.0, namespace, Recovery::RejectIncompleteTail).unwrap();
        journal
            .commit(vec![Mutation::Put {
                key: DURABLE_PUBLICATION.into(),
                value: durable_publication_bytes(forged).unwrap(),
            }])
            .unwrap();
        drop(journal);
        assert!(
            matches!(
                SourceDurableReplay::open(&t.0, ctx, forged, Recovery::RejectIncompleteTail),
                Err(SourceDurableError::Replay(
                    SourceContractReplayError::ReplayMismatch
                ))
            ),
            "{variant}"
        );
    }
}

#[test]
fn ambiguous_io_poison_and_same_batch_retry_do_not_double_apply() {
    for fault in [
        DurableTestFault::TornTemporary,
        DurableTestFault::AfterSegmentSync,
        DurableTestFault::AfterPublicationSync,
    ] {
        let t = Temp::new();
        let ctx = context();
        let mut node = SourceDurableReplay::initialize(&t.0, ctx.clone()).unwrap();
        let old = node.publication().unwrap();
        let commands = vec![fee(), fee()];
        let expected = planned(&node, &commands);
        node.test_fault = Some(fault);
        assert!(
            matches!(node.commit_batch(old,"crash-test",commands.clone()),Err(SourceDurableError::PublicationFailed{attempted,..})if *attempted==expected)
        );
        assert!(matches!(node.runtime(), Err(SourceDurableError::Poisoned)));
        assert!(matches!(
            node.publication(),
            Err(SourceDurableError::Poisoned)
        ));
        drop(node);
        let mut reference = SourceContractRuntime::with_policy(ctx.policy);
        for command in &commands {
            direct(&mut reference, command);
        }
        if fault == DurableTestFault::AfterPublicationSync {
            assert!(matches!(
                SourceDurableReplay::open(&t.0, ctx.clone(), old, Recovery::RejectIncompleteTail),
                Err(SourceDurableError::WrongHead)
            ));
        } else {
            let mut resumed =
                SourceDurableReplay::open(&t.0, ctx.clone(), old, Recovery::RejectIncompleteTail)
                    .unwrap();
            resumed.commit_batch(old, "crash-test", commands).unwrap();
            assert_eq!(resumed.publication().unwrap(), expected);
            drop(resumed);
        }
        let resumed =
            SourceDurableReplay::open(&t.0, ctx, expected, Recovery::RejectIncompleteTail).unwrap();
        assert_eq!(resumed.runtime().unwrap(), &reference);
    }
}

#[test]
#[ignore = "spawned only by process_termination_at_real_io_boundaries"]
fn durable_process_child() {
    let root = PathBuf::from(std::env::var_os("TL_SOURCE_DURABLE_CHILD_ROOT").unwrap());
    let fault = match std::env::var("TL_SOURCE_DURABLE_CHILD_FAULT")
        .unwrap()
        .as_str()
    {
        "temporary" => DurableTestFault::TornTemporary,
        "segment" => DurableTestFault::AfterSegmentSync,
        "publication" => DurableTestFault::AfterPublicationSync,
        _ => panic!(),
    };
    let ctx = context();
    let bytes = durable_context(&ctx).unwrap();
    let initial = SourceDurablePublication {
        revision: 0,
        history: durable_bootstrap(
            sha256(&bytes),
            &SourceContractReplay::with_policy(ctx.policy).unwrap(),
        )
        .unwrap(),
    };
    let mut node =
        SourceDurableReplay::open(&root, ctx, initial, Recovery::RejectIncompleteTail).unwrap();
    node.test_fault = Some(fault);
    node.commit_batch(initial, "crash-test", vec![fee(), fee()])
        .unwrap();
    panic!("fault must terminate this process before destructors");
}

#[test]
fn process_termination_at_real_io_boundaries_recovers_exactly_once() {
    for fault in ["temporary", "segment", "publication"] {
        let t = Temp::new();
        let ctx = context();
        let node = SourceDurableReplay::initialize(&t.0, ctx.clone()).unwrap();
        let old = node.publication().unwrap();
        let expected = planned(&node, &[fee(), fee()]);
        drop(node);
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "source_contract_replay::source_durable::tests::durable_process_child",
                "--nocapture",
            ])
            .env("TL_SOURCE_DURABLE_CHILD_ROOT", &t.0)
            .env("TL_SOURCE_DURABLE_CHILD_FAULT", fault)
            .env("TL_SOURCE_DURABLE_TERMINATE", "1")
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(86),
            "{fault}: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let mut reference = SourceContractRuntime::with_policy(ctx.policy);
        direct(&mut reference, &fee());
        direct(&mut reference, &fee());
        if fault != "publication" {
            let mut node =
                SourceDurableReplay::open(&t.0, ctx.clone(), old, Recovery::RejectIncompleteTail)
                    .unwrap();
            node.commit_batch(old, "crash-test", vec![fee(), fee()])
                .unwrap();
            assert_eq!(node.publication().unwrap(), expected);
            drop(node);
        }
        let node =
            SourceDurableReplay::open(&t.0, ctx, expected, Recovery::RejectIncompleteTail).unwrap();
        assert_eq!(node.runtime().unwrap(), &reference);
    }
}

#[test]
fn explicit_context_expected_publication_and_archive_bounds_are_enforced() {
    let t = Temp::new();
    let ctx = context();
    let mut node = SourceDurableReplay::initialize(&t.0, ctx.clone()).unwrap();
    let old = node.publication().unwrap();
    let path = node.journal.path().to_path_buf();
    assert!(matches!(
        node.commit_batch(old, "empty", vec![]),
        Err(SourceDurableError::Capacity)
    ));
    assert!(matches!(
        node.commit_batch(
            old,
            "oversized",
            vec![fee(); MAX_DURABLE_SEGMENT_COMMANDS + 1]
        ),
        Err(SourceDurableError::Capacity)
    ));
    assert_eq!(node.publication().unwrap(), old);
    drop(node);
    let old_bytes = fs::read(&path).unwrap();
    let mut node =
        SourceDurableReplay::open(&t.0, ctx.clone(), old, Recovery::RejectIncompleteTail).unwrap();
    node.commit_batch(old, "new", vec![fee()]).unwrap();
    let expected = node.publication().unwrap();
    assert!(matches!(
        node.commit_batch(old, "stale", vec![fee()]),
        Err(SourceDurableError::WrongHead)
    ));
    drop(node);
    let mut wrong = ctx.clone();
    wrong.test = false;
    assert!(
        SourceDurableReplay::open(&t.0, wrong, expected, Recovery::RejectIncompleteTail).is_err()
    );
    let mut wrong = ctx.clone();
    wrong.policy = SourceContractRuntimePolicy::MainCallableV8;
    assert!(matches!(
        SourceDurableReplay::open(&t.0, wrong, expected, Recovery::RejectIncompleteTail),
        Err(SourceDurableError::WrongContext)
    ));
    fs::write(&path, old_bytes).unwrap();
    assert!(
        matches!(
            SourceDurableReplay::open(&t.0, ctx.clone(), expected, Recovery::RejectIncompleteTail),
            Err(SourceDurableError::WrongHead)
        ),
        "a structurally valid old journal does not meet the independent expected head"
    );
    let node =
        SourceDurableReplay::open(&t.0, ctx.clone(), old, Recovery::RejectIncompleteTail).unwrap();
    assert_eq!(node.publication().unwrap(), old);
    drop(node);
    if !cfg!(unix) {
        let mut wrong = ctx;
        wrong.filesystem = SourceDurableFilesystem::FileAndDirectorySync;
        assert!(matches!(
            durable_context(&wrong),
            Err(SourceDurableError::UnsupportedFilesystem)
        ));
    }
}

#[test]
fn tlcr_policy_tags_canonical_roundtrips_and_4096_capacity() {
    for (i, policy) in [
        SourceContractRuntimePolicy::OnchainV1,
        SourceContractRuntimePolicy::GlobalClearingV2,
        SourceContractRuntimePolicy::SourceLifecycleV3,
        SourceContractRuntimePolicy::OptionLifecycleV4,
        SourceContractRuntimePolicy::DistributionLifecycleV5,
        SourceContractRuntimePolicy::ProofLifecycleV6,
        SourceContractRuntimePolicy::LowerLifecycleV7,
        SourceContractRuntimePolicy::MainCallableV8,
        SourceContractRuntimePolicy::SpotLifecycleV9,
        SourceContractRuntimePolicy::ActivationLifecycleV10,
        SourceContractRuntimePolicy::AmmLifecycleV11,
    ]
    .into_iter()
    .enumerate()
    {
        let mut replay = SourceContractReplay::with_policy(policy).unwrap();
        for populated in [false, true] {
            if populated {
                replay
                    .apply(SourceContractCommand::SetHeight(n(10.)))
                    .unwrap();
            }
            let bytes = replay.checkpoint().unwrap();
            assert_eq!(&bytes[..5], &[b'T', b'L', b'C', b'R', (i + 1) as u8]);
            let restored = SourceContractReplay::from_checkpoint(&bytes, replay.head()).unwrap();
            assert_eq!(
                restored, replay,
                "current canonical roundtrip, not an older-library comparison"
            );
            assert_eq!(restored.checkpoint().unwrap(), bytes);
        }
    }
    assert_eq!(MAX_COMMANDS, 4096);
    assert_eq!(MAX_CHECKPOINT_BYTES, 32 * 1024 * 1024);
    let mut bounded = SourceContractReplay::new().unwrap();
    for _ in 0..MAX_COMMANDS {
        bounded
            .apply(SourceContractCommand::SetHeight(n(10.)))
            .unwrap();
    }
    let before_head = bounded.head();
    let before_runtime = bounded.runtime().clone();
    let before_checkpoint = bounded.checkpoint().unwrap();
    assert_eq!(
        bounded.apply(SourceContractCommand::SetHeight(n(11.))),
        Err(SourceContractReplayError::Capacity)
    );
    assert_eq!(bounded.head(), before_head);
    assert_eq!(bounded.runtime(), &before_runtime);
    assert_eq!(bounded.checkpoint().unwrap(), before_checkpoint);
}

#[test]
fn genuine_contract_queue_aliases_and_late_failure_retry_reexecute_across_segments() {
    let t = Temp::new();
    let mut ctx = context();
    ctx.policy = SourceContractRuntimePolicy::OnchainV1;
    let mut node = SourceDurableReplay::initialize(&t.0, ctx.clone()).unwrap();
    let mut reference = SourceContractRuntime::with_policy(ctx.policy);
    let mut threw = false;
    let mut retry = false;
    for line in include_str!("../fixtures/source_durable_contract_inputs.jsonl").lines() {
        let v = crate::property_catalog::parse_diagnostic_value(line).unwrap();
        if v.get("kind") != s("SourceFixtureInput") {
            commit(
                &mut node,
                &mut reference,
                vec![SourceContractCommand::from_value(&v).unwrap()],
            );
            continue;
        }
        let input = v.get("input");
        let command = if let V::String(raw) = input.get("raw") {
            SourceContractCommand::ExecuteContract(vec![
                input.get("txid"),
                n(18.),
                s("tl"),
                s(&raw),
                input.get("sender"),
                V::Null,
                n(0.),
                n(0.),
                n(10.),
            ])
        } else if input.get("drain") != V::Undefined {
            retry |= threw;
            SourceContractCommand::EvaluateQueue(SourceCall {
                method: "processQueuedOnChainOrdersForBlock".into(),
                arguments: vec![input.get("drain")],
            })
        } else if input.get("failure") != V::Undefined {
            let f = input.get("failure");
            SourceContractCommand::FailStorage(StorageFailure {
                collection: string(f.get("collection")).unwrap(),
                operation: format!("{}Async", string(f.get("op")).unwrap()),
                skip: 0,
                message: string(f.get("message")).unwrap(),
            })
        } else {
            panic!("unsupported original fixture input")
        };
        let outcome = commit(&mut node, &mut reference, vec![command]);
        assert!(
            !matches!(
                outcome[0].source.outcome,
                SourceContractCommandOutcome::LocalError(_)
            ),
            "reached real owned source operation"
        );
        threw |= matches!(
            outcome[0].source.outcome,
            SourceContractCommandOutcome::Source(SourceOutcome::Thrown(_))
        );
        let expected = node.publication().unwrap();
        drop(node);
        node =
            SourceDurableReplay::open(&t.0, ctx.clone(), expected, Recovery::RejectIncompleteTail)
                .unwrap();
        assert_eq!(
            node.runtime().unwrap(),
            &reference,
            "full receiver/position/job identity is reconstructed by actual commands"
        );
    }
    assert!(
        threw && retry,
        "genuine volume-write financial prefix survives restart into the real queue retry"
    );
}

#[test]
fn orphan_retry_repeats_failed_directory_barrier_before_publication() {
    struct ResetProbe;
    impl Drop for ResetProbe {
        fn drop(&mut self) {
            DURABLE_DIRECTORY_SYNC_PROBE.with(|probe| {
                let _ = probe.borrow_mut().take();
            });
        }
    }
    DURABLE_DIRECTORY_SYNC_PROBE.with(|probe| {
        assert!(probe
            .replace(Some(DurableDirectorySyncProbe {
                segment_attempts: 0,
                failures_remaining: 2,
            }))
            .is_none());
    });
    let _reset = ResetProbe;
    let t = Temp::new();
    let mut ctx = context();
    ctx.filesystem = SourceDurableFilesystem::FileAndDirectorySync;
    let mut node = SourceDurableReplay::initialize(&t.0, ctx.clone()).unwrap();
    let old = node.publication().unwrap();
    let old_runtime = node.runtime().unwrap().clone();
    let commands = vec![fee(), fee()];
    let expected = planned(&node, &commands);
    // Failure one occurs in the actual first write path after file sync and
    // rename, before any successful segment-directory barrier or publication.
    for attempt in 1..=2 {
        let error = node
            .commit_batch(old, "crash-test", commands.clone())
            .unwrap_err();
        let SourceDurableError::PublicationFailed { attempted, detail } = error else {
            panic!("attempt {attempt} failed outside publication: {error:?}");
        };
        assert_eq!(*attempted, expected);
        assert!(
            detail.contains("injected segment-directory sync failure"),
            "attempt {attempt}: {detail}"
        );
        assert!(matches!(node.runtime(), Err(SourceDurableError::Poisoned)));
        assert!(matches!(
            node.publication(),
            Err(SourceDurableError::Poisoned)
        ));
        let segment = durable_segment_path(&t.0, expected.history.last_segment);
        assert!(
            segment.is_file(),
            "rename completed before the injected failure"
        );
        DURABLE_DIRECTORY_SYNC_PROBE.with(|probe| {
            assert_eq!(probe.borrow().as_ref().unwrap().segment_attempts, attempt);
        });
        drop(node);
        // Failure two uses this already verified immutable orphan. If that
        // fast path skips its barrier, it publishes and this loop fails.
        node = SourceDurableReplay::open(&t.0, ctx.clone(), old, Recovery::RejectIncompleteTail)
            .unwrap();
        assert_eq!(
            node.publication().unwrap(),
            old,
            "a failed barrier cannot publish"
        );
        assert_eq!(node.runtime().unwrap(), &old_runtime);
    }
    let receipts = node
        .commit_batch(old, "crash-test", commands.clone())
        .unwrap();
    assert_eq!(node.publication().unwrap(), expected);
    assert_eq!(receipts.len(), 2);
    assert_eq!(expected.history.commands, 2);
    assert_eq!(expected.history.segments, 1);
    DURABLE_DIRECTORY_SYNC_PROBE.with(|probe| {
        assert_eq!(probe.borrow().as_ref().unwrap().segment_attempts, 3);
    });
    let mut reference = SourceContractRuntime::with_policy(ctx.policy);
    for command in &commands {
        direct(&mut reference, command);
    }
    assert_eq!(
        node.runtime().unwrap(),
        &reference,
        "two failed candidates did not apply twice"
    );
    assert!(matches!(
        node.commit_batch(old, "crash-test", commands),
        Err(SourceDurableError::WrongHead)
    ));
    DURABLE_DIRECTORY_SYNC_PROBE.with(|probe| {
        assert_eq!(
            probe.borrow().as_ref().unwrap().segment_attempts,
            3,
            "stale CAS never reaches I/O"
        );
    });
    drop(node);
    let restored =
        SourceDurableReplay::open(&t.0, ctx, expected, Recovery::RejectIncompleteTail).unwrap();
    assert_eq!(restored.runtime().unwrap(), &reference);
}

// Separate current-owner continuation evidence; explicit unverified DB inputs.
fn durable_policy_available(runtime: &SourceContractRuntime, asset: &str) -> V {
    let V::Array(rows) = runtime.tally_cache() else {
        panic!("Tally rows")
    };
    for row in rows {
        let V::Array(pair) = row else {
            panic!("Tally pair")
        };
        if pair[0] == s("alice") {
            return pair[1].get(asset).get("available");
        }
    }
    panic!("missing actual alice Tally entry")
}

#[test]
fn v10_durable_restart_retains_actual_property_tally_and_source_throw_prefix() {
    let t = Temp::new();
    let mut ctx = context();
    ctx.policy = SourceContractRuntimePolicy::ActivationLifecycleV10;
    let mut node = SourceDurableReplay::initialize(&t.0, ctx.clone()).unwrap();
    let mut reference = SourceContractRuntime::with_policy(ctx.policy);
    let mut inputs = initial_inputs();
    inputs.push(update(1., -100.));
    let receipts = commit(&mut node, &mut reference, inputs);
    assert!(
        matches!(&receipts.last().unwrap().source.outcome,SourceContractCommandOutcome::Dependency(DependencyReturn::PendingThrow(e)) if e.message.contains("Reserved balance cannot go negative"))
    );
    assert_eq!(
        durable_policy_available(node.runtime().unwrap(), "7"),
        n(11.)
    );
    let retained = node.publication().unwrap();
    drop(node);
    let mut node =
        SourceDurableReplay::open(&t.0, ctx.clone(), retained, Recovery::RejectIncompleteTail)
            .unwrap();
    assert_eq!(node.runtime().unwrap(), &reference);
    commit(
        &mut node,
        &mut reference,
        vec![
            update(2., 0.),
            call("Property.getPropertyData", vec![n(7.)]),
        ],
    );
    // Pinned tally.js updateBalance awaits loadFromDB before each update.
    // The prior reserved-negative throw retained11 only in memory; the next
    // source call reloads the persisted10 and adds2, rather than repairing it.
    assert_eq!(
        durable_policy_available(node.runtime().unwrap(), "7"),
        n(12.)
    );
    assert_eq!(
        node.runtime()
            .unwrap()
            .state_value()
            .unwrap()
            .get("runtimePolicy"),
        s("ActivationLifecycleV10")
    );
    let expected = node.publication().unwrap();
    drop(node);
    let restored =
        SourceDurableReplay::open(&t.0, ctx, expected, Recovery::RejectIncompleteTail).unwrap();
    assert_eq!(restored.runtime().unwrap(), &reference);
    assert_eq!(expected.history.commands, 7);
    assert_eq!(expected.history.segments, 2);
}

#[test]
fn v11_durable_restart_retains_actual_amm_property_alias_and_capital_roundtrip() {
    let t = Temp::new();
    let mut ctx = context();
    ctx.policy = SourceContractRuntimePolicy::AmmLifecycleV11;
    let mut node = SourceDurableReplay::initialize(&t.0, ctx.clone()).unwrap();
    let mut reference = SourceContractRuntime::with_policy(ctx.policy);
    let mut inputs = initial_inputs();
    inputs[0] = SourceContractCommand::Import {
        collection: "propertyList".into(),
        rows: vec![obj(&[
            ("_id", s("propertyIndex")),
            (
                "value",
                s(
                    r#"[[7,{"ticker":"A","totalInCirculation":10,"type":1,"whitelistId":0,"issuer":"alice"}],[8,{"ticker":"B","totalInCirculation":20,"type":1,"whitelistId":0,"issuer":"alice"}]]"#,
                ),
            ),
        ])],
    };
    inputs[1] = SourceContractCommand::Import {
        collection: "tallyMap".into(),
        rows: vec![obj(&[
            ("_id", s("tallyMap")),
            (
                "data",
                s(
                    r#"[["alice",{"7":{"amount":10,"available":10,"reserved":0,"margin":0,"vesting":0,"channelBalance":0},"8":{"amount":20,"available":20,"reserved":0,"margin":0,"vesting":0,"channelBalance":0}}]]"#,
                ),
            ),
        ])],
    };
    inputs.push(call("SourceGlobal.bindPropertyRegistry", vec![]));
    inputs.push(call(
        "SourceAMM.construct",
        vec![n(0.), n(100.), n(10.), n(0.)],
    ));
    let receipts = commit(&mut node, &mut reference, inputs);
    let SourceContractCommandOutcome::Dependency(DependencyReturn::Immediate(handle)) =
        &receipts.last().unwrap().source.outcome
    else {
        panic!("actual AMM constructor handle")
    };
    let handle = handle.clone();
    commit(
        &mut node,
        &mut reference,
        vec![
            call("SourceAMM.attachPropertyPool", vec![n(7.), n(8.), handle]),
            call(
                "Logic.AMMPool",
                vec![
                    s("alice"),
                    n(10.),
                    V::Bool(false),
                    V::Bool(false),
                    n(7.),
                    n(2.),
                    n(8.),
                    n(3.),
                ],
            ),
        ],
    );
    assert_eq!(
        durable_policy_available(node.runtime().unwrap(), "7"),
        n(8.)
    );
    assert_eq!(
        durable_policy_available(node.runtime().unwrap(), "8"),
        n(17.)
    );
    let V::Array(instances) = node
        .runtime()
        .unwrap()
        .state_value()
        .unwrap()
        .get("ammLifecycle")
        .get("instances")
    else {
        panic!()
    };
    assert_eq!(instances.len(), 1);
    assert_eq!(instances[0].get("position"), n(2.));
    let retained = node.publication().unwrap();
    drop(node);
    let mut node =
        SourceDurableReplay::open(&t.0, ctx.clone(), retained, Recovery::RejectIncompleteTail)
            .unwrap();
    assert_eq!(
        node.runtime().unwrap(),
        &reference,
        "same actual Tally, Property Map and AMM receiver after restart"
    );
    commit(
        &mut node,
        &mut reference,
        vec![call(
            "Logic.AMMPool",
            vec![
                s("alice"),
                n(11.),
                V::Bool(true),
                V::Bool(false),
                n(7.),
                n(2.),
                n(8.),
                n(3.),
            ],
        )],
    );
    assert_eq!(
        durable_policy_available(node.runtime().unwrap(), "7"),
        n(10.)
    );
    assert_eq!(
        durable_policy_available(node.runtime().unwrap(), "8"),
        n(20.)
    );
    let V::Array(instances) = node
        .runtime()
        .unwrap()
        .state_value()
        .unwrap()
        .get("ammLifecycle")
        .get("instances")
    else {
        panic!()
    };
    assert_eq!(instances.len(), 1);
    assert_eq!(instances[0].get("position"), n(0.));
    assert_eq!(
        node.runtime()
            .unwrap()
            .state_value()
            .unwrap()
            .get("runtimePolicy"),
        s("AmmLifecycleV11")
    );
    let expected = node.publication().unwrap();
    drop(node);
    let restored =
        SourceDurableReplay::open(&t.0, ctx, expected, Recovery::RejectIncompleteTail).unwrap();
    assert_eq!(restored.runtime().unwrap(), &reference);
    assert_eq!(expected.history.commands, 9);
    assert_eq!(expected.history.segments, 3);
}
