/// Expected metadata only; cannot create an append token, source result or
/// producer binding. Physical/source closure remains independently required.
pub struct ActualValidityPlanFactExpectationV1 {
    value: V,
    original: crate::source_compact_index_v2::SourceIndexHistoryPinV1,
    mutations: Vec<u8>,
}
impl ActualValidityPlanFactExpectationV1 {
    pub fn from_canonical_fact_bytes(namespace: &Namespace, raw: &[u8]) -> Result<Self> {
        let value = actual_fact_parse_bounded(raw)?;
        actual_fact_exact_keys(
            &value,
            &[
                "profile",
                "kind",
                "actualWal",
                "actualContextSHA256",
                "actualRecordSHA256",
                "originalHistoryHex",
                "nativeReadContext",
                "nativeCandidates",
                "exactPreparedMutationsHex",
            ],
        )?;
        if value.get("profile") != s(ACTUAL_VALIDITY_PLAN_FACT_PROFILE)
            || value.get("kind") != s("ActualPendingValidityPlan")
        {
            return Err(ActiveServiceWalErrorV1::WrongContext);
        }
        history_hash(&value.get("actualContextSHA256"))?;
        history_hash(&value.get("actualRecordSHA256"))?;
        let wal = value.get("actualWal");
        actual_fact_exact_keys(
            &wal,
            &[
                "profile",
                "root",
                "prefixProfile",
                "namespace",
                "file",
                "sequence",
                "kv",
            ],
        )?;
        if wal.get("profile") != s(PROFILE)
            || !Path::new(history_text(&wal.get("root"))?).is_absolute()
            || !Path::new(history_text(&wal.get("file"))?).is_absolute()
        {
            return Err(ActiveServiceWalErrorV1::WrongPin);
        }
        history_text(&wal.get("prefixProfile"))?;
        history_text(&wal.get("namespace"))?;
        history_count(&wal.get("sequence"))?;
        let kv = wal.get("kv");
        actual_fact_exact_keys(&kv, &["profile", "keys", "bytes", "complete", "sha256d"])?;
        if kv.get("profile") != s(COMPLETE_KV_RECEIPT_PROFILE)
            || kv.get("complete") != V::Bool(true)
        {
            return Err(ActiveServiceWalErrorV1::WrongPin);
        }
        history_count(&kv.get("keys"))?;
        history_count(&kv.get("bytes"))?;
        history_hash(&kv.get("sha256d"))?;
        let original =
            crate::source_compact_index_v2::SourceIndexHistoryPinV1::from_canonical_scalar_bytes(
                namespace,
                &actual_fact_hex_bytes(&value.get("originalHistoryHex"))?,
            )
            .map_err(|_| ActiveServiceWalErrorV1::WrongPin)?;
        let mutations = actual_fact_hex_bytes(&value.get("exactPreparedMutationsHex"))?;
        if value.get("nativeReadContext").get("profile")
            != s(crate::source_compact_index_v2::PREPARED_VALIDITY_PROFILE)
        {
            return Err(ActiveServiceWalErrorV1::WrongContext);
        }
        if value.get("nativeCandidates") == V::Null || mutations.is_empty() {
            return Err(ActiveServiceWalErrorV1::WrongPlan);
        }
        Ok(Self {
            value,
            original,
            mutations,
        })
    }
    pub fn original(&self) -> &crate::source_compact_index_v2::SourceIndexHistoryPinV1 {
        &self.original
    }
    pub fn native_read_context(&self) -> V {
        self.value.get("nativeReadContext")
    }
    pub fn native_candidates(&self) -> V {
        self.value.get("nativeCandidates")
    }
    pub fn exact_prepared_mutations(&self) -> &[u8] {
        &self.mutations
    }
    /// Call only with independently verified trace/context/record facts. This
    /// comparison does not prove that an OS event or authorized actor existed.
    pub fn verify_origin_record(
        &self,
        wal: &ActiveServiceWalPinV1,
        context: &[u8],
        record: &[u8],
    ) -> Result<()> {
        if self.value.get("actualWal") != actual_fact_wal_value(wal)
            || self.value.get("actualContextSHA256") != s(hex(&sha256(context)))
            || self.value.get("actualRecordSHA256") != s(hex(&sha256(record)))
        {
            return Err(ActiveServiceWalErrorV1::WrongPin);
        }
        Ok(())
    }

