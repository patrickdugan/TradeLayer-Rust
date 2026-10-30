pub(crate) struct NativeMainOuterQueryStatusV1 {
    pub(crate) request_rejected: bool,
    pub(crate) sampled: bool,
    pub(crate) callback_delivered: bool,
}
impl NativeMainOuterBootstrapV1 {
    pub(crate) fn query_status(
        &self,
        await_: NativeMainOuterBootstrapAwaitV1,
    ) -> Result<NativeMainOuterQueryStatusV1, SourceRuleError> {
        self.require(await_)?;
        use NativeMainOuterBootstrapSiteV1 as Site;
        let (failure, query, callback) = match await_.site {
            Site::FindMaxQuery => (
                &self.find_max_failure,
                &self.find_max_query,
                self.find_max_callback,
            ),
            Site::FixedRead => (&self.fixed_failure, &self.fixed_query, self.fixed_callback),
            Site::TallyQuery | Site::MaxProcessedQuery => {
                let query = if await_.site == Site::TallyQuery {
                    self.tally_query.as_ref()
                } else {
                    self.consensus_query.as_ref()
                }
                .ok_or(SourceRuleError::InvalidInput)?;
                return Ok(NativeMainOuterQueryStatusV1 {
                    request_rejected: query.request_rejected(),
                    sampled: query.sampled(),
                    callback_delivered: query.callback_delivered(),
                });
            }
            _ => return Err(SourceRuleError::InvalidInput),
        };
        Ok(NativeMainOuterQueryStatusV1 {
            request_rejected: failure.is_some(),
            sampled: query.as_ref().is_some_and(|q| q.sampled()),
            callback_delivered: callback,
        })
    }
    pub(crate) fn visit_canonical_fields(
        &self,
        visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
    ) -> Result<(), SourceRuleError> {
        visitor("profile", &s("TL-MAIN-ACTUAL-OUTER-BOOTSTRAP-AWAITS-13-1"))?;
        visitor("sourceJob", &s(self.source_job.to_string()))?;
        self.native_scope
            .visit_canonical_fields(&mut |path, value| {
                visitor(&format!("nativeScope/{path}"), value)
            })?;
        visitor(
            "tallyJob",
            &self.tally_job.map_or(V::Null, |n| s(n.to_string())),
        )?;
        visitor("nextOrdinal", &s(self.next_ordinal.to_string()))?;
        visitor("issued/count", &s(self.issued.len().to_string()))?;
        for (at, await_) in self.issued.iter().enumerate() {
            visitor(
                &format!("issued/{at}/sourceJob"),
                &s(await_.source_job.to_string()),
            )?;
            visitor(
                &format!("issued/{at}/lane"),
                &s(format!("{:?}", await_.lane)),
            )?;
            visitor(
                &format!("issued/{at}/site"),
                &s(format!("{:?}", await_.site)),
            )?;
            visitor(
                &format!("issued/{at}/ordinal"),
                &s(await_.ordinal.to_string()),
            )?;
        }
        for (path, flag) in [
            ("started", self.started),
            ("findMaxCallback", self.find_max_callback),
            ("fixedCallback", self.fixed_callback),
            ("tallyLoadComplete", self.tally_load_complete),
            ("preparedTaken", self.prepared_taken),
        ] {
            visitor(path, &V::Bool(flag))?;
        }
        for (path, await_) in [
            ("mainAwait", self.main_await),
            ("tallyAwait", self.tally_await),
        ] {
            visitor(&format!("{path}/present"), &V::Bool(await_.is_some()))?;
            if let Some(await_) = await_ {
                visitor(
                    &format!("{path}/sourceJob"),
                    &s(await_.source_job.to_string()),
                )?;
                visitor(&format!("{path}/lane"), &s(format!("{:?}", await_.lane)))?;
                visitor(&format!("{path}/site"), &s(format!("{:?}", await_.site)))?;
                visitor(&format!("{path}/ordinal"), &s(await_.ordinal.to_string()))?;
                visitor(&format!("{path}/methodScope"), &s(await_.method_scope()))?;
                visitor(
                    &format!("{path}/hostQuery"),
                    &V::Bool(await_.is_host_query()),
                )?;
            }
        }
        native_main_row_visit_value("start", &self.start, visitor)?;
        for (path, value) in [
            ("lastIndexedBlock", &self.last_indexed_block),
            ("lastConsensusHeight", &self.last_consensus_height),
            ("tallyInstance", &self.tally_instance),
            ("tallyCaughtUndefined", &self.tally_caught_undefined),
        ] {
            visitor(&format!("{path}/present"), &V::Bool(value.is_some()))?;
            if let Some(value) = value {
                native_main_row_visit_value(path, value, visitor)?;
            }
        }
        for (path, result) in [
            ("findMaxFactory", &self.find_max_factory),
            ("txIndexFactory", &self.tx_index_factory),
            ("tallyFactory", &self.tally_factory),
            ("consensusFactory", &self.consensus_factory),
            ("findMaxReturn", &self.find_max_return),
            ("maxProcessedReturn", &self.max_processed_return),
            ("tallyQueryReturn", &self.tally_query_return),
            ("consensusQueryReturn", &self.consensus_query_return),
        ] {
            visitor(&format!("{path}/present"), &V::Bool(result.is_some()))?;
            if let Some(result) = result {
                outer_bootstrap_visit_result(path, result, visitor)?;
            }
        }
        for (path, failure) in [
            ("findMaxFailure", &self.find_max_failure),
            ("fixedFailure", &self.fixed_failure),
        ] {
            visitor(&format!("{path}/present"), &V::Bool(failure.is_some()))?;
            if let Some(error) = failure {
                outer_bootstrap_visit_fault(path, error, visitor)?;
            }
        }
        for (path, query) in [
            ("findMaxQuery", &self.find_max_query),
            ("fixedQuery", &self.fixed_query),
        ] {
            visitor(&format!("{path}/present"), &V::Bool(query.is_some()))?;
            if let Some(query) = query {
                query.visit_canonical_fields(&mut |key, value| {
                    visitor(&format!("{path}/{key}"), value)
                })?;
            }
        }
        for (path, query) in [
            ("tallyQuery", &self.tally_query),
            ("consensusQuery", &self.consensus_query),
        ] {
            visitor(&format!("{path}/present"), &V::Bool(query.is_some()))?;
            if let Some(query) = query {
                query.visit_fields(&mut |key, value| visitor(&format!("{path}/{key}"), value))?;
            }
        }
        visitor(
            "tallyMap/present",
            &V::Bool(self.tally_returned_map.is_some()),
        )?;
        if let Some(entries) = &self.tally_returned_map {
            visitor("tallyMap/size", &s(entries.len().to_string()))?;
            for (at, (key, value)) in entries.iter().enumerate() {
                native_main_row_visit_value(&format!("tallyMap/{at}/key"), key, visitor)?;
                native_main_row_visit_value(&format!("tallyMap/{at}/value"), value, visitor)?;
            }
        }
        visitor(
            "tallySwallowed/present",
            &V::Bool(self.tally_swallowed.is_some()),
        )?;
        if let Some(error) = &self.tally_swallowed {
            visitor("tallySwallowed/name", &s(&error.name))?;
            visitor("tallySwallowed/message", &s(&error.message))?;
        }
        visitor("prepared/present", &V::Bool(self.prepared.is_some()))?;
        if let Some(result) = &self.prepared {
            match result {
                Ok(None) => visitor("prepared/kind", &s("FiniteEarlyReturn"))?,
                Ok(Some(prepared)) => {
                    visitor("prepared/kind", &s("ActualPlan"))?;
                    visitor("prepared/height", &n(prepared.h))?;
                    visitor("prepared/last", &n(prepared.last))?;
                    prepared.plan.visit_native_plan_fields(&mut |path, value| {
                        visitor(&format!("prepared/plan/{path}"), value)
                    })?;
                }
                Err(error) => outer_bootstrap_visit_fault("prepared", error, visitor)?,
            }
        }
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
        visitor(
            "storageStop",
            &self.storage_stop.as_ref().map_or(V::Null, s),
        )?;
        visitor(
            "ownerStop",
            &self
                .stopped
                .as_ref()
                .map_or(V::Null, |e| s(format!("{e:?}"))),
        )
    }
}
fn outer_bootstrap_visit_result(
    path: &str,
    result: &R<V>,
    visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
) -> Result<(), SourceRuleError> {
    match result {
        Ok(value) => {
            visitor(&format!("{path}/kind"), &s("Returned"))?;
            native_main_row_visit_value(&format!("{path}/value"), value, visitor)
        }
        Err(error) => outer_bootstrap_visit_fault(path, error, visitor),
    }
}
fn outer_bootstrap_visit_fault(
    path: &str,
    error: &Fault,
    visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
) -> Result<(), SourceRuleError> {
    match error {
        Fault::Thrown(error) => {
            visitor(&format!("{path}/kind"), &s("Thrown"))?;
            visitor(&format!("{path}/name"), &s(&error.name))?;
            visitor(&format!("{path}/message"), &s(&error.message))
        }
        Fault::Local(error) => {
            visitor(&format!("{path}/kind"), &s("LocalStop"))?;
            visitor(&format!("{path}/error"), &s(format!("{error:?}")))
        }
    }
}
