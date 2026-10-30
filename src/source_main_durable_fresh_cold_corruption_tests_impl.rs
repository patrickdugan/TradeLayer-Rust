// TEST ONLY actual-Journal adversaries; financial/source bytes are never
// imported into execution. A re-pinned metadata forgery is NOT authority.
#[cfg(test)]
#[derive(Clone, Copy)]
pub(crate) enum DurableFreshCorruptionV1 {
    Missing,
    Reorder,
    Substitute,
}
#[cfg(test)]
pub(crate) fn test_rewrite_durable_fresh_outer(
    expected: &DurableFreshColdPinV1,
    kind: DurableFreshCorruptionV1,
) -> Result<DurableFreshColdPinV1> {
    let root = directory(&expected.outer.root)?;
    let namespace = expected.outer.prefix.namespace.clone();
    let (mut journal, _) = Journal::open_existing_verified_prefix(
        &root,
        namespace.clone(),
        Recovery::RejectIncompleteTail,
        &expected.outer.prefix,
    )?;
    let context = journal
        .get(CONTEXT)?
        .ok_or(ActiveServiceWalErrorV1::InvalidRecord)?
        .to_vec();
    let filesystem = match history_text(&parse(&context)?.get("filesystem"))? {
        "FileSyncOnly" => SourceDurableFilesystem::FileSyncOnly,
        "FileAndDirectorySync" => SourceDurableFilesystem::FileAndDirectorySync,
        _ => return Err(ActiveServiceWalErrorV1::WrongContext),
    };
    let frames = journal
        .read_verified_current_commits(
            &expected.outer.prefix,
            65536,
            MAX_VERIFIED_ACK_HISTORY_BYTES as usize,
        )?
        .commits()
        .to_vec();
    let mut records = vec![];
    for commit in &frames[1..] {
        let [Mutation::Put { key, value }] = commit.mutations.as_slice() else {
            return Err(ActiveServiceWalErrorV1::InvalidRecord);
        };
        if key != RECORD {
            return Err(ActiveServiceWalErrorV1::WrongContext);
        }
        records.push(parse(value)?);
    }
    let store = ActiveServiceWalV1 {
        root: root.clone(),
        namespace: namespace.clone(),
        journal: Some(journal),
        context: context.clone(),
        filesystem,
        owner: NEXT.fetch_add(1, Ordering::Relaxed),
        ambiguous: false,
        fault: None,
        commit_probe: None,
    };
    let mut selected = None;
    for (group, rows) in records.chunks_exact(4).enumerate() {
        let body = parse(&store.read_blob(&history_blob(&rows[3].get("entry"))?)?)?;
        if body.get("outcome").get("kind") == s("ActualFeedPublished") {
            selected = Some(group);
            break;
        }
    }
    let selected = selected.ok_or(ActiveServiceWalErrorV1::WrongContext)?;
    if matches!(kind, DurableFreshCorruptionV1::Missing) {
        let pin = history_blob(&records[selected * 4].get("action"))?;
        let target = root
            .join(BLOBS)
            .join(format!("{}.asw1", hex(&pin.sha256)))
            .canonicalize()?;
        if !target.starts_with(&root) || target.extension() != Some(std::ffi::OsStr::new("asw1")) {
            return Err(ActiveServiceWalErrorV1::WrongContext);
        }
        drop(store);
        fs::remove_file(target)?;
        return Ok(expected.clone());
    }
    if matches!(kind, DurableFreshCorruptionV1::Reorder) {
        if selected + 2 >= records.len() / 4 {
            return Err(ActiveServiceWalErrorV1::WrongContext);
        }
        for at in 0..4 {
            records.swap(selected * 4 + at, (selected + 1) * 4 + at);
        }
    }
    if matches!(kind, DurableFreshCorruptionV1::Substitute) {
        let mut previous_head = sha256(&bytes(&parse(&context)?.get("origin"))?);
        let mut feed_head = sha256(DURABLE_FRESH_COORDINATION_PROFILE.as_bytes());
        let mut count = 0u64;
        for (group, rows) in records.chunks_exact_mut(4).enumerate() {
            let old = parse(&store.read_blob(&history_blob(&rows[3].get("entry"))?)?)?;
            let outcome = old.get("outcome");
            let raw = store.read_blob(&history_blob(&old.get("action"))?)?;
            let mut action=crate::source_main_durable_fresh_action::NativeMainDurableFreshActionV1::from_canonical_action_bytes(&raw).map_err(|_|ActiveServiceWalErrorV1::InvalidRecord)?;
            if group == selected {
                let crate::source_main_durable_fresh_action::NativeMainDurableFreshActionV1::Feed(
                    input,
                ) = action
                else {
                    return Err(ActiveServiceWalErrorV1::WrongContext);
                };
                let mut io = input.io().clone();
                io.insert("height", V::number(999.))
                    .map_err(|_| ActiveServiceWalErrorV1::InvalidRecord)?;
                action =
                    crate::source_main_durable_fresh_action::NativeMainDurableFreshActionV1::Feed(
                        crate::source_main_fresh_input::NativeMainFreshRpcInputV1::new(
                            io,
                            Some(V::Array(vec![obj(&[
                                ("id", s("negative-only-substituted-document")),
                                ("document", V::Null),
                            ])])),
                        )
                        .map_err(|_| ActiveServiceWalErrorV1::InvalidRecord)?,
                    );
            }
            let action = store.write_blob(
                &action
                    .canonical_action_bytes()
                    .map_err(|_| ActiveServiceWalErrorV1::InvalidRecord)?,
            )?;
            let mut before = parse(&store.read_blob(&history_blob(&old.get("before"))?)?)?;
            before
                .insert("feeds", s(count.to_string()))
                .map_err(|_| ActiveServiceWalErrorV1::InvalidRecord)?;
            before
                .insert("feedHead", s(hex(&feed_head)))
                .map_err(|_| ActiveServiceWalErrorV1::InvalidRecord)?;
            let before = store.write_blob(&bytes(&before)?)?;
            if outcome.get("kind") == s("ActualFeedPublished") {
                count += 1;
                let raw = store.read_blob(&action)?;
                let crate::source_main_durable_fresh_action::NativeMainDurableFreshActionV1::Feed(input)=crate::source_main_durable_fresh_action::NativeMainDurableFreshActionV1::from_canonical_action_bytes(&raw).map_err(|_|ActiveServiceWalErrorV1::InvalidRecord)? else{return Err(ActiveServiceWalErrorV1::WrongContext)};
                let raw = bytes(&obj(&[
                    ("ordinal", s(count.to_string())),
                    ("raw", input.value()),
                    ("actualBoundary", outcome.get("actualBoundary")),
                ]))?;
                let mut next = feed_head.to_vec();
                next.extend_from_slice(&sha256(&raw));
                feed_head = sha256(&next);
            }
            let mut after = parse(&store.read_blob(&history_blob(&old.get("after"))?)?)?;
            after
                .insert("feeds", s(count.to_string()))
                .map_err(|_| ActiveServiceWalErrorV1::InvalidRecord)?;
            after
                .insert("feedHead", s(hex(&feed_head)))
                .map_err(|_| ActiveServiceWalErrorV1::InvalidRecord)?;
            let after = store.write_blob(&bytes(&after)?)?;
            let mut entry = old.clone();
            for (name, value) in [
                ("previousHead", s(hex(&previous_head))),
                ("action", action.value()),
                ("before", before.value()),
                ("after", after.value()),
            ] {
                entry
                    .insert(name, value)
                    .map_err(|_| ActiveServiceWalErrorV1::InvalidRecord)?;
            }
            let raw = bytes(&entry)?;
            let mut next = previous_head.to_vec();
            next.extend_from_slice(&sha256(&raw));
            let head = sha256(&next);
            let entry = store.write_blob(&raw)?;
            for row in rows {
                for (name, value) in [
                    ("previousHead", s(hex(&previous_head))),
                    ("action", action.value()),
                    ("before", before.value()),
                ] {
                    row.insert(name, value)
                        .map_err(|_| ActiveServiceWalErrorV1::InvalidRecord)?;
                }
                if row.get("after") != V::Null {
                    row.insert("after", after.value())
                        .map_err(|_| ActiveServiceWalErrorV1::InvalidRecord)?;
                }
                if row.get("entry") != V::Null {
                    row.insert("entry", entry.value())
                        .map_err(|_| ActiveServiceWalErrorV1::InvalidRecord)?;
                }
                if row.get("head") != V::Null {
                    row.insert("head", s(hex(&head)))
                        .map_err(|_| ActiveServiceWalErrorV1::InvalidRecord)?;
                }
            }
            previous_head = head;
        }
    }
    drop(store);
    let target = expected.outer.prefix.canonical_file.canonicalize()?;
    if !target.starts_with(&root) || target != namespace.path(&root) {
        return Err(ActiveServiceWalErrorV1::WrongContext);
    }
    fs::remove_file(&target)?;
    let (mut journal, _) = Journal::open(&root, namespace, Recovery::RejectIncompleteTail)?;
    journal.commit(frames[0].mutations.clone())?;
    for record in records {
        journal.commit(vec![Mutation::Put {
            key: RECORD.into(),
            value: bytes(&record)?,
        }])?;
    }
    let prefix = journal.verified_prefix_pin()?;
    let trace = journal
        .read_verified_current_commits(&prefix, 65536, MAX_VERIFIED_ACK_HISTORY_BYTES as usize)?
        .trace()
        .clone();
    let mut result = expected.clone();
    result.outer.prefix = prefix;
    result.trace = trace;
    drop(journal);
    Ok(result)
}
