impl<'a> NativeHistoricalValidityPlaybackV1<'a> {
    // Crate-private separate admission. Original open still verifies ORIGINAL
    // current-publication files and does not gain a copied-data fallback.
    pub(crate) fn open_verified_views(
        index: &'a mut SourceCompactIndex,
        capability: &NativeColdIndexExecutionV1,
        boundaries: &[SourceIndexHistoryPinV1],
        views: Vec<MainIndexReadViewV1>,
    ) -> Result<Self> {
        if boundaries.is_empty() || boundaries.len() > 4097 || views.len() != boundaries.len() {
            return Err(CompactIndexError::Limit);
        }
        if index.cold_execution != Some(capability.scope)
            || index.capture_verified_history_pin()? != *capability.original_current()
        {
            return Err(CompactIndexError::ColdWriteRefused);
        }
        let context = index.native_store_context()?;
        let mut previous = None;
        for (pin, view) in boundaries.iter().zip(&views) {
            if pin.storage.namespace != *index.native_namespace()
                || pin.storage.persistent_identity != context.0
                || pin.storage.canonical_catalog != context.1
                || view.pin() != &pin.metadata
                || view.original_history() != Some(pin)
                || prepared_storage_receipt(&view.complete_metadata_state()?) != pin.storage.content
            {
                return Err(CompactIndexError::WrongContext);
            }
            let mut cursor = view.cursor();
            while cursor.next_row()?.is_some() {}
            if cursor.receipt() != pin.rows {
                return Err(CompactIndexError::WrongContext);
            }
            if previous.is_some_and(|old: u64| old.checked_add(1) != Some(pin.metadata.sequence)) {
                return Err(CompactIndexError::WrongContext);
            }
            previous = Some(pin.metadata.sequence);
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
            current: views[0].clone(),
            verified_views: Some(views),
            boundaries: boundaries.to_vec(),
            context,
            next: 0,
            scope,
            active: None,
        })
    }
}
