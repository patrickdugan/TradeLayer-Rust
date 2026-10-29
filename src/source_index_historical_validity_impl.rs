/// Sealed metadata-only playback. Views are read from the actual currently
/// locked journal using independent history pins, never supplied row values.
/// This does not construct a mutable index or authorize source/financial input.
pub struct NativeHistoricalValidityPlaybackV1<'a> {
    index: &'a mut SourceCompactIndex,
    current: SourceIndexReadSnapshot,
    boundaries: Vec<SourceIndexHistoryPinV1>,
    context: (String, std::path::PathBuf),
    next: usize,
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
            if previous.as_ref().is_some_and(|previous| {
                previous.publication != view.pin.publication
                    || previous.context != view.pin.context
                    || previous.sequence.checked_add(1) != Some(view.pin.sequence)
            }) {
                return Err(CompactIndexError::WrongContext);
            }
            previous = Some(view.pin.clone());
            if first.is_none() {
                first = Some(view);
            }
        }
        Ok(Self {
            index,
            current: first.expect("nonempty checked"),
            boundaries: boundaries.to_vec(),
            context,
            next: 0,
        })
    }

    pub fn current(&self) -> &MetadataSnapshot {
        self.current.pin()
    }
    pub fn complete(&self) -> bool {
        self.next + 1 == self.boundaries.len()
    }
    pub fn consumed_transitions(&self) -> usize {
        self.next
    }
    /// Computes actual literal first-suffix selection and update from caller
    /// arguments. The Runtime calls this with the values computed by actual
    /// source execution. Arguments are never used as a source outcome.
    pub fn verify_source_update(
        &mut self,
        txid: V,
        ty: V,
        valid: V,
        reason: V,
        pre_update: &mut impl FnMut(&[V]) -> Result<Option<DependencyReturn>>,
    ) -> Result<StreamedValidityEvaluation> {
        let before = self.current.clone();
        let query = prepared_validity_read_snapshot(
            before.clone(),
            || Ok(self.context.clone()),
            txid,
            ty,
            valid,
            reason,
        )?;
        let selection = match query {
            NativePreparedValidityQueryV1::Complete(value) => return Ok(*value),
            NativePreparedValidityQueryV1::Selected(value) => *value,
        };
        if let Some(returned) = pre_update(&selection.arguments)? {
            if !matches!(
                returned,
                DependencyReturn::Throw(_) | DependencyReturn::PendingThrow(_)
            ) {
                return Err(CompactIndexError::WrongContext);
            }
            return Ok(prepared_validity_evaluation(
                selection.query,
                selection.arguments,
                returned,
                before.pin.clone(),
            ));
        }
        let mut row = selection.selected;
        let V::Object(sets) = selection.arguments[1].get("$set") else {
            return Err(CompactIndexError::Corrupt);
        };
        for (key, value) in sets {
            row.insert(&key, value)?;
        }
        profile_value(&row)?;
        let row = match memory_copy(&row) {
            Ok(row) => row,
            Err(error) => {
                return Ok(prepared_validity_evaluation(
                    selection.query,
                    selection.arguments,
                    DependencyReturn::PendingThrow(error),
                    before.pin.clone(),
                ))
            }
        };
        let (directory, mutations) =
            prepared_metadata_mutations(&before.directory, row, |position, _| {
                before.page(position)
            })?;
        let post_pin = self
            .boundaries
            .get(self.next + 1)
            .ok_or(CompactIndexError::StaleSnapshot)?;
        let post = self.index.read_verified_history_generation(post_pin)?;
        // Complete KV, not a selected row or supplied validity flag, determines
        // whether the actual past batch was this computed source transition.
        if prepared_metadata_state(&before.data, &mutations)?
            != prepared_metadata_state(&post.data, &[])?
            || directory.encode()? != post.directory.encode()?
        {
            return Err(CompactIndexError::WrongContext);
        }
        let evaluated = prepared_validity_evaluation(
            selection.query,
            selection.arguments,
            DependencyReturn::Pending(V::number(1.)),
            post.pin.clone(),
        );
        self.current = post;
        self.next += 1;
        Ok(evaluated)
    }
}
