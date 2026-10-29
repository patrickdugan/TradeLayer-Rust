/// API-only preparation profile; existing original/superseding ACK bytes stay
/// unchanged. This never resolves the source await or imports financial state.
pub const NATIVE_PREPARED_PREFIX_ACK_PROFILE: &str = "TL-MAIN-NATIVE-PREPARED-PREFIX-ACK-13-1";
pub struct NativePreparedPrefixAckV1 {
    token: u64,
    before: NativeIntentStorePinV1,
    after: NativeIntentStorePinV1,
    command: NativeIntentBlobV1,
    pending: (NativeIntentBlobV1, NativeWriteIntentCandidatesV1),
    previous_prefix: Option<NativeIntentBlobV1>,
    prefix: NativeIntentBlobV1,
    raw: Vec<u8>,
    state: V,
    superseding_profile: bool,
}
impl NativePreparedPrefixAckV1 {
    pub fn profile(&self) -> &'static str {
        NATIVE_PREPARED_PREFIX_ACK_PROFILE
    }
    pub fn independently_expected_before(&self) -> &NativeIntentStorePinV1 {
        &self.before
    }
    pub fn independently_expected_after(&self) -> &NativeIntentStorePinV1 {
        &self.after
    }
    pub fn original_native_intent(&self) -> &NativeIntentBlobV1 {
        &self.pending.0
    }
    pub fn acknowledged_prefix(&self) -> &NativeIntentBlobV1 {
        &self.prefix
    }
}
fn prepared_prefix_ack_state(
    permit: &NativeCommandIntentPermitV1,
    prefix: &NativeIntentBlobV1,
    explicit_profile: bool,
) -> Result<V> {
    let mut state = obj(&[
        ("kind", text("CommandIntent")),
        ("command", blob_value(&permit.command)),
        ("before", permit.before.clone()),
        ("receiptBudget", text(&permit.receipt_budget.to_string())),
        ("nativePrefix", blob_value(prefix)),
        ("previousAck", permit.previous_ack.clone()),
    ]);
    if explicit_profile {
        mark_superseding_state(&mut state)?
    }
    Ok(state)
}
impl NativeIntentStoreV1 {
    /// One independently expected synced POST only; never repairs a tail or
    /// selects a newer state from unknown bytes. No new permit is constructed.
    pub fn open_synced_native_prefix_ack(
        root: &Path,
        namespace: Namespace,
        prepared: &NativePreparedPrefixAckV1,
        filesystem: SourceDurableFilesystem,
    ) -> Result<Self> {
        let opened = if prepared.superseding_profile {
            Self::open_superseding_profile(root, namespace, &prepared.after, filesystem)?
        } else {
            Self::open(root, namespace, &prepared.after, filesystem)?
        };
        opened.require_prepared_prefix_ack_post(prepared)?;
        Ok(opened)
    }
    fn require_prepared_prefix_ack_post(&self, prepared: &NativePreparedPrefixAckV1) -> Result<()> {
        self.health()?;
        if self.pin()? != prepared.after
            || self.state()? != prepared.state
            || self.superseding_profile != prepared.superseding_profile
            || self.read_blob(&prepared.prefix)? != prepared.raw
        {
            return Err(NativeIntentStoreError::WrongPin);
        }
        self.require_superseding_lineage(
            &prepared.pending.0,
            &prepared.command,
            &prepared.pending.1.raw_mutations,
        )?;
        Ok(())
    }
    /// All fallible checks precede changing the retained actual same-command
    /// permit. Source frame completion remains a separate controller action.
    pub fn recover_synced_native_prefix_ack(
        &self,
        prepared: &NativePreparedPrefixAckV1,
        permit: &mut NativeCommandIntentPermitV1,
    ) -> Result<NativeIntentStorePinV1> {
        self.require_prepared_prefix_ack_post(prepared)?;
        if permit.token != prepared.token
            || permit.pin != prepared.before
            || permit.command != prepared.command
            || permit.pending_native.as_ref() != Some(&prepared.pending)
            || permit.native_prefix != prepared.previous_prefix
            || prepared.state
                != prepared_prefix_ack_state(
                    permit,
                    &prepared.prefix,
                    prepared.superseding_profile,
                )?
        {
            return Err(NativeIntentStoreError::ForeignOrReopenedPermit);
        }
        permit.token = self.token;
        permit.pin = prepared.after.clone();
        permit.native_prefix = Some(prepared.prefix.clone());
        permit.pending_native = None;
        Ok(permit.pin.clone())
    }
    pub fn prepare_native_prefix_ack(
        &self,
        permit: &NativeCommandIntentPermitV1,
        index: &SourceCompactIndex,
        write: &NativePreparedValidityWriteV1,
    ) -> Result<NativePreparedPrefixAckV1> {
        let pending = self.checked_superseding_predecessor(permit)?;
        let state = self.state()?;
        let superseding = state.get("kind") == text(SUPERSEDING_KIND);
        if !superseding && state.get("kind") != text("NativeIntent") {
            return Err(NativeIntentStoreError::WrongPhase);
        }
        let context = pending.1.facts.get("readContext");
        if replacement_read_context(&context, write, write)? != context
            || NativeWriteIntentCandidatesV1::from_prepared_append(write.append_plan(), context)?
                != pending.1
        {
            return Err(NativeIntentStoreError::WrongContext);
        }
        index
            .resolve_prepared_validity_post(write)
            .map_err(|_| NativeIntentStoreError::WrongPin)?;
        let observed = index
            .native_observed_append_binding()
            .map_err(|_| NativeIntentStoreError::WrongPin)?;
        if observed.binding != pending.1.post {
            return Err(NativeIntentStoreError::WrongPin);
        }
        let mut fields = vec![];
        if superseding {
            fields.push(("profile", text(NATIVE_SUPERSEDING_INTENT_PROFILE)))
        }
        fields.extend([
            ("nativeIntent", blob_value(&pending.0)),
            ("observedPost", observed.binding),
            (
                "previousNativePrefix",
                permit.native_prefix.as_ref().map_or(V::Null, blob_value),
            ),
        ]);
        let raw = bytes(&obj(&fields))?;
        let prefix = superseding_blob(&raw)?;
        let next = prepared_prefix_ack_state(permit, &prefix, self.superseding_profile)?;
        let before = self.pin()?;
        let mut after = before.clone();
        after.sequence = after
            .sequence
            .checked_add(1)
            .ok_or(NativeIntentStoreError::Capacity)?;
        after.state_sha256 = sha256(&bytes(&next)?);
        Ok(NativePreparedPrefixAckV1 {
            token: self.token,
            before,
            after,
            command: permit.command.clone(),
            pending,
            previous_prefix: permit.native_prefix.clone(),
            prefix,
            raw,
            state: next,
            superseding_profile: self.superseding_profile,
        })
    }
    pub fn sync_prepared_native_prefix_ack(
        &mut self,
        permit: &mut NativeCommandIntentPermitV1,
        prepared: &NativePreparedPrefixAckV1,
    ) -> Result<NativeIntentStorePinV1> {
        let pending = self.checked_superseding_predecessor(permit)?;
        if prepared.token != self.token
            || prepared.before != self.pin()?
            || prepared.command != permit.command
            || prepared.pending != pending
            || prepared.previous_prefix != permit.native_prefix
            || prepared.superseding_profile != self.superseding_profile
            || prepared.state
                != prepared_prefix_ack_state(permit, &prepared.prefix, self.superseding_profile)?
        {
            return Err(NativeIntentStoreError::ForeignOrReopenedPermit);
        }
        if self.write_blob(&prepared.raw)? != prepared.prefix {
            return Err(NativeIntentStoreError::InvalidRecord);
        }
        self.write_state(prepared.state.clone())?;
        if self.pin()? != prepared.after {
            self.poisoned = true;
            return Err(NativeIntentStoreError::WrongPin);
        }
        permit.native_prefix = Some(prepared.prefix.clone());
        permit.pending_native = None;
        permit.pin = prepared.after.clone();
        Ok(permit.pin.clone())
    }
}
