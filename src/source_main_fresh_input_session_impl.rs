pub use crate::source_main_fresh_input::NativeMainFreshRpcInputV1;
pub const NATIVE_MAIN_FRESH_INPUT_SERVICE_PROFILE: &str =
    "TL-MAIN-NATIVE-FRESH-RPC-INPUT-SERVICE-13-1";
pub const NATIVE_MAIN_FRESH_INPUT_CALLER_PROFILE: &str =
    "TL-MAIN-RETAINED-FRESH-RPC-INPUT-CALLER-13-1";
const MAX_FRESH_INPUT_FEEDS: usize = 4096;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeMainFreshInputReceiptV1 {
    pub ordinal: u64,
    pub raw: NativeMainFreshRpcInputV1,
    /// Actual receiver-local boundary, captured before factual publication.
    /// This is observation metadata, not a source return or native permit.
    pub boundary: V,
}
pub struct SourceMainNativeFreshInputServiceV1 {
    active: Box<SourceMainNativeActiveServiceV1>,
    origin: NativeMainActiveServiceExpectationV1,
    feeds: Vec<NativeMainFreshInputReceiptV1>,
}
pub struct RejectedNativeMainFreshInputServiceV1 {
    pub error: NativeMainActiveServiceErrorV1,
    active: Box<SourceMainNativeActiveServiceV1>,
}
impl RejectedNativeMainFreshInputServiceV1 {
    pub fn into_service(self) -> SourceMainNativeActiveServiceV1 {
        *self.active
    }
}
impl core::fmt::Debug for RejectedNativeMainFreshInputServiceV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RejectedNativeMainFreshInputServiceV1")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}
fn fresh_input_boundary(
    active: &SourceMainNativeActiveServiceV1,
) -> Result<V, NativeMainActiveServiceErrorV1> {
    let expected = active.expectation()?;
    Ok(obj(&[
        ("sourceActions", s(&expected.head.actions.to_string())),
        ("sourceHead", s(&hex(&expected.head.commitment))),
        ("physical", physical_value(&expected.physical)),
        (
            "pendingNative",
            active.pending_native_context().unwrap_or(V::Null),
        ),
    ]))
}
fn fresh_input_visit(
    feeds: &[NativeMainFreshInputReceiptV1],
    visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
) -> Result<(), SourceRuleError> {
    visitor("feeds/count", &s(&feeds.len().to_string()))?;
    for (at, feed) in feeds.iter().enumerate() {
        visitor(
            &format!("feeds/{at}/ordinal"),
            &s(&feed.ordinal.to_string()),
        )?;
        crate::source_contract_runtime::native_main_row_visit_value(
            &format!("feeds/{at}/raw"),
            &feed.raw.value(),
            visitor,
        )?;
        crate::source_contract_runtime::native_main_row_visit_value(
            &format!("feeds/{at}/actualBoundary"),
            &feed.boundary,
            visitor,
        )?;
    }
    Ok(())
}
impl SourceMainNativeActiveServiceV1 {
    /// Select this new input profile over the same already-started owner.
    /// Old service actions/codecs/readers remain unchanged and unadmitted.
    pub fn into_fresh_input_service(
        self,
    ) -> Result<SourceMainNativeFreshInputServiceV1, Box<RejectedNativeMainFreshInputServiceV1>>
    {
        let checked = (|| {
            if self.profile()
                != crate::source_contract_runtime::NATIVE_MAIN_INLINE_ACTIVE_SERVICE_PROFILE
                || self.source_start_error().is_some()
            {
                return Err(NativeMainActiveServiceErrorV1::InvalidInput);
            }
            let frame = self
                .frame
                .as_ref()
                .ok_or(NativeMainActiveServiceErrorV1::NoFrame)?;
            if let Some(code) = frame.terminal_code() {
                return Err(NativeMainActiveServiceErrorV1::AlreadyExited(code));
            }
            self.expectation()
        })();
        match checked {
            Ok(origin) => Ok(SourceMainNativeFreshInputServiceV1 {
                active: Box::new(self),
                origin,
                feeds: vec![],
            }),
            Err(error) => Err(Box::new(RejectedNativeMainFreshInputServiceV1 {
                error,
                active: Box::new(self),
            })),
        }
    }
}
impl SourceMainNativeFreshInputServiceV1 {
    pub fn profile(&self) -> &'static str {
        NATIVE_MAIN_FRESH_INPUT_SERVICE_PROFILE
    }
    pub fn feed(
        &mut self,
        input: NativeMainFreshRpcInputV1,
    ) -> Result<&NativeMainFreshInputReceiptV1, NativeMainActiveServiceErrorV1> {
        if self.feeds.len() >= MAX_FRESH_INPUT_FEEDS {
            return Err(NativeMainActiveServiceErrorV1::Capacity);
        }
        let frame = self
            .active
            .frame
            .as_ref()
            .ok_or(NativeMainActiveServiceErrorV1::NoFrame)?;
        if let Some(code) = frame.terminal_code() {
            return Err(NativeMainActiveServiceErrorV1::AlreadyExited(code));
        }
        let boundary = fresh_input_boundary(&self.active)?;
        self.feeds
            .try_reserve(1)
            .map_err(|_| NativeMainActiveServiceErrorV1::Capacity)?;
        let frame = self
            .active
            .frame
            .as_deref_mut()
            .expect("same checked source frame");
        self.active
            .owner
            .financial
            .native_main_runtime_mut()
            .native_main_feed_fresh_rpc(frame, &input)
            .map_err(|error| NativeMainActiveServiceErrorV1::Backend(error.to_string()))?;
        self.feeds.push(NativeMainFreshInputReceiptV1 {
            ordinal: self.feeds.len() as u64 + 1,
            raw: input,
            boundary,
        });
        Ok(self.feeds.last().expect("admitted factual input"))
    }
    pub fn feeds(&self) -> &[NativeMainFreshInputReceiptV1] {
        &self.feeds
    }
    pub fn apply(
        &mut self,
        action: NativeMainActiveServiceActionV1,
    ) -> Result<&NativeMainActiveServiceReceiptV1, NativeMainActiveServiceErrorV1> {
        let expected = self.active.expectation()?;
        self.active.apply(&expected, action)
    }
    /// Observation retries never reapply source actions or factual feeds.
    pub fn retry_source_observation(
        &mut self,
    ) -> Result<&NativeMainActiveServiceReceiptV1, NativeMainActiveServiceErrorV1> {
        self.active.retry_observation()
    }
    pub fn next_timer(&self) -> Option<u64> {
        self.active.next_timer()
    }
    pub fn pending_write(
        &self,
    ) -> Option<&crate::source_compact_index_v2::NativePreparedValidityWriteV1> {
        self.active.pending_write()
    }
    pub fn pending_inline_write(
        &self,
    ) -> Option<&crate::source_compact_index_v2::NativePreparedInlineDatabaseV1> {
        self.active.pending_inline_write()
    }
    pub fn caller_result(&self) -> Option<&Result<DependencyReturn, SourceRuleError>> {
        self.active.caller_result()
    }
    pub fn effects(&self) -> &[V] {
        self.active.effects()
    }
    pub fn financial_state(&self) -> Result<V, SourceRuleError> {
        self.active.financial_runtime().state_value()
    }
    pub fn visit_canonical_fields(
        &self,
        visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
    ) -> Result<(), SourceRuleError> {
        visitor("profile", &s(self.profile()))?;
        visitor(
            "entry/sourceActions",
            &s(&self.origin.head.actions.to_string()),
        )?;
        visitor("entry/sourceHead", &s(&hex(&self.origin.head.commitment)))?;
        crate::source_contract_runtime::native_main_row_visit_value(
            "entry/physical",
            &physical_value(&self.origin.physical),
            visitor,
        )?;
        fresh_input_visit(&self.feeds, visitor)?;
        self.active
            .visit_canonical_fields(&mut |p, v| visitor(&format!("source/{p}"), v))
    }
}

