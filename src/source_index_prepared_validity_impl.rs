// Metadata preparation only. All values come from an actual complete read and
// actual source update arguments; there is no financial owner or state importer.
pub const PREPARED_VALIDITY_PROFILE: &str = "TL-TXINDEX-PREPARED-VALIDITY-13-1";
pub const PREPARED_METADATA_STATE_PROFILE: &str = "TL-NATIVE-COMPLETE-KV-PREPARED-13-1";
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeMetadataStateReceiptV1 {
    pub profile: String,
    pub keys: u64,
    pub bytes: u64,
    pub chain_sha256d: [u8; 32],
    pub complete: bool,
}
impl NativeMetadataStateReceiptV1 {
    pub fn require_authenticated_admission(&self) -> std::result::Result<(), SourceRuleError> {
        Err(SourceRuleError::UnauthenticatedInputAndState)
    }
}
fn prepared_obj(fields: Vec<(&str, V)>) -> V {
    V::Object(fields.into_iter().map(|(k, v)| (k.into(), v)).collect())
}
fn prepared_text(text: impl Into<String>) -> V {
    V::String(text.into())
}
fn prepared_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn prepared_state_value(state: &NativeMetadataStateReceiptV1) -> V {
    prepared_obj(vec![
        ("profile", prepared_text(&state.profile)),
        ("keys", prepared_text(state.keys.to_string())),
        ("bytes", prepared_text(state.bytes.to_string())),
        ("chain", prepared_text(prepared_hex(&state.chain_sha256d))),
        ("complete", V::Bool(state.complete)),
    ])
}
fn prepared_snapshot_value(pin: &MetadataSnapshot) -> V {
    let p = &pin.publication;
    prepared_obj(vec![
        ("profile", prepared_text(&pin.profile)),
        ("context", prepared_text(&pin.context)),
        ("sequence", prepared_text(pin.sequence.to_string())),
        ("generation", prepared_text(p.generation.to_string())),
        (
            "parent",
            p.parent.map_or(V::Null, |n| prepared_text(n.to_string())),
        ),
        (
            "physicalCreation",
            prepared_text(p.physical_creation_sequence.to_string()),
        ),
        (
            "logicalCreation",
            prepared_text(p.creation_sequence.to_string()),
        ),
        ("manifest", prepared_text(prepared_hex(&p.manifest_sha256d))),
    ])
}
fn prepared_metadata_state(
    data: &CompactReadSnapshot,
    mutations: &[Mutation],
) -> Result<NativeMetadataStateReceiptV1> {
    let mut overlay = std::collections::BTreeMap::<&str, Option<&[u8]>>::new();
    for mutation in mutations {
        let (key, value) = match mutation {
            Mutation::Put { key, value } => (key.as_str(), Some(value.as_slice())),
            Mutation::Delete { key } => (key.as_str(), None),
        };
        if overlay.insert(key, value).is_some() {
            return Err(CompactIndexError::Corrupt);
        }
    }
    let mut receipt = NativeMetadataStateReceiptV1 {
        profile: PREPARED_METADATA_STATE_PROFILE.into(),
        keys: 0,
        bytes: 0,
        chain_sha256d: crate::hash256::double_sha256(PREPARED_METADATA_STATE_PROFILE.as_bytes()),
        complete: false,
    };
    let mut emit = |key: &str, value: &[u8]| -> Result<()> {
        let mut raw = receipt.chain_sha256d.to_vec();
        raw.extend_from_slice(&receipt.keys.to_be_bytes());
        raw.extend_from_slice(&(key.len() as u32).to_be_bytes());
        raw.extend_from_slice(key.as_bytes());
        raw.extend_from_slice(&(value.len() as u64).to_be_bytes());
        raw.extend_from_slice(value);
        receipt.chain_sha256d = crate::hash256::double_sha256(&raw);
        receipt.keys = receipt
            .keys
            .checked_add(1)
            .ok_or(CompactIndexError::Limit)?;
        receipt.bytes = receipt
            .bytes
            .checked_add(key.len() as u64)
            .and_then(|n| n.checked_add(value.len() as u64))
            .ok_or(CompactIndexError::Limit)?;
        Ok(())
    };
    let mut changes = overlay.into_iter().peekable();
    for (key, value) in data.entries() {
        while changes.peek().is_some_and(|(next, _)| *next < key) {
            let (key, value) = changes.next().expect("peeked change");
            if let Some(value) = value {
                emit(key, value)?;
            }
        }
        if changes.peek().is_some_and(|(next, _)| *next == key) {
            let (key, value) = changes.next().expect("peeked equal key");
            if let Some(value) = value {
                emit(key, value)?;
            }
        } else {
            emit(key, value)?;
        }
    }
    for (key, value) in changes {
        if let Some(value) = value {
            emit(key, value)?;
        }
    }
    receipt.complete = true;
    Ok(receipt)
}
struct NativeMetadataPutPlanV1 {
    expected: MetadataSnapshot,
    directory: Directory,
    append: crate::compact_journal::NativeAppendPlan,
}
pub enum NativePreparedValidityQueryV1 {
    Complete(Box<StreamedValidityEvaluation>),
    Selected(Box<NativePreparedValiditySelectionV1>),
}
pub enum NativePreparedValidityOperationV1 {
    Complete(Box<StreamedValidityEvaluation>),
    Write(Box<NativePreparedValidityWriteV1>),
}
pub struct NativePreparedValiditySelectionV1 {
    read: SourceIndexReadSnapshot,
    query: PrefixQueryReceipt,
    selected: V,
    arguments: Vec<V>,
    context: (String, std::path::PathBuf),
    before_state: NativeMetadataStateReceiptV1,
}
pub struct NativePreparedValidityWriteV1 {
    read: SourceIndexReadSnapshot,
    query: PrefixQueryReceipt,
    arguments: Vec<V>,
    context: (String, std::path::PathBuf),
    put: NativeMetadataPutPlanV1,
    before_state: NativeMetadataStateReceiptV1,
    after_state: NativeMetadataStateReceiptV1,
    recovery_before: crate::compact_journal::VerifiedCompactPrefixV1,
}
impl NativePreparedValiditySelectionV1 {
    pub fn query(&self) -> &PrefixQueryReceipt {
        &self.query
    }
    pub fn update_arguments(&self) -> &[V] {
        &self.arguments
    }
    /// Source update failure facts are handled by the actual Runtime before
    /// calling prepare_write. This method only records a genuine source throw.
    pub fn source_rejected(
        self,
        index: &SourceCompactIndex,
        error: SourceException,
    ) -> Result<StreamedValidityEvaluation> {
        if index.native_store_context()? != self.context {
            return Err(CompactIndexError::WrongContext);
        }
        Ok(prepared_validity_evaluation(
            self.query,
            self.arguments,
            DependencyReturn::PendingThrow(error),
            index.snapshot()?,
        ))
    }
    pub fn prepare_write(
        self,
        index: &SourceCompactIndex,
        write_expected: &MetadataSnapshot,
    ) -> Result<NativePreparedValidityOperationV1> {
        index.require_snapshot(write_expected)?;
        if index.native_store_context()? != self.context
            || index.native_complete_metadata_state()? != self.before_state
        {
            return Err(CompactIndexError::WrongContext);
        }
        let mut row = self.selected;
        let V::Object(sets) = self.arguments[1].get("$set") else {
            return Err(CompactIndexError::Corrupt);
        };
        for (key, value) in sets {
            row.insert(&key, value)?;
        }
        profile_value(&row)?;
        let row = match memory_copy(&row) {
            Ok(row) => row,
            Err(error) => {
                return Ok(NativePreparedValidityOperationV1::Complete(Box::new(
                    prepared_validity_evaluation(
                        self.query,
                        self.arguments,
                        DependencyReturn::PendingThrow(error),
                        index.snapshot()?,
                    ),
                )))
            }
        };
        let put = index.native_prepare_metadata_put(write_expected, row)?;
        let after_state = prepared_metadata_state(&self.read.data, put.append.mutations())?;
        let recovery_before =
            index.native_capture_validity_recovery(&put.append, &self.before_state)?;
        Ok(NativePreparedValidityOperationV1::Write(Box::new(
            NativePreparedValidityWriteV1 {
                read: self.read,
                query: self.query,
                arguments: self.arguments,
                context: self.context,
                put,
                before_state: self.before_state,
                after_state,
                recovery_before,
            },
        )))
    }
}
impl NativePreparedValidityWriteV1 {
    pub fn require_authenticated_admission(&self) -> std::result::Result<(), SourceRuleError> {
        Err(SourceRuleError::UnauthenticatedInputAndState)
    }
    pub fn profile(&self) -> &'static str {
        PREPARED_VALIDITY_PROFILE
    }
    pub fn query(&self) -> &PrefixQueryReceipt {
        &self.query
    }
    pub fn update_arguments(&self) -> &[V] {
        &self.arguments
    }
    pub fn append_plan(&self) -> &crate::compact_journal::NativeAppendPlan {
        &self.put.append
    }
    pub fn write_expected(&self) -> &MetadataSnapshot {
        &self.put.expected
    }
    pub fn expected_before_state(&self) -> &NativeMetadataStateReceiptV1 {
        &self.before_state
    }
    pub fn expected_after_state(&self) -> &NativeMetadataStateReceiptV1 {
        &self.after_state
    }
    pub fn pinned_read(&self) -> &MetadataSnapshot {
        self.read.pin()
    }
    /// Bind complete actual read/content observations separately from original
    /// physical read/write pins. This does not authorize those inputs.
    pub fn durable_read_context(&self) -> V {
        prepared_durable_read_context(
            &self.query,
            &self.arguments,
            &self.before_state,
            &self.after_state,
            &self.put.expected,
        )
    }
}
fn prepared_validity_evaluation(
    query: PrefixQueryReceipt,
    arguments: Vec<V>,
    returned: DependencyReturn,
    post_state: MetadataSnapshot,
) -> StreamedValidityEvaluation {
    let outcome = match &returned {
        DependencyReturn::Immediate(_) | DependencyReturn::Pending(_) => {
            crate::validity_source::SourceOutcome::Returned(V::Undefined)
        }
        DependencyReturn::Throw(e) | DependencyReturn::PendingThrow(e) => {
            crate::validity_source::SourceOutcome::Thrown(e.clone())
        }
    };
    let mut call_args = vec![V::Object(vec![(
        "$database".into(),
        V::String("txIndex".into()),
    )])];
    call_args.extend(arguments);
    StreamedValidityEvaluation {
        profile: VALIDITY_STREAM_PROFILE.into(),
        query,
        outcome,
        update: Some(crate::validity_source::DependencyObservation {
            call: DependencyCall {
                target: "db.updateAsync".into(),
                arguments: call_args,
            },
            returned,
        }),
        post_state,
    }
}
impl SourceCompactIndex {
    pub fn native_complete_metadata_state(&self) -> Result<NativeMetadataStateReceiptV1> {
        prepared_metadata_state(&self.journal.read_snapshot()?, &[])
    }
    pub fn prepare_validity_read(
        &self,
        expected: &MetadataSnapshot,
        txid: V,
        ty: V,
        valid: V,
        reason: V,
    ) -> Result<NativePreparedValidityQueryV1> {
        for value in [&txid, &ty, &valid, &reason] {
            value.validate()?;
        }
        self.require_snapshot(expected)?;
        prepared_validity_read_snapshot(
            self.read_generation(expected)?,
            || self.native_store_context(),
            txid,
            ty,
            valid,
            reason,
        )
    }

