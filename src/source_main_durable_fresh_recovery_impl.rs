// Whole coordinator + actual NoIndex owner move. No financial/base escape.
pub struct SourceMainNativeDurableFreshNoIndexV1 {
    source: SourceMainNativeActualFactNoIndexV1,
    control: DurableFreshCoordinatorV1,
}
pub struct NativeMainDurableFreshVerifiedReopenV1 {
    source: NativeMainActualFactVerifiedReopenV1,
    control: DurableFreshCoordinatorV1,
}
impl SourceMainNativeDurableFreshInputV1 {
    pub fn into_no_inline_index(
        self,
    ) -> Result<SourceMainNativeDurableFreshNoIndexV1, Box<RejectedNativeMainDurableFreshV1<Self>>>
    {
        if self.control.phase != NativeMainDurableFreshPhaseV1::NativeRecoveryRequired
            || self.control.pending.as_ref().is_none_or(|p| {
                !p.executed
                    || !p.source_prepared
                    || !matches!(
                        p.action,
                        NativeMainDurableFreshActionV1::SourceAction(
                            NativeMainActiveServiceActionV1::AppendPending
                        )
                    )
            })
            || self
                .control
                .pending
                .as_ref()
                .is_some_and(|p| p.plan.is_some())
        {
            return Err(Box::new(RejectedNativeMainDurableFreshV1 {
                cause: "no matching durable native action".into(),
                owner: self,
            }));
        }
        let Self { source, control } = self;
        match source.into_no_inline_index() {
            Ok(source) => Ok(SourceMainNativeDurableFreshNoIndexV1 { source, control }),
            Err(rejected) => Err(Box::new(RejectedNativeMainDurableFreshV1 {
                cause: rejected.cause.clone(),
                owner: Self {
                    source: rejected.into_owner(),
                    control,
                },
            })),
        }
    }
}
impl SourceMainNativeDurableFreshNoIndexV1 {
    pub fn profile(&self) -> &'static str {
        NATIVE_MAIN_DURABLE_FRESH_PROFILE
    }
    pub fn feeds(&self) -> &[NativeMainFreshInputReceiptV1] {
        &self.control.feeds
    }
    pub fn effects(&self) -> &[V] {
        self.source.effects()
    }
    pub fn try_reopen(
        self,
        root: &std::path::Path,
        candidate: crate::source_compact_index_v2::NativePreparedValidityCandidateV1,
        recovery: crate::storage::Recovery,
    ) -> Result<NativeMainDurableFreshVerifiedReopenV1, Box<RejectedNativeMainDurableFreshV1<Self>>>
    {
        let Self { source, control } = self;
        match source.try_reopen(root, candidate, recovery) {
            Ok(source) => Ok(NativeMainDurableFreshVerifiedReopenV1 { source, control }),
            Err(rejected) => Err(Box::new(RejectedNativeMainDurableFreshV1 {
                cause: rejected.cause.clone(),
                owner: Self {
                    source: rejected.into_owner(),
                    control,
                },
            })),
        }
    }
}
impl NativeMainDurableFreshVerifiedReopenV1 {
    pub fn candidate(&self) -> crate::source_compact_index_v2::NativePreparedValidityCandidateV1 {
        self.source.candidate()
    }
    pub fn feeds(&self) -> &[NativeMainFreshInputReceiptV1] {
        &self.control.feeds
    }
    pub fn effects(&self) -> &[V] {
        self.source.effects()
    }
    pub fn finish_verified_post(
        self,
    ) -> Result<SourceMainNativeDurableFreshInputV1, Box<RejectedNativeMainDurableFreshV1<Self>>>
    {
        let Self {
            source,
            mut control,
        } = self;
        match source.finish_verified_post() {
            Ok(source) => {
                control.phase = NativeMainDurableFreshPhaseV1::Execute;
                Ok(SourceMainNativeDurableFreshInputV1 { source, control })
            }
            Err(rejected) => Err(Box::new(RejectedNativeMainDurableFreshV1 {
                cause: rejected.cause.clone(),
                owner: Self {
                    source: rejected.into_owner(),
                    control,
                },
            })),
        }
    }
    pub fn prepare_verified_replacement(
        self,
    ) -> Result<SourceMainNativeDurableFreshInputV1, Box<RejectedNativeMainDurableFreshV1<Self>>>
    {
        let Self {
            source,
            mut control,
        } = self;
        match source.prepare_verified_replacement() {
            Ok(source) => {
                control.phase = NativeMainDurableFreshPhaseV1::Execute;
                Ok(SourceMainNativeDurableFreshInputV1 { source, control })
            }
            Err(rejected) => Err(Box::new(RejectedNativeMainDurableFreshV1 {
                cause: rejected.cause.clone(),
                owner: Self {
                    source: rejected.into_owner(),
                    control,
                },
            })),
        }
    }
}
