// Actual catchup locals and settlement PCs, outside Clone/Eq financial Core.
// This initial enclosing profile refuses a configured daemon/timer service.
#[cfg(test)]
std::thread_local! {
    static NATIVE_MAIN_ACTUAL_VESTING_OBSERVATIONS: std::cell::RefCell<Vec<(f64, f64, V)>> = const { std::cell::RefCell::new(Vec::new()) };
}
#[derive(Clone, Copy)]
enum NativeMainCatchupPcV1 {
    Prepare,
    Height,
    Funding,
    Trade,
    Skip,
    Flush,
    Rewards,
    Orders,
    ChannelTrades,
    Cumulative,
    Volumes,
    Vesting,
    Withdrawals,
    Clearing,
    NextHeight,
    SaveHeight,
    Tail,
}
pub(crate) struct NativeMainCatchupContinuationV1 {
    invocation: Option<SourceMainInvocationV1>,
    start: V,
    plan: Option<Box<crate::source_compact_index_v2::MainIndexStreamPlan>>,
    grouping: Option<crate::source_compact_index_v2::RowStreamReceipt>,
    funding: Option<crate::source_compact_index_v2::MainIndexPhaseCursor>,
    trade: Option<crate::source_compact_index_v2::MainIndexPhaseCursor>,
    row: Option<NativeMainRowContinuationV1>,
    pc: NativeMainCatchupPcV1,
    h: f64,
    last: f64,
    max: f64,
    iterations: usize,
    funding_skips: u64,
    trade_skips: u64,
    total: u64,
    skip: bool,
    cumulative: V,
    volume: V,
    consumed: u64,
    validity: Vec<StreamedValidityEvaluation>,
    result: Option<Result<DependencyReturn, SourceRuleError>>,
    stopped: Option<SourceRuleError>,
    storage_stop: Option<String>,
}
impl NativeMainCatchupContinuationV1 {
    pub(crate) fn result(&self) -> Option<&Result<DependencyReturn, SourceRuleError>> {
        self.result.as_ref()
    }
    pub(crate) fn pending_write(&self) -> Option<&NativePreparedValidityWriteV1> {
        self.row.as_ref()?.pending_write()
    }
    pub(crate) fn consumed_rows(&self) -> u64 {
        self.consumed + self.row.as_ref().map_or(0, |row| row.consumed_rows())
    }
    pub(crate) fn grouping(&self) -> Option<&crate::source_compact_index_v2::RowStreamReceipt> {
        self.grouping.as_ref()
    }
    pub(crate) fn validity(&self) -> &[StreamedValidityEvaluation] {
        &self.validity
    }
    pub(crate) fn visit_canonical_fields(
        &self,
        visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
    ) -> Result<(), SourceRuleError> {
        visitor("profile", &s("TL-MAIN-ACTUAL-CATCHUP-CONTINUATION-13-1"))?;
        native_main_row_visit_value("start", &self.start, visitor)?;
        visitor(
            "pc",
            &s(match self.pc {
                NativeMainCatchupPcV1::Prepare => "Prepare",
                NativeMainCatchupPcV1::Height => "Height",
                NativeMainCatchupPcV1::Funding => "Funding",
                NativeMainCatchupPcV1::Trade => "Trade",
                NativeMainCatchupPcV1::Skip => "Skip",
                NativeMainCatchupPcV1::Flush => "Flush",
                NativeMainCatchupPcV1::Rewards => "Rewards",
                NativeMainCatchupPcV1::Orders => "Orders",
                NativeMainCatchupPcV1::ChannelTrades => "ChannelTrades",
                NativeMainCatchupPcV1::Cumulative => "Cumulative",
                NativeMainCatchupPcV1::Volumes => "Volumes",
                NativeMainCatchupPcV1::Vesting => "Vesting",
                NativeMainCatchupPcV1::Withdrawals => "Withdrawals",
                NativeMainCatchupPcV1::Clearing => "Clearing",
                NativeMainCatchupPcV1::NextHeight => "NextHeight",
                NativeMainCatchupPcV1::SaveHeight => "SaveHeight",
                NativeMainCatchupPcV1::Tail => "Tail",
            }),
        )?;
        for (name, value) in [("height", self.h), ("last", self.last), ("max", self.max)] {
            visitor(name, &n(value))?;
        }
        for (name, value) in [
            ("iterations", self.iterations as u64),
            ("fundingSkips", self.funding_skips),
            ("tradeSkips", self.trade_skips),
            ("total", self.total),
            ("consumed", self.consumed),
        ] {
            visitor(name, &s(value.to_string()))?;
        }
        visitor("skip", &V::Bool(self.skip))?;
        native_main_row_visit_value("cumulative", &self.cumulative, visitor)?;
        native_main_row_visit_value("volume", &self.volume, visitor)?;
        visitor(
            "stopped",
            &self
                .stopped
                .as_ref()
                .map_or(V::Null, |error| s(format!("{error:?}"))),
        )?;
        visitor(
            "storageStop",
            &self.storage_stop.as_ref().map_or(V::Null, s),
        )?;
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
        visitor("hasPlan", &V::Bool(self.plan.is_some()))?;
        visitor("grouping/present", &V::Bool(self.grouping.is_some()))?;
        if let Some(receipt) = &self.grouping {
            visitor("grouping/profile", &s(&receipt.profile))?;
            visitor("grouping/rows", &s(receipt.rows.to_string()))?;
            visitor(
                "grouping/chain",
                &s(native_main_row_hex(&receipt.chain_sha256d)),
            )?;
            visitor("grouping/complete", &V::Bool(receipt.complete))?;
            native_main_row_visit_snapshot("grouping/snapshot", &receipt.snapshot, visitor)?;
        }
        if let Some(plan) = &self.plan {
            plan.visit_native_plan_fields(&mut |path, value| {
                visitor(&format!("plan/{path}"), value)
            })?;
        }
        for (name, cursor) in [("funding", &self.funding), ("trade", &self.trade)] {
            visitor(&format!("{name}/present"), &V::Bool(cursor.is_some()))?;
            if let Some(cursor) = cursor {
                cursor.visit_native_phase_fields(&mut |path, value| {
                    visitor(&format!("{name}/{path}"), value)
                })?;
            }
        }
        visitor("hasRow", &V::Bool(self.row.is_some()))?;
        if let Some(row) = &self.row {
            row.visit_canonical_fields(&mut |path, value| visitor(&format!("row/{path}"), value))?;
        }
        visitor("validity/count", &s(self.validity.len().to_string()))?;
        for (at, evaluation) in self.validity.iter().enumerate() {
            native_main_catchup_visit_validity(&format!("validity/{at}"), evaluation, visitor)?;
        }
        visitor("complete", &V::Bool(self.result.is_some()))?;
        if let Some(result) = &self.result {
            match result {
                Ok(returned) => native_main_row_visit_return("result", returned, visitor)?,
                Err(error) => {
                    visitor("result/kind", &s("Local"))?;
                    visitor("result/error", &s(format!("{error:?}")))?;
                }
            }
        }
        Ok(())
    }
}
fn native_main_catchup_visit_validity(
    path: &str,
    evaluation: &StreamedValidityEvaluation,
    visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
) -> Result<(), SourceRuleError> {
    visitor(&format!("{path}/profile"), &s(&evaluation.profile))?;
    let query = &evaluation.query;
    for (name, value) in [
        ("profile", s(&query.profile)),
        ("predicate", s(&query.predicate)),
        ("visited", s(query.visited_rows.to_string())),
        ("matched", s(query.matched_rows.to_string())),
        ("chain", s(native_main_row_hex(&query.chain_sha256d))),
        ("complete", V::Bool(query.complete)),
    ] {
        visitor(&format!("{path}/query/{name}"), &value)?;
    }
    native_main_row_visit_snapshot(&format!("{path}/query/snapshot"), &query.snapshot, visitor)?;
    native_main_row_visit_snapshot(
        &format!("{path}/postState"),
        &evaluation.post_state,
        visitor,
    )?;
    match &evaluation.outcome {
        SourceOutcome::Returned(value) => {
            visitor(&format!("{path}/outcome/kind"), &s("Returned"))?;
            native_main_row_visit_value(&format!("{path}/outcome/value"), value, visitor)?;
        }
        SourceOutcome::Thrown(error) => {
            visitor(&format!("{path}/outcome/kind"), &s("Thrown"))?;
            visitor(&format!("{path}/outcome/name"), &s(&error.name))?;
            visitor(&format!("{path}/outcome/message"), &s(&error.message))?;
        }
    }
    visitor(
        &format!("{path}/hasUpdate"),
        &V::Bool(evaluation.update.is_some()),
    )?;
    if let Some(update) = &evaluation.update {
        visitor(&format!("{path}/update/target"), &s(&update.call.target))?;
        visitor(
            &format!("{path}/update/argumentCount"),
            &s(update.call.arguments.len().to_string()),
        )?;
        for (at, value) in update.call.arguments.iter().enumerate() {
            native_main_row_visit_value(&format!("{path}/update/arguments/{at}"), value, visitor)?;
        }
        native_main_row_visit_return(
            &format!("{path}/update/returned"),
            &update.returned,
            visitor,
        )?;
    }
    Ok(())
}
impl SourceContractRuntime {
    pub(crate) fn native_main_catchup_continuation_preflight(&self) -> Result<(), SourceRuleError> {
        if self.core.native_main_binding.is_none() || self.core.source_main_service_enabled() {
            Err(SourceRuleError::UnknownDependency)
        } else {
            Ok(())
        }
    }
    pub(crate) fn native_main_catchup_begin(
        &mut self,
        start: V,
    ) -> Result<NativeMainCatchupContinuationV1, SourceRuleError> {
        self.native_main_catchup_continuation_preflight()?;
        let invocation = self.source_main_invocation_begin(&DependencyCall {
            target: "Main.constructConsensusFromIndex".into(),
            arguments: vec![start.clone()],
        })?;
        Ok(NativeMainCatchupContinuationV1 {
            invocation: Some(invocation),
            start,
            plan: None,
            grouping: None,
            funding: None,
            trade: None,
            row: None,
            pc: NativeMainCatchupPcV1::Prepare,
            h: 0.,
            last: 0.,
            max: 0.,
            iterations: 0,
            funding_skips: 0,
            trade_skips: 0,
            total: 0,
            skip: true,
            cumulative: V::Undefined,
            volume: V::Undefined,
            consumed: 0,
            validity: vec![],
            result: None,
            stopped: None,
            storage_stop: None,
        })
    }
    fn native_main_catchup_finish(
        &mut self,
        frame: &mut NativeMainCatchupContinuationV1,
        result: R<V>,
    ) {
        frame.result = Some(self.source_main_invocation_finish(
            frame.invocation.take().expect("linear catchup invocation"),
            result,
        ));
    }
    fn native_main_catchup_fault(
        &mut self,
        frame: &mut NativeMainCatchupContinuationV1,
        fault: Fault,
    ) {
        match fault {
            Fault::Thrown(error) => {
                self.native_main_catchup_finish(frame, Err(Fault::Thrown(error)))
            }
            Fault::Local(error) => frame.stopped = Some(error),
        }
    }
    pub(crate) fn native_main_catchup_advance(
        &mut self,
        index: &mut SourceCompactIndex,
        frame: &mut NativeMainCatchupContinuationV1,
    ) -> NativeMainRowStopV1 {
        loop {
            if frame.result.is_some() {
                return NativeMainRowStopV1::Complete;
            }
            if let Some(error) = &frame.storage_stop {
                return NativeMainRowStopV1::StorageStopped(error.clone());
            }
            if let Some(error) = &frame.stopped {
                return NativeMainRowStopV1::SourceStopped(error.clone());
            }
            match frame.pc {
                NativeMainCatchupPcV1::Prepare => match self
                    .native_main_catchup_prepare_continuation(
                        frame.start.clone(),
                        index,
                        &mut frame.grouping,
                        &mut frame.storage_stop,
                    ) {
                    Ok(Some(NativeMainCatchupPreparedV1 { plan, h, last })) => {
                        frame.plan = Some(plan);
                        frame.h = h;
                        frame.last = last;
                        frame.max = h - 1.;
                        frame.pc = NativeMainCatchupPcV1::Height;
                    }
                    Ok(None) => self.native_main_catchup_finish(frame, Ok(V::Undefined)),
                    Err(error) => self.native_main_catchup_fault(frame, error),
                },
                NativeMainCatchupPcV1::Height => {
                    if frame.h > frame.last {
                        frame.pc = NativeMainCatchupPcV1::SaveHeight;
                        continue;
                    }
                    if frame.iterations >= crate::txindex_source_owner::MAX_BLOCKS {
                        frame.stopped = Some(SourceRuleError::Capacity);
                        continue;
                    }
                    frame.iterations += 1;
                    let Some(production) = self.core.main_production.as_mut() else {
                        frame.stopped = Some(SourceRuleError::InvalidInput);
                        continue;
                    };
                    production.parse_block = n(frame.h);
                    frame.skip = true;
                    frame.funding_skips = 0;
                    frame.trade_skips = 0;
                    frame.total = 0;
                    frame.cumulative = V::Undefined;
                    frame.volume = V::Undefined;
                    let plan = frame.plan.as_ref().expect("actual fixed grouping");
                    if plan.heights().any(|height| height == frame.h as u64) {
                        let funding = plan.phase(frame.h as u64, true);
                        let trade = plan.phase(frame.h as u64, false);
                        frame.total = funding.remaining_rows() + trade.remaining_rows();
                        frame.funding = Some(funding);
                        frame.trade = Some(trade);
                        frame.pc = NativeMainCatchupPcV1::Funding;
                    } else {
                        frame.pc = NativeMainCatchupPcV1::Skip;
                    }
                }
                NativeMainCatchupPcV1::Funding | NativeMainCatchupPcV1::Trade => {
                    let funding = matches!(frame.pc, NativeMainCatchupPcV1::Funding);
                    if frame.row.is_none() {
                        let cursor = if funding {
                            &mut frame.funding
                        } else {
                            &mut frame.trade
                        };
                        match cursor.as_mut().expect("actual fixed phase").next_row() {
                            Ok(Some(row)) => {
                                frame.row = Some(Self::native_main_row_inside(row, n(frame.h)))
                            }
                            Ok(None) => {
                                frame.pc = if funding {
                                    NativeMainCatchupPcV1::Trade
                                } else {
                                    NativeMainCatchupPcV1::Skip
                                };
                                continue;
                            }
                            Err(error) => {
                                frame.storage_stop = Some(error.to_string());
                                continue;
                            }
                        }
                    }
                    let stop = self.native_main_rows_advance(
                        index,
                        frame.row.as_mut().expect("actual active row"),
                    );
                    frame
                        .validity
                        .append(&mut frame.row.as_mut().expect("actual active row").validity);
                    if stop != NativeMainRowStopV1::Complete {
                        return stop;
                    }
                    let row = frame.row.take().expect("completed actual row");
                    frame.consumed += row.consumed_rows;
                    frame.validity.extend(row.validity);
                    match row.result.expect("completed row result") {
                        Ok(
                            DependencyReturn::Pending(value) | DependencyReturn::Immediate(value),
                        ) => match number(&value) {
                            Ok(skips) => {
                                if funding {
                                    frame.funding_skips += skips as u64;
                                } else {
                                    frame.trade_skips += skips as u64;
                                }
                            }
                            Err(error) => self.native_main_catchup_fault(frame, error),
                        },
                        Ok(
                            DependencyReturn::PendingThrow(error) | DependencyReturn::Throw(error),
                        ) => self.native_main_catchup_fault(frame, Fault::Thrown(error)),
                        Err(error) => frame.stopped = Some(error),
                    }
                }
                NativeMainCatchupPcV1::Skip => {
                    if frame.funding.is_some() {
                        frame.skip = frame.funding_skips + frame.trade_skips >= frame.total;
                    }
                    if frame.h % 10000. == 0. {
                        frame.skip = false;
                    }
                    frame.pc = if frame.skip {
                        NativeMainCatchupPcV1::NextHeight
                    } else {
                        NativeMainCatchupPcV1::Flush
                    };
                }
                NativeMainCatchupPcV1::Flush
                | NativeMainCatchupPcV1::Rewards
                | NativeMainCatchupPcV1::Withdrawals
                | NativeMainCatchupPcV1::Clearing => {
                    let (target, next, args) = match frame.pc {
                        NativeMainCatchupPcV1::Flush => (
                            "Tally.flushQueuedFeeAccruals",
                            NativeMainCatchupPcV1::Rewards,
                            vec![n(frame.h)],
                        ),
                        NativeMainCatchupPcV1::Rewards => (
                            "Logic.settleLiquidityRewards",
                            NativeMainCatchupPcV1::Orders,
                            vec![n(frame.h)],
                        ),
                        NativeMainCatchupPcV1::Withdrawals => (
                            "Channels.processWithdrawals",
                            NativeMainCatchupPcV1::Clearing,
                            vec![n(frame.h)],
                        ),
                        _ => (
                            "Clearing.clearingFunction",
                            NativeMainCatchupPcV1::NextHeight,
                            vec![n(frame.h), V::Bool(false)],
                        ),
                    };
                    match self.source_main_phase(target, args) {
                        Ok(_) => frame.pc = next,
                        Err(error) => self.native_main_catchup_fault(frame, error),
                    }
                }
                NativeMainCatchupPcV1::Orders | NativeMainCatchupPcV1::ChannelTrades => {
                    let (target, next) = if matches!(frame.pc, NativeMainCatchupPcV1::Orders) {
                        (
                            "Orderbook.processQueuedOnChainOrdersForBlock",
                            NativeMainCatchupPcV1::ChannelTrades,
                        )
                    } else {
                        (
                            "Orderbook.processQueuedChannelTradesForBlock",
                            NativeMainCatchupPcV1::Cumulative,
                        )
                    };
                    self.core.emit(obj(&[
                        ("kind", s("main-phase")),
                        ("target", s(target)),
                        ("args", V::Array(vec![n(frame.h)])),
                    ]));
                    match self
                        .evaluate_queue(&SourceCall {
                            method: target
                                .strip_prefix("Orderbook.")
                                .expect("literal source target")
                                .into(),
                            arguments: vec![n(frame.h)],
                        })
                        .map_err(Fault::Local)
                        .and_then(outcome)
                    {
                        Ok(_) => frame.pc = next,
                        Err(error) => self.native_main_catchup_fault(frame, error),
                    }
                }
                NativeMainCatchupPcV1::Cumulative => {
                    match self.core.source_liquidity_cumulative_volumes(n(frame.h)) {
                        Ok(value) => {
                            frame.cumulative = value;
                            frame.pc = NativeMainCatchupPcV1::Volumes;
                        }
                        Err(error) => self.native_main_catchup_fault(frame, error),
                    }
                }
                NativeMainCatchupPcV1::Volumes => {
                    match self.core.source_main_block_volumes(n(frame.h)) {
                        Ok(value) => {
                            frame.volume = value;
                            frame.pc = NativeMainCatchupPcV1::Vesting;
                        }
                        Err(error) => self.native_main_catchup_fault(frame, error),
                    }
                }
                NativeMainCatchupPcV1::Vesting => {
                    let result = (|| {
                        if number(&funding_prop(&frame.volume, "global")?)? > 0. {
                            self.core.source_main_update_vesting(
                                frame.cumulative.clone(),
                                frame.volume.clone(),
                                n(frame.h),
                            )?;
                        }
                        Ok(())
                    })();
                    match result {
                        Ok(()) => frame.pc = NativeMainCatchupPcV1::Withdrawals,
                        Err(error) => self.native_main_catchup_fault(frame, error),
                    }
                }
                NativeMainCatchupPcV1::NextHeight => {
                    frame.max = frame.h;
                    let next = frame.h + 1.;
                    if next <= frame.h || !next.is_finite() {
                        frame.stopped = Some(SourceRuleError::UnsupportedNumericDomain);
                        continue;
                    }
                    frame.h = next;
                    frame.funding = None;
                    frame.trade = None;
                    frame.pc = NativeMainCatchupPcV1::Height;
                }
                NativeMainCatchupPcV1::SaveHeight => match self
                    .core
                    .source_main_height_write("MaxProcessedHeight", n(frame.max))
                {
                    Ok(_) => frame.pc = NativeMainCatchupPcV1::Tail,
                    Err(error) => self.native_main_catchup_fault(frame, error),
                },
                NativeMainCatchupPcV1::Tail => {
                    // The preflight excluded the legacy auto-drained live loop.
                    let result: R<V> = self
                        .source_main_production_lag()
                        .and_then(|_| Err(SourceRuleError::UnknownDependency.into()));
                    match result {
                        Ok(value) => self.native_main_catchup_finish(frame, Ok(value)),
                        Err(error) => self.native_main_catchup_fault(frame, error),
                    }
                }
            }
        }
    }
    pub(crate) fn native_main_catchup_append(
        &mut self,
        index: &mut SourceCompactIndex,
        frame: &mut NativeMainCatchupContinuationV1,
    ) -> NativeMainRowStopV1 {
        let Some(row) = &mut frame.row else {
            return NativeMainRowStopV1::SourceStopped(SourceRuleError::InvalidInput);
        };
        self.native_main_rows_append(index, row)
    }
    pub(crate) fn native_main_catchup_resolve_post(
        &mut self,
        index: &mut SourceCompactIndex,
        frame: &mut NativeMainCatchupContinuationV1,
    ) -> NativeMainRowStopV1 {
        let Some(row) = &mut frame.row else {
            return NativeMainRowStopV1::SourceStopped(SourceRuleError::InvalidInput);
        };
        let stop = self.native_main_rows_resolve_post(index, row);
        if stop == NativeMainRowStopV1::Complete {
            self.native_main_catchup_advance(index, frame)
        } else {
            stop
        }
    }
}