    pub fn commit_prepared_validity(
        &mut self,
        plan: &NativePreparedValidityWriteV1,
    ) -> Result<StreamedValidityEvaluation> {
        self.require_live_metadata_write()?;
        self.require_snapshot(&plan.put.expected)?;
        if self.native_store_context()? != plan.context
            || self.native_complete_metadata_state()? != plan.before_state
        {
            return Err(CompactIndexError::WrongContext);
        }
        self.journal
            .commit_prepared_native_append(&plan.put.append)?;
        self.directory = plan.put.directory.clone();
        if self.native_complete_metadata_state()? != plan.after_state {
            return Err(CompactIndexError::Corrupt);
        }
        Ok(prepared_validity_evaluation(
            plan.query.clone(),
            plan.arguments.clone(),
            DependencyReturn::Pending(V::number(1.)),
            self.snapshot()?,
        ))
    }
    /// Resolve a retained original await after an independently verified reopen.
    /// Owner-token commit is deliberately not retried; complete content and
    /// original identity/location/publication/sequence must match the real plan.
    pub fn resolve_prepared_validity_post(
        &self,
        plan: &NativePreparedValidityWriteV1,
    ) -> Result<StreamedValidityEvaluation> {
        let current = self.snapshot()?;
        if self.native_store_context()? != plan.context
            || current.context != plan.put.expected.context
            || current.profile != plan.put.expected.profile
            || current.sequence != plan.put.append.after().logical_sequence
            || current.publication != plan.put.append.after().publication
            || self.native_complete_metadata_state()? != plan.after_state
            || self.directory.encode()? != plan.put.directory.encode()?
        {
            return Err(CompactIndexError::WrongContext);
        }
        Ok(prepared_validity_evaluation(
            plan.query.clone(),
            plan.arguments.clone(),
            DependencyReturn::Pending(V::number(1.)),
            current,
        ))
    }
}

