// Derived from the pinned actual catchup body by prepare_runtime.py.
impl SourceContractRuntime {
    fn native_main_catchup(
        &mut self,
        start: V,
        index: &mut crate::source_compact_index_v2::SourceCompactIndex,
        grouping: &mut Option<crate::source_compact_index_v2::RowStreamReceipt>,
        validity: &mut Vec<crate::source_compact_index_v2::StreamedValidityEvaluation>,
        consumed: &mut u64,
        storage_stop: &mut Option<String>,
    ) -> R<V> {
        if !matches!(&start,V::Number(bits) if f64::from_bits(*bits).is_finite() && f64::from_bits(*bits)>=0. && f64::from_bits(*bits).fract()==0.)
        {
            return Err(SourceRuleError::UnsupportedNumericDomain.into());
        }
        self.core
            .native_main_index(index, "findMaxIndexedBlock", vec![])?;
        self.core.database("txIndex")?;
        self.core.source_send_tally_instance_initialized = true;
        // Original Tally.getInstance is unawaited: its load factory executes
        // before Main's consensus factory, and its read resumes at that await.
        let tally_pending = match self.core.database("tallyMap") {
            Ok(_) => true,
            Err(Fault::Thrown(_)) => false,
            Err(e) => return Err(e),
        };
        let consensus_factory = self.core.database("consensus");
        if tally_pending {
            let loaded = self.core.find("tallyMap", obj(&[("_id", s("tallyMap"))]));
            match loaded.and_then(|row| ContractCore::tally_parsed_entries(&row)) {
                Ok(entries) => {
                    self.core.tally = entries;
                    self.core.tally_static_initialized = true;
                }
                Err(Fault::Thrown(_)) => (),
                Err(e) => return Err(e),
            }
        }
        consensus_factory?;
        self.core
            .find("consensus", obj(&[("_id", s("MaxProcessedHeight"))]))?;
        let pin = index
            .snapshot()
            .map_err(|e| native_main_store_fault(e, storage_stop))?;
        let read = index
            .read_generation(&pin)
            .map_err(|e| native_main_store_fault(e, storage_stop))?;
        let plan = match read
            .plan_main_groups()
            .map_err(|e| native_main_store_fault(e, storage_stop))?
        {
            crate::source_compact_index_v2::MainIndexPlanningOutcome::Ready(plan) => {
                *grouping = Some(plan.receipt().clone());
                plan
            }
            crate::source_compact_index_v2::MainIndexPlanningOutcome::Thrown {
                exception,
                receipt,
            } => {
                *grouping = Some(*receipt);
                return Err(Fault::Thrown(exception));
            }
        };
        let last_value = plan.max_index_block();
        if last_value == V::Null {
            return Ok(V::Undefined);
        }
        let last = number(&last_value)?;
        if last == 0. || last.is_nan() {
            return Ok(V::Undefined);
        }
        let mut h = number(&start)?;
        const MAX_FINITE_MAIN_HEIGHT: f64 = 9_007_199_254_740_991.;
        if h > MAX_FINITE_MAIN_HEIGHT || last > MAX_FINITE_MAIN_HEIGHT {
            return Err(SourceRuleError::UnsupportedNumericDomain.into());
        }
        if !h.is_finite() || h.fract() != 0. || h < 0. || !last.is_finite() || last - h > 1024. {
            return Err(SourceRuleError::Capacity.into());
        }
        let mut max = h - 1.;
        let mut iterations = 0usize;
        while h <= last {
            if iterations >= crate::txindex_source_owner::MAX_BLOCKS {
                return Err(SourceRuleError::Capacity.into());
            }
            iterations += 1;
            self.core
                .main_production
                .as_mut()
                .ok_or(SourceRuleError::InvalidInput)?
                .parse_block = n(h);
            let mut skip = true;
            if plan.heights().any(|height| height == h as u64) {
                let mut funding = plan.phase(h as u64, true);
                let mut trade = plan.phase(h as u64, false);
                let total = funding.remaining_rows() + trade.remaining_rows();
                let one = self.native_main_process_phase_inner(
                    index,
                    &mut funding,
                    n(h),
                    validity,
                    consumed,
                    storage_stop,
                )?;
                let two = self.native_main_process_phase_inner(
                    index,
                    &mut trade,
                    n(h),
                    validity,
                    consumed,
                    storage_stop,
                )?;
                skip = number(&one)? + number(&two)? >= total as f64;
            }
            if h % 10000. == 0. {
                skip = false;
            }
            if !skip {
                for target in [
                    "Tally.flushQueuedFeeAccruals",
                    "Logic.settleLiquidityRewards",
                ] {
                    self.source_main_phase(target, vec![n(h)])?;
                }
                for target in [
                    "Orderbook.processQueuedOnChainOrdersForBlock",
                    "Orderbook.processQueuedChannelTradesForBlock",
                ] {
                    self.core.emit(obj(&[
                        ("kind", s("main-phase")),
                        ("target", s(target)),
                        ("args", V::Array(vec![n(h)])),
                    ]));
                    self.evaluate_queue(&SourceCall {
                        method: target.strip_prefix("Orderbook.").unwrap().into(),
                        arguments: vec![n(h)],
                    })
                    .map_err(Fault::Local)
                    .and_then(outcome)?;
                }
                let cumulative = self.core.source_liquidity_cumulative_volumes(n(h))?;
                let volume = self.core.source_main_block_volumes(n(h))?;
                if number(&funding_prop(&volume, "global")?)? > 0. {
                    self.core
                        .source_main_update_vesting(cumulative, volume, n(h))?;
                }
                self.source_main_phase("Channels.processWithdrawals", vec![n(h)])?;
                self.source_main_phase("Clearing.clearingFunction", vec![n(h), V::Bool(false)])?;
            }
            max = h;
            let next = h + 1.;
            if next <= h || !next.is_finite() {
                return Err(SourceRuleError::UnsupportedNumericDomain.into());
            }
            h = next;
        }
        self.core
            .source_main_height_write("MaxProcessedHeight", n(max))?;
        if self.core.source_main_service_enabled() {
            return self.source_main_service_sync(&[]);
        }
        self.source_main_production_lag()?;
        // Actual success here starts processIncomingBlocks without awaiting it.
        // This finite phase profile has no stable boundary for that live loop.
        Err(SourceRuleError::UnknownDependency.into())
    }
}
