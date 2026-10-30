enum MixedNoIndexKindV1 {
    Inline(SourceMainNativeActualFactNoIndexV1),
    Validity(SourceMainNativeDurableServiceNoIndexV1),
}
pub struct SourceMainNativeMixedFactNoIndexV1 {
    inner: MixedNoIndexKindV1,
}
pub struct RejectedMixedFactNoIndexV1 {
    pub cause: String,
    owner: SourceMainNativeMixedFactServiceV1,
}
impl RejectedMixedFactNoIndexV1 {
    pub fn into_owner(self) -> SourceMainNativeMixedFactServiceV1 {
        self.owner
    }
}
impl core::fmt::Debug for RejectedMixedFactNoIndexV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RejectedMixedFactNoIndexV1")
            .field("cause", &self.cause)
            .finish_non_exhaustive()
    }
}
pub struct RejectedMixedFactReopenV1 {
    pub cause: String,
    owner: SourceMainNativeMixedFactNoIndexV1,
}
impl RejectedMixedFactReopenV1 {
    pub fn into_owner(self) -> SourceMainNativeMixedFactNoIndexV1 {
        self.owner
    }
}
impl core::fmt::Debug for RejectedMixedFactReopenV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RejectedMixedFactReopenV1")
            .field("cause", &self.cause)
            .finish_non_exhaustive()
    }
}
pub struct NativeMainMixedFactVerifiedReopenV1 {
    owner: SourceMainNativeMixedFactServiceV1,
    candidate: crate::source_compact_index_v2::NativePreparedValidityCandidateV1,
    inline: bool,
}
pub struct RejectedMixedFactVerifiedV1 {
    pub cause: String,
    owner: NativeMainMixedFactVerifiedReopenV1,
}
impl RejectedMixedFactVerifiedV1 {
    pub fn into_owner(self) -> NativeMainMixedFactVerifiedReopenV1 {
        self.owner
    }
}
impl core::fmt::Debug for RejectedMixedFactVerifiedV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RejectedMixedFactVerifiedV1")
            .field("cause", &self.cause)
            .finish_non_exhaustive()
    }
}
impl SourceMainNativeMixedFactServiceV1 {
    pub fn into_no_index(
        mut self,
    ) -> Result<SourceMainNativeMixedFactNoIndexV1, Box<RejectedMixedFactNoIndexV1>> {
        let checked = (|| -> ActiveWalResult<bool> {
            self.require_action_route()?;
            let actual = self.actual();
            if actual.fact_barrier_pending()
                || actual.deferred_error.is_some()
                || actual.phase() != NativeMainActiveWalPhaseV1::NativeRecoveryRequired
            {
                return Err(NativeMainActiveWalErrorV1::Busy);
            }
            match actual
                .inner
                .active_service()
                .ok_or(NativeMainActiveWalErrorV1::WrongAction)?
                .actual_mixed_native_selection()?
            {
                Some(MixedSourceNativeSelectionV1::Inline(_)) => {
                    actual.require_actual_io_durable_before_close()?;
                    Ok(true)
                }
                Some(MixedSourceNativeSelectionV1::Validity(_)) => {
                    actual.inner.require_mixed_ordinary_durable_io()?;
                    Ok(false)
                }
                None => Err(NativeMainActiveWalErrorV1::WrongAction),
            }
        })();
        let inline = match checked {
            Ok(v) => v,
            Err(error) => {
                return Err(Box::new(RejectedMixedFactNoIndexV1 {
                    cause: error.to_string(),
                    owner: self,
                }))
            }
        };
        let actual = self.inner.take().expect("one actual whole writer");
        if inline {
            match actual.into_no_inline_index() {
                Ok(inner) => Ok(SourceMainNativeMixedFactNoIndexV1 {
                    inner: MixedNoIndexKindV1::Inline(inner),
                }),
                Err(rejected) => {
                    let cause = rejected.cause.clone();
                    self.inner = Some(rejected.into_owner());
                    Err(Box::new(RejectedMixedFactNoIndexV1 { cause, owner: self }))
                }
            }
        } else {
            let SourceMainNativeActualFactServiceV1 {
                inner,
                fact_barrier,
                deferred_error,
            } = actual;
            match inner.into_no_native_index() {
                Ok(inner) => Ok(SourceMainNativeMixedFactNoIndexV1 {
                    inner: MixedNoIndexKindV1::Validity(inner),
                }),
                Err(rejected) => {
                    let cause = rejected.cause.clone();
                    self.inner = Some(SourceMainNativeActualFactServiceV1 {
                        inner: rejected.into_controller(),
                        fact_barrier,
                        deferred_error,
                    });
                    Err(Box::new(RejectedMixedFactNoIndexV1 { cause, owner: self }))
                }
            }
        }
    }
}
impl SourceMainNativeDurableActiveServiceV1 {
    fn require_mixed_ordinary_durable_io(&self) -> ActiveWalResult<()> {
        let service = self
            .active_service()
            .ok_or(NativeMainActiveWalErrorV1::WrongAction)?;
        let selection = service
            .actual_mixed_native_selection()?
            .ok_or(NativeMainActiveWalErrorV1::WrongAction)?
            .value();
        let slots = self
            .pending
            .as_ref()
            .and_then(|p| p.mixed_ordinary_slots.as_ref())
            .ok_or(NativeMainActiveWalErrorV1::WrongAction)?;
        let io = slots
            .latest_io()
            .ok_or(NativeMainActiveWalErrorV1::WrongAction)?;
        let raw = io
            .attempt
            .canonical_io_fact_bytes()
            .map_err(active_wal_backend)?;
        let pending = io.attempt.pending_fact().canonical_fact_bytes();
        let record = self.wal.record()?;
        if !slots.delivered
            || io.attempt.actual_result().is_ok()
            || selection != io.selection
            || record.get("profile") != s(MIXED_SOURCE_FACT_WAL_PROFILE)
            || record.get("stage") != s("NativeIoObserved")
            || record.get("ordinaryIoSelection") != io.selection
            || record.get("ordinaryNativeIoFact") != ActiveServiceWalBlobV1::of(&raw)?.value()
            || record.get("ordinaryNativeIoPendingFact")
                != ActiveServiceWalBlobV1::of(pending)?.value()
            || self.wal.read_blob(&ActiveServiceWalBlobV1::of(&raw)?)? != raw
            || self.wal.read_blob(&ActiveServiceWalBlobV1::of(pending)?)? != pending
        {
            return Err(NativeMainActiveWalErrorV1::WrongAction);
        }
        Ok(())
    }
}
impl SourceMainNativeMixedFactNoIndexV1 {
    pub fn effects(&self) -> &[V] {
        match &self.inner {
            MixedNoIndexKindV1::Inline(owner) => owner.effects(),
            MixedNoIndexKindV1::Validity(owner) => owner.effects(),
        }
    }
    pub fn try_reopen(
        self,
        root: &Path,
        candidate: crate::source_compact_index_v2::NativePreparedValidityCandidateV1,
        recovery: Recovery,
    ) -> Result<NativeMainMixedFactVerifiedReopenV1, Box<RejectedMixedFactReopenV1>> {
        match self.inner {
            MixedNoIndexKindV1::Inline(owner) => {
                match owner.try_reopen(root, candidate, recovery) {
                    Ok(verified) => Ok(NativeMainMixedFactVerifiedReopenV1 {
                        owner: SourceMainNativeMixedFactServiceV1 {
                            inner: Some(verified.inner),
                            checkpoint: None,
                        },
                        candidate,
                        inline: true,
                    }),
                    Err(rejected) => Err(Box::new(RejectedMixedFactReopenV1 {
                        cause: rejected.cause.clone(),
                        owner: Self {
                            inner: MixedNoIndexKindV1::Inline(rejected.into_owner()),
                        },
                    })),
                }
            }
            MixedNoIndexKindV1::Validity(owner) => {
                match owner.try_reopen(root, candidate, recovery) {
                    Ok(inner) => Ok(NativeMainMixedFactVerifiedReopenV1 {
                        owner: SourceMainNativeMixedFactServiceV1 {
                            inner: Some(SourceMainNativeActualFactServiceV1 {
                                inner,
                                fact_barrier: false,
                                deferred_error: None,
                            }),
                            checkpoint: None,
                        },
                        candidate,
                        inline: false,
                    }),
                    Err(rejected) => Err(Box::new(RejectedMixedFactReopenV1 {
                        cause: rejected.cause.clone(),
                        owner: Self {
                            inner: MixedNoIndexKindV1::Validity(rejected.into_owner()),
                        },
                    })),
                }
            }
        }
    }
}
impl NativeMainMixedFactVerifiedReopenV1 {
    pub fn candidate(&self) -> crate::source_compact_index_v2::NativePreparedValidityCandidateV1 {
        self.candidate
    }
    pub fn effects(&self) -> &[V] {
        self.owner
            .active_service()
            .expect("same reopened source owner")
            .effects()
    }
    pub fn finish_verified_post(
        mut self,
    ) -> Result<SourceMainNativeMixedFactServiceV1, Box<RejectedMixedFactVerifiedV1>> {
        use crate::source_compact_index_v2::NativePreparedValidityCandidateV1 as Candidate;
        let checked = (|| -> ActiveWalResult<()> {
            let base = &self.owner.actual().inner;
            if self.candidate != Candidate::Post
                || base.phase != NativeMainActiveWalPhaseV1::Observe
            {
                return Err(NativeMainActiveWalErrorV1::WrongAction);
            }
            let service = base
                .active_service()
                .ok_or(NativeMainActiveWalErrorV1::WrongAction)?;
            SourceMainNativeDurableActiveServiceV1::verify_current_native_post(service)?;
            let recovery = base
                .pending
                .as_ref()
                .and_then(|p| p.native_recovery.as_ref())
                .ok_or(NativeMainActiveWalErrorV1::WrongAction)?;
            if recovery.replacement.is_some()
                || recovery.mapping.is_some()
                || recovery.inline_replacement.is_some()
                || recovery.inline_mapping.is_some()
                || recovery.installed
            {
                return Err(NativeMainActiveWalErrorV1::WrongAction);
            }
            Ok(())
        })();
        if let Err(error) = checked {
            return Err(Box::new(RejectedMixedFactVerifiedV1 {
                cause: error.to_string(),
                owner: self,
            }));
        }
        let facts = self
            .owner
            .actual_mut()
            .inner
            .actual_facts
            .as_mut()
            .expect("immutable mixed origin");
        facts.needs_barrier = true;
        facts.recovery_observation = true;
        Ok(self.owner)
    }
    pub fn prepare_verified_replacement(
        mut self,
    ) -> Result<SourceMainNativeMixedFactServiceV1, Box<RejectedMixedFactVerifiedV1>> {
        use crate::source_compact_index_v2::NativePreparedValidityCandidateV1 as Candidate;
        if !matches!(self.candidate, Candidate::Before | Candidate::Intermediate) {
            return Err(Box::new(RejectedMixedFactVerifiedV1 {
                cause: "Before or Intermediate required".into(),
                owner: self,
            }));
        }
        if self.inline {
            let actual = self.owner.inner.take().expect("one actual writer");
            let verified = NativeMainActualFactVerifiedReopenV1 {
                inner: actual,
                candidate: self.candidate,
            };
            match verified.prepare_verified_replacement() {
                Ok(actual) => {
                    self.owner.inner = Some(actual);
                    Ok(self.owner)
                }
                Err(rejected) => {
                    let cause = rejected.cause.clone();
                    let verified = rejected.into_owner();
                    self.owner.inner = Some(verified.inner);
                    Err(Box::new(RejectedMixedFactVerifiedV1 { cause, owner: self }))
                }
            }
        } else {
            let result = self
                .owner
                .actual_mut()
                .inner
                .prepare_mixed_ordinary_replacement();
            match result {
                Ok(()) => Ok(self.owner),
                Err(error) => Err(Box::new(RejectedMixedFactVerifiedV1 {
                    cause: error.to_string(),
                    owner: self,
                })),
            }
        }
    }
}
