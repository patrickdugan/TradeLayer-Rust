use crate::source_compact_index_v2::{
    NativeHistoricalInlineDatabaseV1, SourceCompactIndex, SourceIndexGenerationEscrowPinV1,
    SourceIndexHistoryPinV1, VerifiedOriginalMetadataViewV1,
};
pub const MODE4_INLINE_CLOSURE_PROFILE: &str = "TL-MAIN-MODE4-INLINE-METADATA-CLOSURE-13-1";
pub struct VerifiedMode4InlineClosureV1 {
    sequence: u64,
    action: ActiveServiceWalBlobV1,
    before: SourceIndexHistoryPinV1,
    post: SourceIndexHistoryPinV1,
    recovery_links: usize,
    escrow_links: usize,
    wal: InlineActiveServiceColdPinV1,
}
impl VerifiedMode4InlineClosureV1 {
    pub fn profile(&self) -> &'static str {
        MODE4_INLINE_CLOSURE_PROFILE
    }
    pub fn sequence(&self) -> u64 {
        self.sequence
    }
    pub fn action(&self) -> &ActiveServiceWalBlobV1 {
        &self.action
    }
    pub fn before(&self) -> &SourceIndexHistoryPinV1 {
        &self.before
    }
    pub fn post(&self) -> &SourceIndexHistoryPinV1 {
        &self.post
    }
    pub fn recovery_links(&self) -> usize {
        self.recovery_links
    }
    pub fn escrow_links(&self) -> usize {
        self.escrow_links
    }
    pub fn wal_pin(&self) -> &InlineActiveServiceColdPinV1 {
        &self.wal
    }
}
#[derive(Clone, PartialEq, Eq)]
struct Mode4AppendPoint {
    sequence: u64,
    publication: crate::compact_journal::CatalogPublication,
}
impl Mode4AppendPoint {
    fn of(pin: &SourceIndexHistoryPinV1) -> Self {
        Self {
            sequence: pin.metadata.sequence,
            publication: pin.metadata.publication.clone(),
        }
    }
    fn value(&self) -> V {
        let p = &self.publication;
        obj(&[
            ("logicalSequence", s(self.sequence.to_string())),
            ("generation", s(p.generation.to_string())),
            ("parent", p.parent.map_or(V::Null, |v| s(v.to_string()))),
            (
                "physicalCreation",
                s(p.physical_creation_sequence.to_string()),
            ),
            ("logicalCreation", s(p.creation_sequence.to_string())),
            ("manifest", s(hex(&p.manifest_sha256d))),
        ])
    }
    fn parse(value: &V) -> Result<Self> {
        history_keys(
            value,
            &[
                "logicalSequence",
                "generation",
                "parent",
                "physicalCreation",
                "logicalCreation",
                "manifest",
            ],
            &[],
        )?;
        let result = Self {
            sequence: history_count(&value.get("logicalSequence"))?,
            publication: crate::compact_journal::CatalogPublication {
                generation: history_count(&value.get("generation"))?,
                parent: match value.get("parent") {
                    V::Null => None,
                    v => Some(history_count(&v)?),
                },
                physical_creation_sequence: history_count(&value.get("physicalCreation"))?,
                creation_sequence: history_count(&value.get("logicalCreation"))?,
                manifest_sha256d: history_hash(&value.get("manifest"))?,
            },
        };
        let p = &result.publication;
        if result.value() != *value
            || p.parent.is_some_and(|v| v >= p.generation)
            || p.creation_sequence > result.sequence
            || result
                .sequence
                .checked_sub(p.creation_sequence)
                .and_then(|v| v.checked_add(p.physical_creation_sequence))
                .is_none()
        {
            return Err(ActiveServiceWalErrorV1::InvalidRecord);
        }
        Ok(result)
    }
}
struct Mode4Candidates {
    raw: V,
    before: Mode4AppendPoint,
    intermediate: Option<Mode4AppendPoint>,
    post: Mode4AppendPoint,
    read_context: V,
}
impl Mode4Candidates {
    fn parse(raw: V, origin: &SourceIndexHistoryPinV1) -> Result<Self> {
        history_keys(
            &raw,
            &[
                "profile",
                "identity",
                "catalog",
                "before",
                "intermediate",
                "post",
                "targetLogBytes",
                "retainedAncestors",
                "readContext",
            ],
            &[],
        )?;
        if raw.get("profile") != s("TL-MAIN-NATIVE-PREPARED-INTENT-13-1")
            || raw.get("identity") != s(&origin.storage.persistent_identity)
            || raw.get("catalog") != s(origin.storage.canonical_catalog.to_string_lossy())
        {
            return Err(ActiveServiceWalErrorV1::WrongPin);
        }
        let before = Mode4AppendPoint::parse(&raw.get("before"))?;
        let intermediate = match raw.get("intermediate") {
            V::Null => None,
            v => Some(Mode4AppendPoint::parse(&v)?),
        };
        let post = raw.get("post");
        history_keys(&post, &["identity", "catalog", "append"], &[])?;
        if post.get("identity") != raw.get("identity") || post.get("catalog") != raw.get("catalog")
        {
            return Err(ActiveServiceWalErrorV1::WrongPin);
        }
        let post = Mode4AppendPoint::parse(&post.get("append"))?;
        let target = history_count(&raw.get("targetLogBytes"))?;
        let retained = history_count(&raw.get("retainedAncestors"))?;
        if !(65536..=crate::storage::MAX_LOG_BYTES).contains(&target)
            || retained > 8
            || before.sequence.checked_add(1) != Some(post.sequence)
        {
            return Err(ActiveServiceWalErrorV1::InvalidRecord);
        }
        if let Some(mid) = &intermediate {
            if mid.sequence != before.sequence
                || mid.publication.parent != Some(before.publication.generation)
                || mid.publication.creation_sequence != before.sequence
                || post.publication != mid.publication
            {
                return Err(ActiveServiceWalErrorV1::InvalidRecord);
            }
        } else if post.publication != before.publication {
            return Err(ActiveServiceWalErrorV1::InvalidRecord);
        }
        let read_context = raw.get("readContext");
        Ok(Self {
            raw,
            before,
            intermediate,
            post,
            read_context,
        })
    }
}
struct Mode4Escrow {
    reference: ActiveServiceWalBlobV1,
    pin: SourceIndexGenerationEscrowPinV1,
    context: V,
}
struct Mode4Recovery {
    reference: ActiveServiceWalBlobV1,
    value: V,
}
fn mode4_decode_hex(value: &V) -> Result<Vec<u8>> {
    let text = history_text(value)?;
    if text.len() > 2 * 1024 * 1024
        || !text.len().is_multiple_of(2)
        || !text
            .bytes()
            .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
    {
        return Err(ActiveServiceWalErrorV1::InvalidRecord);
    }
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let nibble = |v: u8| {
                if v.is_ascii_digit() {
                    v - b'0'
                } else {
                    v - b'a' + 10
                }
            };
            Ok(nibble(pair[0]) * 16 + nibble(pair[1]))
        })
        .collect()
}
fn mode4_physical(pin: &SourceIndexHistoryPinV1) -> V {
    let m = &pin.metadata;
    let p = &m.publication;
    obj(&[
        ("identity", s(&pin.storage.persistent_identity)),
        (
            "catalog",
            s(pin.storage.canonical_catalog.to_string_lossy()),
        ),
        ("profile", s(&m.profile)),
        ("context", s(&m.context)),
        ("sequence", s(m.sequence.to_string())),
        ("generation", s(p.generation.to_string())),
        ("parent", p.parent.map_or(V::Null, |v| s(v.to_string()))),
        (
            "physicalCreation",
            s(p.physical_creation_sequence.to_string()),
        ),
        ("logicalCreation", s(p.creation_sequence.to_string())),
        ("manifest", s(hex(&p.manifest_sha256d))),
    ])
}
fn mode4_state(state: &crate::source_compact_index_v2::NativeMetadataStateReceiptV1) -> V {
    obj(&[
        ("profile", s(&state.profile)),
        ("keys", s(state.keys.to_string())),
        ("bytes", s(state.bytes.to_string())),
        ("chain", s(hex(&state.chain_sha256d))),
        ("complete", V::Bool(state.complete)),
    ])
}
fn mode4_snapshot(pin: &SourceIndexHistoryPinV1) -> V {
    let m = &pin.metadata;
    let p = &m.publication;
    obj(&[
        ("profile", s(&m.profile)),
        ("context", s(&m.context)),
        ("sequence", s(m.sequence.to_string())),
        ("generation", s(p.generation.to_string())),
        ("parent", p.parent.map_or(V::Null, |v| s(v.to_string()))),
        (
            "physicalCreation",
            s(p.physical_creation_sequence.to_string()),
        ),
        ("logicalCreation", s(p.creation_sequence.to_string())),
        ("manifest", s(hex(&p.manifest_sha256d))),
    ])
}
impl VerifiedInlineActiveServiceAckHistoryV1 {
    fn mode4_pin(&self, raw: &[u8]) -> Result<SourceIndexHistoryPinV1> {
        SourceIndexHistoryPinV1::from_canonical_scalar_bytes(
            &self.independent.metadata_namespace,
            raw,
        )
        .map_err(|_| ActiveServiceWalErrorV1::WrongPin)
    }
    fn mode4_chain(&self, start: V, escrow: bool) -> Result<Vec<(ActiveServiceWalBlobV1, V)>> {
        let mut reference = start;
        let mut reversed = vec![];
        let mut seen = std::collections::BTreeSet::new();
        let mut charged = 0usize;
        while reference != V::Undefined && reference != V::Null {
            if reversed.len() >= 4096 {
                return Err(ActiveServiceWalErrorV1::Capacity);
            }
            let pin = history_blob(&reference)?;
            if !seen.insert(pin.sha256) {
                return Err(ActiveServiceWalErrorV1::WrongPin);
            }
            let raw = self.read_reference(&pin)?;
            charged = charged
                .checked_add(raw.len())
                .ok_or(ActiveServiceWalErrorV1::Capacity)?;
            if charged > MAX_VERIFIED_ACK_HISTORY_BYTES as usize {
                return Err(ActiveServiceWalErrorV1::Capacity);
            }
            let value = parse(&raw)?;
            reference = value.get(if escrow {
                "previous"
            } else {
                "previousRecovery"
            });
            if reference == V::Undefined {
                return Err(ActiveServiceWalErrorV1::InvalidRecord);
            }
            reversed.push((pin, value));
        }
        reversed.reverse();
        Ok(reversed)
    }
    fn mode4_escrows(&self, entry: &VerifiedActiveServiceAckEntryV1) -> Result<Vec<Mode4Escrow>> {
        let value = self.entry_value(entry)?;
        let config = self.origin().get("generationEscrow");
        let origin = self.mode4_pin(self.origin_history())?;
        let mut out = vec![];
        for (reference, raw) in self.mode4_chain(value.get("generationEscrow"), true)? {
            history_keys(
                &raw,
                &[
                    "profile",
                    "previous",
                    "nativeReadContext",
                    "expectedEscrowFieldsHex",
                ],
                &[],
            )?;
            if raw.get("profile") != s("TL-ACTIVE-GENERATION-ESCROW-CHAIN-13-1") {
                return Err(ActiveServiceWalErrorV1::WrongContext);
            }
            let pin = SourceIndexGenerationEscrowPinV1::from_canonical_scalar_bytes(
                &self.independent.metadata_namespace,
                &mode4_decode_hex(&raw.get("expectedEscrowFieldsHex"))?,
            )
            .map_err(|_| ActiveServiceWalErrorV1::WrongPin)?;
            if pin.points.len() != 1
                || pin.points[0].storage != pin.storage.original_current
                || pin.storage.original_current.namespace != origin.storage.namespace
                || pin.storage.original_current.persistent_identity
                    != origin.storage.persistent_identity
                || pin.storage.original_current.canonical_catalog
                    != origin.storage.canonical_catalog
                || pin.points[0].metadata.context != origin.metadata.context
                || s(pin.storage.copied_root.to_string_lossy()) != config.get("root")
                || s(format!("{:?}", pin.storage.durability)) != config.get("durability")
            {
                return Err(ActiveServiceWalErrorV1::WrongPin);
            }
            // An expected descriptor alone is insufficient: open the actual
            // published copy and verify its record/log/full-KV/row evidence.
            VerifiedOriginalMetadataViewV1::open_copy(&pin, &pin.points[0])
                .map_err(|_| ActiveServiceWalErrorV1::WrongPin)?;
            out.push(Mode4Escrow {
                reference,
                pin,
                context: raw.get("nativeReadContext"),
            });
        }
        Ok(out)
    }
    fn mode4_all_escrows(&self) -> Result<Vec<Mode4Escrow>> {
        let mut all = vec![];
        for entry in self.entries() {
            all.extend(self.mode4_escrows(entry)?);
            if all.len() > 4096 {
                return Err(ActiveServiceWalErrorV1::Capacity);
            }
        }
        Ok(all)
    }
    fn mode4_view(
        &self,
        index: &mut SourceCompactIndex,
        pin: &SourceIndexHistoryPinV1,
        escrows: &[Mode4Escrow],
    ) -> Result<VerifiedOriginalMetadataViewV1> {
        let current = index
            .capture_verified_history_pin()
            .map_err(|_| ActiveServiceWalErrorV1::WrongPin)?;
        if pin.storage.namespace != current.storage.namespace
            || pin.storage.persistent_identity != current.storage.persistent_identity
            || pin.storage.canonical_catalog != current.storage.canonical_catalog
        {
            return Err(ActiveServiceWalErrorV1::WrongPin);
        }
        if pin.storage.publication == current.storage.publication {
            return VerifiedOriginalMetadataViewV1::open_original(index, pin)
                .map_err(|_| ActiveServiceWalErrorV1::WrongPin);
        }
        let copy = escrows
            .iter()
            .find(|e| {
                e.pin.storage.original_current.publication == pin.storage.publication
                    && e.pin.storage.original_current.persistent_identity
                        == pin.storage.persistent_identity
                    && e.pin.storage.original_current.canonical_catalog
                        == pin.storage.canonical_catalog
                    && e.pin.storage.original_current.logical_sequence
                        >= pin.storage.logical_sequence
            })
            .ok_or(ActiveServiceWalErrorV1::WrongPin)?;
        VerifiedOriginalMetadataViewV1::open_copy(&copy.pin, pin)
            .map_err(|_| ActiveServiceWalErrorV1::WrongPin)
    }
    fn mode4_barrier(
        &self,
        sequence: u64,
        stage: &str,
        key: &str,
        reference: &ActiveServiceWalBlobV1,
    ) -> Result<u64> {
        let mut found = None;
        for (physical, record) in &self.records {
            if history_count(&record.get("sequence"))? == sequence
                && record.get("stage") == s(stage)
                && record.get(key) == reference.value()
                && found.replace(*physical).is_some()
            {
                return Err(ActiveServiceWalErrorV1::WrongContext);
            }
        }
        found.ok_or(ActiveServiceWalErrorV1::WrongContext)
    }
    /// The caller supplies its independently recomputed opaque historical
    /// operation, never an Accepted result. Full source/action replay and the
    /// caller/job/await-role selector remain separate admission requirements.
    pub fn verify_inline_operation(
        &self,
        index: &mut SourceCompactIndex,
        sequence: u64,
        source: &mut NativeHistoricalInlineDatabaseV1,
    ) -> Result<VerifiedMode4InlineClosureV1> {
        let entry = self
            .entries()
            .get(
                usize::try_from(
                    sequence
                        .checked_sub(1)
                        .ok_or(ActiveServiceWalErrorV1::WrongContext)?,
                )
                .map_err(|_| ActiveServiceWalErrorV1::Capacity)?,
            )
            .ok_or(ActiveServiceWalErrorV1::WrongContext)?;
        if source.append_verified()
            || source.resolved()
            || !matches!(
                self.decoded_action(entry)?,
                crate::source_main_native_session::NativeMainActiveServiceActionV1::AppendPending
            )
        {
            return Err(ActiveServiceWalErrorV1::WrongContext);
        }
        let before = self.mode4_pin(entry.verified_history_before())?;
        let post = self.mode4_pin(entry.verified_history_post())?;
        if source.original_read() != &before {
            return Err(ActiveServiceWalErrorV1::WrongPin);
        }
        let final_pin = self.mode4_pin(
            self.entries()
                .last()
                .ok_or(ActiveServiceWalErrorV1::WrongContext)?
                .verified_history_post(),
        )?;
        if index
            .capture_verified_history_pin()
            .map_err(|_| ActiveServiceWalErrorV1::WrongPin)?
            != final_pin
        {
            return Err(ActiveServiceWalErrorV1::WrongPin);
        }
        let escrows = self.mode4_all_escrows()?;
        let _actual_before = self.mode4_view(index, &before, &escrows)?;
        let actual_post = self.mode4_view(index, &post, &escrows)?;
        let target_escrows = self.mode4_escrows(entry)?;
        let value = self.entry_value(entry)?;
        let recoveries = self
            .mode4_chain(value.get("nativeRecovery"), false)?
            .into_iter()
            .map(|(reference, value)| Mode4Recovery { reference, value })
            .collect::<Vec<_>>();
        self.mode4_require_all_recovery_barriers(entry, &recoveries, &target_escrows)?;
        if !source.requires_native_write() {
            if before != post
                || entry.native_candidates().is_some()
                || entry.native_mutations().is_some()
                || !recoveries.is_empty()
                || !target_escrows.is_empty()
            {
                return Err(ActiveServiceWalErrorV1::WrongPin);
            }
        } else {
            let raw = entry
                .native_mutations()
                .ok_or(ActiveServiceWalErrorV1::WrongPin)?;
            if raw != source.exact_prepared_mutations() {
                return Err(ActiveServiceWalErrorV1::WrongPin);
            }
            let mut candidates = Mode4Candidates::parse(
                parse(
                    entry
                        .native_candidates()
                        .ok_or(ActiveServiceWalErrorV1::WrongPin)?,
                )?,
                &before,
            )?;
            if candidates.before != Mode4AppendPoint::of(&before)
                || candidates.read_context != source.durable_read_context()
            {
                return Err(ActiveServiceWalErrorV1::WrongPin);
            }
            let mut escrow_at = 0;
            let mut lower = self
                .records
                .iter()
                .find(|(_, r)| {
                    r.get("sequence") == s(sequence.to_string()) && r.get("stage") == s("Intent")
                })
                .map(|(n, _)| *n)
                .ok_or(ActiveServiceWalErrorV1::WrongPin)?;
            let mut original_read = before.clone();
            for recovery in &recoveries {
                let proof = &recovery.value;
                let candidate = history_text(&proof.get("candidate"))?.to_owned();
                let needs_replacement = matches!(candidate.as_str(), "Before" | "Intermediate");
                history_keys(
                    proof,
                    &[
                        "profile",
                        "candidate",
                        "snapshot",
                        "discardedBytes",
                        "previousRecovery",
                        "verifiedReopenedHistory",
                    ],
                    if needs_replacement {
                        &[
                            "binding",
                            "replacementCandidates",
                            "replacementMutationsSha256",
                        ]
                    } else {
                        &[]
                    },
                )?;
                if proof.get("profile") != s("TL-MAIN-ACTIVE-INLINE-NATIVE-RECOVERY-13-1") {
                    return Err(ActiveServiceWalErrorV1::WrongContext);
                }
                history_count(&proof.get("discardedBytes"))?;
                let reopened =
                    self.mode4_pin(&mode4_decode_hex(&proof.get("verifiedReopenedHistory"))?)?;
                let expected = match candidate.as_str() {
                    "Before" => &candidates.before,
                    "Intermediate" => candidates
                        .intermediate
                        .as_ref()
                        .ok_or(ActiveServiceWalErrorV1::WrongPin)?,
                    "Post" => &candidates.post,
                    _ => return Err(ActiveServiceWalErrorV1::InvalidRecord),
                };
                if Mode4AppendPoint::of(&reopened) != *expected
                    || proof.get("snapshot") != mode4_physical(&reopened)
                {
                    return Err(ActiveServiceWalErrorV1::WrongPin);
                }
                let actual = self.mode4_view(index, &reopened, &escrows)?;
                let upper = if needs_replacement {
                    self.mode4_barrier(
                        sequence,
                        "NativeSupersessionIntent",
                        "nativeRecovery",
                        &recovery.reference,
                    )?
                } else {
                    u64::MAX
                };
                if needs_replacement && upper <= lower {
                    return Err(ActiveServiceWalErrorV1::WrongPin);
                }
                self.mode4_attempt_escrow(
                    &candidates,
                    &original_read,
                    &target_escrows,
                    &mut escrow_at,
                    sequence,
                    (lower, upper),
                )?;
                if needs_replacement {
                    let recomputed = actual
                        .prepare_historical_inline_database(
                            source.method(),
                            source.arguments().to_vec(),
                        )
                        .map_err(|_| ActiveServiceWalErrorV1::WrongPin)?;
                    if recomputed.before_state() != source.before_state()
                        || recomputed.after_state() != source.after_state()
                        || recomputed.exact_prepared_mutations() != raw
                    {
                        return Err(ActiveServiceWalErrorV1::WrongPin);
                    }
                    let binding = proof.get("binding");
                    let expected_binding = obj(&[
                        ("profile", s("TL-NATIVE-INLINE-DATABASE-REOPEN-13-1")),
                        ("method", s(source.method())),
                        ("arguments", V::Array(source.arguments().to_vec())),
                        ("snapshot", mode4_snapshot(&reopened)),
                        ("completeState", mode4_state(source.before_state())),
                        ("discardedBytes", proof.get("discardedBytes")),
                    ]);
                    if binding != expected_binding
                        || history_hash(&proof.get("replacementMutationsSha256"))? != sha256(raw)
                    {
                        return Err(ActiveServiceWalErrorV1::WrongPin);
                    }
                    let replacement =
                        Mode4Candidates::parse(proof.get("replacementCandidates"), &before)?;
                    if replacement.before != Mode4AppendPoint::of(&reopened)
                        || replacement.read_context != recomputed.durable_read_context()
                        || replacement.raw.get("targetLogBytes")
                            != candidates.raw.get("targetLogBytes")
                        || replacement.raw.get("retainedAncestors")
                            != candidates.raw.get("retainedAncestors")
                    {
                        return Err(ActiveServiceWalErrorV1::WrongPin);
                    }
                    candidates = replacement;
                    original_read = reopened;
                    lower = upper;
                } else {
                    if proof.get("binding") != V::Undefined
                        || proof.get("replacementCandidates") != V::Undefined
                        || proof.get("replacementMutationsSha256") != V::Undefined
                        || reopened != post
                    {
                        return Err(ActiveServiceWalErrorV1::WrongPin);
                    }
                    // No retry or new attempt follows a verified actual Post.
                    if recovery.reference != recoveries.last().expect("nonempty").reference {
                        return Err(ActiveServiceWalErrorV1::WrongPin);
                    }
                    lower = u64::MAX;
                }
            }
            if lower != u64::MAX {
                self.mode4_attempt_escrow(
                    &candidates,
                    &original_read,
                    &target_escrows,
                    &mut escrow_at,
                    sequence,
                    (lower, u64::MAX),
                )?;
            }
            if escrow_at != target_escrows.len() || candidates.post != Mode4AppendPoint::of(&post) {
                return Err(ActiveServiceWalErrorV1::WrongPin);
            }
            source
                .verify_recorded_native_append(&actual_post, raw)
                .map_err(|_| ActiveServiceWalErrorV1::WrongPin)?;
        }
        Ok(VerifiedMode4InlineClosureV1 {
            sequence,
            action: ActiveServiceWalBlobV1::of(entry.raw_action())?,
            before,
            post,
            recovery_links: recoveries.len(),
            escrow_links: target_escrows.len(),
            wal: self.independent.clone(),
        })
    }
    fn mode4_attempt_escrow(
        &self,
        candidates: &Mode4Candidates,
        before: &SourceIndexHistoryPinV1,
        escrows: &[Mode4Escrow],
        at: &mut usize,
        sequence: u64,
        (lower, upper): (u64, u64),
    ) -> Result<()> {
        if candidates.intermediate.is_none() {
            return Ok(());
        }
        let escrow = escrows.get(*at).ok_or(ActiveServiceWalErrorV1::WrongPin)?;
        let physical = self.mode4_barrier(
            sequence,
            "EscrowBoundIntent",
            "generationEscrow",
            &escrow.reference,
        )?;
        if physical <= lower
            || physical >= upper
            || escrow.context != candidates.read_context
            || escrow.pin.points[0] != *before
        {
            return Err(ActiveServiceWalErrorV1::WrongPin);
        }
        *at += 1;
        Ok(())
    }
    fn mode4_require_all_recovery_barriers(
        &self,
        entry: &VerifiedActiveServiceAckEntryV1,
        recoveries: &[Mode4Recovery],
        escrows: &[Mode4Escrow],
    ) -> Result<()> {
        let mut current_recovery = V::Undefined;
        let mut current_escrow = V::Undefined;
        let mut escrow_count = 0usize;
        for (_, record) in &self.records {
            if record.get("sequence") != s(entry.sequence.to_string()) {
                continue;
            }
            match history_text(&record.get("stage"))? {
                "NativeSupersessionIntent" => {
                    let reference = record.get("nativeRecovery");
                    let proof = recoveries
                        .iter()
                        .find(|r| r.reference.value() == reference)
                        .ok_or(ActiveServiceWalErrorV1::WrongPin)?;
                    let previous = if current_recovery == V::Undefined {
                        V::Null
                    } else {
                        current_recovery.clone()
                    };
                    if proof.value.get("candidate") == s("Post")
                        || proof.value.get("previousRecovery") != previous
                        || reference == current_recovery
                        || record.get("generationEscrow") != current_escrow
                    {
                        return Err(ActiveServiceWalErrorV1::WrongPin);
                    }
                    current_recovery = reference;
                }
                "EscrowBoundIntent" => {
                    let reference = record.get("generationEscrow");
                    if escrows
                        .get(escrow_count)
                        .is_none_or(|e| e.reference.value() != reference)
                        || reference == current_escrow
                        || record.get("nativeRecovery") != current_recovery
                    {
                        return Err(ActiveServiceWalErrorV1::WrongPin);
                    }
                    current_escrow = reference;
                    escrow_count += 1;
                }
                "Accepted" | "Ack" => {
                    if record.get("generationEscrow") != current_escrow {
                        return Err(ActiveServiceWalErrorV1::WrongPin);
                    }
                    let reference = record.get("nativeRecovery");
                    if reference != current_recovery {
                        let proof = recoveries
                            .last()
                            .filter(|r| r.reference.value() == reference)
                            .ok_or(ActiveServiceWalErrorV1::WrongPin)?;
                        let previous = if current_recovery == V::Undefined {
                            V::Null
                        } else {
                            current_recovery.clone()
                        };
                        if proof.value.get("candidate") != s("Post")
                            || proof.value.get("previousRecovery") != previous
                        {
                            return Err(ActiveServiceWalErrorV1::WrongPin);
                        }
                        current_recovery = reference;
                    }
                }
                _ => (),
            }
        }
        if escrow_count != escrows.len()
            || current_recovery
                != recoveries
                    .last()
                    .map_or(V::Undefined, |r| r.reference.value())
        {
            return Err(ActiveServiceWalErrorV1::WrongPin);
        }
        Ok(())
    }
}
