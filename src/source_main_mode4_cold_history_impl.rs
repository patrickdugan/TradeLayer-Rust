// Separate Mode4 metadata reader. It is never converted to ordinary HISTORY or
// supplied as a financial/source runtime. Trace order is conditional integrity,
// not proof of hardware sync, actor authority or block consensus.
pub const MODE4_INLINE_WAL_PROFILE: &str = "TL-MAIN-ACTIVE-SERVICE-INLINE-ESCROW-WAL-13-1";
pub const MODE4_INLINE_SERVICE_PROFILE: &str = "TL-MAIN-NATIVE-INLINE-ACTIVE-SERVICE-13-1";
pub const MODE4_COLD_PIN_PROFILE: &str = "TL-MAIN-MODE4-INDEPENDENT-COMMIT-TRACE-13-1";
pub const VERIFIED_MODE4_ACK_PROFILE: &str = "TL-MAIN-MODE4-VERIFIED-ACK-13-1";
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InlineActiveServiceColdPinV1 {
    pub profile: String,
    pub wal: ActiveServiceWalPinV1,
    pub metadata_namespace: Namespace,
    pub trace: crate::storage::JournalCommitTracePinV1,
}
pub struct VerifiedInlineActiveServiceAckHistoryV1 {
    inner: VerifiedActiveServiceAckHistoryV1,
    independent: InlineActiveServiceColdPinV1,
    records: Vec<(u64, V)>,
}
fn mode4_require_origin(origin: &V) -> Result<()> {
    if origin.get("profile") != s(MODE4_INLINE_WAL_PROFILE)
        || origin.get("requiredInlineServiceProfile") != s(MODE4_INLINE_SERVICE_PROFILE)
    {
        return Err(ActiveServiceWalErrorV1::WrongContext);
    }
    let escrow = origin.get("generationEscrow");
    history_keys(&escrow, &["profile", "root", "durability"], &[])?;
    if escrow.get("profile") != s("TL-COMPACT-GENERATION-ESCROW-13-1")
        || !Path::new(history_text(&escrow.get("root"))?).is_absolute()
        || !matches!(
            history_text(&escrow.get("durability"))?,
            "FileSyncOnly" | "FileAndDirectorySync"
        )
    {
        return Err(ActiveServiceWalErrorV1::WrongContext);
    }
    Ok(())
}
fn mode4_namespace(wal: &Namespace, metadata: &Namespace) -> Result<()> {
    let root = Path::new("");
    let wal = wal.path(root);
    let metadata = metadata.path(root);
    if wal.parent() != metadata.parent()
        || metadata.file_name() != Some(std::ffi::OsStr::new("txIndex.tldb"))
    {
        return Err(ActiveServiceWalErrorV1::WrongContext);
    }
    Ok(())
}
impl ActiveServiceWalV1 {
    /// Read-only actual capture. The caller must independently retain this pin
    /// before later cold use; open never self-selects a replacement trace.
    pub fn capture_inline_cold_pin(
        &mut self,
        metadata_namespace: &Namespace,
    ) -> Result<InlineActiveServiceColdPinV1> {
        mode4_namespace(&self.namespace, metadata_namespace)?;
        let context = parse(&self.context)?;
        let origin = context.get("origin");
        mode4_require_origin(&origin)?;
        let record = self.record()?;
        if record.get("profile") != s(MODE4_INLINE_WAL_PROFILE) || record.get("stage") != s("Ack") {
            return Err(ActiveServiceWalErrorV1::WrongContext);
        }
        let raw = self.read_blob(&history_blob(&origin.get("requiredVerifiedHistory"))?)?;
        crate::source_compact_index_v2::SourceIndexHistoryPinV1::from_canonical_scalar_bytes(
            metadata_namespace,
            &raw,
        )
        .map_err(|_| ActiveServiceWalErrorV1::WrongPin)?;
        let pin = self.pin()?;
        let history = self
            .journal
            .as_mut()
            .ok_or(ActiveServiceWalErrorV1::Ambiguous)?
            .read_verified_current_commits(
                &pin.prefix,
                65536,
                MAX_VERIFIED_ACK_HISTORY_BYTES as usize,
            )?;
        Ok(InlineActiveServiceColdPinV1 {
            profile: MODE4_COLD_PIN_PROFILE.into(),
            wal: pin,
            metadata_namespace: metadata_namespace.clone(),
            trace: history.trace().clone(),
        })
    }
}
impl VerifiedInlineActiveServiceAckHistoryV1 {
    pub fn open(
        root: &Path,
        namespace: Namespace,
        expected: &InlineActiveServiceColdPinV1,
    ) -> Result<Self> {
        if expected.profile != MODE4_COLD_PIN_PROFILE
            || expected.trace.profile != crate::storage::VERIFIED_COMMIT_TRACE_PROFILE
        {
            return Err(ActiveServiceWalErrorV1::WrongPin);
        }
        mode4_namespace(&namespace, &expected.metadata_namespace)?;
        let mut inner =
            VerifiedActiveServiceAckHistoryV1::open_profile(root, namespace, &expected.wal, true)?;
        let actual = inner
            .store
            .journal
            .as_mut()
            .ok_or(ActiveServiceWalErrorV1::Ambiguous)?
            .read_verified_current_commits(
                &expected.wal.prefix,
                65536,
                MAX_VERIFIED_ACK_HISTORY_BYTES as usize,
            )?;
        if actual.trace() != &expected.trace {
            return Err(ActiveServiceWalErrorV1::WrongPin);
        }
        let mut records = vec![];
        for (at, commit) in actual.commits().iter().enumerate() {
            if at == 0 {
                if commit.sequence != 1 || commit.mutations.len() != 2 {
                    return Err(ActiveServiceWalErrorV1::InvalidRecord);
                }
                let expected_created =
                    bytes(&obj(&[("profile", s(PROFILE)), ("phase", s("Created"))]))?;
                if commit.mutations
                    != vec![
                        Mutation::Put {
                            key: CONTEXT.into(),
                            value: inner.store.context.clone(),
                        },
                        Mutation::Put {
                            key: RECORD.into(),
                            value: expected_created,
                        },
                    ]
                {
                    return Err(ActiveServiceWalErrorV1::WrongContext);
                }
            } else {
                let [Mutation::Put { key, value }] = commit.mutations.as_slice() else {
                    return Err(ActiveServiceWalErrorV1::InvalidRecord);
                };
                if key != RECORD {
                    return Err(ActiveServiceWalErrorV1::InvalidRecord);
                }
                records.push((commit.sequence, parse(value)?));
            }
        }
        let result = Self {
            inner,
            independent: expected.clone(),
            records,
        };
        result.require_recorded_barriers()?;
        Ok(result)
    }
    pub fn profile(&self) -> &'static str {
        VERIFIED_MODE4_ACK_PROFILE
    }
    pub fn pin(&self) -> &InlineActiveServiceColdPinV1 {
        &self.independent
    }
    pub fn origin(&self) -> &V {
        self.inner.origin()
    }
    pub fn origin_archive(&self) -> &[u8] {
        self.inner.origin_archive()
    }
    pub fn origin_history(&self) -> &[u8] {
        self.inner.origin_history()
    }
    pub fn entries(&self) -> &[VerifiedActiveServiceAckEntryV1] {
        self.inner.entries()
    }
    pub fn read_reference(&self, pin: &ActiveServiceWalBlobV1) -> Result<Vec<u8>> {
        self.inner.read_reference(pin)
    }
    fn entry_value(&self, entry: &VerifiedActiveServiceAckEntryV1) -> Result<V> {
        parse(&self.read_reference(&entry.entry)?)
    }
    pub fn decoded_action(
        &self,
        entry: &VerifiedActiveServiceAckEntryV1,
    ) -> Result<crate::source_main_native_session::NativeMainActiveServiceActionV1> {
        if self
            .entries()
            .get(entry.sequence.saturating_sub(1) as usize)
            != Some(entry)
        {
            return Err(ActiveServiceWalErrorV1::WrongContext);
        }
        crate::source_main_native_session::NativeMainActiveServiceActionV1::from_canonical_action_bytes(entry.raw_action())
            .map_err(|_|ActiveServiceWalErrorV1::InvalidRecord)
    }
    fn require_recorded_barriers(&self) -> Result<()> {
        let origin_pin =
            crate::source_compact_index_v2::SourceIndexHistoryPinV1::from_canonical_scalar_bytes(
                &self.independent.metadata_namespace,
                self.origin_history(),
            )
            .map_err(|_| ActiveServiceWalErrorV1::WrongPin)?;
        let mut at = 0;
        let mut last_append = None;
        for entry in self.entries() {
            let decoded = self.decoded_action(entry)?;
            let (_, diagnostic) = history_accepted_parts_profile(entry.actual_accepted(), true)?;
            if entry.sequence == 1 {
                if !matches!(
                    decoded,
                    crate::source_main_native_session::NativeMainActiveServiceActionV1::Start { .. }
                ) {
                    return Err(ActiveServiceWalErrorV1::WrongContext);
                }
            } else if matches!(
                decoded,
                crate::source_main_native_session::NativeMainActiveServiceActionV1::Start { .. }
            ) {
                return Err(ActiveServiceWalErrorV1::WrongContext);
            }
            let value = self.entry_value(entry)?;
            for name in ["nativeRecovery", "generationEscrow"] {
                let reference = value.get(name);
                if reference != V::Undefined {
                    history_blob(&reference)?;
                }
            }
            let before=crate::source_compact_index_v2::SourceIndexHistoryPinV1::from_canonical_scalar_bytes(&self.independent.metadata_namespace,entry.verified_history_before()).map_err(|_|ActiveServiceWalErrorV1::WrongPin)?;
            let post=crate::source_compact_index_v2::SourceIndexHistoryPinV1::from_canonical_scalar_bytes(&self.independent.metadata_namespace,entry.verified_history_post()).map_err(|_|ActiveServiceWalErrorV1::WrongPin)?;
            for pin in [&before, &post] {
                if pin.storage.namespace != origin_pin.storage.namespace
                    || pin.storage.persistent_identity != origin_pin.storage.persistent_identity
                    || pin.storage.canonical_catalog != origin_pin.storage.canonical_catalog
                    || pin.metadata.context != origin_pin.metadata.context
                {
                    return Err(ActiveServiceWalErrorV1::WrongPin);
                }
            }
            if !matches!(
                decoded,
                crate::source_main_native_session::NativeMainActiveServiceActionV1::AppendPending
            ) {
                if before != post
                    || entry.native_candidates().is_some()
                    || entry.native_recovery().is_some()
                    || value.get("generationEscrow") != V::Undefined
                {
                    return Err(ActiveServiceWalErrorV1::WrongContext);
                }
            } else if diagnostic.get("apiRejection") == s("WriteAlreadyAttempted") {
                self.mode4_require_rejected_reappend(entry, last_append, &before, &post)?;
            } else if entry.native_candidates().is_none() {
                if before != post
                    || entry.native_recovery().is_some()
                    || value.get("generationEscrow") != V::Undefined
                {
                    return Err(ActiveServiceWalErrorV1::WrongContext);
                }
            } else if before.metadata.sequence.checked_add(1) != Some(post.metadata.sequence) {
                return Err(ActiveServiceWalErrorV1::WrongContext);
            }
            let mut stages = vec![];
            while let Some((physical, record)) = self.records.get(at) {
                if history_count(&record.get("sequence"))? != entry.sequence {
                    break;
                }
                history_keys(
                    record,
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
                    ],
                    &["verifiedHistoryPost", "nativeRecovery", "generationEscrow"],
                )?;
                if record.get("profile") != s(MODE4_INLINE_WAL_PROFILE)
                    || record.get("origin") != self.origin().get("archive")
                    || record.get("previousHead") != s(hex(&entry.previous_head))
                {
                    return Err(ActiveServiceWalErrorV1::WrongContext);
                }
                for key in [
                    "action",
                    "before",
                    "nativeCandidates",
                    "nativeMutations",
                    "verifiedHistoryBefore",
                ] {
                    if record.get(key) != value.get(key) {
                        return Err(ActiveServiceWalErrorV1::WrongPin);
                    }
                }
                let stage = history_text(&record.get("stage"))?.to_owned();
                for name in ["nativeRecovery", "generationEscrow"] {
                    let reference = record.get(name);
                    if reference != V::Undefined {
                        history_blob(&reference)?;
                    }
                }
                match stage.as_str() {
                    "Intent" => {
                        if !stages.is_empty()
                            || record.get("head") != V::Null
                            || record.get("accepted") != V::Null
                            || record.get("entry") != V::Null
                            || record.get("nativeRecovery") != V::Undefined
                            || record.get("generationEscrow") != V::Undefined
                            || record.get("verifiedHistoryPost") != V::Undefined
                        {
                            return Err(ActiveServiceWalErrorV1::WrongContext);
                        }
                    }
                    "EscrowBoundIntent" | "NativeSupersessionIntent" => {
                        if stages.is_empty()||stages.iter().any(|(_,s):&(u64,String)|s=="Accepted"||s=="Ack")
                            ||!matches!(decoded,crate::source_main_native_session::NativeMainActiveServiceActionV1::AppendPending)
                            ||record.get("head")!=V::Null||record.get("accepted")!=V::Null||record.get("entry")!=V::Null
                            ||record.get("verifiedHistoryPost")!=V::Undefined{return Err(ActiveServiceWalErrorV1::WrongContext);}
                        history_blob(&record.get(if stage == "EscrowBoundIntent" {
                            "generationEscrow"
                        } else {
                            "nativeRecovery"
                        }))?;
                    }
                    "Accepted" | "Ack" => {
                        if stages.is_empty()
                            || record.get("head") != s(hex(&entry.head))
                            || record.get("entry") != entry.entry.value()
                        {
                            return Err(ActiveServiceWalErrorV1::WrongPin);
                        }
                        for key in [
                            "accepted",
                            "verifiedHistoryPost",
                            "nativeRecovery",
                            "generationEscrow",
                        ] {
                            if record.get(key) != value.get(key) {
                                return Err(ActiveServiceWalErrorV1::WrongPin);
                            }
                        }
                        if (stage == "Accepted"
                            && stages.iter().any(|(_, s)| s == "Accepted" || s == "Ack"))
                            || (stage == "Ack"
                                && stages.last().is_none_or(|(_, s)| s != "Accepted"))
                        {
                            return Err(ActiveServiceWalErrorV1::WrongContext);
                        }
                    }
                    _ => return Err(ActiveServiceWalErrorV1::InvalidRecord),
                }
                stages.push((*physical, stage));
                at += 1;
                if stages.last().is_some_and(|(_, s)| s == "Ack") {
                    break;
                }
            }
            if stages.first().is_none_or(|(_, s)| s != "Intent")
                || stages.last().is_none_or(|(_, s)| s != "Ack")
            {
                return Err(ActiveServiceWalErrorV1::WrongContext);
            }
            if diagnostic.get("apiRejection") == V::Null {
                match decoded {
                    crate::source_main_native_session::NativeMainActiveServiceActionV1::AppendPending => {
                        last_append = Some(entry);
                    }
                    crate::source_main_native_session::NativeMainActiveServiceActionV1::ResolvePost => {
                        last_append = None;
                    }
                    _ => (),
                }
            }
        }
        if at != self.records.len() {
            return Err(ActiveServiceWalErrorV1::WrongContext);
        }
        Ok(())
    }
}
