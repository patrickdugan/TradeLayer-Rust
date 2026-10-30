// Work-only follow-on. No base-controller/financial escape is exposed by the
// no-index or verified-reopen carriers. Retry orchestration is a separate seam.
pub struct SourceMainNativeActualFactNoIndexV1 {
    inner: SourceMainNativeDurableInlineNoIndexV1,
}
pub struct RejectedNativeMainActualFactNoIndexV1 {
    pub cause: String,
    owner: SourceMainNativeActualFactServiceV1,
}
impl RejectedNativeMainActualFactNoIndexV1 {
    pub fn into_owner(self) -> SourceMainNativeActualFactServiceV1 {
        self.owner
    }
}
impl core::fmt::Debug for RejectedNativeMainActualFactNoIndexV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RejectedNativeMainActualFactNoIndexV1")
            .field("cause", &self.cause)
            .finish_non_exhaustive()
    }
}
pub struct NativeMainActualFactVerifiedReopenV1 {
    inner: SourceMainNativeActualFactServiceV1,
    candidate: crate::source_compact_index_v2::NativePreparedValidityCandidateV1,
}
pub struct RejectedNativeMainActualFactReopenV1 {
    pub cause: String,
    owner: SourceMainNativeActualFactNoIndexV1,
}
impl RejectedNativeMainActualFactReopenV1 {
    pub fn into_owner(self) -> SourceMainNativeActualFactNoIndexV1 {
        self.owner
    }
}
impl core::fmt::Debug for RejectedNativeMainActualFactReopenV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RejectedNativeMainActualFactReopenV1")
            .field("cause", &self.cause)
            .finish_non_exhaustive()
    }
}
impl SourceMainNativeActualFactServiceV1 {
    fn require_actual_io_durable_before_close(&self) -> ActiveWalResult<()> {
        if self.fact_barrier_pending()
            || self.deferred_error.is_some()
            || self.inner.phase != NativeMainActiveWalPhaseV1::NativeRecoveryRequired
        {
            return Err(NativeMainActiveWalErrorV1::Busy);
        }
        let slots = self
            .inner
            .pending
            .as_ref()
            .and_then(|p| p.actual_fact_slots.as_ref())
            .ok_or(NativeMainActiveWalErrorV1::WrongAction)?;
        let io = slots
            .latest_io()
            .ok_or(NativeMainActiveWalErrorV1::WrongAction)?;
        if self
            .inner
            .active_service()
            .and_then(|service| service.actual_inline_fact_selection())
            .as_ref()
            != Some(&io.selection)
        {
            return Err(NativeMainActiveWalErrorV1::WrongAction);
        }
        if io.attempt.actual_result().is_ok()
            || io.attempt.outcome().get("kind") != s("NativeIoError")
        {
            return Err(NativeMainActiveWalErrorV1::WrongAction);
        }
        let record = self.inner.wal.record()?;
        let raw = io
            .attempt
            .canonical_io_fact_bytes()
            .map_err(active_wal_backend)?;
        let pending = io.attempt.pending_fact().canonical_fact_bytes();
        if record.get("profile") != s(self.inner.profile())
            || record.get("stage") != s("NativeIoObserved")
            || record.get("actualIoSelection") != io.selection
            || record.get("actualNativeIoFact") != ActiveServiceWalBlobV1::of(&raw)?.value()
            || record.get("actualNativeIoPendingFact")
                != ActiveServiceWalBlobV1::of(pending)?.value()
            || self
                .inner
                .wal
                .read_blob(&ActiveServiceWalBlobV1::of(&raw)?)?
                != raw
            || self
                .inner
                .wal
                .read_blob(&ActiveServiceWalBlobV1::of(pending)?)?
                != pending
        {
            return Err(NativeMainActiveWalErrorV1::WrongAction);
        }
        Ok(())
    }
    pub fn into_no_inline_index(
        self,
    ) -> Result<SourceMainNativeActualFactNoIndexV1, Box<RejectedNativeMainActualFactNoIndexV1>>
    {
        if let Err(error) = self.require_actual_io_durable_before_close() {
            return Err(Box::new(RejectedNativeMainActualFactNoIndexV1 {
                cause: error.to_string(),
                owner: self,
            }));
        }
        let Self {
            inner,
            fact_barrier,
            deferred_error,
        } = self;
        match inner.into_no_inline_index() {
            Ok(inner) => Ok(SourceMainNativeActualFactNoIndexV1 { inner }),
            Err(rejected) => Err(Box::new(RejectedNativeMainActualFactNoIndexV1 {
                cause: rejected.cause.clone(),
                owner: Self {
                    inner: rejected.into_controller(),
                    fact_barrier,
                    deferred_error,
                },
            })),
        }
    }
}
impl SourceMainNativeActualFactNoIndexV1 {
    pub fn profile(&self) -> &'static str {
        "TL-MAIN-ACTUAL-FACTS-INLINE-NATIVE-RECOVERY-13-1"
    }
    pub fn effects(&self) -> &[V] {
        self.inner.effects()
    }
    pub fn try_reopen(
        self,
        root: &std::path::Path,
        candidate: crate::source_compact_index_v2::NativePreparedValidityCandidateV1,
        recovery: crate::storage::Recovery,
    ) -> Result<NativeMainActualFactVerifiedReopenV1, Box<RejectedNativeMainActualFactReopenV1>>
    {
        match self.inner.try_reopen(root, candidate, recovery) {
            Ok(inner) => Ok(NativeMainActualFactVerifiedReopenV1 {
                inner: SourceMainNativeActualFactServiceV1 {
                    inner,
                    fact_barrier: false,
                    deferred_error: None,
                },
                candidate,
            }),
            Err(rejected) => Err(Box::new(RejectedNativeMainActualFactReopenV1 {
                cause: rejected.cause.clone(),
                owner: Self {
                    inner: rejected.into_owner(),
                },
            })),
        }
    }
}
impl NativeMainActualFactVerifiedReopenV1 {
    pub fn candidate(&self) -> crate::source_compact_index_v2::NativePreparedValidityCandidateV1 {
        self.candidate
    }
    pub fn effects(&self) -> &[V] {
        self.inner
            .inner
            .active_service()
            .expect("same reopened owner")
            .effects()
    }
    pub fn finish_verified_post(
        mut self,
    ) -> Result<SourceMainNativeActualFactServiceV1, Box<RejectedNativeMainActualFactVerifiedPostV1>>
    {
        use crate::source_compact_index_v2::NativePreparedValidityCandidateV1 as Candidate;
        let checked = (|| {
            if self.candidate != Candidate::Post
                || self.inner.inner.phase != NativeMainActiveWalPhaseV1::Observe
            {
                return Err(NativeMainActiveWalErrorV1::WrongAction);
            }
            let service = self
                .inner
                .inner
                .active_service()
                .ok_or(NativeMainActiveWalErrorV1::WrongAction)?;
            SourceMainNativeDurableActiveServiceV1::verify_current_native_post(service)?;
            let recovery = self
                .inner
                .inner
                .pending
                .as_ref()
                .and_then(|p| p.native_recovery.as_ref())
                .ok_or(NativeMainActiveWalErrorV1::WrongAction)?;
            if recovery.inline_replacement.is_some()
                || recovery.inline_mapping.is_some()
                || recovery.installed
            {
                return Err(NativeMainActiveWalErrorV1::WrongAction);
            }
            Ok(())
        })();
        if let Err(error) = checked {
            return Err(Box::new(RejectedNativeMainActualFactVerifiedPostV1 {
                cause: error.to_string(),
                owner: self,
            }));
        }
        let facts = self
            .inner
            .inner
            .actual_facts
            .as_mut()
            .expect("same explicit actual-fact writer");
        facts.needs_barrier = true;
        facts.recovery_observation = true;
        // The real old StorageStopped stays in the pending source observation.
        // The ordinary observer runs only after the actual recovery proof syncs.
        Ok(self.inner)
    }
    // Before/Intermediate remain private here until replacement intent,
    // ordered IO-chain and actual escrow are wired. There is no base escape.
}
pub struct RejectedNativeMainActualFactVerifiedPostV1 {
    pub cause: String,
    owner: NativeMainActualFactVerifiedReopenV1,
}
impl RejectedNativeMainActualFactVerifiedPostV1 {
    pub fn into_owner(self) -> NativeMainActualFactVerifiedReopenV1 {
        self.owner
    }
}
impl core::fmt::Debug for RejectedNativeMainActualFactVerifiedPostV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RejectedNativeMainActualFactVerifiedPostV1")
            .field("cause", &self.cause)
            .finish_non_exhaustive()
    }
}
