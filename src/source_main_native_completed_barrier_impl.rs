// API-only sealed barriers; original Accepted/Ack wire bytes stay unchanged.
pub const NATIVE_PREPARED_COMPLETED_ACCEPTED_PROFILE: &str =
    "TL-MAIN-PREPARED-COMPLETED-ACCEPTED-13-1";
pub const NATIVE_PREPARED_COMPLETED_COMMAND_ACK_PROFILE: &str =
    "TL-MAIN-PREPARED-COMPLETED-COMMAND-ACK-13-1";
struct NativeCompletedBarrierLineageV1 {
    token: u64,
    before: NativeIntentStorePinV1,
    after: NativeIntentStorePinV1,
    command: NativeIntentBlobV1,
    command_before: V,
    receipt_budget: usize,
    native_prefix: Option<NativeIntentBlobV1>,
    previous_ack: V,
    before_state: V,
    after_state: V,
    superseding_profile: bool,
}
pub struct NativePreparedCompletedAcceptedV1 {
    lineage: NativeCompletedBarrierLineageV1,
    accepted: NativeIntentBlobV1,
    raw: Vec<u8>,
    context: V,
}
pub struct NativePreparedCompletedCommandAckV1 {
    lineage: NativeCompletedBarrierLineageV1,
    accepted: NativeIntentBlobV1,
    accepted_raw: Vec<u8>,
    ack: NativeIntentBlobV1,
    raw: Vec<u8>,
    context: V,
}
impl NativePreparedCompletedAcceptedV1 {
    pub fn profile(&self) -> &'static str {
        NATIVE_PREPARED_COMPLETED_ACCEPTED_PROFILE
    }
    pub fn independently_expected_before(&self) -> &NativeIntentStorePinV1 {
        &self.lineage.before
    }
    pub fn independently_expected_after(&self) -> &NativeIntentStorePinV1 {
        &self.lineage.after
    }
    pub fn accepted_blob(&self) -> &NativeIntentBlobV1 {
        &self.accepted
    }
    pub fn accepted_bytes(&self) -> &[u8] {
        &self.raw
    }
    pub fn after_context(&self) -> &V {
        &self.context
    }
    pub fn command(&self) -> &NativeIntentBlobV1 {
        &self.lineage.command
    }
    pub fn native_prefix(&self) -> Option<&NativeIntentBlobV1> {
        self.lineage.native_prefix.as_ref()
    }
    pub fn previous_ack(&self) -> &V {
        &self.lineage.previous_ack
    }
}
impl NativePreparedCompletedCommandAckV1 {
    pub fn profile(&self) -> &'static str {
        NATIVE_PREPARED_COMPLETED_COMMAND_ACK_PROFILE
    }
    pub fn independently_expected_before(&self) -> &NativeIntentStorePinV1 {
        &self.lineage.before
    }
    pub fn independently_expected_after(&self) -> &NativeIntentStorePinV1 {
        &self.lineage.after
    }
    pub fn accepted_blob(&self) -> &NativeIntentBlobV1 {
        &self.accepted
    }
    pub fn acknowledgement(&self) -> &NativeIntentBlobV1 {
        &self.ack
    }
    pub fn acknowledgement_bytes(&self) -> &[u8] {
        &self.raw
    }
    pub fn after_context(&self) -> &V {
        &self.context
    }
    pub fn command(&self) -> &NativeIntentBlobV1 {
        &self.lineage.command
    }
    pub fn native_prefix(&self) -> Option<&NativeIntentBlobV1> {
        self.lineage.native_prefix.as_ref()
    }
    pub fn previous_ack(&self) -> &V {
        &self.lineage.previous_ack
    }
}
fn completed_command_intent_state(permit: &NativeCommandIntentPermitV1) -> V {
    let mut fields = vec![
        ("kind", text("CommandIntent")),
        ("command", blob_value(&permit.command)),
        ("before", permit.before.clone()),
        ("receiptBudget", text(&permit.receipt_budget.to_string())),
    ];
    // Initial sync_command_intent omits this field. A real native-prefix ACK
    // adds it only once a prefix exists; preserve both exact original shapes.
    if let Some(prefix) = &permit.native_prefix {
        fields.push(("nativePrefix", blob_value(prefix)));
    }
    fields.push(("previousAck", permit.previous_ack.clone()));
    obj(&fields)
}
fn completed_accepted_state(
    permit: &NativeCommandIntentPermitV1,
    accepted: &NativeIntentBlobV1,
    context: &V,
) -> V {
    obj(&[
        ("kind", text("Accepted")),
        ("command", blob_value(&permit.command)),
        ("before", permit.before.clone()),
        (
            "nativePrefix",
            permit.native_prefix.as_ref().map_or(V::Null, blob_value),
        ),
        ("accepted", blob_value(accepted)),
        ("after", context.clone()),
        ("previousAck", permit.previous_ack.clone()),
    ])
}
fn completed_command_ack_bytes(
    permit: &NativeCommandIntentPermitV1,
    accepted: &NativeIntentBlobV1,
    context: &V,
) -> Result<Vec<u8>> {
    bytes(&obj(&[
        ("command", blob_value(&permit.command)),
        (
            "nativePrefix",
            permit.native_prefix.as_ref().map_or(V::Null, blob_value),
        ),
        ("acceptedBlob", blob_value(accepted)),
        ("after", context.clone()),
        ("previousAck", permit.previous_ack.clone()),
    ]))
}
impl NativeIntentStoreV1 {
    fn completed_mark_state(&self, mut state: V) -> Result<V> {
        if self.superseding_profile {
            mark_superseding_state(&mut state)?;
        }
        Ok(state)
    }
    fn completed_check_references(&self, lineage: &NativeCompletedBarrierLineageV1) -> Result<()> {
        self.read_blob(&lineage.command)?;
        if let Some(prefix) = &lineage.native_prefix {
            self.read_blob(prefix)?;
        }
        if !matches!(lineage.previous_ack, V::Null | V::Undefined) {
            self.read_blob(&superseding_blob_reference(&lineage.previous_ack)?)?;
        }
        Ok(())
    }
    fn completed_lineage(
        &self,
        permit: &NativeCommandIntentPermitV1,
        before_state: V,
        after_state: V,
    ) -> Result<NativeCompletedBarrierLineageV1> {
        self.check(permit)?;
        if permit.pending_native.is_some() || self.state()? != before_state {
            return Err(NativeIntentStoreError::WrongPhase);
        }
        if permit.reservation.capacity() < permit.receipt_budget {
            return Err(NativeIntentStoreError::Capacity);
        }
        let before = self.pin()?;
        let mut after = before.clone();
        after.sequence = after
            .sequence
            .checked_add(1)
            .ok_or(NativeIntentStoreError::Capacity)?;
        after.state_sha256 = sha256(&bytes(&after_state)?);
        let lineage = NativeCompletedBarrierLineageV1 {
            token: self.token,
            before,
            after,
            command: permit.command.clone(),
            command_before: permit.before.clone(),
            receipt_budget: permit.receipt_budget,
            native_prefix: permit.native_prefix.clone(),
            previous_ack: permit.previous_ack.clone(),
            before_state,
            after_state,
            superseding_profile: self.superseding_profile,
        };
        self.completed_check_references(&lineage)?;
        Ok(lineage)
    }
    fn completed_require_retained_permit(
        &self,
        lineage: &NativeCompletedBarrierLineageV1,
        permit: &NativeCommandIntentPermitV1,
    ) -> Result<()> {
        if permit.token != lineage.token
            || permit.pin != lineage.before
            || permit.command != lineage.command
            || permit.before != lineage.command_before
            || permit.receipt_budget != lineage.receipt_budget
            || permit.native_prefix != lineage.native_prefix
            || permit.previous_ack != lineage.previous_ack
            || permit.pending_native.is_some()
            || permit.reservation.capacity() < lineage.receipt_budget
        {
            return Err(NativeIntentStoreError::ForeignOrReopenedPermit);
        }
        Ok(())
    }
    fn completed_require_before(
        &self,
        lineage: &NativeCompletedBarrierLineageV1,
        permit: &NativeCommandIntentPermitV1,
    ) -> Result<()> {
        self.check(permit)?;
        self.completed_require_retained_permit(lineage, permit)?;
        if self.token != lineage.token
            || self.pin()? != lineage.before
            || self.state()? != lineage.before_state
            || self.superseding_profile != lineage.superseding_profile
        {
            return Err(NativeIntentStoreError::WrongPin);
        }
        self.completed_check_references(lineage)
    }
    fn completed_require_post(&self, lineage: &NativeCompletedBarrierLineageV1) -> Result<()> {
        if self.pin()? != lineage.after
            || self.state()? != lineage.after_state
            || self.superseding_profile != lineage.superseding_profile
        {
            return Err(NativeIntentStoreError::WrongPin);
        }
        self.completed_check_references(lineage)
    }
    fn completed_open_post(
        root: &Path,
        namespace: Namespace,
        lineage: &NativeCompletedBarrierLineageV1,
        filesystem: SourceDurableFilesystem,
    ) -> Result<Self> {
        if lineage.superseding_profile {
            Self::open_superseding_profile(root, namespace, &lineage.after, filesystem)
        } else {
            Self::open(root, namespace, &lineage.after, filesystem)
        }
    }
    fn completed_sync_state(&mut self, lineage: &NativeCompletedBarrierLineageV1) -> Result<()> {
        self.write_state(lineage.after_state.clone())?;
        if self.pin()? != lineage.after {
            self.poisoned = true;
            return Err(NativeIntentStoreError::WrongPin);
        }
        Ok(())
    }
}
