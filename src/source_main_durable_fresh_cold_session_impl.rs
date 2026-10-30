impl NativeMainColdPrefixV1 {
    /// New paired-view prefix admission; same actual rebuild body. The child
    /// reader never escapes this new profile to an old public constructor.
    pub fn reexecute_with_paired_fresh_views(
        bytes: &[u8],
        expected: &NativeMainColdPrefixExpectationV1,
        index: SourceCompactIndex,
        pair: &crate::source_main_active_service_wal::VerifiedDurableFreshPairedHistoryV1,
    ) -> Result<Self, Box<RejectedNativeMainColdPrefixV1>> {
        Self::reexecute_with_actual_fact_views(bytes, expected, index, pair.source())
    }
}
pub const NATIVE_MAIN_COLD_DURABLE_FRESH_PROFILE: &str =
    "TL-MAIN-COLD-DURABLE-FRESH-SOURCE-REEXECUTION-13-1";
struct NativeMainFreshInterleaveV1 {
    outer: crate::source_main_active_service_wal::VerifiedDurableFreshOuterTraceV1,
    next: usize,
    adopted: bool,
    waiting: Option<usize>,
    feeds: Vec<NativeMainFreshInputReceiptV1>,
    feed_head: [u8; 32],
    head: [u8; 32],
}
pub struct NativeMainColdDurableFreshInputV1 {
    source: NativeMainColdActualFactServiceV1,
    raw: NativeMainFreshInterleaveV1,
}
pub struct RejectedNativeMainColdDurableFreshV1 {
    pub cause: String,
    owner: NativeMainColdDurableFreshInputV1,
}
impl core::fmt::Debug for RejectedNativeMainColdDurableFreshV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RejectedNativeMainColdDurableFreshV1")
            .field("cause", &self.cause)
            .field("verified_outer_actions", &self.owner.raw.next)
            .finish_non_exhaustive()
    }
}
impl RejectedNativeMainColdDurableFreshV1 {
    pub fn financial_state(&self) -> Result<V, SourceRuleError> {
        self.owner.financial_state()
    }
    pub fn index(&self) -> &SourceCompactIndex {
        self.owner.index()
    }
}
impl NativeMainColdDurableFreshInputV1 {
    pub fn profile(&self) -> &'static str {
        NATIVE_MAIN_COLD_DURABLE_FRESH_PROFILE
    }
    pub fn financial_state(&self) -> Result<V, SourceRuleError> {
        self.source.financial_runtime().state_value()
    }
    pub fn index(&self) -> &SourceCompactIndex {
        self.source.index()
    }
    pub fn verified_actions(&self) -> usize {
        self.raw.next
    }
    pub fn acknowledged_head(&self) -> [u8; 32] {
        self.raw.head
    }
    pub fn feeds(&self) -> &[NativeMainFreshInputReceiptV1] {
        &self.raw.feeds
    }
    pub fn original_source_receipts(&self) -> &[NativeMainActiveServiceReceiptV1] {
        self.source.original_action_receipts()
    }
    pub fn reexecute(
        prefix: NativeMainColdPrefixV1,
        paired: crate::source_main_active_service_wal::VerifiedDurableFreshPairedHistoryV1,
        observation_limit: usize,
    ) -> Result<Self, Box<RejectedNativeMainColdDurableFreshV1>> {
        let (source, outer) = paired.into_parts();
        let head = outer.origin_head();
        let mut raw = NativeMainFreshInterleaveV1 {
            outer,
            next: 0,
            adopted: false,
            waiting: None,
            feeds: vec![],
            feed_head: sha256(NATIVE_MAIN_DURABLE_FRESH_PROFILE.as_bytes()),
            head,
        };
        match NativeMainColdActualFactServiceV1::reexecute_with_interleave(
            prefix,
            source,
            observation_limit,
            &mut raw,
        ) {
            Ok(source) => Ok(Self { source, raw }),
            Err(rejected) => {
                let RejectedNativeMainColdActualFactServiceV1 {
                    cause,
                    owner: source,
                } = *rejected;
                Err(Box::new(RejectedNativeMainColdDurableFreshV1 {
                    cause,
                    owner: Self { source, raw },
                }))
            }
        }
    }
}
fn fresh_cold_api_rejection(error: NativeMainDurableFreshErrorV1) -> V {
    obj(&[
        ("kind", s("ActualApiRejected")),
        ("error", s(&error.to_string())),
    ])
}
// The live outer writer records the child prepare error directly.
// Feed errors use the separate outer error type and retain its wrapper.
fn fresh_cold_source_api_rejection(error: NativeMainActiveWalErrorV1) -> V {
    obj(&[
        ("kind", s("ActualApiRejected")),
        ("error", s(&error.to_string())),
    ])
}
fn fresh_cold_source_prepare_rejection(
    state: &NativeMainColdServiceStateV1,
    source_actions: u64,
    action: &NativeMainActiveServiceActionV1,
) -> Option<NativeMainActiveWalErrorV1> {
    if source_actions >= 4096 {
        return Some(NativeMainActiveWalErrorV1::Capacity);
    }
    if matches!(
        (state.started, action),
        (false, NativeMainActiveServiceActionV1::Start { .. })
    ) {
    } else if !state.started || matches!(action, NativeMainActiveServiceActionV1::Start { .. }) {
        return Some(NativeMainActiveWalErrorV1::WrongAction);
    }
    if matches!(
        action,
        NativeMainActiveServiceActionV1::AppendPending
            | NativeMainActiveServiceActionV1::ResolvePost
    ) {
        let context = state
            .frame
            .as_deref()
            .and_then(cold_service_pending_context);
        if context.is_none()
            || (matches!(action, NativeMainActiveServiceActionV1::ResolvePost)
                && context.as_ref() != state.attempted_write.as_ref())
        {
            return Some(NativeMainActiveWalErrorV1::WrongAction);
        }
    }
    None
}
fn fresh_cold_receiver(
    state: &NativeMainColdServiceStateV1,
    history: &NativeHistoricalValidityPlaybackV1<'_>,
    expected: &crate::source_main_active_service_wal::DurableFreshVerifiedReceiverV1,
) -> Result<Vec<u8>, String> {
    if history.source_history_pin() != expected.metadata() {
        return Err("fresh actual receiver metadata point".into());
    }
    let selection = state.actual_cold_fact_selection().unwrap_or(V::Null);
    if &selection != expected.selection() {
        return Err("fresh actual source selector".into());
    }
    match (
        expected.pending(),
        state
            .frame
            .as_ref()
            .and_then(|f| f.durable_fresh_historical_inline_source()),
    ) {
        (None, None) => (),
        (Some(metadata), Some(source)) => metadata
            .verify_recomputed_plan(source)
            .map_err(|e| e.to_string())?,
        _ => return Err("fresh actual pending source owner".into()),
    }
    let metadata = history
        .source_history_pin()
        .canonical_scalar_bytes()
        .map_err(|e| e.to_string())?;
    let raw = obj(&[
        ("profile", s("TL-MAIN-DURABLE-FRESH-ACTUAL-RECEIVER-13-1")),
        (
            "sourcePrefix",
            ActiveServiceWalBlobV1::of(expected.prefix_raw())
                .map_err(|e| e.to_string())?
                .value(),
        ),
        (
            "metadata",
            ActiveServiceWalBlobV1::of(&metadata)
                .map_err(|e| e.to_string())?
                .value(),
        ),
        (
            "pendingPlan",
            expected
                .pending_raw()
                .map(|raw| ActiveServiceWalBlobV1::of(raw).map(|p| p.value()))
                .transpose()
                .map_err(|e| e.to_string())?
                .unwrap_or(V::Null),
        ),
        ("selection", selection),
    ])
    .diagnostic_json()
    .map_err(|e| e.to_string())?
    .into_bytes();
    if raw != expected.raw() {
        return Err("fresh actual receiver record".into());
    }
    Ok(raw)
}
fn fresh_cold_observation(
    state: &NativeMainColdServiceStateV1,
    history: &NativeHistoricalValidityPlaybackV1<'_>,
    head: &[u8; 32],
    feeds: usize,
) -> Result<(Vec<u8>, Vec<u8>), String> {
    let source = state.actual_fact_observation(
        history.source_history_pin(),
        history.source_actual_rows().map_err(|e| e.to_string())?,
    )?;
    let raw = obj(&[
        ("profile", s(NATIVE_MAIN_DURABLE_FRESH_PROFILE)),
        (
            "sourceObservation",
            ActiveServiceWalBlobV1::of(&source)
                .map_err(|e| e.to_string())?
                .value(),
        ),
        ("feeds", s(&feeds.to_string())),
        ("feedHead", s(&hex(head))),
    ])
    .diagnostic_json()
    .map_err(|e| e.to_string())?
    .into_bytes();
    Ok((raw, source))
}
impl NativeMainFreshInterleaveV1 {
    fn check_before(
        &self,
        state: &NativeMainColdServiceStateV1,
        history: &NativeHistoricalValidityPlaybackV1<'_>,
        at: usize,
    ) -> Result<(), String> {
        let e = &self.outer.entries()[at];
        if e.previous_head() != self.head {
            return Err("fresh outer head before action".into());
        }
        fresh_cold_receiver(state, history, e.before_receiver())?;
        let (raw, source) =
            fresh_cold_observation(state, history, &self.feed_head, self.feeds.len())?;
        if raw != e.before() || source != e.before_source() {
            return Err(format!("fresh source Before mismatch at {at}"));
        }
        Ok(())
    }
    fn complete(
        &mut self,
        state: &NativeMainColdServiceStateV1,
        history: &NativeHistoricalValidityPlaybackV1<'_>,
        at: usize,
        outcome: V,
    ) -> Result<(), String> {
        let e = &self.outer.entries()[at];
        fresh_cold_receiver(state, history, e.after_receiver())?;
        let (after, source) =
            fresh_cold_observation(state, history, &self.feed_head, self.feeds.len())?;
        if after != e.after() || source != e.after_source() || &outcome != e.outcome() {
            return Err(format!("fresh source After/outcome mismatch at {at}"));
        }
        let entry = obj(&[
            ("profile", s(NATIVE_MAIN_DURABLE_FRESH_PROFILE)),
            ("sequence", s(&e.sequence().to_string())),
            ("previousHead", s(&hex(&self.head))),
            (
                "action",
                ActiveServiceWalBlobV1::of(e.raw_action())
                    .map_err(|e| e.to_string())?
                    .value(),
            ),
            (
                "before",
                ActiveServiceWalBlobV1::of(e.before())
                    .map_err(|e| e.to_string())?
                    .value(),
            ),
            (
                "after",
                ActiveServiceWalBlobV1::of(&after)
                    .map_err(|e| e.to_string())?
                    .value(),
            ),
            (
                "sourceBefore",
                ActiveServiceWalBlobV1::of(e.before_receiver().raw())
                    .map_err(|e| e.to_string())?
                    .value(),
            ),
            (
                "sourceAfter",
                ActiveServiceWalBlobV1::of(e.after_receiver().raw())
                    .map_err(|e| e.to_string())?
                    .value(),
            ),
            ("outcome", outcome),
        ])
        .diagnostic_json()
        .map_err(|e| e.to_string())?
        .into_bytes();
        let mut next = self.head.to_vec();
        next.extend_from_slice(&sha256(&entry));
        let next = sha256(&next);
        if entry != e.entry() || next != e.head() {
            return Err("fresh actual outer receipt/head".into());
        }
        self.head = next;
        self.next += 1;
        Ok(())
    }
    fn feed(
        &mut self,
        state: &mut NativeMainColdServiceStateV1,
        history: &NativeHistoricalValidityPlaybackV1<'_>,
        input: NativeMainFreshRpcInputV1,
    ) -> Result<V, String> {
        let result = (|| -> Result<V, NativeMainDurableFreshErrorV1> {
            if self.feeds.len() >= 4096 {
                return Err(NativeMainDurableFreshErrorV1::Capacity);
            }
            self.feeds
                .try_reserve(1)
                .map_err(|_| NativeMainDurableFreshErrorV1::Capacity)?;
            let frame = state
                .frame
                .as_ref()
                .ok_or(NativeMainActiveServiceErrorV1::NoFrame)?;
            if let Some(code) = frame.terminal_code() {
                return Err(NativeMainActiveServiceErrorV1::AlreadyExited(code).into());
            }
            let boundary = obj(&[
                ("sourceActions", s(&state.head.actions.to_string())),
                ("sourceHead", s(&hex(&state.head.commitment))),
                (
                    "physical",
                    physical_value(&cold_prefix_physical(history.source_history_pin())),
                ),
                (
                    "pendingNative",
                    cold_service_pending_context(frame).unwrap_or(V::Null),
                ),
            ]);
            let ordinal = self.feeds.len() as u64 + 1;
            let body = durable_fresh_bytes(&obj(&[
                ("ordinal", s(&ordinal.to_string())),
                ("raw", input.value()),
                ("actualBoundary", boundary.clone()),
            ]))?;
            let mut head = [0; 64];
            head[..32].copy_from_slice(&self.feed_head);
            head[32..].copy_from_slice(&sha256(&body));
            let head = sha256(&head);
            let outcome = obj(&[
                ("kind", s("ActualFeedPublished")),
                ("ordinal", s(&ordinal.to_string())),
                ("actualBoundary", boundary.clone()),
            ]);
            state
                .owner
                .financial
                .native_main_runtime_mut()
                .native_main_feed_fresh_rpc(
                    state
                        .frame
                        .as_deref_mut()
                        .expect("same actual historical source frame"),
                    &input,
                )
                .map_err(|e| NativeMainActiveServiceErrorV1::Backend(e.to_string()))?;
            self.feed_head = head;
            self.feeds.push(NativeMainFreshInputReceiptV1 {
                ordinal,
                raw: input,
                boundary,
            });
            Ok(outcome)
        })();
        Ok(result.unwrap_or_else(fresh_cold_api_rejection))
    }
    fn drive(
        &mut self,
        state: &mut NativeMainColdServiceStateV1,
        history: &mut NativeHistoricalValidityPlaybackV1<'_>,
        transcript: &crate::source_main_active_service_wal::VerifiedActualSourceFactTraceV1,
        completed: u64,
        expect_source: bool,
    ) -> Result<(), String> {
        let origin = self.outer.origin_receiver().source_actions();
        if !self.adopted {
            if completed < origin {
                return Ok(());
            }
            if completed != origin {
                return Err("fresh adoption missed source boundary".into());
            }
            fresh_cold_receiver(state, history, self.outer.origin_receiver())?;
            let (raw, source) = fresh_cold_observation(state, history, &self.feed_head, 0)?;
            if raw != self.outer.origin_observation() || source != self.outer.origin_source() {
                return Err("fresh actual adoption observation".into());
            }
            self.adopted = true;
        }
        loop {
            let Some(e) = self.outer.entries().get(self.next) else {
                if expect_source {
                    return Err("unrecorded child source action after fresh history".into());
                }
                return Ok(());
            };
            if e.before_receiver().source_actions() != completed {
                return Err("fresh raw action source cursor".into());
            }
            let action = e.action().clone();
            self.check_before(state, history, self.next)?;
            match action {
                NativeMainDurableFreshActionV1::Feed(input) => {
                    let outcome = self.feed(state, history, input)?;
                    self.complete(state, history, self.next, outcome)?;
                }
                NativeMainDurableFreshActionV1::SourceAction(raw) => {
                    if let Some(error) = fresh_cold_source_prepare_rejection(state, completed, &raw)
                    {
                        let outcome = fresh_cold_source_api_rejection(error);
                        self.complete(state, history, self.next, outcome)?;
                        continue;
                    }
                    if !expect_source {
                        return Err("fresh source action lacks actual child ACK".into());
                    }
                    let child = transcript
                        .entries()
                        .get(completed as usize)
                        .ok_or("fresh child action missing")?;
                    if e.after_receiver().source_actions() != completed + 1
                        || raw
                            .canonical_action_bytes()
                            .map_err(|e| format!("{e:?}"))?
                            .as_slice()
                            != child.raw_action()
                    {
                        return Err("fresh source action/child route".into());
                    }
                    self.waiting = Some(self.next);
                    return Ok(());
                }
            }
        }
    }
}
impl NativeMainActualFactInterleaveV1 for NativeMainFreshInterleaveV1 {
    fn before_source(
        &mut self,
        state: &mut NativeMainColdServiceStateV1,
        history: &mut NativeHistoricalValidityPlaybackV1<'_>,
        _capability: &NativeColdIndexExecutionV1,
        transcript: &crate::source_main_active_service_wal::VerifiedActualSourceFactTraceV1,
        sequence: u64,
    ) -> Result<(), String> {
        self.drive(state, history, transcript, sequence - 1, true)
    }
    fn after_source(
        &mut self,
        state: &mut NativeMainColdServiceStateV1,
        history: &mut NativeHistoricalValidityPlaybackV1<'_>,
        _capability: &NativeColdIndexExecutionV1,
        transcript: &crate::source_main_active_service_wal::VerifiedActualSourceFactTraceV1,
        sequence: u64,
    ) -> Result<(), String> {
        if let Some(at) = self.waiting.take() {
            let child = transcript
                .entries()
                .get(sequence as usize - 1)
                .ok_or("fresh actual child ACK missing")?;
            let outcome = obj(&[
                ("kind", s("ActualSourceAcknowledged")),
                ("sourceActions", s(&sequence.to_string())),
                ("sourceHead", s(&hex(&child.head))),
            ]);
            self.complete(state, history, at, outcome)?;
        }
        Ok(())
    }
    fn finish(
        &mut self,
        state: &mut NativeMainColdServiceStateV1,
        history: &mut NativeHistoricalValidityPlaybackV1<'_>,
        _capability: &NativeColdIndexExecutionV1,
        transcript: &crate::source_main_active_service_wal::VerifiedActualSourceFactTraceV1,
        completed: u64,
    ) -> Result<(), String> {
        self.drive(state, history, transcript, completed, false)?;
        if self.waiting.is_some() || self.next != self.outer.entries().len() {
            return Err("fresh acknowledged raw actions remain".into());
        }
        Ok(())
    }
}
