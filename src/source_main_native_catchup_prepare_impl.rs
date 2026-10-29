// Actual existing native bootstrap/grouping calculation, without row IO.
struct NativeMainCatchupPreparedV1 {
    plan: Box<crate::source_compact_index_v2::MainIndexStreamPlan>,
    h: f64,
    last: f64,
}
impl SourceContractRuntime {
    fn native_main_catchup_prepare_continuation(
        &mut self,
        start: V,
        index: &mut SourceCompactIndex,
        grouping: &mut Option<crate::source_compact_index_v2::RowStreamReceipt>,
        storage_stop: &mut Option<String>,
    ) -> R<Option<NativeMainCatchupPreparedV1>> {
        self.native_main_catchup_prepare_with_metadata(
            start,
            &mut NativeMainLiveSourceMetadataAccess { index },
            grouping,
            storage_stop,
        )
    }
    fn native_main_catchup_prepare_with_metadata(
        &mut self,
        start: V,
        access: &mut impl NativeMainSourceMetadataAccess,
        grouping: &mut Option<crate::source_compact_index_v2::RowStreamReceipt>,
        storage_stop: &mut Option<String>,
    ) -> R<Option<NativeMainCatchupPreparedV1>> {
        if !matches!(&start,V::Number(bits) if f64::from_bits(*bits).is_finite() && f64::from_bits(*bits)>=0. && f64::from_bits(*bits).fract()==0.)
        {
            return Err(SourceRuleError::UnsupportedNumericDomain.into());
        }
        access.find_max(&mut self.core)?;
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
        self.core.check_failure("txIndex", "findAsync")?;
        let read = access
            .read_view()
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
            return Ok(None);
        }
        let last = number(&last_value)?;
        if last == 0. || last.is_nan() {
            return Ok(None);
        }
        let h = number(&start)?;
        const MAX_FINITE_MAIN_HEIGHT: f64 = 9_007_199_254_740_991.;
        if h > MAX_FINITE_MAIN_HEIGHT || last > MAX_FINITE_MAIN_HEIGHT {
            return Err(SourceRuleError::UnsupportedNumericDomain.into());
        }
        if !h.is_finite() || h.fract() != 0. || h < 0. || !last.is_finite() || last - h > 1024. {
            return Err(SourceRuleError::Capacity.into());
        }
        Ok(Some(NativeMainCatchupPreparedV1 { plan, h, last }))
    }
}
