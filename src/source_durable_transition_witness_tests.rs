use std::sync::atomic::AtomicUsize;
static TEMP_ID: AtomicUsize = AtomicUsize::new(0);
struct Temp {
    path: PathBuf,
    parent: PathBuf,
}
impl Temp {
    fn new() -> Self {
        let parent = std::env::var_os("TL_DURABLE_WITNESS_TMP")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        fs::create_dir_all(&parent).unwrap();
        let parent = parent.canonicalize().unwrap();
        let path = parent.join(format!(
            "tl-dw5-{}-{}",
            std::process::id(),
            TEMP_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        assert!(path.canonicalize().unwrap().starts_with(&parent));
        Self { path, parent }
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        if let Ok(path) = self.path.canonicalize() {
            if path.starts_with(&self.parent)
                && path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("tl-dw5-")
            {
                let _ = fs::remove_dir_all(path);
            }
        }
    }
}
fn copy_tree(from: &Path, to: &Path) {
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let kind = entry.file_type().unwrap();
        assert!(!kind.is_symlink());
        let target = to.join(entry.file_name());
        if kind.is_dir() {
            fs::create_dir(&target).unwrap();
            copy_tree(&entry.path(), &target)
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}
fn context() -> SourceDurableContext {
    SourceDurableContext {
        chain: "TL-DURABLE-WITNESS".into(),
        test: true,
        policy: SourceContractRuntimePolicy::AmmLifecycleV11,
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
fn update(a: f64, r: f64) -> SourceContractCommand {
    call(
        "Tally.updateBalance",
        vec![
            s("alice"),
            n(7.),
            n(a),
            n(r),
            n(0.),
            n(0.),
            s("durable-witness"),
            n(10.),
        ],
    )
}
fn inputs() -> Vec<SourceContractCommand> {
    vec![
        SourceContractCommand::Import {
            collection: "propertyList".into(),
            rows: vec![obj(&[
                ("_id", s("propertyIndex")),
                (
                    "value",
                    s(
                        r#"[[7,{"ticker":"DW5","totalInCirculation":10,"type":1,"whitelistId":0,"issuer":"alice"}]]"#,
                    ),
                ),
            ])],
        },
        SourceContractCommand::Import {
            collection: "tallyMap".into(),
            rows: vec![obj(&[
                ("_id", s("tallyMap")),
                (
                    "data",
                    s(
                        r#"[["alice",{"7":{"amount":10,"available":10,"reserved":0,"margin":0,"vesting":0,"channelBalance":0}}]]"#,
                    ),
                ),
            ])],
        },
        call("Tally.loadFromDB", vec![]),
        call("Property.getPropertyData", vec![n(7.)]),
    ]
}
fn append(
    archive: &mut SourceDurableReplay,
    label: &str,
    commands: Vec<SourceContractCommand>,
) -> Vec<SourceDurableReceipt> {
    archive
        .commit_batch(archive.publication().unwrap(), label, commands)
        .unwrap()
}
struct Pins {
    context: SourceDurableContext,
    descriptor: SourceFullDispatchDescriptor,
    archive: SourceDurablePublication,
    pre: SourceDurableHistoryHead,
    post: SourceDurableHistoryHead,
    pre_context: [u8; 32],
    post_context: [u8; 32],
    commands: [u8; 32],
    receipts: [u8; 32],
    after: SourceContractRuntime,
}
impl Pins {
    fn new(root: &Path, label: &str, commands: &[SourceContractCommand]) -> Self {
        let c = context();
        let namespace = Namespace::new(&c.chain, c.test, "consensus").unwrap();
        let (journal, _) =
            Journal::open_existing(root, namespace, Recovery::RejectIncompleteTail).unwrap();
        let publication =
            durable_parse_publication(journal.get(DURABLE_PUBLICATION).unwrap().unwrap()).unwrap();
        drop(journal);
        let before =
            SourceDurableReplay::open(root, c.clone(), publication, Recovery::RejectIncompleteTail)
                .unwrap();
        let pre_context = source_durable_transition_context_commitment(
            &before,
            SourceFullDispatchScope::SourceCommandSlice,
        )
        .unwrap();
        drop(before);
        let clone = Temp::new();
        copy_tree(root, &clone.path);
        let mut reference = SourceDurableReplay::open(
            &clone.path,
            c.clone(),
            publication,
            Recovery::RejectIncompleteTail,
        )
        .unwrap();
        let receipts = append(&mut reference, label, commands.to_vec());
        let post = reference.publication().unwrap().history;
        Self {
            context: c.clone(),
            descriptor: SourceFullDispatchDescriptor::reference(),
            archive: publication,
            pre: publication.history,
            post,
            pre_context,
            post_context: source_durable_transition_context_commitment(
                &reference,
                SourceFullDispatchScope::SourceCommandSlice,
            )
            .unwrap(),
            commands: source_durable_transition_commands_commitment(
                &c,
                SourceFullDispatchScope::SourceCommandSlice,
                publication.history,
                label,
                commands,
            )
            .unwrap(),
            receipts: source_durable_transition_receipts_commitment(
                &c,
                SourceFullDispatchScope::SourceCommandSlice,
                publication.history,
                &receipts,
            )
            .unwrap(),
            after: reference.runtime().unwrap().clone(),
        }
    }
    fn expected(&self) -> SourceDurableTransitionExpectation<'_> {
        SourceDurableTransitionExpectation {
            context: &self.context,
            archive_publication: self.archive,
            descriptor: &self.descriptor,
            scope: SourceFullDispatchScope::SourceCommandSlice,
            pre_history: self.pre,
            pre_context_commitment: self.pre_context,
            commands_commitment: self.commands,
            receipts_commitment: self.receipts,
            post_history: self.post,
            post_context_commitment: self.post_context,
        }
    }
}

#[test]
fn long_financial_prefix_native_throw_retry_and_current_publication_replay() {
    let root = Temp::new();
    let mut archive = SourceDurableReplay::initialize(&root.path, context()).unwrap();
    append(&mut archive, "actual-inputs", inputs());
    let mut prefix = (0..32)
        .map(|i| update(if i % 2 == 0 { 0.25 } else { -0.25 }, 0.))
        .collect::<Vec<_>>();
    prefix.extend((0..5000).map(|_| fee()));
    for (i, batch) in prefix.chunks(256).enumerate() {
        append(&mut archive, &format!("real-prefix-{i}"), batch.to_vec());
    }
    let before = archive.publication().unwrap();
    assert_eq!(before.history.commands, 5036);
    assert!(before.history.segments > 16);
    let pre_context = source_durable_transition_context_commitment(
        &archive,
        SourceFullDispatchScope::SourceCommandSlice,
    )
    .unwrap();
    let prior = archive.runtime().unwrap().clone();
    let commands = vec![update(1., -1.)];
    let receipts = append(&mut archive, "native-error", commands.clone());
    let after = archive.publication().unwrap();
    let actual = archive.runtime().unwrap().clone();
    assert!(matches!(
        receipts[0].source.outcome,
        SourceContractCommandOutcome::Dependency(DependencyReturn::PendingThrow(_))
    ));
    assert_ne!(actual, prior);
    let descriptor = SourceFullDispatchDescriptor::reference();
    let c = context();
    let scope = SourceFullDispatchScope::SourceCommandSlice;
    let mut expected = SourceDurableTransitionExpectation {
        context: &c,
        archive_publication: after,
        descriptor: &descriptor,
        scope,
        pre_history: before.history,
        pre_context_commitment: pre_context,
        commands_commitment: source_durable_transition_commands_commitment(
            &c,
            scope,
            before.history,
            "native-error",
            &commands,
        )
        .unwrap(),
        receipts_commitment: source_durable_transition_receipts_commitment(
            &c,
            scope,
            before.history,
            &receipts,
        )
        .unwrap(),
        post_history: after.history,
        post_context_commitment: source_durable_transition_context_commitment(&archive, scope)
            .unwrap(),
    };
    let witness = SourceDurableTransitionWitness::produce_from_archive(
        &archive,
        "native-error",
        &commands,
        &expected,
    )
    .unwrap();
    let bytes = witness.canonical_bytes().unwrap();
    assert_eq!(&bytes[..5], b"TLSW\x05");
    let checked =
        SourceDurableTransitionWitness::verify_bytes_from_archive(&archive, &bytes, &expected)
            .unwrap();
    assert_eq!(checked.runtime(), &actual);
    assert_eq!(checked.receipts()[0].absolute_sequence, 5036);
    assert_eq!(
        checked.require_authenticated_input_and_state(),
        Err(SourceRuleError::UnauthenticatedInputAndState)
    );
    assert_eq!(archive.publication().unwrap(), after);
    assert_eq!(
        archive.runtime().unwrap(),
        &actual,
        "verification cannot change the live owner"
    );
    let retry = append(&mut archive, "real-retry", commands);
    assert!(matches!(
        retry[0].source.outcome,
        SourceContractCommandOutcome::Dependency(DependencyReturn::PendingThrow(_))
    ));
    // Original updateBalance loads the unchanged persisted row again, so this
    // failed retry retains the SAME prefix; do not invent a second debit.
    assert_eq!(archive.runtime().unwrap(), &actual);
    expected.archive_publication = archive.publication().unwrap();
    assert_eq!(
        witness
            .verify_from_archive(&archive, &expected)
            .unwrap()
            .runtime(),
        &actual
    );
    let current = archive.publication().unwrap();
    drop(archive);
    let restored = SourceDurableReplay::open(
        &root.path,
        c.clone(),
        current,
        Recovery::RejectIncompleteTail,
    )
    .unwrap();
    assert_eq!(restored.runtime().unwrap(), &actual);
    drop(restored);
    let mut stale = expected;
    stale.archive_publication = before;
    assert!(witness.verify(&root.path, &stale).is_err());
}

#[test]
fn local_capability_error_prefix_is_distinct_from_native_source_throw() {
    let root = Temp::new();
    let mut archive = SourceDurableReplay::initialize(&root.path, context()).unwrap();
    let receipts = append(
        &mut archive,
        "unowned",
        vec![call("Unowned.external", vec![])],
    );
    assert!(matches!(
        receipts[0].source.outcome,
        SourceContractCommandOutcome::LocalError(_)
    ));
    let pub_before = archive.publication().unwrap();
    drop(archive);
    // Existing open must still preserve its original acceptance and rollback.
    SourceDurableReplay::open(
        &root.path,
        context(),
        pub_before,
        Recovery::RejectIncompleteTail,
    )
    .unwrap();
    let commands = vec![fee()];
    let pins = Pins::new(&root.path, "next", &commands);
    assert!(matches!(
        SourceDurableTransitionWitness::produce(&root.path, "next", &commands, &pins.expected()),
        Err(DWError::UnownedHistory)
    ));
}

#[test]
fn observer_noop_preserves_original_provider_archive_bytes_open_and_outcomes() {
    const OLD: &str = include_str!("../fixtures/source_durable_transition_compatibility.tsv");
    fn decode(v: &str) -> Vec<u8> {
        assert!(v.len().is_multiple_of(2));
        v.as_bytes()
            .chunks_exact(2)
            .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
            .collect()
    }
    let rows = OLD
        .lines()
        .filter(|s| !s.starts_with('#'))
        .map(|s| s.split('\t').collect::<Vec<_>>())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), 39);
    let expected = |kind: &str, keys: &[String]| -> Vec<u8> {
        let matches = rows
            .iter()
            .filter(|r| {
                r[0] == kind
                    && r.len() == keys.len() + 2
                    && r[1..r.len() - 1].iter().zip(keys).all(|(a, b)| *a == b)
            })
            .collect::<Vec<_>>();
        assert_eq!(
            matches.len(),
            1,
            "unique old-provider observation {kind} {keys:?}"
        );
        decode(matches[0].last().unwrap())
    };
    let root = Temp::new();
    let c = context();
    let mut current = SourceDurableReplay::initialize(&root.path, c.clone()).unwrap();
    assert_eq!(current.context_bytes(), expected("context", &[]));
    for (i, commands) in [
        inputs(),
        vec![fee(), update(0.5, 0.), update(-0.5, 0.)],
        vec![update(1., -1.)],
        vec![
            call("Unowned.external", vec![]),
            SourceContractCommand::SetHeight(n(11.)),
        ],
    ]
    .into_iter()
    .enumerate()
    {
        for (j, command) in commands.iter().enumerate() {
            assert_eq!(
                command.value().diagnostic_json().unwrap().as_bytes(),
                expected("command", &[i.to_string(), j.to_string()]),
                "original input sequence"
            );
        }
        let receipts = append(&mut current, &format!("compatibility-{i}"), commands);
        for (j, receipt) in receipts.iter().enumerate() {
            assert_eq!(
                format!("{receipt:?}").as_bytes(),
                expected("receipt", &[i.to_string(), j.to_string()]),
                "complete original outcome/effects/head"
            );
        }
        assert_eq!(
            format!("{:?}", current.publication().unwrap()).as_bytes(),
            expected("publication", &[i.to_string()])
        );
        assert_eq!(
            format!("{:?}", current.runtime().unwrap()).as_bytes(),
            expected("runtime", &[i.to_string()]),
            "complete original runtime diagnostic"
        );
        assert_eq!(
            current
                .runtime()
                .unwrap()
                .state_value()
                .unwrap()
                .diagnostic_json()
                .unwrap()
                .as_bytes(),
            expected("state", &[i.to_string()]),
            "original state inventory and typed values"
        );
    }
    let publication = current.publication().unwrap();
    drop(current);
    let reopened =
        SourceDurableReplay::open(&root.path, c, publication, Recovery::RejectIncompleteTail)
            .unwrap();
    assert_eq!(
        format!("{:?}", reopened.runtime().unwrap()).as_bytes(),
        expected("reopened_runtime", &[])
    );
    drop(reopened);
    fn files(root: &Path, at: &Path, out: &mut Vec<(String, Vec<u8>)>) {
        for entry in fs::read_dir(at).unwrap() {
            let entry = entry.unwrap();
            assert!(!entry.file_type().unwrap().is_symlink());
            if entry.file_type().unwrap().is_dir() {
                files(root, &entry.path(), out)
            } else {
                let relative = entry
                    .path()
                    .strip_prefix(root)
                    .unwrap()
                    .components()
                    .map(|p| p.as_os_str().to_str().unwrap().to_string())
                    .collect::<Vec<_>>()
                    .join("/");
                out.push((relative, fs::read(entry.path()).unwrap()));
            }
        }
    }
    let mut actual = vec![];
    files(&root.path, &root.path, &mut actual);
    actual.sort_by(|a, b| a.0.cmp(&b.0));
    let mut original = rows
        .iter()
        .filter(|r| r[0] == "file")
        .map(|r| {
            assert_eq!(r.len(), 3);
            (String::from_utf8(decode(r[1])).unwrap(), decode(r[2]))
        })
        .collect::<Vec<_>>();
    original.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(original.len(), 5);
    assert_eq!(
        actual, original,
        "exact old archive/journal/publication file bytes"
    );
}

#[test]
fn missing_corrupt_and_repaired_reordered_duplicated_omitted_segments_refuse() {
    for mode in ["missing", "corrupt", "reorder", "duplicate", "omit"] {
        let root = Temp::new();
        let mut archive = SourceDurableReplay::initialize(&root.path, context()).unwrap();
        append(&mut archive, "two-real-fees", vec![fee(), fee()]);
        let current = archive.publication().unwrap();
        drop(archive);
        let commands = vec![fee()];
        let pins = Pins::new(&root.path, "next", &commands);
        let witness = SourceDurableTransitionWitness::produce(
            &root.path,
            "next",
            &commands,
            &pins.expected(),
        )
        .unwrap();
        let path = durable_segment_path(&root.path, current.history.last_segment);
        if mode == "missing" {
            fs::rename(&path, path.with_extension("missing")).unwrap();
            assert!(witness.verify(&root.path, &pins.expected()).is_err());
            continue;
        }
        if mode == "corrupt" {
            let mut data = fs::read(&path).unwrap();
            data[50] ^= 1;
            fs::write(&path, data).unwrap();
            assert!(witness.verify(&root.path, &pins.expected()).is_err());
            continue;
        }
        let mut archive = SourceDurableReplay::open(
            &root.path,
            context(),
            current,
            Recovery::RejectIncompleteTail,
        )
        .unwrap();
        let mut segment = durable_read_segment(
            &root.path,
            archive.context_hash,
            archive.publication().unwrap().history.last_segment,
        )
        .unwrap();
        match mode {
            "reorder" => segment.entries.swap(0, 1),
            "duplicate" => segment.entries[1] = segment.entries[0].clone(),
            "omit" => {
                segment.entries.remove(0);
            }
            _ => unreachable!(),
        }
        let data = durable_encode_segment(archive.context_hash, &segment).unwrap();
        let digest = sha256(&data);
        fs::write(durable_segment_path(&root.path, digest), &data).unwrap();
        let fake = SourceDurablePublication {
            revision: archive.publication().unwrap().revision + 1,
            history: durable_after(archive.context_hash, &segment, digest).unwrap(),
        };
        archive
            .journal
            .commit(vec![Mutation::Put {
                key: DURABLE_PUBLICATION.into(),
                value: durable_publication_bytes(fake).unwrap(),
            }])
            .unwrap();
        // Every segment checksum/count/hash and publication pin is repaired.
        // Actual replay still rejects stale sequence/head/outcome receipts.
        drop(archive);
        let mut expected = pins.expected();
        expected.archive_publication = fake;
        expected.pre_history = fake.history;
        assert!(matches!(
            SourceDurableReplay::open(&root.path, context(), fake, Recovery::RejectIncompleteTail),
            Err(SourceDurableError::Replay(
                SourceContractReplayError::ReplayMismatch
            ))
        ));
        assert!(witness.verify(&root.path, &expected).is_err());
    }
}

#[test]
fn every_pin_and_a_fully_repaired_forged_native_receipt_still_refuse() {
    let root = Temp::new();
    let mut archive = SourceDurableReplay::initialize(&root.path, context()).unwrap();
    append(&mut archive, "inputs", inputs());
    drop(archive);
    let commands = vec![update(1., -1.)];
    let pins = Pins::new(&root.path, "native", &commands);
    let witness =
        SourceDurableTransitionWitness::produce(&root.path, "native", &commands, &pins.expected())
            .unwrap();
    for which in 0..7 {
        let mut e = pins.expected();
        match which {
            0 => e.pre_history.source_state[0] ^= 1,
            1 => e.post_history.commitment[0] ^= 1,
            2 => e.commands_commitment[0] ^= 1,
            3 => e.receipts_commitment[0] ^= 1,
            4 => e.pre_context_commitment[0] ^= 1,
            5 => e.post_context_commitment[0] ^= 1,
            6 => e.archive_publication.revision += 1,
            _ => unreachable!(),
        }
        assert!(witness.verify(&root.path, &e).is_err());
    }
    let mut wrong = context();
    wrong.chain.push('X');
    let mut e = pins.expected();
    e.context = &wrong;
    assert!(witness.verify(&root.path, &e).is_err());
    let mut forged = witness.clone();
    let hash = dw_context(&pins.context).unwrap().1;
    let mut segment = durable_decode_segment(hash, &forged.segment).unwrap();
    segment.entries[0].receipt.outcome =
        SourceContractCommandOutcome::Dependency(DependencyReturn::Pending(V::Undefined));
    forged.segment = durable_encode_segment(hash, &segment).unwrap();
    forged.post_history = durable_after(hash, &segment, sha256(&forged.segment)).unwrap();
    forged.receipts = source_durable_transition_receipts_commitment(
        &pins.context,
        SourceFullDispatchScope::SourceCommandSlice,
        pins.pre,
        &dw_receipts(&segment),
    )
    .unwrap();
    let mut expected = pins.expected();
    expected.post_history = forged.post_history;
    expected.receipts_commitment = forged.receipts;
    let repaired = forged.canonical_bytes().unwrap();
    assert!(matches!(
        SourceDurableTransitionWitness::verify_bytes(&root.path, &repaired, &expected),
        Err(DWError::WrongReceipts)
    ));
}

#[test]
fn main_scope_binds_pending_work_across_real_durable_boundaries() {
    let root = Temp::new();
    let mut archive = SourceDurableReplay::initialize(&root.path, context()).unwrap();
    let before = archive.publication().unwrap();
    let c = context();
    let descriptor = SourceFullDispatchDescriptor::reference();
    let scope = SourceFullDispatchScope::MainCallableSlice;
    let pre_context = source_durable_transition_context_commitment(&archive, scope).unwrap();
    let commands = vec![call("Main.saveMaxProcessedHeight", vec![n(123.)])];
    let receipts = append(&mut archive, "main-pending", commands.clone());
    let after = archive.publication().unwrap();
    let actual = archive.runtime().unwrap().clone();
    let post_context = source_durable_transition_context_commitment(&archive, scope).unwrap();
    assert_ne!(pre_context, post_context);
    let mut expected = SourceDurableTransitionExpectation {
        context: &c,
        archive_publication: after,
        descriptor: &descriptor,
        scope,
        pre_history: before.history,
        pre_context_commitment: pre_context,
        commands_commitment: source_durable_transition_commands_commitment(
            &c,
            scope,
            before.history,
            "main-pending",
            &commands,
        )
        .unwrap(),
        receipts_commitment: source_durable_transition_receipts_commitment(
            &c,
            scope,
            before.history,
            &receipts,
        )
        .unwrap(),
        post_history: after.history,
        post_context_commitment: post_context,
    };
    let witness = SourceDurableTransitionWitness::produce_from_archive(
        &archive,
        "main-pending",
        &commands,
        &expected,
    )
    .unwrap();
    assert_eq!(
        witness
            .verify_from_archive(&archive, &expected)
            .unwrap()
            .runtime(),
        &actual
    );
    append(
        &mut archive,
        "main-resume",
        vec![call("Main.loadMaxProcessedHeight", vec![])],
    );
    expected.archive_publication = archive.publication().unwrap();
    let checked = SourceDurableTransitionWitness::verify_bytes_from_archive(
        &archive,
        &witness.canonical_bytes().unwrap(),
        &expected,
    )
    .unwrap();
    assert_eq!(checked.runtime(), &actual);
    assert_eq!(
        checked.claim(),
        "conditional-archive-backed-source-main-callable-slice-only-v5"
    );
}

#[test]
fn bounded_new_segment_cannot_change_profiles_or_absorb_old_formats() {
    let c = context();
    let root = Temp::new();
    let archive = SourceDurableReplay::initialize(&root.path, c.clone()).unwrap();
    let pre = archive.publication().unwrap().history;
    let scope = SourceFullDispatchScope::SourceCommandSlice;
    assert!(matches!(
        source_durable_transition_commands_commitment(&c, scope, pre, "empty", &[]),
        Err(DWError::Capacity)
    ));
    assert!(matches!(
        source_durable_transition_commands_commitment(&c, scope, pre, "large", &vec![fee(); 257]),
        Err(DWError::Capacity)
    ));
    assert!(matches!(
        source_durable_transition_commands_commitment(&c, scope, pre, &"x".repeat(257), &[fee()]),
        Err(DWError::Capacity)
    ));
    assert!(matches!(
        source_durable_transition_commands_commitment(
            &c,
            scope,
            pre,
            "profile",
            &[SourceContractCommand::SetActivation(true)]
        ),
        Err(DWError::Source(SourceFullDispatchError::UnsupportedCommand))
    ));
    let mut old = c.clone();
    old.policy = SourceContractRuntimePolicy::MainCallableV8;
    assert!(matches!(
        source_durable_transition_commands_commitment(&old, scope, pre, "old", &[fee()]),
        Err(DWError::Source(SourceFullDispatchError::WrongPolicy))
    ));
}

#[test]
fn crash_child() {
    let Some(root) = std::env::var_os("TL_DW5_CRASH_ROOT") else {
        return;
    };
    let expected = durable_parse_publication(
        &fs::read(PathBuf::from(&root).join("expected-publication.bin")).unwrap(),
    )
    .unwrap();
    let mut archive = SourceDurableReplay::open(
        Path::new(&root),
        context(),
        expected,
        Recovery::RejectIncompleteTail,
    )
    .unwrap();
    archive.test_fault = Some(
        match std::env::var("TL_DW5_CRASH_POINT").unwrap().as_str() {
            "torn" => DurableTestFault::TornTemporary,
            "segment" => DurableTestFault::AfterSegmentSync,
            "publication" => DurableTestFault::AfterPublicationSync,
            _ => panic!("point"),
        },
    );
    let _ = archive.commit_batch(expected, "crash-native", vec![update(1., -1.)]);
    panic!("owned child must terminate at86")
}
#[test]
fn actual_child_crash_boundaries_recover_retry_and_support_witnesses() {
    for point in ["torn", "segment", "publication"] {
        let root = Temp::new();
        let mut archive = SourceDurableReplay::initialize(&root.path, context()).unwrap();
        append(&mut archive, "inputs", inputs());
        let old = archive.publication().unwrap();
        drop(archive);
        let commands = vec![update(1., -1.)];
        let pins = Pins::new(&root.path, "crash-native", &commands);
        let witness = SourceDurableTransitionWitness::produce(
            &root.path,
            "crash-native",
            &commands,
            &pins.expected(),
        )
        .unwrap();
        fs::write(
            root.path.join("expected-publication.bin"),
            durable_publication_bytes(old).unwrap(),
        )
        .unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap());
        child
            .args([
                "--exact",
                "source_contract_replay::source_durable::durable_witness_tests::crash_child",
                "--nocapture",
            ])
            .env("TL_DW5_CRASH_ROOT", &root.path)
            .env("TL_DW5_CRASH_POINT", point)
            .env("TL_SOURCE_DURABLE_TERMINATE", "1");
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            child.creation_flags(0x0800_0000);
        }
        assert_eq!(child.status().unwrap().code(), Some(86));
        let actual_pub = if point == "publication" {
            SourceDurablePublication {
                revision: old.revision + 1,
                history: pins.post,
            }
        } else {
            old
        };
        if point == "publication" {
            assert!(matches!(
                SourceDurableReplay::open(
                    &root.path,
                    context(),
                    old,
                    Recovery::RejectIncompleteTail
                ),
                Err(SourceDurableError::WrongHead)
            ));
        }
        let mut restored = SourceDurableReplay::open(
            &root.path,
            context(),
            actual_pub,
            Recovery::RejectIncompleteTail,
        )
        .unwrap();
        if point != "publication" {
            assert_eq!(restored.publication().unwrap(), old);
            append(&mut restored, "crash-native", commands);
        }
        assert_eq!(restored.runtime().unwrap(), &pins.after);
        let mut expected = pins.expected();
        expected.archive_publication = restored.publication().unwrap();
        assert_eq!(
            witness
                .verify_from_archive(&restored, &expected)
                .unwrap()
                .runtime(),
            &pins.after
        );
    }
}
