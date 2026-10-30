impl SourceMainNativeDurableActiveServiceV1 {
    // The new segmented Before carries extra fields, so it is distinct from
    // this actual original BASE ready observation. Persist it under the first
    // real source Intent, before constructing/running the active source owner.
    fn segmented_ready_origin_blob(&self) -> ActiveWalResult<Option<Vec<u8>>> {
        if self.segmented_history.is_none() {
            return Ok(None);
        }
        if self.mixed_ordinary_facts.is_some() && self.pending.as_ref().is_some_and(|p| p.executed)
        {
            // The immutable actual Ready blob was synced by the first Intent.
            // A partial/rejected constructor cannot recompute that prestate.
            return Ok(None);
        }
        let Some(ActiveWalOwner::Ready(owner)) = &self.owner else {
            return Ok(None);
        };
        let mut raw = Self::ready_observation(owner, self.observation_limit)?;
        if self.mixed_ordinary_facts.is_some() {
            Self::append_escrow_scope_observation(
                &mut raw,
                owner.index.inline_escrow_scope_active(),
            )?;
            Self::append_actual_facts_scope_observation_for(&mut raw, MIXED_SOURCE_FACT_SCOPE)?;
            Self::append_mixed_ordinary_scope_observation(&mut raw)?;
            Self::append_mixed_recorded_scope(
                &mut raw,
                owner.index.recorded_native_only_scope_active(),
            )?;
        }
        let pin = ActiveServiceWalBlobV1::of(&raw)?;
        if self.wal.origin_context()?.get("origin").get("observation") != pin.value() {
            return Err(NativeMainActiveWalErrorV1::WrongAction);
        }
        Ok(Some(raw))
    }
}
