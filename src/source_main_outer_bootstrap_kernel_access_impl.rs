pub(crate) struct NativeMainOuterQueryRequestViewV1<'a> {
    pub(crate) collection: &'static str,
    pub(crate) method: &'static str,
    pub(crate) arguments: &'a [V],
}
impl NativeMainOuterBootstrapAwaitV1 {
    pub(crate) fn source_site_name(self) -> &'static str {
        use NativeMainOuterBootstrapSiteV1 as Site;
        match self.site {
            Site::FindMaxFactory => "FindMaxFactory",
            Site::FindMaxQuery => "FindMaxQuery",
            Site::FindMaxResult => "FindMaxResult",
            Site::TxIndexFactory => "TxIndexFactory",
            Site::TallyFactory => "TallyFactory",
            Site::TallyQuery => "TallyQuery",
            Site::TallyLoadResult => "TallyLoadResult",
            Site::ConsensusFactory => "ConsensusFactory",
            Site::MaxProcessedQuery => "MaxProcessedQuery",
            Site::MaxProcessedResult => "MaxProcessedResult",
            Site::FixedRead => "FixedRead",
        }
    }
    pub(crate) fn lane_name(self) -> &'static str {
        match self.lane {
            NativeMainOuterBootstrapLaneV1::Main => "Main",
            NativeMainOuterBootstrapLaneV1::Tally => "Tally",
        }
    }
}
impl NativeMainStartupDbQueryV1 {
    // Only the containing actual frame can expose this recorded raw request.
    fn outer_request_view(&self) -> NativeMainOuterQueryRequestViewV1<'_> {
        NativeMainOuterQueryRequestViewV1 {
            collection: self.collection,
            method: self.method,
            arguments: &self.arguments,
        }
    }
}
impl NativeMainOuterBootstrapV1 {
    pub(crate) fn actual_tally_instance_completion(
        &self,
    ) -> Option<NativeMainOuterActualCompletionV1<'_>> {
        self.tally_instance
            .as_ref()
            .map(NativeMainOuterActualCompletionV1::Value)
    }
    pub(crate) fn actual_query_request_view(
        &self,
        await_: NativeMainOuterBootstrapAwaitV1,
    ) -> Result<NativeMainOuterQueryRequestViewV1<'_>, SourceRuleError> {
        self.require_issued(await_)?;
        use NativeMainOuterBootstrapSiteV1 as Site;
        match await_.site {
            Site::TallyQuery => Ok(self
                .tally_query
                .as_ref()
                .ok_or(SourceRuleError::InvalidInput)?
                .outer_request_view()),
            Site::MaxProcessedQuery => Ok(self
                .consensus_query
                .as_ref()
                .ok_or(SourceRuleError::InvalidInput)?
                .outer_request_view()),
            Site::FindMaxQuery | Site::FixedRead => {
                let query = if await_.site == Site::FindMaxQuery {
                    self.find_max_query.as_ref()
                } else {
                    self.fixed_query.as_ref()
                }
                .ok_or(SourceRuleError::InvalidInput)?;
                let (method, arguments) = query.outer_startup_request_view();
                Ok(NativeMainOuterQueryRequestViewV1 {
                    collection: "txIndex",
                    method,
                    arguments,
                })
            }
            _ => Err(SourceRuleError::InvalidInput),
        }
    }
}
