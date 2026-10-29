/// Real source-derived historical native await, separate from a live append
/// token. No constructor accepts a persisted plan/source validity answer.
pub enum NativeHistoricalValidityOperationV1 {
    Complete(Box<StreamedValidityEvaluation>),
    Await(Box<NativeHistoricalValidityAwaitV1>),
}
#[derive(PartialEq, Eq)]
enum HistoricalNativePhase {
    Prepared,
    PostVerified,
    Resolved,
}
pub struct NativeHistoricalValidityAwaitV1 {
    scope: u64,
    position: usize,
    phase: HistoricalNativePhase,
    query: PrefixQueryReceipt,
    arguments: Vec<V>,
    before_state: NativeMetadataStateReceiptV1,
    after_state: NativeMetadataStateReceiptV1,
    directory: Directory,
    raw_mutations: Vec<u8>,
    post: SourceIndexHistoryPinV1,
}
impl NativeHistoricalValidityAwaitV1 {
    pub fn visit_canonical_fields(
        &self,
        visitor: &mut impl FnMut(&str, &V) -> std::result::Result<(), SourceRuleError>,
    ) -> std::result::Result<(), SourceRuleError> {
        visitor(
            "profile",
            &prepared_text("TL-TXINDEX-HISTORICAL-NATIVE-AWAIT-13-1"),
        )?;
        visitor("position", &prepared_text(self.position.to_string()))?;
        visitor(
            "phase",
            &prepared_text(match self.phase {
                HistoricalNativePhase::Prepared => "Prepared",
                HistoricalNativePhase::PostVerified => "PostVerified",
                HistoricalNativePhase::Resolved => "Resolved",
            }),
        )?;
        native_row_binding_value("sourceReadContext", &self.durable_read_context(), visitor)?;
        self.post.visit_canonical_fields(&mut |path, value| {
            visitor(&format!("originalPost/{path}"), value)
        })?;
        visitor(
            "mutations/bytes",
            &prepared_text(self.raw_mutations.len().to_string()),
        )?;
        visitor(
            "mutations/sha256",
            &prepared_text(prepared_hex(&crate::hash256::sha256(&self.raw_mutations))),
        )
    }
    pub fn durable_read_context(&self) -> V {
        prepared_durable_read_context(
            &self.query,
            &self.arguments,
            &self.before_state,
            &self.after_state,
            &self.query.snapshot,
        )
    }
    pub fn exact_prepared_mutations(&self) -> &[u8] {
        &self.raw_mutations
    }
    pub fn query(&self) -> &PrefixQueryReceipt {
        &self.query
    }
    pub fn update_arguments(&self) -> &[V] {
        &self.arguments
    }
    pub fn original_post(&self) -> &SourceIndexHistoryPinV1 {
        &self.post
    }
    pub fn append_verified(&self) -> bool {
        self.phase == HistoricalNativePhase::PostVerified
    }
    pub fn resolved(&self) -> bool {
        self.phase == HistoricalNativePhase::Resolved
    }
}
static NEXT_HISTORICAL_NATIVE_SCOPE: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(1);
/// Sealed metadata-only playback over the SAME current locked actual index.
/// Its original historical view advances only at a verified raw append action.
pub struct NativeHistoricalValidityPlaybackV1<'a> {
    index: &'a mut SourceCompactIndex,
    current: SourceIndexReadSnapshot,
    boundaries: Vec<SourceIndexHistoryPinV1>,
    context: (String, std::path::PathBuf),
    next: usize,
    scope: u64,
    active: Option<usize>,
}
impl<'a> NativeHistoricalValidityPlaybackV1<'a> {
    pub fn open(
        index: &'a mut SourceCompactIndex,
        boundaries: &[SourceIndexHistoryPinV1],
    ) -> Result<Self> {
        if boundaries.is_empty() || boundaries.len() > 4097 {
            return Err(CompactIndexError::Limit);
        }
        let context = index.native_store_context()?;
        let mut first = None;
        let mut previous: Option<MetadataSnapshot> = None;
        for pin in boundaries {
            let view = index.read_verified_history_generation(pin)?;
            if previous.as_ref().is_some_and(|prior| {
                prior.publication != view.pin.publication
                    || prior.context != view.pin.context
                    || prior.sequence.checked_add(1) != Some(view.pin.sequence)
            }) {
                return Err(CompactIndexError::WrongContext);
            }
            previous = Some(view.pin.clone());
            if first.is_none() {
                first = Some(view);
            }
        }
        let scope = NEXT_HISTORICAL_NATIVE_SCOPE
            .fetch_update(
                std::sync::atomic::Ordering::Relaxed,
                std::sync::atomic::Ordering::Relaxed,
                |n| n.checked_add(1),
            )
            .map_err(|_| CompactIndexError::Limit)?;
        Ok(Self {
            index,
            current: first.expect("nonempty checked"),
            boundaries: boundaries.to_vec(),
            context,
            next: 0,
            scope,
            active: None,
        })
    }
    pub fn visit_canonical_fields(
        &self,
        visitor: &mut impl FnMut(&str, &V) -> std::result::Result<(), SourceRuleError>,
    ) -> std::result::Result<(), SourceRuleError> {
        visitor(
            "profile",
            &prepared_text("TL-TXINDEX-HISTORICAL-NATIVE-PLAYBACK-13-1"),
        )?;
        visitor("consumedTransitions", &prepared_text(self.next.to_string()))?;
        visitor("heldAwait", &V::Bool(self.active.is_some()))?;
        visitor(
            "heldPosition",
            &self
                .active
                .map_or(V::Null, |position| prepared_text(position.to_string())),
        )?;
        self.boundaries[self.next].visit_canonical_fields(&mut |path, value| {
            visitor(&format!("originalRead/{path}"), value)
        })?;
        let actual = self
            .index
            .capture_verified_history_pin()
            .map_err(|_| SourceRuleError::Backend)?;
        actual.visit_canonical_fields(&mut |path, value| {
            visitor(&format!("actualCurrent/{path}"), value)
        })
    }
    pub fn current(&self) -> &MetadataSnapshot {
        self.current.pin()
    }
    pub fn complete(&self) -> bool {
        self.next + 1 == self.boundaries.len() && self.active.is_none()
    }
    pub fn consumed_transitions(&self) -> usize {
        self.next
    }
    pub fn prepare_source_update(
        &mut self,
        txid: V,
        ty: V,
        valid: V,
        reason: V,
        pre_update: &mut impl FnMut(&[V]) -> Result<Option<DependencyReturn>>,
    ) -> Result<NativeHistoricalValidityOperationV1> {
        if self.active.is_some() {
            return Err(CompactIndexError::StaleSnapshot);
        }
        let before = self.current.clone();
        let selected = match prepared_validity_read_snapshot(
            before.clone(),
            || Ok(self.context.clone()),
            txid,
            ty,
            valid,
            reason,
        )? {
            NativePreparedValidityQueryV1::Complete(evaluation) => {
                return Ok(NativeHistoricalValidityOperationV1::Complete(evaluation))
            }
            NativePreparedValidityQueryV1::Selected(selected) => *selected,
        };
        if let Some(returned) = pre_update(&selected.arguments)? {
            if !matches!(
                returned,
                DependencyReturn::Throw(_) | DependencyReturn::PendingThrow(_)
            ) {
                return Err(CompactIndexError::WrongContext);
            }
            return Ok(NativeHistoricalValidityOperationV1::Complete(Box::new(
                prepared_validity_evaluation(
                    selected.query,
                    selected.arguments,
                    returned,
                    before.pin.clone(),
                ),
            )));
        }
        let mut row = selected.selected;
        let V::Object(sets) = selected.arguments[1].get("$set") else {
            return Err(CompactIndexError::Corrupt);
        };
        for (key, value) in sets {
            row.insert(&key, value)?;
        }
        profile_value(&row)?;
        let row = match memory_copy(&row) {
            Ok(row) => row,
            Err(error) => {
                return Ok(NativeHistoricalValidityOperationV1::Complete(Box::new(
                    prepared_validity_evaluation(
                        selected.query,
                        selected.arguments,
                        DependencyReturn::PendingThrow(error),
                        before.pin.clone(),
                    ),
                )))
            }
        };
        let (directory, mutations) =
            prepared_metadata_mutations(&before.directory, row, |position, _| {
                before.page(position)
            })?;
        let after_state = prepared_metadata_state(&before.data, &mutations)?;
        let raw_mutations =
            crate::source_main_native_intent_store::native_prepared_mutation_bytes(&mutations)
                .map_err(|_| CompactIndexError::Limit)?;
        let post = self
            .boundaries
            .get(self.next + 1)
            .ok_or(CompactIndexError::StaleSnapshot)?
            .clone();
        let awaiting = NativeHistoricalValidityAwaitV1 {
            scope: self.scope,
            position: self.next,
            phase: HistoricalNativePhase::Prepared,
            query: selected.query,
            arguments: selected.arguments,
            before_state: selected.before_state,
            after_state,
            directory,
            raw_mutations,
            post,
        };
        self.active = Some(self.next);
        Ok(NativeHistoricalValidityOperationV1::Await(Box::new(
            awaiting,
        )))
    }
    /// Called only for the acknowledged raw native append action. Expected
    /// mutation bytes are a comparison from the independently pinned WAL blob;
    /// they never construct the source update, selected row or Post state.
    pub fn verify_recorded_native_append(
        &mut self,
        awaiting: &mut NativeHistoricalValidityAwaitV1,
        expected_raw_mutations: &[u8],
    ) -> Result<()> {
        if awaiting.scope != self.scope
            || self.active != Some(awaiting.position)
            || self.next != awaiting.position
            || awaiting.phase != HistoricalNativePhase::Prepared
            || self.current.pin() != &awaiting.query.snapshot
            || awaiting.raw_mutations != expected_raw_mutations
        {
            return Err(CompactIndexError::WrongContext);
        }
        let post = self
            .index
            .read_verified_history_generation(&awaiting.post)?;
        if prepared_metadata_state(&post.data, &[])? != awaiting.after_state
            || post.directory.encode()? != awaiting.directory.encode()?
        {
            return Err(CompactIndexError::WrongContext);
        }
        self.current = post;
        self.next += 1;
        awaiting.phase = HistoricalNativePhase::PostVerified;
        Ok(())
    }
    /// Resolve the SAME original await once. Source decode/Logic/mark and native
    /// append do not run here. The caller then advances its actual source PC.
    pub fn resolve_recorded_native_post(
        &mut self,
        awaiting: &mut NativeHistoricalValidityAwaitV1,
    ) -> Result<StreamedValidityEvaluation> {
        if awaiting.scope != self.scope
            || self.active != Some(awaiting.position)
            || self.next != awaiting.position + 1
            || awaiting.phase != HistoricalNativePhase::PostVerified
            || self.current.pin() != &awaiting.post.metadata
        {
            return Err(CompactIndexError::WrongContext);
        }
        let evaluated = prepared_validity_evaluation(
            awaiting.query.clone(),
            awaiting.arguments.clone(),
            DependencyReturn::Pending(V::number(1.)),
            awaiting.post.metadata.clone(),
        );
        awaiting.phase = HistoricalNativePhase::Resolved;
        self.active = None;
        Ok(evaluated)
    }
    /// Existing completed-row API composes the new split phases synchronously.
    /// Its archive/physical/KV comparisons remain unchanged; full WAL consumers
    /// must compare their independently retained raw mutation blob explicitly.
    pub fn verify_source_update(
        &mut self,
        txid: V,
        ty: V,
        valid: V,
        reason: V,
        pre_update: &mut impl FnMut(&[V]) -> Result<Option<DependencyReturn>>,
    ) -> Result<StreamedValidityEvaluation> {
        match self.prepare_source_update(txid, ty, valid, reason, pre_update)? {
            NativeHistoricalValidityOperationV1::Complete(evaluation) => Ok(*evaluation),
            NativeHistoricalValidityOperationV1::Await(mut awaiting) => {
                let expected = awaiting.raw_mutations.clone();
                if let Err(error) = self.verify_recorded_native_append(&mut awaiting, &expected) {
                    // Preserve the original synchronous metadata API's atomic
                    // refusal: this local, unexposed await has never advanced
                    // a source PC or native transition. Split PC callers retain
                    // their actual await instead and do not use this cleanup.
                    if awaiting.phase == HistoricalNativePhase::Prepared
                        && awaiting.scope == self.scope
                        && self.next == awaiting.position
                        && self.active == Some(awaiting.position)
                    {
                        self.active = None;
                    }
                    return Err(error);
                }
                self.resolve_recorded_native_post(&mut awaiting)
            }
        }
    }
}