    pub fn verify_recomputed_initial_plan(
        &self,
        plan: &crate::source_compact_index_v2::NativeHistoricalValidityAwaitV1,
    ) -> Result<()> {
        if self.original.metadata != plan.query().snapshot
            || self.native_read_context() != plan.durable_read_context()
            || self.mutations != plan.exact_prepared_mutations()
        {
            return Err(ActiveServiceWalErrorV1::WrongPlan);
        }
        Ok(())
    }
}
pub struct ActualValidityIoFactExpectationV1 {
    io: Option<ActualValidityIoDataV1>,
    post: Option<crate::source_compact_index_v2::SourceIndexHistoryPinV1>,
}
impl ActualValidityIoFactExpectationV1 {
    pub fn from_canonical_fact_bytes(
        namespace: &Namespace,
        pending_raw: &[u8],
        raw: &[u8],
    ) -> Result<Self> {
        let pending =
            ActualValidityPlanFactExpectationV1::from_canonical_fact_bytes(namespace, pending_raw)?;
        let v = actual_fact_parse_bounded(raw)?;
        actual_fact_exact_keys(
            &v,
            &[
                "profile",
                "actualPendingFactSHA256",
                "stage",
                "actualOutcome",
                "verifiedPostHex",
            ],
        )?;
        if v.get("profile")
            != s(crate::source_compact_index_v2::ACTUAL_VALIDITY_APPEND_FACT_PROFILE)
            || v.get("actualPendingFactSHA256") != s(hex(&sha256(pending_raw)))
            || v.get("stage") != s("SourceCompactIndex.commit_prepared_validity")
        {
            return Err(ActiveServiceWalErrorV1::WrongContext);
        }
        let outcome = v.get("actualOutcome");
        let io = match history_text(&outcome.get("kind"))? {
            "NativeAppendReturned" => {
                actual_fact_exact_keys(&outcome, &["kind"])?;
                if v.get("verifiedPostHex") == V::Null {
                    return Err(ActiveServiceWalErrorV1::WrongPin);
                }
                None
            }
            "NativeIoError" => {
                actual_fact_exact_keys(&outcome, &["kind", "io"])?;
                if v.get("verifiedPostHex") != V::Null {
                    return Err(ActiveServiceWalErrorV1::WrongPin);
                }
                Some(ActualValidityIoDataV1::from_value(&outcome.get("io"))?)
            }
            _ => return Err(ActiveServiceWalErrorV1::InvalidRecord),
        };
        let post = if v.get("verifiedPostHex") == V::Null {
            None
        } else {
            Some(crate::source_compact_index_v2::SourceIndexHistoryPinV1::from_canonical_scalar_bytes(namespace,&actual_fact_hex_bytes(&v.get("verifiedPostHex"))?).map_err(|_|ActiveServiceWalErrorV1::WrongPin)?)
        };
        if post.as_ref().is_some_and(|p| {
            p.storage.namespace != pending.original.storage.namespace
                || p.storage.persistent_identity != pending.original.storage.persistent_identity
                || p.storage.canonical_catalog != pending.original.storage.canonical_catalog
        }) {
            return Err(ActiveServiceWalErrorV1::WrongPin);
        }
        Ok(Self { io, post })
    }
    pub fn native_io(&self) -> Option<&ActualValidityIoDataV1> {
        self.io.as_ref()
    }
    pub fn verified_post(
        &self,
    ) -> Option<&crate::source_compact_index_v2::SourceIndexHistoryPinV1> {
        self.post.as_ref()
    }
}
