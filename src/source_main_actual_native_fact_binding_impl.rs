// Distinct opt-in metadata prerequisite; no old origin/profile is upgraded.
pub const ACTUAL_NATIVE_FACT_PROFILE: &str = "TL-MAIN-ACTUAL-NATIVE-FACT-13-1";
pub const ACTUAL_NATIVE_FACT_WAL_PROFILE: &str =
    "TL-MAIN-ACTIVE-SERVICE-ACTUAL-NATIVE-FACTS-WAL-13-1";
pub struct ActualNativeFactBindingV1 {
    owner: u64,
    wal: ActiveServiceWalPinV1,
    context_sha256: [u8; 32],
    record_sha256: [u8; 32],
    original: crate::source_compact_index_v2::SourceIndexHistoryPinV1,
    read_context: V,
    candidates: Option<V>,
    mutations: Vec<u8>,
}
pub struct ActualNativePendingPlanFactV1 {
    raw: Vec<u8>,
}
impl ActualNativePendingPlanFactV1 {
    pub fn canonical_fact_bytes(&self) -> &[u8] {
        &self.raw
    }
}
fn actual_fact_wal_value(pin: &ActiveServiceWalPinV1) -> V {
    let c = &pin.prefix.content;
    obj(&[
        ("profile", s(&pin.profile)),
        ("root", s(pin.root.to_string_lossy())),
        ("prefixProfile", s(&pin.prefix.profile)),
        (
            "namespace",
            s(pin.prefix.namespace.path(Path::new("")).to_string_lossy()),
        ),
        ("file", s(pin.prefix.canonical_file.to_string_lossy())),
        ("sequence", s(pin.prefix.physical_sequence.to_string())),
        (
            "kv",
            obj(&[
                ("profile", s(&c.profile)),
                ("keys", s(c.keys.to_string())),
                ("bytes", s(c.bytes.to_string())),
                ("complete", V::Bool(c.complete)),
                ("sha256d", s(hex(&c.chain_sha256d))),
            ]),
        ),
    ])
}
impl ActiveServiceWalV1 {
    pub fn bind_actual_inline_native_fact(
        &self,
        index: &crate::source_compact_index_v2::SourceCompactIndex,
        plan: &crate::source_compact_index_v2::NativePreparedInlineDatabaseV1,
    ) -> Result<ActualNativeFactBindingV1> {
        let origin = parse(&self.context)?.get("origin");
        let source_mode = origin.get("profile") == s(ACTUAL_SOURCE_FACT_WAL_PROFILE)
            && origin.get("actualNativeFactScope") == s(ACTUAL_SOURCE_FACT_SCOPE);
        if (origin.get("profile") != s(ACTUAL_NATIVE_FACT_WAL_PROFILE) && !source_mode)
            || origin.get("requiredNativeFactProfile") != s(ACTUAL_NATIVE_FACT_PROFILE)
        {
            return Err(ActiveServiceWalErrorV1::WrongContext);
        }
        let wal = self.pin()?;
        let record = bytes(&self.record()?)?;
        let original = index
            .capture_verified_history_pin()
            .map_err(|_| ActiveServiceWalErrorV1::WrongPin)?;
        let actual = index
            .native_store_context()
            .map_err(|_| ActiveServiceWalErrorV1::WrongPin)?;
        let read_context = plan.durable_read_context();
        if plan.expected() != &original.metadata
            || index
                .native_complete_metadata_state()
                .map_err(|_| ActiveServiceWalErrorV1::WrongPin)?
                != *plan.before_state()
            || read_context.get("context") != s(&actual.0)
            || read_context.get("canonicalFile") != s(actual.1.to_string_lossy())
        {
            return Err(ActiveServiceWalErrorV1::WrongPin);
        }
        let (candidates, mutations) = if let Some(append) = plan.append_plan() {
            let facts=crate::source_main_native_intent_store::NativeWriteIntentCandidatesV1::from_prepared_append(append,read_context.clone())
                .map_err(|_|ActiveServiceWalErrorV1::WrongPlan)?;
            (
                Some(facts.independently_retained_facts().clone()),
                facts.exact_prepared_mutations().to_vec(),
            )
        } else {
            (None, vec![])
        };
        Ok(ActualNativeFactBindingV1 {
            owner: self.owner,
            wal,
            context_sha256: sha256(&self.context),
            record_sha256: sha256(&record),
            original,
            read_context,
            candidates,
            mutations,
        })
    }
}
impl ActualNativeFactBindingV1 {
    pub(crate) fn require_synced_native_intent(&self, wal: &ActiveServiceWalV1) -> Result<()> {
        let record = wal.record()?;
        let source_mode =
            parse(&wal.context)?.get("origin").get("profile") == s(ACTUAL_SOURCE_FACT_WAL_PROFILE);
        let (context_key, history_key, candidates_key, mutations_key) = if source_mode {
            if record.get("profile") != s(ACTUAL_SOURCE_FACT_WAL_PROFILE)
                || !matches!(record.get("stage"),V::String(ref stage) if ["Intent","NativeSupersessionIntent","EscrowBoundIntent"].contains(&stage.as_str()))
                || record.get("actualSourceSelection").get("profile")
                    != s("TL-MAIN-ACTUAL-SINGLE-INLINE-SELECTION-13-1")
            {
                return Err(ActiveServiceWalErrorV1::WrongContext);
            }
            (
                "actualNativeReadContext",
                "actualNativeBeforeHistory",
                "actualNativeCandidates",
                "actualNativeMutations",
            )
        } else {
            if record.get("profile") != s(ACTUAL_NATIVE_FACT_WAL_PROFILE)
                || record.get("stage") != s("Intent")
            {
                return Err(ActiveServiceWalErrorV1::WrongContext);
            }
            (
                "nativeReadContext",
                "nativeBeforeHistory",
                "nativeCandidates",
                "nativeMutations",
            )
        };
        if record.get(context_key) != self.read_context {
            return Err(ActiveServiceWalErrorV1::WrongContext);
        }
        let history = self
            .original
            .canonical_scalar_bytes()
            .map_err(|_| ActiveServiceWalErrorV1::WrongPin)?;
        let pin = ActiveServiceWalBlobV1::of(&history)?;
        if record.get(history_key) != pin.value() || wal.read_blob(&pin)? != history {
            return Err(ActiveServiceWalErrorV1::WrongPin);
        }
        if let Some(candidates) = &self.candidates {
            for (name, raw) in [
                (candidates_key, bytes(candidates)?),
                (mutations_key, self.mutations.clone()),
            ] {
                let pin = ActiveServiceWalBlobV1::of(&raw)?;
                if record.get(name) != pin.value() || wal.read_blob(&pin)? != raw {
                    return Err(ActiveServiceWalErrorV1::WrongPin);
                }
            }
        } else if record.get(candidates_key) != V::Null || record.get(mutations_key) != V::Null {
            return Err(ActiveServiceWalErrorV1::WrongPin);
        }
        Ok(())
    }
    pub(crate) fn require_current(
        &self,
        wal: &ActiveServiceWalV1,
        index: &crate::source_compact_index_v2::SourceCompactIndex,
        plan: &crate::source_compact_index_v2::NativePreparedInlineDatabaseV1,
    ) -> Result<()> {
        if self.owner != wal.owner
            || self.wal != wal.pin()?
            || self.context_sha256 != sha256(&wal.context)
            || self.record_sha256 != sha256(&bytes(&wal.record()?)?)
            || self.original
                != index
                    .capture_verified_history_pin()
                    .map_err(|_| ActiveServiceWalErrorV1::WrongPin)?
            || self.read_context != plan.durable_read_context()
        {
            return Err(ActiveServiceWalErrorV1::WrongContext);
        }
        let (candidates, mutations) = if let Some(append) = plan.append_plan() {
            let parts=crate::source_main_native_intent_store::NativeWriteIntentCandidatesV1::from_prepared_append(append,plan.durable_read_context()).map_err(|_|ActiveServiceWalErrorV1::WrongPlan)?;
            (
                Some(parts.independently_retained_facts().clone()),
                parts.exact_prepared_mutations().to_vec(),
            )
        } else {
            (None, vec![])
        };
        if candidates != self.candidates || mutations != self.mutations {
            return Err(ActiveServiceWalErrorV1::WrongPlan);
        }
        Ok(())
    }
    pub fn capture_pending_plan_fact(
        &self,
        wal: &ActiveServiceWalV1,
        index: &crate::source_compact_index_v2::SourceCompactIndex,
        plan: &crate::source_compact_index_v2::NativePreparedInlineDatabaseV1,
    ) -> Result<ActualNativePendingPlanFactV1> {
        self.require_current(wal, index, plan)?;
        let original = self
            .original
            .canonical_scalar_bytes()
            .map_err(|_| ActiveServiceWalErrorV1::WrongPin)?;
        let raw = bytes(&obj(&[
            ("profile", s(ACTUAL_NATIVE_FACT_PROFILE)),
            ("kind", s("ActualPendingInlinePlan")),
            ("actualWal", actual_fact_wal_value(&self.wal)),
            ("actualContextSHA256", s(hex(&self.context_sha256))),
            ("actualRecordSHA256", s(hex(&self.record_sha256))),
            ("originalHistoryHex", s(hex(&original))),
            ("nativeReadContext", self.read_context.clone()),
            (
                "nativeCandidates",
                self.candidates.clone().unwrap_or(V::Null),
            ),
            ("exactPreparedMutationsHex", s(hex(&self.mutations))),
        ]))?;
        if raw.len() > MAX_BLOB {
            return Err(ActiveServiceWalErrorV1::Capacity);
        }
        Ok(ActualNativePendingPlanFactV1 { raw })
    }
}
