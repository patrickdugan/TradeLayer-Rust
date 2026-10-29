pub const NATIVE_SUPERSEDING_INTENT_PROFILE: &str = "TL-MAIN-NATIVE-SUPERSEDING-INTENT-13-1";
const SUPERSEDING_KIND: &str = "NativeSupersessionIntentV1";
use crate::source_compact_index_v2::{
    NativePreparedValidityRecoveryObservationV1, NativePreparedValiditySupersessionV1,
    NativePreparedValidityWriteV1, SourceCompactIndex,
};

/// Sealed actual transition retained before sync. No constructor from files,
/// financial snapshots, expected answers, or supplied actor identities.
pub struct NativePreparedSupersedingIntentV1 {
    token: u64,
    before: NativeIntentStorePinV1,
    after: NativeIntentStorePinV1,
    original_intent: NativeIntentBlobV1,
    original_candidates: NativeWriteIntentCandidatesV1,
    command: NativeIntentBlobV1,
    new_intent: NativeIntentBlobV1,
    new_candidates: NativeWriteIntentCandidatesV1,
    blobs: Vec<(NativeIntentBlobV1, Vec<u8>)>,
    state: V,
}
impl NativePreparedSupersedingIntentV1 {
    pub fn profile(&self) -> &'static str {
        NATIVE_SUPERSEDING_INTENT_PROFILE
    }
    pub fn independently_expected_before(&self) -> &NativeIntentStorePinV1 {
        &self.before
    }
    pub fn independently_expected_after(&self) -> &NativeIntentStorePinV1 {
        &self.after
    }
    pub fn original_native_intent(&self) -> &NativeIntentBlobV1 {
        &self.original_intent
    }
    pub fn replacement_candidates(&self) -> &NativeWriteIntentCandidatesV1 {
        &self.new_candidates
    }
}
fn superseding_blob(raw: &[u8]) -> Result<NativeIntentBlobV1> {
    if raw.is_empty() || raw.len() > MAX_BLOB {
        return Err(NativeIntentStoreError::Capacity);
    }
    Ok(NativeIntentBlobV1 {
        bytes: raw.len() as u64,
        sha256: sha256(raw),
    })
}
fn superseding_pin_value(pin: &NativeIntentStorePinV1) -> V {
    obj(&[
        ("root", text(&pin.canonical_root.to_string_lossy())),
        ("contextSha256", text(&hex(&sha256(&pin.context)))),
        ("sequence", text(&pin.sequence.to_string())),
        ("stateSha256", text(&hex(&pin.state_sha256))),
    ])
}
fn superseding_state(permit: &NativeCommandIntentPermitV1, intent: &NativeIntentBlobV1) -> V {
    obj(&[
        ("kind", text(SUPERSEDING_KIND)),
        ("operationProfile", text(NATIVE_SUPERSEDING_INTENT_PROFILE)),
        ("command", blob_value(&permit.command)),
        ("before", permit.before.clone()),
        ("receiptBudget", text(&permit.receipt_budget.to_string())),
        ("nativeIntent", blob_value(intent)),
        ("previousAck", permit.previous_ack.clone()),
    ])
}
fn mark_superseding_state(state: &mut V) -> Result<()> {
    let V::Object(fields) = state else {
        return Err(NativeIntentStoreError::InvalidRecord);
    };
    let count = fields
        .iter()
        .filter(|(key, _)| key == "operationProfile")
        .count();
    if count == 0 {
        fields.push((
            "operationProfile".into(),
            text(NATIVE_SUPERSEDING_INTENT_PROFILE),
        ))
    } else if count != 1
        || fields
            .iter()
            .find(|(key, _)| key == "operationProfile")
            .map(|(_, value)| value)
            != Some(&text(NATIVE_SUPERSEDING_INTENT_PROFILE))
    {
        return Err(NativeIntentStoreError::InvalidRecord);
    }
    Ok(())
}
fn replacement_read_context(
    retained: &V,
    original: &NativePreparedValidityWriteV1,
    replacement: &NativePreparedValidityWriteV1,
) -> Result<V> {
    let original_read = original.durable_read_context();
    if *retained == original_read {
        return Ok(replacement.durable_read_context());
    }
    let V::Object(fields) = retained else {
        return Err(NativeIntentStoreError::WrongContext);
    };
    if fields.iter().filter(|(key, _)| key == "actualRead").count() != 1
        || retained.get("actualRead") != original_read
    {
        return Err(NativeIntentStoreError::WrongContext);
    }
    // Every existing financial/control field, value, and field order is retained.
    Ok(V::Object(
        fields
            .iter()
            .map(|(key, value)| {
                (
                    key.clone(),
                    if key == "actualRead" {
                        replacement.durable_read_context()
                    } else {
                        value.clone()
                    },
                )
            })
            .collect(),
    ))
}
fn superseding_blob_reference(value: &V) -> Result<NativeIntentBlobV1> {
    let V::String(count) = value.get("bytes") else {
        return Err(NativeIntentStoreError::InvalidRecord);
    };
    let count: u64 = count
        .parse()
        .map_err(|_| NativeIntentStoreError::InvalidRecord)?;
    let V::String(hash) = value.get("sha256") else {
        return Err(NativeIntentStoreError::InvalidRecord);
    };
    if hash.len() != 64
        || !hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(NativeIntentStoreError::InvalidRecord);
    }
    let mut digest = [0; 32];
    for (at, slot) in digest.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&hash[at * 2..at * 2 + 2], 16)
            .map_err(|_| NativeIntentStoreError::InvalidRecord)?;
    }
    let receipt = NativeIntentBlobV1 {
        bytes: count,
        sha256: digest,
    };
    if blob_value(&receipt) != *value {
        return Err(NativeIntentStoreError::InvalidRecord);
    }
    Ok(receipt)
}
impl NativeIntentStoreV1 {
    pub fn open(
        root: &Path,
        namespace: Namespace,
        expected: &NativeIntentStorePinV1,
        filesystem: SourceDurableFilesystem,
    ) -> Result<Self> {
        Self::open_profile(root, namespace, expected, filesystem, false)
    }
    /// Explicit profile selection; legacy open never admits this marker, even
    /// after native resolution, command acceptance, or Idle acknowledgement.
    pub fn open_superseding_profile(
        root: &Path,
        namespace: Namespace,
        expected: &NativeIntentStorePinV1,
        filesystem: SourceDurableFilesystem,
    ) -> Result<Self> {
        Self::open_profile(root, namespace, expected, filesystem, true)
    }
    pub fn open_synced_superseding_intent(
        root: &Path,
        namespace: Namespace,
        prepared: &NativePreparedSupersedingIntentV1,
        filesystem: SourceDurableFilesystem,
    ) -> Result<Self> {
        let opened = Self::open_superseding_profile(root, namespace, &prepared.after, filesystem)?;
        if opened.state()? != prepared.state {
            return Err(NativeIntentStoreError::InvalidRecord);
        }
        for (expected, raw) in &prepared.blobs {
            if opened.read_blob(expected)? != *raw {
                return Err(NativeIntentStoreError::InvalidRecord);
            }
        }
        opened.require_superseding_lineage(
            &prepared.new_intent,
            &prepared.command,
            &prepared.new_candidates.raw_mutations,
        )?;
        Ok(opened)
    }
    fn require_superseding_lineage(
        &self,
        start: &NativeIntentBlobV1,
        command: &NativeIntentBlobV1,
        mutations: &[u8],
    ) -> Result<usize> {
        let mut next = start.clone();
        let mutation_blob = superseding_blob(mutations)?;
        let mut seen = std::collections::BTreeSet::new();
        let mut child: Option<V> = None;
        for _ in 0..1024 {
            if !seen.insert(next.sha256) {
                return Err(NativeIntentStoreError::InvalidRecord);
            }
            let body = parse(&self.read_blob(&next)?)?;
            if body.get("command") != blob_value(command)
                || body.get("mutations") != blob_value(&mutation_blob)
                || self.read_blob(&mutation_blob)? != mutations
            {
                return Err(NativeIntentStoreError::InvalidRecord);
            }
            if let Some(child) = child {
                let predecessor = body.get("candidates");
                let current = child.get("candidates");
                if child.get("originalBefore") != predecessor.get("before")
                    || child.get("originalPost") != predecessor.get("post").get("append")
                    || child.get("replacementBefore") != current.get("before")
                    || child.get("replacementPost") != current.get("post").get("append")
                    || predecessor.get("identity") != current.get("identity")
                    || predecessor.get("catalog") != current.get("catalog")
                    || body.get("previousNativePrefix") != child.get("previousNativePrefix")
                {
                    return Err(NativeIntentStoreError::InvalidRecord);
                }
            }
            match body.get("profile") {
                V::Undefined => return Ok(seen.len()),
                V::String(profile) if profile == NATIVE_SUPERSEDING_INTENT_PROFILE => {
                    next = superseding_blob_reference(&body.get("supersedes"))?;
                    child = Some(body);
                }
                _ => return Err(NativeIntentStoreError::InvalidRecord),
            }
        }
        Err(NativeIntentStoreError::Capacity)
    }
    fn checked_superseding_predecessor(
        &self,
        permit: &NativeCommandIntentPermitV1,
    ) -> Result<(NativeIntentBlobV1, NativeWriteIntentCandidatesV1)> {
        self.check(permit)?;
        let (intent, candidates) = permit
            .pending_native
            .as_ref()
            .ok_or(NativeIntentStoreError::WrongPhase)?;
        let state = self.state()?;
        let mut expected = if state.get("kind") == text("NativeIntent") {
            obj(&[
                ("kind", text("NativeIntent")),
                ("command", blob_value(&permit.command)),
                ("before", permit.before.clone()),
                ("receiptBudget", text(&permit.receipt_budget.to_string())),
                ("nativeIntent", blob_value(intent)),
                ("previousAck", permit.previous_ack.clone()),
            ])
        } else if state.get("kind") == text(SUPERSEDING_KIND) {
            superseding_state(permit, intent)
        } else {
            return Err(NativeIntentStoreError::WrongPhase);
        };
        if self.superseding_profile {
            mark_superseding_state(&mut expected)?
        }
        if state != expected {
            return Err(NativeIntentStoreError::InvalidRecord);
        }
        let raw = parse(&self.read_blob(intent)?)?;
        let mutation_blob = superseding_blob(&candidates.raw_mutations)?;
        if raw.get("command") != blob_value(&permit.command)
            || raw.get("candidates") != candidates.facts
            || raw.get("mutations") != blob_value(&mutation_blob)
            || raw.get("previousNativePrefix")
                != permit.native_prefix.as_ref().map_or(V::Null, blob_value)
            || self.read_blob(&mutation_blob)? != candidates.raw_mutations
            || state.get("kind") == text(SUPERSEDING_KIND)
                && raw.get("profile") != text(NATIVE_SUPERSEDING_INTENT_PROFILE)
        {
            return Err(NativeIntentStoreError::InvalidRecord);
        }
        self.require_superseding_lineage(intent, &permit.command, &candidates.raw_mutations)?;
        Ok((intent.clone(), candidates.clone()))
    }
    /// Pure preparation: retain expected control-post before any sync attempt.
    pub fn prepare_superseding_native_intent(
        &self,
        permit: &NativeCommandIntentPermitV1,
        original: &NativePreparedValidityWriteV1,
        reopened: &SourceCompactIndex,
        observation: &NativePreparedValidityRecoveryObservationV1,
        replacement: &NativePreparedValidityWriteV1,
        mapping: &NativePreparedValiditySupersessionV1,
    ) -> Result<NativePreparedSupersedingIntentV1> {
        let (original_intent, original_candidates) =
            self.checked_superseding_predecessor(permit)?;
        if self.require_superseding_lineage(
            &original_intent,
            &permit.command,
            &original_candidates.raw_mutations,
        )? >= 1024
        {
            return Err(NativeIntentStoreError::Capacity);
        }
        let observed = reopened
            .require_actual_native_supersession(original, observation, replacement, mapping)
            .map_err(|_| NativeIntentStoreError::WrongContext)?;
        let retained = original_candidates.facts.get("readContext");
        if NativeWriteIntentCandidatesV1::from_prepared_append(
            original.append_plan(),
            retained.clone(),
        )? != original_candidates
        {
            return Err(NativeIntentStoreError::WrongContext);
        }
        let context = replacement_read_context(&retained, original, replacement)?;
        let new_candidates = NativeWriteIntentCandidatesV1::from_prepared_append(
            replacement.append_plan(),
            context,
        )?;
        if new_candidates.raw_mutations != original_candidates.raw_mutations {
            return Err(NativeIntentStoreError::WrongContext);
        }
        let mutations = superseding_blob(&new_candidates.raw_mutations)?;
        let before = self.pin()?;
        let raw = bytes(&obj(&[
            ("profile", text(NATIVE_SUPERSEDING_INTENT_PROFILE)),
            ("command", blob_value(&permit.command)),
            ("candidates", new_candidates.facts.clone()),
            ("mutations", blob_value(&mutations)),
            (
                "previousNativePrefix",
                permit.native_prefix.as_ref().map_or(V::Null, blob_value),
            ),
            ("supersedes", blob_value(&original_intent)),
            ("predecessorControl", superseding_pin_value(&before)),
            ("verifiedReopen", observed),
            (
                "originalBefore",
                native_pin_value(mapping.original_before()),
            ),
            ("originalPost", native_pin_value(mapping.original_after())),
            (
                "replacementBefore",
                native_pin_value(mapping.replacement_before()),
            ),
            (
                "replacementPost",
                native_pin_value(mapping.replacement_after()),
            ),
        ]))?;
        let new_intent = superseding_blob(&raw)?;
        let state = superseding_state(permit, &new_intent);
        let mut after = before.clone();
        after.sequence = after
            .sequence
            .checked_add(1)
            .ok_or(NativeIntentStoreError::Capacity)?;
        after.state_sha256 = sha256(&bytes(&state)?);
        Ok(NativePreparedSupersedingIntentV1 {
            token: self.token,
            before,
            after,
            original_intent,
            original_candidates,
            command: permit.command.clone(),
            new_intent,
            blobs: vec![
                (mutations, new_candidates.raw_mutations.clone()),
                (superseding_blob(&raw)?, raw),
            ],
            new_candidates,
            state,
        })
    }
    /// This syncs only the new intent. Fresh native append is a separate action.
    pub fn sync_prepared_superseding_native_intent(
        &mut self,
        permit: &mut NativeCommandIntentPermitV1,
        prepared: &NativePreparedSupersedingIntentV1,
    ) -> Result<NativeIntentStorePinV1> {
        let pending = self.checked_superseding_predecessor(permit)?;
        if prepared.token != self.token
            || self.pin()? != prepared.before
            || pending
                != (
                    prepared.original_intent.clone(),
                    prepared.original_candidates.clone(),
                )
            || permit.command != prepared.command
            || prepared.state != superseding_state(permit, &prepared.new_intent)
        {
            return Err(NativeIntentStoreError::ForeignOrReopenedPermit);
        }
        for (expected, raw) in &prepared.blobs {
            if self.write_blob(raw)? != *expected {
                return Err(NativeIntentStoreError::InvalidRecord);
            }
        }
        self.superseding_profile = true;
        self.write_state(prepared.state.clone())?;
        if self.pin()? != prepared.after {
            self.poisoned = true;
            return Err(NativeIntentStoreError::WrongPin);
        }
        permit.pending_native =
            Some((prepared.new_intent.clone(), prepared.new_candidates.clone()));
        permit.pin = prepared.after.clone();
        Ok(permit.pin.clone())
    }
    /// Rebinds only this retained actual permit after an independently expected
    /// synced-post reopen. Never constructs a permit from a stored record.
    pub fn recover_synced_superseding_native_intent(
        &self,
        prepared: &NativePreparedSupersedingIntentV1,
        permit: &mut NativeCommandIntentPermitV1,
    ) -> Result<NativeIntentStorePinV1> {
        self.health()?;
        if !self.superseding_profile
            || self.pin()? != prepared.after
            || permit.token != prepared.token
            || permit.pin != prepared.before
            || permit.command != prepared.command
            || permit.pending_native.as_ref()
                != Some(&(
                    prepared.original_intent.clone(),
                    prepared.original_candidates.clone(),
                ))
            || self.state()? != prepared.state
            || prepared.state != superseding_state(permit, &prepared.new_intent)
        {
            return Err(NativeIntentStoreError::ForeignOrReopenedPermit);
        }
        for (expected, raw) in &prepared.blobs {
            if self.read_blob(expected)? != *raw {
                return Err(NativeIntentStoreError::InvalidRecord);
            }
        }
        self.require_superseding_lineage(
            &prepared.new_intent,
            &prepared.command,
            &prepared.new_candidates.raw_mutations,
        )?;
        permit.token = self.token;
        permit.pin = prepared.after.clone();
        permit.pending_native =
            Some((prepared.new_intent.clone(), prepared.new_candidates.clone()));
        Ok(permit.pin.clone())
    }
    /// Only actual complete replacement POST can acknowledge this native await.
    pub fn sync_superseding_native_prefix_ack(
        &mut self,
        permit: &mut NativeCommandIntentPermitV1,
        index: &SourceCompactIndex,
        replacement: &NativePreparedValidityWriteV1,
    ) -> Result<NativeIntentStorePinV1> {
        if self.state()?.get("kind") != text(SUPERSEDING_KIND) {
            return Err(NativeIntentStoreError::WrongPhase);
        }
        let (intent, candidates) = self.checked_superseding_predecessor(permit)?;
        let retained = candidates.facts.get("readContext");
        if replacement_read_context(&retained, replacement, replacement)? != retained
            || NativeWriteIntentCandidatesV1::from_prepared_append(
                replacement.append_plan(),
                retained,
            )? != candidates
        {
            return Err(NativeIntentStoreError::WrongContext);
        }
        index
            .resolve_prepared_validity_post(replacement)
            .map_err(|_| NativeIntentStoreError::WrongPin)?;
        let observed = index
            .native_observed_append_binding()
            .map_err(|_| NativeIntentStoreError::WrongPin)?;
        if observed.binding != candidates.post {
            return Err(NativeIntentStoreError::WrongPin);
        }
        let prefix = self.write_blob(&bytes(&obj(&[
            ("profile", text(NATIVE_SUPERSEDING_INTENT_PROFILE)),
            ("nativeIntent", blob_value(&intent)),
            ("observedPost", observed.binding),
            (
                "previousNativePrefix",
                permit.native_prefix.as_ref().map_or(V::Null, blob_value),
            ),
        ]))?)?;
        self.write_state(obj(&[
            ("kind", text("CommandIntent")),
            ("command", blob_value(&permit.command)),
            ("before", permit.before.clone()),
            ("receiptBudget", text(&permit.receipt_budget.to_string())),
            ("nativePrefix", blob_value(&prefix)),
            ("previousAck", permit.previous_ack.clone()),
        ]))?;
        permit.native_prefix = Some(prefix);
        permit.pending_native = None;
        permit.pin = self.pin()?;
        Ok(permit.pin.clone())
    }
}