fn prepared_validity_read_snapshot(
    snapshot: SourceIndexReadSnapshot,
    context: impl FnOnce() -> Result<(String, std::path::PathBuf)>,
    txid: V,
    ty: V,
    valid: V,
    reason: V,
) -> Result<NativePreparedValidityQueryV1> {
    for value in [&txid, &ty, &valid, &reason] {
        value.validate()?;
    }
    let mut cursor = snapshot.cursor();
    let mut matched = 0u64;
    let mut chain = crate::hash256::double_sha256(VALIDITY_STREAM_PROFILE.as_bytes());
    while let Some(row) = cursor.next_row()? {
        if row_id(&row)?.starts_with("tx") {
            let body = row.diagnostic_json()?;
            let mut raw = chain.to_vec();
            raw.extend_from_slice(&matched.to_be_bytes());
            raw.extend_from_slice(&(body.len() as u32).to_be_bytes());
            raw.extend_from_slice(body.as_bytes());
            chain = crate::hash256::double_sha256(&raw);
            matched += 1;
        }
    }
    let read = cursor.receipt();
    let query = PrefixQueryReceipt {
        profile: VALIDITY_STREAM_PROFILE.into(),
        predicate: "^tx".into(),
        snapshot: snapshot.pin().clone(),
        visited_rows: read.rows,
        matched_rows: matched,
        chain_sha256d: chain,
        complete: read.complete,
    };
    coercion_domain(&txid)?;
    let suffix = match source_text(&txid) {
        Ok(t) => format!("-{t}"),
        Err(error) => {
            return Ok(NativePreparedValidityQueryV1::Complete(Box::new(
                StreamedValidityEvaluation {
                    profile: VALIDITY_STREAM_PROFILE.into(),
                    query,
                    outcome: crate::validity_source::SourceOutcome::Thrown(error),
                    update: None,
                    post_state: snapshot.pin().clone(),
                },
            )))
        }
    };
    let mut selected = None;
    let mut find = snapshot.cursor();
    while let Some(row) = find.next_row()? {
        let id = row_id(&row)?;
        if id.starts_with("tx") && id.ends_with(&suffix) {
            selected = Some((id, row));
            break;
        }
    }
    let Some((id, row)) = selected else {
        return Ok(NativePreparedValidityQueryV1::Complete(Box::new(
            StreamedValidityEvaluation {
                profile: VALIDITY_STREAM_PROFILE.into(),
                query,
                outcome: crate::validity_source::SourceOutcome::Returned(V::Undefined),
                update: None,
                post_state: snapshot.pin().clone(),
            },
        )));
    };
    let arguments = vec![
        V::Object(vec![("_id".into(), V::String(id))]),
        V::Object(vec![(
            "$set".into(),
            V::Object(vec![
                ("type".into(), ty),
                ("valid".into(), valid),
                ("reason".into(), reason),
            ]),
        )]),
        V::Object(vec![("upsert".into(), V::Bool(true))]),
    ];
    let before_state = prepared_metadata_state(&snapshot.data, &[])?;
    Ok(NativePreparedValidityQueryV1::Selected(Box::new(
        NativePreparedValiditySelectionV1 {
            read: snapshot,
            query,
            selected: row,
            arguments,
            context: context()?,
            before_state,
        },
    )))
}

fn prepared_durable_read_context(
    query: &PrefixQueryReceipt,
    arguments: &[V],
    before_state: &NativeMetadataStateReceiptV1,
    after_state: &NativeMetadataStateReceiptV1,
    write_expected: &MetadataSnapshot,
) -> V {
    prepared_obj(vec![
        ("profile", prepared_text(PREPARED_VALIDITY_PROFILE)),
        (
            "logical",
            prepared_obj(vec![
                ("queryProfile", prepared_text(&query.profile)),
                ("predicate", prepared_text(&query.predicate)),
                ("visitedRows", prepared_text(query.visited_rows.to_string())),
                ("matchedRows", prepared_text(query.matched_rows.to_string())),
                (
                    "queryChain",
                    prepared_text(prepared_hex(&query.chain_sha256d)),
                ),
                ("complete", V::Bool(query.complete)),
                ("beforeState", prepared_state_value(before_state)),
                ("afterState", prepared_state_value(after_state)),
                ("updateArguments", V::Array(arguments.to_vec())),
            ]),
        ),
        ("readPhysical", prepared_snapshot_value(&query.snapshot)),
        ("writePhysical", prepared_snapshot_value(write_expected)),
    ])
}