pub struct SourceMainNativeRetainedFreshInputV1 {
    consensus: Box<SourceMainNativeRetainedConsensusV1>,
    feeds: Vec<NativeMainFreshInputReceiptV1>,
}
impl SourceMainNativeRetainedConsensusV1 {
    /// The whole opaque caller/service owner moves; its actual route stays
    /// unchanged. This does not admit the new profile into selected WAL.
    pub fn into_fresh_input_caller(self) -> SourceMainNativeRetainedFreshInputV1 {
        SourceMainNativeRetainedFreshInputV1 {
            consensus: Box::new(self),
            feeds: vec![],
        }
    }
}
impl SourceMainNativeRetainedFreshInputV1 {
    pub fn profile(&self) -> &'static str {
        NATIVE_MAIN_FRESH_INPUT_CALLER_PROFILE
    }
    pub fn feed(
        &mut self,
        input: NativeMainFreshRpcInputV1,
    ) -> Result<&NativeMainFreshInputReceiptV1, NativeMainRetainedConsensusErrorV1> {
        if self.feeds.len() >= MAX_FRESH_INPUT_FEEDS {
            return Err(NativeMainRetainedConsensusErrorV1::Local(
                SourceRuleError::Capacity,
            ));
        }
        if self.consensus.service.terminal_code().is_some() {
            return Err(NativeMainRetainedConsensusErrorV1::InvalidInput);
        }
        let physical = physical(&self.consensus.caller.owner.index)
            .map_err(|e| NativeMainRetainedConsensusErrorV1::Backend(format!("{e:?}")))?;
        let boundary = obj(&[
            ("actualRoute", self.consensus.route.value().clone()),
            ("physical", physical_value(&physical)),
            (
                "pending",
                V::Array(
                    self.consensus
                        .pending_operations()
                        .iter()
                        .map(|p| p.value())
                        .collect(),
                ),
            ),
        ]);
        self.feeds
            .try_reserve(1)
            .map_err(|_| NativeMainRetainedConsensusErrorV1::Local(SourceRuleError::Capacity))?;
        let documents = input.documents().cloned();
        self.consensus
            .caller
            .owner
            .financial
            .native_main_runtime_mut()
            .native_main_feed_fresh_rpc(&mut self.consensus.service, &input)
            .map_err(NativeMainRetainedConsensusErrorV1::Local)?;
        if let Some(documents) = documents {
            self.consensus.caller.documents = documents;
        }
        self.feeds.push(NativeMainFreshInputReceiptV1 {
            ordinal: self.feeds.len() as u64 + 1,
            raw: input,
            boundary,
        });
        Ok(self.feeds.last().expect("admitted actual caller facts"))
    }
    pub fn feeds(&self) -> &[NativeMainFreshInputReceiptV1] {
        &self.feeds
    }
    pub fn pending_operations(&self) -> Vec<NativeMainRetainedConsensusPendingV1> {
        self.consensus.pending_operations()
    }
    pub fn advance_consensus(
        &mut self,
        limit: usize,
    ) -> Result<NativeMainRetainedConsensusStopV1, NativeMainRetainedConsensusErrorV1> {
        self.consensus.advance_consensus(limit)
    }
    pub fn advance_detached(
        &mut self,
        operation: &NativeMainRetainedConsensusPendingV1,
    ) -> Result<NativeMainRetainedConsensusStopV1, NativeMainRetainedConsensusErrorV1> {
        self.consensus.advance_detached(operation)
    }
    pub fn append_pending(
        &mut self,
        operation: &NativeMainRetainedConsensusPendingV1,
        permit: Option<&crate::source_compact_index_v2::SourceIndexGenerationEscrowPermitV1>,
    ) -> Result<
        crate::source_contract_runtime::NativeMainServiceStopV1,
        NativeMainRetainedConsensusErrorV1,
    > {
        self.consensus.append_pending(operation, permit)
    }
    pub fn resolve_post(
        &mut self,
        operation: &NativeMainRetainedConsensusPendingV1,
    ) -> Result<
        crate::source_contract_runtime::NativeMainServiceStopV1,
        NativeMainRetainedConsensusErrorV1,
    > {
        self.consensus.resolve_post(operation)
    }
    pub fn next_timer(&self) -> Option<u64> {
        self.consensus.next_timer()
    }
    pub fn fire_timer(
        &mut self,
        id: u64,
        generation: u64,
    ) -> Result<(), crate::source_main_step_kernel::MainStepError> {
        self.consensus.fire_timer(id, generation)
    }
    pub fn pause(&mut self) -> Result<DependencyReturn, SourceRuleError> {
        self.consensus.pause()
    }
    pub fn caller_result(&self) -> Option<&DependencyReturn> {
        self.consensus.caller_result()
    }
    pub fn effects(&self) -> &[V] {
        self.consensus.effects()
    }
    pub fn financial_state(&self) -> Result<V, SourceRuleError> {
        self.consensus.financial_state()
    }
    pub fn visit_canonical_fields(
        &self,
        visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
    ) -> Result<(), SourceRuleError> {
        visitor("profile", &s(self.profile()))?;
        fresh_input_visit(&self.feeds, visitor)?;
        self.consensus
            .visit_canonical_fields(&mut |p, v| visitor(&format!("source/{p}"), v))
    }
}
