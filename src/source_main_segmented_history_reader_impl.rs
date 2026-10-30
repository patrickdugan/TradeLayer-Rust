struct SegmentedHistoryNodeV1 {
    pin: ActiveServiceWalBlobV1,
    trace: crate::storage::JournalCommitTracePinV1,
    first: u64,
    last: u64,
}
pub struct VerifiedSegmentedWindowV1 {
    entries: Vec<VerifiedActiveServiceAckEntryV1>,
    checkpoint: Option<(u64, ActiveServiceWalBlobV1, V)>,
}
impl VerifiedSegmentedWindowV1 {
    pub fn entries(&self) -> &[VerifiedActiveServiceAckEntryV1] {
        &self.entries
    }
    pub fn checkpoint(&self) -> Option<(u64, &ActiveServiceWalBlobV1, &V)> {
        self.checkpoint.as_ref().map(|(n, p, v)| (*n, p, v))
    }
}
/// Actual-file read-only profile. Source comparison blobs are never a state or
/// IO-result reader. Ordinary native ambiguity is refused without typed facts.
pub struct VerifiedSegmentedActiveHistoryV1 {
    store: ActiveServiceWalV1,
    expected: SegmentedActiveHistoryPinV1,
    origin: V,
    archive: Vec<u8>,
    origin_history: Vec<u8>,
    origin_observation: ActiveServiceWalBlobV1,
    nodes: Vec<SegmentedHistoryNodeV1>,
}
fn segmented_trace(v: &V) -> Result<crate::storage::JournalCommitTracePinV1> {
    history_keys(v, &["profile", "bytes", "commits", "chain"], &[])?;
    if v.get("profile") != s(crate::storage::VERIFIED_COMMIT_TRACE_PROFILE) {
        return Err(ActiveServiceWalErrorV1::WrongContext);
    }
    Ok(crate::storage::JournalCommitTracePinV1 {
        profile: crate::storage::VERIFIED_COMMIT_TRACE_PROFILE.into(),
        bytes: segment_count(&v.get("bytes"))?,
        commits: segment_count(&v.get("commits"))?,
        chain_sha256d: segment_hash(&v.get("chain"))?,
    })
}
fn segmented_commit_record(commit: &crate::storage::JournalObservedCommitV1) -> Result<V> {
    let [Mutation::Put { key, value }] = commit.mutations.as_slice() else {
        return Err(ActiveServiceWalErrorV1::InvalidRecord);
    };
    if key != RECORD {
        return Err(ActiveServiceWalErrorV1::InvalidRecord);
    }
    parse(value)
}
impl VerifiedSegmentedActiveHistoryV1 {
    pub fn open(
        root: &Path,
        namespace: Namespace,
        expected: &SegmentedActiveHistoryPinV1,
    ) -> Result<Self> {
        let root = directory(root)?;
        if expected.wal.root != root
            || expected.wal.profile != PROFILE
            || expected.wal.prefix.namespace != namespace
        {
            return Err(ActiveServiceWalErrorV1::WrongPin);
        }
        let (journal, _) = Journal::open_existing_verified_prefix(
            &root,
            namespace.clone(),
            Recovery::RejectIncompleteTail,
            &expected.wal.prefix,
        )?;
        if journal.entries()?.count() != 2 {
            return Err(ActiveServiceWalErrorV1::InvalidRecord);
        }
        let context = journal
            .get(CONTEXT)?
            .ok_or(ActiveServiceWalErrorV1::InvalidRecord)?
            .to_vec();
        let decoded = parse(&context)?;
        history_keys(
            &decoded,
            &["profile", "origin", "filesystem", "creation"],
            &[],
        )?;
        if decoded.get("profile") != s(PROFILE) {
            return Err(ActiveServiceWalErrorV1::WrongContext);
        }
        let filesystem = match history_text(&decoded.get("filesystem"))? {
            "FileSyncOnly" => SourceDurableFilesystem::FileSyncOnly,
            "FileAndDirectorySync" => SourceDurableFilesystem::FileAndDirectorySync,
            _ => return Err(ActiveServiceWalErrorV1::WrongContext),
        };
        let origin = decoded.get("origin");
        history_keys(
            &origin,
            &[
                "profile",
                "archive",
                "observation",
                "requiredVerifiedHistory",
                "underlyingSourceHistoryProfile",
                "acknowledgedWindowLimit",
            ],
            &[],
        )?;
        if origin.get("profile") != s(SEGMENTED_ACTIVE_HISTORY_PROFILE)
            || origin.get("underlyingSourceHistoryProfile") != s(ENHANCED_HISTORY_PROFILE)
            || origin.get("acknowledgedWindowLimit") != s("256")
        {
            return Err(ActiveServiceWalErrorV1::WrongContext);
        }
        let store = ActiveServiceWalV1 {
            root,
            namespace,
            journal: Some(journal),
            context,
            filesystem,
            owner: NEXT.fetch_add(1, Ordering::Relaxed),
            ambiguous: false,
            #[cfg(test)]
            fault: None,
            #[cfg(test)]
            commit_probe: None,
        };
        let mut budget = 0;
        let archive = history_read_blob(&store, &origin.get("archive"), &mut budget)?;
        let origin_history =
            history_read_blob(&store, &origin.get("requiredVerifiedHistory"), &mut budget)?;
        let origin_observation = segment_blob(&origin.get("observation"))?;
        store.read_blob(&origin_observation)?;
        let record = store.record()?;
        if segment_count(&record.get("sequence"))? != expected.actions
            || segment_hash(&record.get("head"))? != expected.head
            || record.get("sourceHead") != expected.source_head
        {
            return Err(ActiveServiceWalErrorV1::WrongPin);
        }
        if ![s("Ack"), s("SegmentAck")].contains(&record.get("stage")) {
            return Err(ActiveServiceWalErrorV1::WrongContext);
        }
        let mut nodes = Vec::new();
        let mut next = expected.latest_segment.clone();
        let mut upper = expected
            .actions
            .checked_add(1)
            .ok_or(ActiveServiceWalErrorV1::Capacity)?;
        let mut ordinal = None;
        while let Some(pin) = next {
            let v = parse(&store.read_blob(&pin)?)?;
            history_keys(
                &v,
                &[
                    "profile",
                    "ordinal",
                    "firstAction",
                    "lastAction",
                    "previousSegment",
                    "sourceHead",
                    "walHead",
                    "entries",
                    "actualAck",
                    "walSequence",
                    "trace",
                ],
                &[],
            )?;
            if v.get("profile") != s(ACTIVE_HISTORY_SEGMENT_PROFILE) {
                return Err(ActiveServiceWalErrorV1::WrongContext);
            }
            let first = segment_count(&v.get("firstAction"))?;
            let last = segment_count(&v.get("lastAction"))?;
            let at = segment_count(&v.get("ordinal"))?;
            if first == 0
                || last < first
                || last - first >= 256
                || last >= upper
                || ordinal.is_some_and(|n| at.checked_add(1) != Some(n))
            {
                return Err(ActiveServiceWalErrorV1::WrongPin);
            }
            if ordinal.is_some() && last + 1 != upper {
                return Err(ActiveServiceWalErrorV1::WrongPin);
            }
            let trace = segmented_trace(&v.get("trace"))?;
            if trace.commits != segment_count(&v.get("walSequence"))? {
                return Err(ActiveServiceWalErrorV1::WrongPin);
            }
            next = match v.get("previousSegment") {
                V::Null => None,
                ref value => Some(segment_blob(value)?),
            };
            nodes
                .try_reserve(1)
                .map_err(|_| ActiveServiceWalErrorV1::Capacity)?;
            nodes.push(SegmentedHistoryNodeV1 {
                pin,
                trace,
                first,
                last,
            });
            upper = first;
            ordinal = Some(at);
        }
        if !nodes.is_empty() && (upper != 1 || ordinal != Some(1)) {
            return Err(ActiveServiceWalErrorV1::WrongPin);
        }
        nodes.reverse();
        let mut owner = Self {
            store,
            expected: expected.clone(),
            origin,
            archive,
            origin_history,
            origin_observation,
            nodes,
        };
        // Every physical action barrier is independently checked before any
        // financial/source execution; only one bounded window is live at once.
        for at in 0..owner.window_count() {
            owner.read_window(at)?;
        }
        Ok(owner)
    }
    pub fn origin_archive(&self) -> &[u8] {
        &self.archive
    }
    pub fn origin_history(&self) -> &[u8] {
        &self.origin_history
    }
    pub fn origin_observation(&self) -> &ActiveServiceWalBlobV1 {
        &self.origin_observation
    }
    pub fn read_reference(&self, p: &ActiveServiceWalBlobV1) -> Result<Vec<u8>> {
        self.store.read_blob(p)
    }
    pub fn head(&self) -> [u8; 32] {
        self.expected.head
    }
    pub fn actions(&self) -> u64 {
        self.expected.actions
    }
    pub fn window_count(&self) -> usize {
        self.nodes.len() + 1
    }
    pub fn read_window(&mut self, at: usize) -> Result<VerifiedSegmentedWindowV1> {
        if at > self.nodes.len() {
            return Err(ActiveServiceWalErrorV1::WrongPin);
        }
        let previous = if at == 0 {
            None
        } else {
            Some(parse(&self.store.read_blob(&self.nodes[at - 1].pin)?)?)
        };
        let descriptor = self
            .nodes
            .get(at)
            .map(|n| self.store.read_blob(&n.pin).and_then(|b| parse(&b)))
            .transpose()?;
        let first = if at == 0 {
            1
        } else {
            self.nodes[at - 1].last + 1
        };
        if self.nodes.get(at).is_some_and(|n| n.first != first) {
            return Err(ActiveServiceWalErrorV1::WrongPin);
        }
        let last = descriptor.as_ref().map_or(self.expected.actions, |v| {
            segment_count(&v.get("lastAction")).expect("opened strict descriptor")
        });
        if last + 1 < first || last + 1 - first > 256 {
            return Err(ActiveServiceWalErrorV1::Capacity);
        }
        let prior_entry = previous.as_ref().map_or(Ok(V::Null), |v| {
            let V::Array(entries) = v.get("entries") else {
                return Err(ActiveServiceWalErrorV1::InvalidRecord);
            };
            entries
                .last()
                .cloned()
                .ok_or(ActiveServiceWalErrorV1::InvalidRecord)
        })?;
        let prior_head = previous.as_ref().map_or_else(
            || Ok(sha256(&bytes(&self.origin)?)),
            |v| segment_hash(&v.get("walHead")),
        )?;
        let roots = if let Some(v) = &descriptor {
            let V::Array(values) = v.get("entries") else {
                return Err(ActiveServiceWalErrorV1::InvalidRecord);
            };
            values
                .iter()
                .map(segment_blob)
                .collect::<Result<Vec<_>>>()?
        } else {
            let record = self.store.record()?;
            let mut roots = Vec::new();
            let mut next = record.get(if record.get("stage") == s("SegmentAck") {
                "lastEntry"
            } else {
                "entry"
            });
            for _ in first..=last {
                let p = segment_blob(&next)?;
                let v = parse(&self.store.read_blob(&p)?)?;
                next = v.get("previousEntry");
                roots.push(p);
            }
            if next != prior_entry {
                return Err(ActiveServiceWalErrorV1::WrongPin);
            }
            roots.reverse();
            roots
        };
        if roots.len() as u64 != last + 1 - first {
            return Err(ActiveServiceWalErrorV1::WrongPin);
        }
        let before = if at == 0 {
            self.store.healthy()?.verified_commit_trace_origin()?
        } else {
            self.nodes[at - 1].trace.clone()
        };
        let after = self
            .nodes
            .get(at)
            .map_or(&self.expected.trace, |n| &n.trace)
            .clone();
        let range = self
            .store
            .journal
            .as_mut()
            .ok_or(ActiveServiceWalErrorV1::Ambiguous)?
            .read_verified_commit_range(
                &self.expected.wal.prefix,
                &self.expected.trace,
                &before,
                &after,
                (8192, 64 * 1024 * 1024),
            )?;
        let mut records = range.commits().iter();
        if at == 0 {
            let creation = records
                .next()
                .ok_or(ActiveServiceWalErrorV1::InvalidRecord)?;
            if creation.sequence != 1
                || creation.mutations
                    != vec![
                        Mutation::Put {
                            key: CONTEXT.into(),
                            value: self.store.context.clone(),
                        },
                        Mutation::Put {
                            key: RECORD.into(),
                            value: bytes(&obj(&[
                                ("profile", s(PROFILE)),
                                ("phase", s("Created")),
                            ]))?,
                        },
                    ]
            {
                return Err(ActiveServiceWalErrorV1::WrongPin);
            }
        } else {
            let v = segmented_commit_record(
                records
                    .next()
                    .ok_or(ActiveServiceWalErrorV1::InvalidRecord)?,
            )?;
            let prior = previous.as_ref().expect("previous descriptor");
            if v.get("profile") != s(SEGMENTED_ACTIVE_HISTORY_PROFILE)
                || v.get("stage") != s("SegmentAck")
                || v.get("historySegment") != self.nodes[at - 1].pin.value()
                || v.get("sequence") != prior.get("lastAction")
                || v.get("head") != prior.get("walHead")
                || v.get("sourceHead") != prior.get("sourceHead")
                || v.get("lastEntry") != prior_entry
                || v.get("actualAck") != prior.get("actualAck")
            {
                return Err(ActiveServiceWalErrorV1::WrongPin);
            }
        }
        let mut last_ack = None;
        let mut head = prior_head;
        let mut source_head = previous.as_ref().map_or(V::Null, |v| v.get("sourceHead"));
        let mut previous_entry = prior_entry;
        let mut entries = Vec::new();
        let mut budget = 0;
        for (offset, pin) in roots.into_iter().enumerate() {
            let sequence = first + offset as u64;
            let v = parse(&self.store.read_blob(&pin)?)?;
            history_keys(
                &v,
                &[
                    "profile",
                    "sequence",
                    "previousEntry",
                    "previousHead",
                    "action",
                    "before",
                    "nativeCandidates",
                    "nativeMutations",
                    "accepted",
                    "verifiedHistoryBefore",
                    "verifiedHistoryPost",
                    "sourceHead",
                    "historySegment",
                ],
                &[],
            )?;
            if v.get("profile") != s(SEGMENTED_ACTIVE_HISTORY_PROFILE)
                || segment_count(&v.get("sequence"))? != sequence
                || v.get("previousEntry") != previous_entry
                || segment_hash(&v.get("previousHead"))? != head
            {
                return Err(ActiveServiceWalErrorV1::WrongPin);
            }
            let mut chain = b"Native13/ActiveWalPhysicalAck/v1".to_vec();
            chain.extend_from_slice(&head);
            chain.extend_from_slice(&sequence.to_be_bytes());
            chain.extend_from_slice(&pin.sha256);
            let next_head = sha256(&chain);
            let prior_segment = if at == 0 {
                V::Null
            } else {
                self.nodes[at - 1].pin.value()
            };
            if v.get("historySegment") != prior_segment {
                return Err(ActiveServiceWalErrorV1::WrongPin);
            }
            segment_source_head(&v.get("sourceHead"))?;
            let action = history_read_blob(&self.store, &v.get("action"), &mut budget)?;
            let decoded=crate::source_main_native_session::NativeMainActiveServiceActionV1::from_canonical_action_bytes(&action).map_err(|_|ActiveServiceWalErrorV1::InvalidRecord)?;
            use crate::source_main_native_session::NativeMainActiveServiceActionV1 as Action;
            if (sequence == 1) != matches!(decoded, Action::Start { .. }) {
                return Err(ActiveServiceWalErrorV1::WrongContext);
            }
            let native_candidates =
                history_optional_blob(&self.store, &v.get("nativeCandidates"), &mut budget)?;
            let native_mutations =
                history_optional_blob(&self.store, &v.get("nativeMutations"), &mut budget)?;
            let history_before =
                history_read_blob(&self.store, &v.get("verifiedHistoryBefore"), &mut budget)?;
            let history_post =
                history_read_blob(&self.store, &v.get("verifiedHistoryPost"), &mut budget)?;
            if matches!(decoded, Action::AppendPending) {
                if history_before == history_post
                    || native_candidates.is_none()
                    || native_mutations.is_none()
                {
                    return Err(ActiveServiceWalErrorV1::WrongContext);
                }
            } else if history_before != history_post
                || native_candidates.is_some()
                || native_mutations.is_some()
            {
                return Err(ActiveServiceWalErrorV1::WrongContext);
            }
            for stage in ["Intent", "Accepted", "Ack"] {
                let record = segmented_commit_record(
                    records
                        .next()
                        .ok_or(ActiveServiceWalErrorV1::InvalidRecord)?,
                )?;
                history_keys(
                    &record,
                    &[
                        "profile",
                        "stage",
                        "sequence",
                        "previousHead",
                        "head",
                        "action",
                        "before",
                        "origin",
                        "nativeCandidates",
                        "nativeMutations",
                        "accepted",
                        "entry",
                        "verifiedHistoryBefore",
                        "sourceHead",
                        "historySegment",
                    ],
                    &["verifiedHistoryPost"],
                )?;
                if record.get("profile") != s(SEGMENTED_ACTIVE_HISTORY_PROFILE)
                    || record.get("stage") != s(stage)
                    || record.get("sequence") != v.get("sequence")
                    || record.get("previousHead") != v.get("previousHead")
                    || record.get("origin") != self.origin.get("archive")
                {
                    return Err(ActiveServiceWalErrorV1::WrongPin);
                }
                for name in [
                    "action",
                    "before",
                    "nativeCandidates",
                    "nativeMutations",
                    "verifiedHistoryBefore",
                    "historySegment",
                ] {
                    if record.get(name) != v.get(name) {
                        return Err(ActiveServiceWalErrorV1::WrongPin);
                    }
                }
                if stage != "Intent" {
                    for name in ["accepted", "verifiedHistoryPost", "sourceHead"] {
                        if record.get(name) != v.get(name) {
                            return Err(ActiveServiceWalErrorV1::WrongPin);
                        }
                    }
                    if record.get("entry") != pin.value()
                        || segment_hash(&record.get("head"))? != next_head
                    {
                        return Err(ActiveServiceWalErrorV1::WrongPin);
                    }
                } else if record.get("entry") != V::Null
                    || record.get("accepted") != V::Null
                    || record.get("head") != V::Null
                    || record.get("verifiedHistoryPost") != V::Undefined
                    || record.get("sourceHead") != source_head
                {
                    return Err(ActiveServiceWalErrorV1::WrongPin);
                }
                if stage == "Ack" {
                    last_ack = Some(record);
                }
            }
            let actual_accepted = history_read_blob(&self.store, &v.get("accepted"), &mut budget)?;
            let (_, diagnostic) = history_accepted_parts(&actual_accepted)?;
            if diagnostic.get("nativeRecovery") != V::Undefined {
                return Err(ActiveServiceWalErrorV1::WrongContext);
            }
            entries.push(VerifiedActiveServiceAckEntryV1 {
                sequence,
                entry: pin.clone(),
                previous_head: head,
                head: next_head,
                raw_action: action,
                actual_before: history_read_blob(&self.store, &v.get("before"), &mut budget)?,
                actual_accepted,
                verified_history_before: history_before,
                verified_history_post: history_post,
                native_candidates,
                native_mutations,
                native_recovery: None,
            });
            head = next_head;
            source_head = v.get("sourceHead");
            previous_entry = pin.value();
        }
        if records.next().is_some() {
            return Err(ActiveServiceWalErrorV1::WrongContext);
        }
        let checkpoint = if let Some(v) = descriptor {
            if segment_hash(&v.get("walHead"))? != head
                || segment_count(&v.get("lastAction"))? != last
                || v.get("sourceHead") != source_head
            {
                return Err(ActiveServiceWalErrorV1::WrongPin);
            }
            let ack = parse(&self.store.read_blob(&segment_blob(&v.get("actualAck"))?)?)?;
            if Some(&ack) != last_ack.as_ref() {
                return Err(ActiveServiceWalErrorV1::WrongPin);
            }
            Some((last, self.nodes[at].pin.clone(), v.get("sourceHead")))
        } else {
            if head != self.expected.head
                || last != self.expected.actions
                || source_head != self.expected.source_head
            {
                return Err(ActiveServiceWalErrorV1::WrongPin);
            }
            None
        };
        Ok(VerifiedSegmentedWindowV1 {
            entries,
            checkpoint,
        })
    }
}
