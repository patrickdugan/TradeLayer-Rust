// Actual live block locals; no Runtime, balance map or decoded result importer.
enum NativeMainServiceBlockPc {
    Prelude,
    InlineIndex,
    Funding,
    Trade,
    Flush,
    Rewards,
    HeightContinuation,
    Orders,
    ChannelTrades,
    Clearing,
    Withdrawals,
    End,
}
struct NativeMainServiceBlockCompletion {
    request: crate::source_main_step_kernel::MainServiceBlockRequest,
    result: R<V>,
    effect_start: usize,
    file_invocation_effect_start: Option<usize>,
}
struct NativeMainServiceBlockFrame {
    request: crate::source_main_step_kernel::MainServiceBlockRequest,
    invocation: Option<SourceMainInvocationV1>,
    pc: NativeMainServiceBlockPc,
    inline_profile: bool,
    file_prefix_done: bool,
    file_invocation_effect_start: Option<usize>,
    inline_index: Option<NativeMainInlineIndexFrameV1>,
    funding: Vec<V>,
    other: Vec<V>,
    row: Option<NativeMainRowContinuationV1>,
    did_work: bool,
    mid_started: bool,
    effect_start: usize,
    stopped: Option<SourceRuleError>,
    result: Option<R<V>>,
    validity: Vec<StreamedValidityEvaluation>,
    consumed: u64,
}
impl SourceContractRuntime {
    fn native_main_service_block_begin(
        &mut self,
        request: crate::source_main_step_kernel::MainServiceBlockRequest,
    ) -> Result<NativeMainServiceBlockFrame, SourceRuleError> {
        let effect_start = self.core.effects.len();
        let invocation = self.source_main_invocation_begin(&DependencyCall {
            target: "Main.processBlock".into(),
            arguments: vec![request.block.clone(), request.height.clone()],
        })?;
        self.native_main_service_block_frame(request, Some(invocation), effect_start)
    }
    fn native_main_service_block_frame(
        &self,
        request: crate::source_main_step_kernel::MainServiceBlockRequest,
        invocation: Option<SourceMainInvocationV1>,
        effect_start: usize,
    ) -> Result<NativeMainServiceBlockFrame, SourceRuleError> {
        Ok(NativeMainServiceBlockFrame {
            request,
            invocation,
            pc: NativeMainServiceBlockPc::Prelude,
            inline_profile: false,
            file_prefix_done: false,
            file_invocation_effect_start: None,
            inline_index: None,
            funding: vec![],
            other: vec![],
            row: None,
            did_work: false,
            mid_started: false,
            effect_start,
            stopped: None,
            result: None,
            validity: vec![],
            consumed: 0,
        })
    }
    fn native_main_service_live_rows(rows: Vec<V>, height: V) -> NativeMainRowContinuationV1 {
        NativeMainRowContinuationV1 {
            invocation: None,
            rows: rows.into(),
            height,
            live: true,
            skips: 0,
            processed_any: false,
            consumed_rows: 0,
            current: None,
            validity: vec![],
            result: None,
        }
    }
    fn native_main_service_block_finish(
        &mut self,
        frame: &mut NativeMainServiceBlockFrame,
        result: R<V>,
    ) {
        let returned = match frame.invocation.take() {
            Some(invocation) => self.source_main_invocation_finish(invocation, result),
            None => returned(result),
        };
        frame.result = Some(match returned {
            Ok(DependencyReturn::Immediate(v) | DependencyReturn::Pending(v)) => Ok(v),
            Ok(DependencyReturn::Throw(e) | DependencyReturn::PendingThrow(e)) => {
                Err(Fault::Thrown(e))
            }
            Err(e) => Err(Fault::Local(e)),
        });
    }
    fn native_main_service_block_fault(
        &mut self,
        frame: &mut NativeMainServiceBlockFrame,
        fault: Fault,
    ) {
        match fault {
            Fault::Thrown(e) => self.native_main_service_block_finish(frame, Err(Fault::Thrown(e))),
            Fault::Local(e) => frame.stopped = Some(e),
        }
    }
    fn native_main_service_block_advance(
        &mut self,
        index: &mut SourceCompactIndex,
        frame: &mut NativeMainServiceBlockFrame,
        documents: &V,
    ) -> NativeMainRowStopV1 {
        self.native_main_service_block_advance_with_metadata(
            frame,
            documents,
            &mut NativeMainLiveSourceMetadataAccess { index },
        )
    }
    fn native_main_service_block_advance_with_metadata(
        &mut self,
        frame: &mut NativeMainServiceBlockFrame,
        documents: &V,
        access: &mut impl NativeMainSourceMetadataAccess,
    ) -> NativeMainRowStopV1 {
        loop {
            if frame.result.is_some() {
                return NativeMainRowStopV1::Complete;
            }
            if let Some(e) = &frame.stopped {
                return NativeMainRowStopV1::SourceStopped(e.clone());
            }
            let height = frame.request.height.clone();
            match frame.pc {
                NativeMainServiceBlockPc::Prelude if frame.inline_profile => {
                    frame.pc = NativeMainServiceBlockPc::InlineIndex;
                }
                NativeMainServiceBlockPc::InlineIndex => {
                    let stop = access.inline_block(self, frame, documents);
                    if stop != NativeMainRowStopV1::Complete {
                        return stop;
                    }
                }
                NativeMainServiceBlockPc::Prelude => {
                    match access.block_prelude(self, frame.request.block.clone(), height, documents)
                    {
                        Ok(v) if matches!(v, V::Number(_)) => self
                            .native_main_service_block_finish(
                                frame,
                                Ok(obj(&[("reOrg", V::Bool(true)), ("restore", v)])),
                            ),
                        Ok(v) => match array(&v.get("funding")).and_then(|funding| {
                            array(&v.get("other")).map(|other| (funding, other))
                        }) {
                            Ok((funding, other)) => {
                                frame.funding = funding;
                                frame.other = other;
                                frame.pc = NativeMainServiceBlockPc::Funding;
                            }
                            Err(e) => self.native_main_service_block_fault(frame, e),
                        },
                        Err(e) => self.native_main_service_block_fault(frame, e),
                    }
                }
                NativeMainServiceBlockPc::Funding | NativeMainServiceBlockPc::Trade => {
                    let funding = matches!(frame.pc, NativeMainServiceBlockPc::Funding);
                    if !funding && !frame.mid_started {
                        frame.mid_started = true;
                        if !matches!(height,V::Number(bits)if f64::from_bits(bits).is_finite()&&f64::from_bits(bits)>=0.&&f64::from_bits(bits).fract()==0.&&f64::from_bits(bits)<=9_007_199_254_740_991.)
                        {
                            frame.stopped = Some(SourceRuleError::InvalidInput);
                            continue;
                        }
                        let reward_queue = self.core.source_liquidity_rewards_state().get("queue");
                        if (!self.core.spot_lifecycle_enabled() && !self.core.fee_buffer.is_empty())
                            || !matches!(reward_queue, V::Array(ref v) if v.is_empty())
                        {
                            frame.stopped = Some(SourceRuleError::UnknownDependency);
                            continue;
                        }
                    }
                    if frame.row.is_none() {
                        let rows = if funding {
                            &mut frame.funding
                        } else {
                            &mut frame.other
                        };
                        if rows.is_empty() {
                            frame.pc = if funding {
                                NativeMainServiceBlockPc::Trade
                            } else {
                                NativeMainServiceBlockPc::Withdrawals
                            };
                            continue;
                        }
                        frame.row = Some(Self::native_main_service_live_rows(
                            std::mem::take(rows),
                            height.clone(),
                        ));
                    }
                    let stop = access.rows(self, frame.row.as_mut().expect("live rows"));
                    frame
                        .validity
                        .append(&mut frame.row.as_mut().expect("live rows").validity);
                    if stop != NativeMainRowStopV1::Complete {
                        return stop;
                    }
                    let row = frame.row.take().expect("actual completed live rows");
                    frame.consumed += row.consumed_rows;
                    match row.result.expect("completed live result") {
                        Ok(DependencyReturn::Immediate(v) | DependencyReturn::Pending(v)) => {
                            if funding {
                                frame.pc = NativeMainServiceBlockPc::Trade;
                            } else {
                                frame.did_work = truth(&v);
                                match self.core.source_main_start_height_job(height) {
                                    Ok(()) => {
                                        frame.pc = if frame.did_work {
                                            NativeMainServiceBlockPc::Flush
                                        } else {
                                            NativeMainServiceBlockPc::Withdrawals
                                        }
                                    }
                                    Err(e) => self.native_main_service_block_fault(frame, e),
                                }
                            }
                        }
                        Ok(DependencyReturn::Throw(e) | DependencyReturn::PendingThrow(e))
                            if funding =>
                        {
                            self.native_main_service_block_fault(frame, Fault::Thrown(e))
                        }
                        Ok(DependencyReturn::Throw(_) | DependencyReturn::PendingThrow(_)) => {
                            frame.pc = NativeMainServiceBlockPc::Withdrawals
                        }
                        Err(e) => frame.stopped = Some(e),
                    }
                }
                NativeMainServiceBlockPc::HeightContinuation => {
                    match self.core.source_main_drain_height_job() {
                        Ok(()) => frame.pc = NativeMainServiceBlockPc::Orders,
                        Err(e) => self.native_main_service_block_fault(frame, e),
                    }
                }
                NativeMainServiceBlockPc::Orders | NativeMainServiceBlockPc::ChannelTrades => {
                    let (target, next) = if matches!(frame.pc, NativeMainServiceBlockPc::Orders) {
                        (
                            "processQueuedOnChainOrdersForBlock",
                            NativeMainServiceBlockPc::ChannelTrades,
                        )
                    } else {
                        (
                            "processQueuedChannelTradesForBlock",
                            NativeMainServiceBlockPc::Clearing,
                        )
                    };
                    self.core.emit(obj(&[
                        ("kind", s("main-phase")),
                        ("target", s(format!("Orderbook.{target}"))),
                        ("args", V::Array(vec![height.clone()])),
                    ]));
                    match self
                        .evaluate_queue(&SourceCall {
                            method: target.into(),
                            arguments: vec![height],
                        })
                        .map_err(Fault::Local)
                        .and_then(outcome)
                    {
                        Ok(_) => frame.pc = next,
                        Err(e) => self.native_main_service_block_fault(frame, e),
                    }
                }
                NativeMainServiceBlockPc::Flush
                | NativeMainServiceBlockPc::Rewards
                | NativeMainServiceBlockPc::Clearing
                | NativeMainServiceBlockPc::Withdrawals => {
                    let (target, next, args) = match frame.pc {
                        NativeMainServiceBlockPc::Flush => (
                            "Tally.flushQueuedFeeAccruals",
                            NativeMainServiceBlockPc::Rewards,
                            vec![height],
                        ),
                        NativeMainServiceBlockPc::Rewards => (
                            "Logic.settleLiquidityRewards",
                            NativeMainServiceBlockPc::HeightContinuation,
                            vec![height],
                        ),
                        NativeMainServiceBlockPc::Clearing => (
                            "Clearing.clearingFunction",
                            NativeMainServiceBlockPc::Withdrawals,
                            vec![height, V::Bool(false)],
                        ),
                        _ => (
                            "Channels.processWithdrawals",
                            NativeMainServiceBlockPc::End,
                            vec![height],
                        ),
                    };
                    match self.source_main_phase(target, args) {
                        Ok(_) => frame.pc = next,
                        Err(e) => self.native_main_service_block_fault(frame, e),
                    }
                }
                NativeMainServiceBlockPc::End => {
                    match self.source_main_production_dispatch(
                        "Main.blockHandlerEnd",
                        &[frame.request.block.get("hash"), height.clone()],
                    ) {
                        Ok(_) => self.native_main_service_block_finish(
                            frame,
                            Ok(obj(&[("reOrg", V::Bool(false)), ("restore", height)])),
                        ),
                        Err(e) => self.native_main_service_block_fault(frame, e),
                    }
                }
            }
        }
    }
}

