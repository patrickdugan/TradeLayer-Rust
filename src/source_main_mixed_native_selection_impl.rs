// WIP new-profile dispatcher. Never included by an old public constructor.
const MIXED_SOURCE_FACT_WAL_PROFILE: &str =
    crate::source_main_active_service_wal::MIXED_SOURCE_NATIVE_FACT_WAL_PROFILE;
const MIXED_SOURCE_FACT_SCOPE: &str =
    crate::source_main_active_service_wal::MIXED_SOURCE_NATIVE_FACT_SCOPE;
enum MixedSourceNativeSelectionV1 {
    Inline(V),
    Validity(V),
}
impl MixedSourceNativeSelectionV1 {
    fn kind(&self) -> &'static str {
        match self {
            Self::Inline(_) => "InlineDatabase",
            Self::Validity(_) => "OrdinaryValidity",
        }
    }
    fn value(&self) -> V {
        let value = match self {
            Self::Inline(v) | Self::Validity(v) => v,
        };
        obj(&[
            ("profile", s("TL-MAIN-MIXED-NATIVE-SELECTION-13-1")),
            ("kind", s(self.kind())),
            ("actualSource", value.clone()),
        ])
    }
}
impl SourceMainNativeActiveServiceV1 {
    fn actual_mixed_native_selection(
        &self,
    ) -> ActiveWalResult<Option<MixedSourceNativeSelectionV1>> {
        // Check both kinds before selecting either. There is no first-pending
        // or method-name fallback and no caller/detached/file scope admission.
        match (self.pending_inline_write(), self.pending_write()) {
            (Some(_), Some(_)) => Err(NativeMainActiveWalErrorV1::WrongAction),
            (Some(_), None) => self
                .actual_inline_fact_selection()
                .map(MixedSourceNativeSelectionV1::Inline)
                .map(Some)
                .ok_or(NativeMainActiveWalErrorV1::WrongAction),
            (None, Some(_)) => {
                let pending = self
                    .frame
                    .as_ref()
                    .and_then(|f| f.native_main_retained_consensus_descriptor(0))
                    .ok_or(NativeMainActiveWalErrorV1::WrongAction)?;
                Ok(Some(MixedSourceNativeSelectionV1::Validity(obj(&[
                    (
                        "profile",
                        s("TL-MAIN-ACTUAL-ORDINARY-VALIDITY-SELECTION-13-1"),
                    ),
                    ("sourceProfile", s(self.profile())),
                    (
                        "originCommands",
                        s(&self.origin.session.commands.to_string()),
                    ),
                    ("originHead", s(&hex(&self.origin.session.commitment))),
                    ("originPhysical", physical_value(&self.origin.physical)),
                    ("serviceActions", s(&self.head.actions.to_string())),
                    ("serviceHead", s(&hex(&self.head.commitment))),
                    ("pending", pending.value()),
                ]))))
            }
            (None, None) => Ok(None),
        }
    }
}
#[derive(Default)]
struct MixedOrdinaryFactModeV1 {
    current: Option<MixedOrdinaryCachedPlanV1>,
}
struct MixedOrdinaryCachedPlanV1 {
    signature: Vec<u8>,
    raw: Vec<u8>,
}
#[derive(Default)]
struct MixedOrdinaryActionFactsV1 {
    before: Option<(V, Vec<u8>)>,
    after: Option<(V, Vec<u8>)>,
    attempt: Option<MixedActualValidityCaptureV1>,
    retries: Vec<MixedActualValidityCaptureV1>,
    delivered: bool,
    io_chain: Option<Vec<u8>>,
    io_prior: Option<ActiveServiceWalBlobV1>,
    io_chain_count: usize,
    original_physical_before: Option<NativePhysicalBinding>,
    alias: Option<ActualSourceNativeAliasV1>,
}
struct MixedActualValidityCaptureV1 {
    selection: V,
    attempt: crate::source_compact_index_v2::ActualValidityAppendAttemptV1,
}
impl MixedOrdinaryActionFactsV1 {
    fn latest_io(&self) -> Option<&MixedActualValidityCaptureV1> {
        self.retries.last().or(self.attempt.as_ref())
    }
}
