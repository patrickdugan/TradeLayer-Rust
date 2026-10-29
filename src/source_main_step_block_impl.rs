// Derived literal Main block begin/mid/end over the SAME native index and Core.
// Row execution is delegated unchanged to the native13 owner callback seam.
impl SourceContractRuntime {
    fn source_main_step_begin(
        &mut self,
        index: &mut crate::source_compact_index_v2::SourceCompactIndex,
        block: V,
        height: V,
        validity: &mut Vec<crate::source_compact_index_v2::StreamedValidityEvaluation>,
        consumed_rows: &mut u64,
        storage_stop: &mut Option<String>,
    ) -> R<V> {
        self.core.source_main_service_persistence()?;
        let reorg = self.core.source_main_service_reorg_check(
            height.clone(),
            funding_prop(&block, "previousblockhash")?,
        )?;
        if truth(&reorg) {
            let info = self.core.source_main_service_reorg_no_snapshots()?;
            if matches!(info.get("restoredFrom"), V::Number(_)) {
                return Ok(info.get("restoredFrom"));
            }
        }
        self.core.database("persistence")?;
        self.core.update(
            "persistence",
            obj(&[("_id", s(format!("block-{}", text(&height)?)))]),
            obj(&[(
                "$set",
                obj(&[
                    ("height", height.clone()),
                    ("hash", funding_prop(&block, "hash")?),
                    ("prevHash", funding_prop(&block, "previousblockhash")?),
                    ("createdAt", n(self.core.time_millis as f64)),
                ]),
            )]),
            true,
        )?;
        self.core.source_main_snapshot_checkpoint(height.clone())?;
        let txs = array(&self.core.native_main_index(
            index,
            "processBlockData",
            vec![block, height.clone()],
        )?)?;
        let (mut funding, mut other) = (vec![], vec![]);
        for tx in txs {
            let payload = funding_prop(&tx, "payload")?;
            let V::String(payload) = payload else {
                return Err(type_error("tx.payload.startsWith is not a function"));
            };
            if payload.starts_with('4') || payload.starts_with('m') {
                funding.push(tx);
            } else {
                other.push(tx);
            }
        }
        if !funding.is_empty() {
            self.source_main_step_rows(
                index,
                V::Array(funding),
                height,
                validity,
                consumed_rows,
                storage_stop,
            )?;
        }
        Ok(V::Array(other))
    }
    fn source_main_step_block(
        &mut self,
        index: &mut crate::source_compact_index_v2::SourceCompactIndex,
        block: V,
        height: V,
        validity: &mut Vec<crate::source_compact_index_v2::StreamedValidityEvaluation>,
        consumed_rows: &mut u64,
        storage_stop: &mut Option<String>,
    ) -> R<V> {
        let tx = self.source_main_step_begin(
            index,
            block.clone(),
            height.clone(),
            validity,
            consumed_rows,
            storage_stop,
        )?;
        if matches!(tx, V::Number(_)) {
            return Ok(obj(&[("reOrg", V::Bool(true)), ("restore", tx)]));
        }
        let height =
            self.source_main_step_mid(index, &[tx, height], validity, consumed_rows, storage_stop)?;
        self.source_main_production_dispatch(
            "Main.blockHandlerEnd",
            &[funding_prop(&block, "hash")?, height.clone()],
        )?;
        Ok(obj(&[("reOrg", V::Bool(false)), ("restore", height)]))
    }
    fn source_main_step_mid(
        &mut self,
        index: &mut crate::source_compact_index_v2::SourceCompactIndex,
        a: &[V],
        validity: &mut Vec<crate::source_compact_index_v2::StreamedValidityEvaluation>,
        consumed_rows: &mut u64,
        storage_stop: &mut Option<String>,
    ) -> R<V> {
        let rows = arg(a, 0);
        let height = arg(a, 1);
        if !matches!(height,V::Number(bits)if f64::from_bits(bits).is_finite()&&f64::from_bits(bits)>=0.&&f64::from_bits(bits).fract()==0.&&f64::from_bits(bits)<=9_007_199_254_740_991.)
        {
            return Err(SourceRuleError::InvalidInput.into());
        }
        let reward_queue = self.core.source_liquidity_rewards_state().get("queue");
        if (!self.core.spot_lifecycle_enabled() && !self.core.fee_buffer.is_empty())
            || !matches!(reward_queue,V::Array(ref v)if v.is_empty())
        {
            return Err(SourceRuleError::UnknownDependency.into());
        }
        let mut did_work = false;
        if truth(&rows) {
            let V::Array(values) = &rows else {
                return Err(SourceRuleError::InvalidInput.into());
            };
            if !values.is_empty() {
                match self.source_main_step_rows(
                    index,
                    rows,
                    height.clone(),
                    validity,
                    consumed_rows,
                    storage_stop,
                ) {
                    Ok(v) => {
                        did_work = truth(&v);
                        self.core.source_main_start_height_job(height.clone())?;
                    }
                    Err(Fault::Thrown(_)) => (),
                    Err(e) => return Err(e),
                }
            }
        }
        if did_work {
            self.source_main_phase("Tally.flushQueuedFeeAccruals", vec![height.clone()])?;
            self.source_main_phase("Logic.settleLiquidityRewards", vec![height.clone()])?;
            // Exact completed-drain39 empty-buffer suspension: source height
            // continuation starts after rewards invocation, before queue drain.
            self.core.source_main_drain_height_job()?;
            self.core.emit(obj(&[
                ("kind", s("main-phase")),
                ("target", s("Orderbook.processQueuedOnChainOrdersForBlock")),
                ("args", V::Array(vec![height.clone()])),
            ]));
            self.evaluate_queue(&SourceCall {
                method: "processQueuedOnChainOrdersForBlock".into(),
                arguments: vec![height.clone()],
            })
            .map_err(Fault::Local)
            .and_then(outcome)?;
            self.core.emit(obj(&[
                ("kind", s("main-phase")),
                ("target", s("Orderbook.processQueuedChannelTradesForBlock")),
                ("args", V::Array(vec![height.clone()])),
            ]));
            self.evaluate_queue(&SourceCall {
                method: "processQueuedChannelTradesForBlock".into(),
                arguments: vec![height.clone()],
            })
            .map_err(Fault::Local)
            .and_then(outcome)?;
            self.source_main_phase(
                "Clearing.clearingFunction",
                vec![height.clone(), V::Bool(false)],
            )?;
        }
        self.source_main_phase("Channels.processWithdrawals", vec![height.clone()])?;
        Ok(height)
    }
}