impl NativeMainServiceBlockFrame {
    fn visit_canonical_fields(
        &self,
        visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
    ) -> Result<(), SourceRuleError> {
        visitor("profile", &s("TL-MAIN-ACTUAL-LIVE-BLOCK-13-1"))?;
        if self.file_prefix_done {
            visitor(
                "automaticFilePrefixDone",
                &s(NATIVE_MAIN_AUTOMATIC_FILE_SERVICE_PROFILE),
            )?;
        }
        if self.inline_profile {
            visitor("inlineProfile", &s("TL-MAIN-RETAINED-INLINE-INDEX-13-1"))?;
            visitor("inlineIndex/present", &V::Bool(self.inline_index.is_some()))?;
            if let Some(inline) = &self.inline_index {
                inline.visit_fields(&mut |p, v| visitor(&format!("inlineIndex/{p}"), v))?;
            }
        }
        visitor("request/job", &s(self.request.job.to_string()))?;
        native_main_row_visit_value("request/block", &self.request.block, visitor)?;
        native_main_row_visit_value("request/height", &self.request.height, visitor)?;
        visitor(
            "pc",
            &s(match self.pc {
                NativeMainServiceBlockPc::Prelude => "Prelude",
                NativeMainServiceBlockPc::InlineIndex => "InlineIndex",
                NativeMainServiceBlockPc::Funding => "Funding",
                NativeMainServiceBlockPc::Trade => "Trade",
                NativeMainServiceBlockPc::Flush => "Flush",
                NativeMainServiceBlockPc::Rewards => "Rewards",
                NativeMainServiceBlockPc::HeightContinuation => "HeightContinuation",
                NativeMainServiceBlockPc::Orders => "Orders",
                NativeMainServiceBlockPc::ChannelTrades => "ChannelTrades",
                NativeMainServiceBlockPc::Clearing => "Clearing",
                NativeMainServiceBlockPc::Withdrawals => "Withdrawals",
                NativeMainServiceBlockPc::End => "End",
            }),
        )?;
        visitor("invocation/present", &V::Bool(self.invocation.is_some()))?;
        if let Some(invocation) = &self.invocation {
            visitor("invocation/target", &s(&invocation.target))?;
            visitor("invocation/spotMain", &V::Bool(invocation.spot_main))?;
            visitor(
                "invocation/rejectedBefore",
                &s(invocation.rejected_before.to_string()),
            )?;
            visitor(
                "invocation/transferRejectedBefore",
                &s(invocation.transfer_rejected_before.to_string()),
            )?;
        }
        for (name, rows) in [("funding", &self.funding), ("other", &self.other)] {
            visitor(&format!("{name}/count"), &s(rows.len().to_string()))?;
            for (at, row) in rows.iter().enumerate() {
                native_main_row_visit_value(&format!("{name}/{at}"), row, visitor)?;
            }
        }
        visitor("row/present", &V::Bool(self.row.is_some()))?;
        if let Some(row) = &self.row {
            row.visit_canonical_fields(&mut |path, value| visitor(&format!("row/{path}"), value))?;
        }
        visitor("didWork", &V::Bool(self.did_work))?;
        visitor("midStarted", &V::Bool(self.mid_started))?;
        visitor("effectStart", &s(self.effect_start.to_string()))?;
        if let Some(start) = self.file_invocation_effect_start {
            visitor("automaticFileInvocationEffectStart", &s(start.to_string()))?;
        }
        visitor("consumed", &s(self.consumed.to_string()))?;
        visitor(
            "stopped",
            &self
                .stopped
                .as_ref()
                .map_or(V::Null, |e| s(format!("{e:?}"))),
        )?;
        visitor("result/present", &V::Bool(self.result.is_some()))?;
        if let Some(result) = &self.result {
            match result {
                Ok(value) => {
                    visitor("result/kind", &s("Returned"))?;
                    native_main_row_visit_value("result/value", value, visitor)?;
                }
                Err(Fault::Thrown(error)) => {
                    visitor("result/kind", &s("Thrown"))?;
                    visitor("result/name", &s(&error.name))?;
                    visitor("result/message", &s(&error.message))?;
                }
                Err(Fault::Local(error)) => {
                    visitor("result/kind", &s("Local"))?;
                    visitor("result/error", &s(format!("{error:?}")))?;
                }
            }
        }
        visitor("validity/count", &s(self.validity.len().to_string()))?;
        for (at, evaluation) in self.validity.iter().enumerate() {
            native_main_catchup_visit_validity(&format!("validity/{at}"), evaluation, visitor)?;
        }
        Ok(())
    }
}
