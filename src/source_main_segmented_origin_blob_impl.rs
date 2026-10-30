impl SourceMainNativeDurableActiveServiceV1 {
    // The new segmented Before carries extra fields, so it is distinct from
    // this actual original BASE ready observation. Persist it under the first
    // real source Intent, before constructing/running the active source owner.
    fn segmented_ready_origin_blob(&self) -> ActiveWalResult<Option<Vec<u8>>> {
        if self.segmented_history.is_none() {
            return Ok(None);
        }
        let Some(ActiveWalOwner::Ready(owner)) = &self.owner else {
            return Ok(None);
        };
        let raw = Self::ready_observation(owner, self.observation_limit)?;
        let pin = ActiveServiceWalBlobV1::of(&raw)?;
        if self.wal.origin_context()?.get("origin").get("observation") != pin.value() {
            return Err(NativeMainActiveWalErrorV1::WrongAction);
        }
        Ok(Some(raw))
    }
}
