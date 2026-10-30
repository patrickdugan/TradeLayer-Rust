pub const ORDINARY_VALIDITY_OPERATION_WAL_PROFILE: &str =
    "TL-MAIN-ORDINARY-VALIDITY-OPERATION-WAL-13-1";
pub const SEGMENTED_VALIDITY_FACT_WAL_PROFILE: &str = "TL-MAIN-SEGMENTED-VALIDITY-FACTS-WAL-13-1";
pub const ACTUAL_VALIDITY_PLAN_FACT_PROFILE: &str = "TL-TXINDEX-ACTUAL-VALIDITY-PLAN-13-1";
pub const ACTUAL_VALIDITY_FACT_SCOPE: &str = "OrdinaryValidityNativeAppend-1";
pub struct ActualValidityFactBindingV1 {
    owner: u64,
    wal: ActiveServiceWalPinV1,
    context_sha256: [u8; 32],
    record_sha256: [u8; 32],
    original: crate::source_compact_index_v2::SourceIndexHistoryPinV1,
    read_context: V,
    candidates: V,
    mutations: Vec<u8>,
}
pub struct ActualValidityPendingFactV1 {
    raw: Vec<u8>,
}
impl ActualValidityPendingFactV1 {
    pub fn canonical_fact_bytes(&self) -> &[u8] {
        &self.raw
    }
}
impl ActiveServiceWalV1 {
    pub fn bind_actual_validity_fact(
        &self,
        index: &crate::source_compact_index_v2::SourceCompactIndex,
        plan: &crate::source_compact_index_v2::NativePreparedValidityWriteV1,
    ) -> Result<ActualValidityFactBindingV1> {
        let origin = self.origin_context()?.get("origin");
        let operation = origin.get("profile") == s(ORDINARY_VALIDITY_OPERATION_WAL_PROFILE)
            && origin.get("operationSourceScope") == s("RetainedOrdinaryValidityAction-1");
        let mixed = mixed_native_fact_origin(&origin);
        if !mixed
            && ((origin.get("profile") != s(SEGMENTED_VALIDITY_FACT_WAL_PROFILE) && !operation)
                || origin.get("requiredNativeFactProfile") != s(ACTUAL_VALIDITY_PLAN_FACT_PROFILE)
                || origin.get("actualNativeFactScope") != s(ACTUAL_VALIDITY_FACT_SCOPE))
        {
            return Err(ActiveServiceWalErrorV1::WrongContext);
        }
        mode4_namespace(&self.namespace, index.native_namespace())?;
        let original = index
            .capture_verified_history_pin()
            .map_err(|_| ActiveServiceWalErrorV1::WrongPin)?;
        let actual = index
            .native_store_context()
            .map_err(|_| ActiveServiceWalErrorV1::WrongPin)?;
        let context = plan.durable_read_context();
        if plan.write_expected() != &original.metadata
            || index
                .native_complete_metadata_state()
                .map_err(|_| ActiveServiceWalErrorV1::WrongPin)?
                != *plan.expected_before_state()
            || plan.append_plan().catalog_context().0 != actual.0.as_str()
            || plan.append_plan().catalog_context().1 != actual.1.as_path()
        {
            return Err(ActiveServiceWalErrorV1::WrongPin);
        }
        let facts=crate::source_main_native_intent_store::NativeWriteIntentCandidatesV1::from_prepared_append(plan.append_plan(),context.clone()).map_err(|_|ActiveServiceWalErrorV1::WrongPlan)?;
        Ok(ActualValidityFactBindingV1 {
            owner: self.owner,
            wal: self.pin()?,
            context_sha256: sha256(&self.context),
            record_sha256: sha256(&bytes(&self.record()?)?),
            original,
            read_context: context,
            candidates: facts.independently_retained_facts().clone(),
            mutations: facts.exact_prepared_mutations().to_vec(),
        })
    }
}
impl ActualValidityFactBindingV1 {
    pub(crate) fn require_synced_intent(&self, wal: &ActiveServiceWalV1) -> Result<()> {
        let record = wal.record()?;
        let origin = wal.origin_context()?.get("origin");
        let operation = origin.get("profile") == s(ORDINARY_VALIDITY_OPERATION_WAL_PROFILE)
            && origin.get("operationSourceScope") == s("RetainedOrdinaryValidityAction-1");
        let mixed = mixed_native_fact_origin(&origin);
        let (context_key, history_key, candidates_key, mutations_key) = if mixed {
            (
                "ordinaryNativeReadContext",
                "ordinaryNativeBeforeHistory",
                "ordinaryNativeCandidates",
                "ordinaryNativeMutations",
            )
        } else {
            (
                "nativeReadContext",
                "nativeBeforeHistory",
                "nativeCandidates",
                "nativeMutations",
            )
        };
        if record.get("profile") != origin.get("profile")
            || !(record.get("stage") == s("Intent")
                || (mixed
                    && matches!(record.get("stage"),V::String(ref stage) if stage=="NativeSupersessionIntent"||stage=="EscrowBoundIntent"))
                || (operation
                    && record.get("stage") == s("NativeSupersessionIntent")
                    && record.get("operationProfile")
                        == s("TL-MAIN-ORDINARY-VALIDITY-OPERATION-13-1")))
            || record.get(context_key) != self.read_context
        {
            return Err(ActiveServiceWalErrorV1::WrongContext);
        }
        let history = self
            .original
            .canonical_scalar_bytes()
            .map_err(|_| ActiveServiceWalErrorV1::WrongPin)?;
        for (name, raw) in [
            (history_key, history),
            (candidates_key, bytes(&self.candidates)?),
            (mutations_key, self.mutations.clone()),
        ] {
            let pin = ActiveServiceWalBlobV1::of(&raw)?;
            if record.get(name) != pin.value() || wal.read_blob(&pin)? != raw {
                return Err(ActiveServiceWalErrorV1::WrongPin);
            }
        }
        Ok(())
    }
    pub fn capture_pending_fact(
        &self,
        wal: &ActiveServiceWalV1,
        index: &crate::source_compact_index_v2::SourceCompactIndex,
        plan: &crate::source_compact_index_v2::NativePreparedValidityWriteV1,
    ) -> Result<ActualValidityPendingFactV1> {
        let current = wal.bind_actual_validity_fact(index, plan)?;
        if self.owner != current.owner
            || self.wal != current.wal
            || self.context_sha256 != current.context_sha256
            || self.record_sha256 != current.record_sha256
            || self.original != current.original
            || self.read_context != current.read_context
            || self.candidates != current.candidates
            || self.mutations != current.mutations
        {
            return Err(ActiveServiceWalErrorV1::WrongPlan);
        }
        let raw = bytes(&obj(&[
            ("profile", s(ACTUAL_VALIDITY_PLAN_FACT_PROFILE)),
            ("kind", s("ActualPendingValidityPlan")),
            ("actualWal", actual_fact_wal_value(&self.wal)),
            ("actualContextSHA256", s(hex(&self.context_sha256))),
            ("actualRecordSHA256", s(hex(&self.record_sha256))),
            (
                "originalHistoryHex",
                s(hex(&self
                    .original
                    .canonical_scalar_bytes()
                    .map_err(|_| ActiveServiceWalErrorV1::WrongPin)?)),
            ),
            ("nativeReadContext", self.read_context.clone()),
            ("nativeCandidates", self.candidates.clone()),
            ("exactPreparedMutationsHex", s(hex(&self.mutations))),
        ]))?;
        if raw.len() > MAX_BLOB {
            return Err(ActiveServiceWalErrorV1::Capacity);
        }
        Ok(ActualValidityPendingFactV1 { raw })
    }
}
