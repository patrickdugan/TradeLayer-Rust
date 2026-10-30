// Read-only actual-file ACK history. No financial owner, source outcome input,
// state importer, source execution, plan recovery or Journal write is exposed.
pub const VERIFIED_ACK_HISTORY_PROFILE: &str = "TL-MAIN-ACTIVE-VERIFIED-ACK-HISTORY-13-1";
pub const ENHANCED_HISTORY_PROFILE: &str = "TL-MAIN-ACTIVE-SERVICE-HISTORY-WAL-13-1";
const MAX_ACK_HISTORY: u64 = 4096;
pub const MAX_VERIFIED_ACK_HISTORY_BYTES: u64 = 256 * 1024 * 1024;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedActiveServiceAckEntryV1 {
    pub sequence: u64,
    pub entry: ActiveServiceWalBlobV1,
    pub previous_head: [u8; 32],
    pub head: [u8; 32],
    raw_action: Vec<u8>,
    actual_before: Vec<u8>,
    actual_accepted: Vec<u8>,
    verified_history_before: Vec<u8>,
    verified_history_post: Vec<u8>,
    native_candidates: Option<Vec<u8>>,
    native_mutations: Option<Vec<u8>>,
    native_recovery: Option<Vec<u8>>,
}
impl VerifiedActiveServiceAckEntryV1 {
    pub fn raw_action(&self) -> &[u8] {
        &self.raw_action
    }
    pub fn actual_before(&self) -> &[u8] {
        &self.actual_before
    }
    pub fn actual_accepted(&self) -> &[u8] {
        &self.actual_accepted
    }
    pub fn verified_history_before(&self) -> &[u8] {
        &self.verified_history_before
    }
    pub fn verified_history_post(&self) -> &[u8] {
        &self.verified_history_post
    }
    pub fn native_candidates(&self) -> Option<&[u8]> {
        self.native_candidates.as_deref()
    }
    pub fn native_mutations(&self) -> Option<&[u8]> {
        self.native_mutations.as_deref()
    }
    pub fn native_recovery(&self) -> Option<&[u8]> {
        self.native_recovery.as_deref()
    }
}
pub struct VerifiedActiveServiceAckHistoryV1 {
    // Owns the actual verified current journal/lock. This inner writer is never
    // released or loaned; all externally available operations are read-only.
    store: ActiveServiceWalV1,
    origin: V,
    origin_archive: Vec<u8>,
    origin_history: Vec<u8>,
    origin_observation: ActiveServiceWalBlobV1,
    head: [u8; 32],
    entries: Vec<VerifiedActiveServiceAckEntryV1>,
}
impl std::fmt::Debug for VerifiedActiveServiceAckHistoryV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VerifiedActiveServiceAckHistoryV1")
            .field("entries", &self.entries.len())
            .field("head", &self.head)
            .finish_non_exhaustive()
    }
}
fn history_text(value: &V) -> Result<&str> {
    let V::String(value) = value else {
        return Err(ActiveServiceWalErrorV1::InvalidRecord);
    };
    Ok(value)
}
fn history_count(value: &V) -> Result<u64> {
    let text = history_text(value)?;
    let number = text
        .parse::<u64>()
        .map_err(|_| ActiveServiceWalErrorV1::InvalidRecord)?;
    if number.to_string() != text {
        return Err(ActiveServiceWalErrorV1::InvalidRecord);
    };
    Ok(number)
}
fn history_hash(value: &V) -> Result<[u8; 32]> {
    let raw = history_text(value)?;
    if raw.len() != 64
        || raw
            .bytes()
            .any(|b| !b.is_ascii_digit() && !(b'a'..=b'f').contains(&b))
    {
        return Err(ActiveServiceWalErrorV1::InvalidRecord);
    }
    let mut out = [0; 32];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&raw[i * 2..i * 2 + 2], 16)
            .map_err(|_| ActiveServiceWalErrorV1::InvalidRecord)?;
    }
    Ok(out)
}
fn history_blob(value: &V) -> Result<ActiveServiceWalBlobV1> {
    let pin = ActiveServiceWalBlobV1 {
        bytes: history_count(&value.get("bytes"))?,
        sha256: history_hash(&value.get("sha256"))?,
    };
    if pin.bytes > MAX_BLOB as u64 || pin.value() != *value {
        return Err(ActiveServiceWalErrorV1::InvalidRecord);
    };
    Ok(pin)
}
fn history_keys(value: &V, required: &[&str], optional: &[&str]) -> Result<()> {
    let V::Object(fields) = value else {
        return Err(ActiveServiceWalErrorV1::InvalidRecord);
    };
    let mut seen = std::collections::BTreeSet::new();
    if fields.iter().any(|(key, _)| {
        !seen.insert(key.as_str())
            || (!required.contains(&key.as_str()) && !optional.contains(&key.as_str()))
    }) || required.iter().any(|key| !seen.contains(key))
    {
        return Err(ActiveServiceWalErrorV1::InvalidRecord);
    }
    Ok(())
}
fn history_accepted_parts(raw: &[u8]) -> Result<(&[u8], V)> {
    history_accepted_parts_profile(raw, false)
}
fn history_accepted_parts_profile(raw: &[u8], inline: bool) -> Result<(&[u8], V)> {
    if !raw.starts_with(b"TLMAP\x01") || raw.len() < 22 {
        return Err(ActiveServiceWalErrorV1::InvalidRecord);
    }
    let count = u64::from_be_bytes(raw[6..14].try_into().expect("fixed length checked"));
    let count = usize::try_from(count).map_err(|_| ActiveServiceWalErrorV1::Capacity)?;
    let end = 14usize
        .checked_add(count)
        .ok_or(ActiveServiceWalErrorV1::Capacity)?;
    let length_end = end
        .checked_add(8)
        .ok_or(ActiveServiceWalErrorV1::Capacity)?;
    let tail = raw
        .get(end..length_end)
        .ok_or(ActiveServiceWalErrorV1::InvalidRecord)?;
    let tail = u64::from_be_bytes(tail.try_into().expect("fixed slice"));
    let tail = usize::try_from(tail).map_err(|_| ActiveServiceWalErrorV1::Capacity)?;
    if length_end.checked_add(tail) != Some(raw.len()) {
        return Err(ActiveServiceWalErrorV1::InvalidRecord);
    }
    let diagnostic = parse(&raw[length_end..])?;
    history_keys(
        &diagnostic,
        &["apiRejection"],
        if inline {
            &["nativeRecovery", "generationEscrow"]
        } else {
            &["nativeRecovery"]
        },
    )?;
    if !matches!(diagnostic.get("apiRejection"), V::Null | V::String(_)) {
        return Err(ActiveServiceWalErrorV1::InvalidRecord);
    }
    Ok((&raw[14..end], diagnostic))
}
fn history_read_blob(store: &ActiveServiceWalV1, value: &V, budget: &mut u64) -> Result<Vec<u8>> {
    let pin = history_blob(value)?;
    let next = budget
        .checked_add(pin.bytes)
        .ok_or(ActiveServiceWalErrorV1::Capacity)?;
    if next > MAX_VERIFIED_ACK_HISTORY_BYTES {
        return Err(ActiveServiceWalErrorV1::Capacity);
    }
    let raw = store.read_blob(&pin)?;
    *budget = next;
    Ok(raw)
}
fn history_optional_blob(
    store: &ActiveServiceWalV1,
    value: &V,
    budget: &mut u64,
) -> Result<Option<Vec<u8>>> {
    if matches!(value, V::Null | V::Undefined) {
        Ok(None)
    } else {
        history_read_blob(store, value, budget).map(Some)
    }
}
impl VerifiedActiveServiceAckHistoryV1 {
    /// Open only an independently pinned healthy synced ACK. No creation,
    /// repair, pending intent acceptance or in-memory controller is required.
    pub fn open(
        root: &Path,
        namespace: Namespace,
        expected: &ActiveServiceWalPinV1,
    ) -> Result<Self> {
        Self::open_profile(root, namespace, expected, false)
    }
    fn open_profile(
        root: &Path,
        namespace: Namespace,
        expected: &ActiveServiceWalPinV1,
        inline: bool,
    ) -> Result<Self> {
        Self::open_profile_exact(
            root,
            namespace,
            expected,
            if inline {
                ExactAckHistoryProfileV1::Mode4
            } else {
                ExactAckHistoryProfileV1::History
            },
            None,
        )
    }
    fn open_profile_exact(
        root: &Path,
        namespace: Namespace,
        expected: &ActiveServiceWalPinV1,
        exact: ExactAckHistoryProfileV1,
        metadata_namespace: Option<&Namespace>,
    ) -> Result<Self> {
        let inline = exact.inline();
        let facts = exact.facts();
        let history_profile = exact.profile();
        if facts {
            mode4_namespace(
                &namespace,
                metadata_namespace.ok_or(ActiveServiceWalErrorV1::WrongContext)?,
            )?;
        }
        let mut fact_optional = Vec::new();
        if facts {
            fact_optional.extend(["nativeRecovery", "generationEscrow"]);
            fact_optional.extend_from_slice(ACTUAL_SOURCE_FACT_ENTRY_FIELDS);
        }
        let root = directory(root)?;
        if expected.profile != PROFILE
            || expected.root != root
            || expected.prefix.namespace != namespace
        {
            return Err(ActiveServiceWalErrorV1::WrongPin);
        }
        let (journal, _) = Journal::open_existing_verified_prefix(
            &root,
            namespace.clone(),
            Recovery::RejectIncompleteTail,
            &expected.prefix,
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
        if decoded.get("profile") != s(PROFILE)
            || history_text(&decoded.get("creation"))?.is_empty()
        {
            return Err(ActiveServiceWalErrorV1::WrongContext);
        }
        let filesystem = match history_text(&decoded.get("filesystem"))? {
            "FileSyncOnly" => SourceDurableFilesystem::FileSyncOnly,
            "FileAndDirectorySync" => SourceDurableFilesystem::FileAndDirectorySync,
            _ => return Err(ActiveServiceWalErrorV1::WrongContext),
        };
        let origin = decoded.get("origin");
        let mut origin_keys = vec![
            "profile",
            "archive",
            "observation",
            "requiredVerifiedHistory",
        ];
        if inline {
            origin_keys.extend(["generationEscrow", "requiredInlineServiceProfile"]);
        }
        if facts {
            origin_keys.extend(["requiredNativeFactProfile", "actualNativeFactScope"]);
        }
        history_keys(&origin, &origin_keys, &[])?;
        if facts {
            actual_source_fact_origin(&origin)?;
        } else if inline {
            mode4_require_origin(&origin)?;
        }
        if origin.get("profile") != s(history_profile) {
            return Err(ActiveServiceWalErrorV1::WrongContext);
        }
        let owner = NEXT
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| ActiveServiceWalErrorV1::Capacity)?;
        let store = ActiveServiceWalV1 {
            root,
            namespace,
            journal: Some(journal),
            context,
            filesystem,
            owner,
            ambiguous: false,
            #[cfg(test)]
            fault: None,
            #[cfg(test)]
            commit_probe: None,
        };
        let record = store.record()?;
        if facts {
            actual_source_fact_entry_shape(&record)?;
        }
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
                "verifiedHistoryPost",
            ],
            if facts {
                &fact_optional
            } else if inline {
                &["nativeRecovery", "generationEscrow"]
            } else {
                &["nativeRecovery"]
            },
        )?;
        if record.get("profile") != s(history_profile)
            || record.get("stage") != s("Ack")
            || record.get("origin") != origin.get("archive")
        {
            return Err(ActiveServiceWalErrorV1::WrongContext);
        }
        let count = history_count(&record.get("sequence"))?;
        if count == 0 || count > MAX_ACK_HISTORY {
            return Err(ActiveServiceWalErrorV1::Capacity);
        }
        let mut budget = 0;
        let origin_archive = history_read_blob(&store, &origin.get("archive"), &mut budget)?;
        let origin_history =
            history_read_blob(&store, &origin.get("requiredVerifiedHistory"), &mut budget)?;
        let origin_observation = history_blob(&origin.get("observation"))?;
        history_read_blob(&store, &origin.get("observation"), &mut budget)?;
        let origin_head = sha256(&bytes(&origin)?);
        let head = history_hash(&record.get("head"))?;
        let mut expected_head = head;
        let mut next = history_blob(&record.get("entry"))?;
        let mut entries = Vec::with_capacity(count as usize);
        for sequence in (1..=count).rev() {
            let raw = history_read_blob(&store, &next.value(), &mut budget)?;
            let entry = parse(&raw)?;
            if facts {
                actual_source_fact_entry_shape(&entry)?;
            }
            history_keys(
                &entry,
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
                ],
                if facts {
                    &fact_optional
                } else if inline {
                    &["nativeRecovery", "generationEscrow"]
                } else {
                    &["nativeRecovery"]
                },
            )?;
            if entry.get("profile") != s(history_profile)
                || history_count(&entry.get("sequence"))? != sequence
            {
                return Err(ActiveServiceWalErrorV1::WrongContext);
            }
            let previous_head = history_hash(&entry.get("previousHead"))?;
            let mut chain = b"Native13/ActiveWalPhysicalAck/v1".to_vec();
            chain.extend_from_slice(&previous_head);
            chain.extend_from_slice(&sequence.to_be_bytes());
            chain.extend_from_slice(&next.sha256);
            if sha256(&chain) != expected_head {
                return Err(ActiveServiceWalErrorV1::WrongPin);
            }
            if sequence == count {
                for name in [
                    "action",
                    "before",
                    "nativeCandidates",
                    "nativeMutations",
                    "accepted",
                    "verifiedHistoryBefore",
                    "verifiedHistoryPost",
                    "nativeRecovery",
                    "previousHead",
                ]
                .into_iter()
                .chain(inline.then_some("generationEscrow"))
                .chain(
                    ACTUAL_SOURCE_FACT_ENTRY_FIELDS
                        .iter()
                        .copied()
                        .filter(|_| facts),
                ) {
                    if record.get(name) != entry.get(name) {
                        return Err(ActiveServiceWalErrorV1::WrongContext);
                    }
                }
            }
            let native_candidates =
                history_optional_blob(&store, &entry.get("nativeCandidates"), &mut budget)?;
            let native_mutations =
                history_optional_blob(&store, &entry.get("nativeMutations"), &mut budget)?;
            if native_candidates.is_some() != native_mutations.is_some() {
                return Err(ActiveServiceWalErrorV1::InvalidRecord);
            }
            let raw_action = history_read_blob(&store, &entry.get("action"), &mut budget)?;
            if !raw_action.starts_with(b"TLMASA\x01") {
                return Err(ActiveServiceWalErrorV1::InvalidRecord);
            }
            entries.push(VerifiedActiveServiceAckEntryV1 {
                sequence,
                entry: next.clone(),
                previous_head,
                head: expected_head,
                raw_action,
                actual_before: history_read_blob(&store, &entry.get("before"), &mut budget)?,
                actual_accepted: history_read_blob(&store, &entry.get("accepted"), &mut budget)?,
                verified_history_before: history_read_blob(
                    &store,
                    &entry.get("verifiedHistoryBefore"),
                    &mut budget,
                )?,
                verified_history_post: history_read_blob(
                    &store,
                    &entry.get("verifiedHistoryPost"),
                    &mut budget,
                )?,
                native_candidates,
                native_mutations,
                native_recovery: history_optional_blob(
                    &store,
                    &entry.get("nativeRecovery"),
                    &mut budget,
                )?,
            });
            expected_head = previous_head;
            let previous = entry.get("previousEntry");
            if sequence == 1 {
                if previous != V::Null || expected_head != origin_head {
                    return Err(ActiveServiceWalErrorV1::WrongPin);
                }
            } else {
                next = history_blob(&previous)?;
            }
        }
        entries.reverse();
        if entries.first().is_none_or(|entry| {
            ActiveServiceWalBlobV1::of(entry.actual_before())
                .ok()
                .as_ref()
                != Some(&origin_observation)
                || entry.verified_history_before() != origin_history
        }) {
            return Err(ActiveServiceWalErrorV1::WrongPin);
        }
        for (at, entry) in entries.iter().enumerate() {
            let (post_fields, diagnostic) = if inline {
                history_accepted_parts_profile(entry.actual_accepted(), true)?
            } else {
                history_accepted_parts(entry.actual_accepted())?
            };
            let diagnostic_recovery = diagnostic.get("nativeRecovery");
            if let Some(recovery) = entry.native_recovery() {
                if history_blob(&diagnostic_recovery)? != ActiveServiceWalBlobV1::of(recovery)? {
                    return Err(ActiveServiceWalErrorV1::WrongPin);
                }
            } else if diagnostic_recovery != V::Undefined {
                return Err(ActiveServiceWalErrorV1::WrongPin);
            }
            if inline {
                let raw = store.read_blob(&entry.entry)?;
                let value = parse(&raw)?;
                let reference = value.get("generationEscrow");
                let diagnostic_reference = diagnostic.get("generationEscrow");
                if reference == V::Undefined {
                    if diagnostic_reference != V::Undefined {
                        return Err(ActiveServiceWalErrorV1::WrongPin);
                    }
                } else if history_blob(&reference)? != history_blob(&diagnostic_reference)? {
                    return Err(ActiveServiceWalErrorV1::WrongPin);
                }
            }
            if let Some(next) = entries.get(at + 1) {
                if post_fields != next.actual_before()
                    || entry.verified_history_post() != next.verified_history_before()
                {
                    return Err(ActiveServiceWalErrorV1::WrongPin);
                }
            }
        }
        if store.pin()? != *expected {
            return Err(ActiveServiceWalErrorV1::WrongPin);
        }
        Ok(Self {
            store,
            origin,
            origin_archive,
            origin_history,
            origin_observation,
            head,
            entries,
        })
    }
    pub fn profile(&self) -> &'static str {
        VERIFIED_ACK_HISTORY_PROFILE
    }
    pub fn pin(&self) -> Result<ActiveServiceWalPinV1> {
        self.store.pin()
    }
    pub fn origin(&self) -> &V {
        &self.origin
    }
    pub fn origin_archive(&self) -> &[u8] {
        &self.origin_archive
    }
    pub fn origin_history(&self) -> &[u8] {
        &self.origin_history
    }
    pub fn origin_observation(&self) -> &ActiveServiceWalBlobV1 {
        &self.origin_observation
    }
    pub fn head(&self) -> [u8; 32] {
        self.head
    }
    /// Read an additional explicitly pinned raw metadata reference. Semantic
    /// recovery-proof validation remains the source reexecutor's responsibility.
    pub fn read_reference(&self, pin: &ActiveServiceWalBlobV1) -> Result<Vec<u8>> {
        self.store.read_blob(pin)
    }
    pub fn entries(&self) -> &[VerifiedActiveServiceAckEntryV1] {
        &self.entries
    }
}
