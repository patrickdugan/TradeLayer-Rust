// Opaque live-owner preparation; no filesystem mutation occurs here.
pub const NATIVE_APPEND_PROFILE: &str = "TL-NATIVE-PREPARED-APPEND-CANDIDATES-1";
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeAppendPin {
    pub publication: CatalogPublication,
    pub logical_sequence: u64,
}
pub struct NativeAppendPlan {
    owner_token: u64,
    origin_catalog: PathBuf,
    identity: String,
    config: CompactionConfig,
    before: NativeAppendPin,
    after: NativeAppendPin,
    compaction: Option<CompactionPlan>,
    mutations: Vec<Mutation>,
    append_bytes: u64,
    #[cfg(test)]
    append_fault: Option<NativeAppendTestFault>,
}
#[cfg(test)]
#[derive(Clone, Copy)]
enum NativeAppendTestFault {
    Partial,
    Synced,
}
impl core::fmt::Debug for NativeAppendPlan {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("NativeAppendPlan")
            .field("before", &self.before)
            .field("after", &self.after)
            .finish_non_exhaustive()
    }
}
impl NativeAppendPlan {
    pub fn before(&self) -> &NativeAppendPin {
        &self.before
    }
    pub fn after(&self) -> &NativeAppendPin {
        &self.after
    }
    pub fn intermediate(&self) -> Option<NativeAppendPin> {
        self.compaction.as_ref().map(|_| NativeAppendPin {
            publication: self.after.publication.clone(),
            logical_sequence: self.before.logical_sequence,
        })
    }
    pub fn mutations(&self) -> &[Mutation] {
        &self.mutations
    }
    pub fn catalog_context(&self) -> (&str, &Path) {
        (&self.identity, &self.origin_catalog)
    }
    pub fn configuration(&self) -> &CompactionConfig {
        &self.config
    }
    #[cfg(test)]
    pub(crate) fn test_append_fault(&mut self, mode: &str) {
        self.append_fault = Some(match mode {
            "partial" => NativeAppendTestFault::Partial,
            "synced" => NativeAppendTestFault::Synced,
            _ => panic!("append fault"),
        });
    }
}
impl CompactingJournal {
    pub fn commit_prepared_native_append(
        &mut self,
        plan: &NativeAppendPlan,
    ) -> Result<NativeAppendPin, StorageError> {
        self.check()?;
        if plan.owner_token != self.owner_token
            || plan.identity != self.catalog_identity
            || plan.config.target_log_bytes != self.config.target_log_bytes
            || plan.config.retained_ancestors != self.config.retained_ancestors
            || plan.origin_catalog != self.catalog.canonicalize()?
            || plan.before.publication != self.publication
            || plan.before.logical_sequence != self.sequence()?
        {
            return Err(bad("foreign or stale native append plan"));
        }
        self.inner.validate_commit(&plan.mutations)?;
        if let Some(compaction) = &plan.compaction {
            self.publish_compaction(compaction)?;
            self.prune_retention()?;
        }
        #[cfg(test)]
        match plan.append_fault {
            Some(NativeAppendTestFault::Partial) => self.inner.test_fail_after_partial_write(),
            Some(NativeAppendTestFault::Synced) => self.inner.test_fail_after_sync(),
            None => (),
        }
        self.inner.commit(plan.mutations.clone())?;
        self.io.confirmed_append_bytes = self
            .io
            .confirmed_append_bytes
            .saturating_add(plan.append_bytes);
        let after = NativeAppendPin {
            publication: self.publication()?.clone(),
            logical_sequence: self.sequence()?,
        };
        if after != plan.after {
            self.poisoned = true;
            return Err(bad("prepared append candidate mismatch"));
        }
        Ok(after)
    }
    pub fn prepare_native_append(
        &self,
        mutations: Vec<Mutation>,
    ) -> Result<NativeAppendPlan, StorageError> {
        self.check()?;
        self.inner.validate_commit(&mutations)?;
        let logical_sequence = self.sequence()?;
        let before = NativeAppendPin {
            publication: self.publication.clone(),
            logical_sequence,
        };
        let live = self
            .inner
            .entries()?
            .map(|(k, v)| (k.len() + v.len()) as u64)
            .sum::<u64>();
        let limit = self
            .config
            .target_log_bytes
            .max(live.saturating_mul(3).saturating_add(65536))
            .min(MAX_LOG_BYTES.saturating_sub(MAX_BATCH_BYTES as u64 + 65536));
        let append_bytes = batch_size(&mutations)?;
        let compaction = if fs::metadata(self.inner.path())?
            .len()
            .saturating_add(append_bytes)
            > limit
        {
            Some(self.prepare_compaction()?)
        } else {
            None
        };
        let publication = compaction
            .as_ref()
            .map_or_else(|| self.publication.clone(), |plan| plan.new.clone());
        let after = NativeAppendPin {
            publication,
            logical_sequence: logical_sequence
                .checked_add(1)
                .ok_or_else(|| bad("native sequence overflow"))?,
        };
        Ok(NativeAppendPlan {
            owner_token: self.owner_token,
            origin_catalog: self.catalog.canonicalize()?,
            identity: self.catalog_identity.clone(),
            config: self.config.clone(),
            before,
            after,
            compaction,
            mutations,
            append_bytes,
            #[cfg(test)]
            append_fault: None,
        })
    }
}
