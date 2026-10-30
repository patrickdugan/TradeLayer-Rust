/// Decoded expectation only. This type cannot create a binding, native append
/// token, escrow permit, source result or financial owner.
pub struct ActualNativePlanFactExpectationV1 {
    value: V,
    original: crate::source_compact_index_v2::SourceIndexHistoryPinV1,
    mutations: Vec<u8>,
}
fn actual_fact_exact_keys(value: &V, names: &[&str]) -> Result<()> {
    let V::Object(fields) = value else {
        return Err(ActiveServiceWalErrorV1::InvalidRecord);
    };
    if fields.len() != names.len() || fields.iter().zip(names).any(|((key, _), name)| key != name) {
        return Err(ActiveServiceWalErrorV1::InvalidRecord);
    }
    Ok(())
}
fn actual_fact_hex_bytes(value: &V) -> Result<Vec<u8>> {
    let text = history_text(value)?;
    if text.len() % 2 != 0
        || text.len() / 2 > MAX_BLOB
        || text
            .bytes()
            .any(|b| !b.is_ascii_digit() && !(b'a'..=b'f').contains(&b))
    {
        return Err(ActiveServiceWalErrorV1::InvalidRecord);
    }
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            u8::from_str_radix(std::str::from_utf8(pair).expect("ASCII checked"), 16)
                .map_err(|_| ActiveServiceWalErrorV1::InvalidRecord)
        })
        .collect()
}
fn actual_fact_parse_bounded(raw: &[u8]) -> Result<V> {
    if raw.is_empty() || raw.len() > MAX_BLOB {
        return Err(ActiveServiceWalErrorV1::Capacity);
    }
    parse(raw)
}
impl ActualNativePlanFactExpectationV1 {
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
        if value.get("profile") != s(ACTUAL_NATIVE_FACT_PROFILE)
            || value.get("kind") != s("ActualPendingInlinePlan")
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
            != s(crate::source_compact_index_v2::INLINE_DATABASE_PROFILE)
        {
            return Err(ActiveServiceWalErrorV1::WrongContext);
        }
        if value.get("nativeCandidates") == V::Null && !mutations.is_empty() {
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
    pub fn verify_recomputed_plan(
        &self,
        plan: &crate::source_compact_index_v2::NativeHistoricalInlineDatabaseV1,
    ) -> Result<()> {
        let actual_mutations = if plan.requires_native_write() {
            plan.exact_prepared_mutations()
        } else {
            &[]
        };
        if &self.original != plan.original_read()
            || self.native_read_context() != plan.durable_read_context()
            || self.mutations != actual_mutations
            || (self.native_candidates() != V::Null) != plan.requires_native_write()
        {
            return Err(ActiveServiceWalErrorV1::WrongPlan);
        }
        Ok(())
    }
}
pub struct ActualNativeIoFactExpectationV1 {
    value: V,
    post: Option<crate::source_compact_index_v2::SourceIndexHistoryPinV1>,
}
impl ActualNativeIoFactExpectationV1 {
    pub fn from_canonical_fact_bytes(
        namespace: &Namespace,
        pending_raw: &[u8],
        raw: &[u8],
    ) -> Result<Self> {
        let pending =
            ActualNativePlanFactExpectationV1::from_canonical_fact_bytes(namespace, pending_raw)?;
        let value = actual_fact_parse_bounded(raw)?;
        actual_fact_exact_keys(
            &value,
            &[
                "profile",
                "actualPendingFactSHA256",
                "stage",
                "actualOutcome",
                "verifiedPostHex",
            ],
        )?;
        if value.get("profile")
            != s(crate::source_compact_index_v2::ACTUAL_INLINE_APPEND_FACT_PROFILE)
            || value.get("actualPendingFactSHA256") != s(hex(&sha256(pending_raw)))
            || value.get("stage") != s("SourceCompactIndex.append_inline_database")
        {
            return Err(ActiveServiceWalErrorV1::WrongContext);
        }
        let outcome = value.get("actualOutcome");
        match history_text(&outcome.get("kind"))? {
            "NativeAppendReturned" => {
                actual_fact_exact_keys(&outcome, &["kind"])?;
                if pending.native_candidates() == V::Null {
                    return Err(ActiveServiceWalErrorV1::WrongPlan);
                }
            }
            "NoNativeMutation" => {
                actual_fact_exact_keys(&outcome, &["kind"])?;
                if pending.native_candidates() != V::Null {
                    return Err(ActiveServiceWalErrorV1::WrongPlan);
                }
            }
            "NativeIoError" => {
                actual_fact_exact_keys(
                    &outcome,
                    &[
                        "kind",
                        "ioKind",
                        "rawOsError",
                        "ioDisplay",
                        "exactSourceDisplay",
                    ],
                )?;
                for name in ["ioKind", "ioDisplay", "exactSourceDisplay"] {
                    history_text(&outcome.get(name))?;
                }
                if outcome.get("rawOsError") != V::Null {
                    let text = history_text(&outcome.get("rawOsError"))?.to_owned();
                    let number = text
                        .parse::<i32>()
                        .map_err(|_| ActiveServiceWalErrorV1::InvalidRecord)?;
                    if number.to_string() != text {
                        return Err(ActiveServiceWalErrorV1::InvalidRecord);
                    }
                }
                if value.get("verifiedPostHex") != V::Null {
                    return Err(ActiveServiceWalErrorV1::WrongPin);
                }
            }
            "StructuralOrResourceRejection" => {
                actual_fact_exact_keys(&outcome, &["kind", "errorVariant", "exactSourceDisplay"])?;
                history_text(&outcome.get("errorVariant"))?;
                history_text(&outcome.get("exactSourceDisplay"))?;
                if value.get("verifiedPostHex") != V::Null {
                    return Err(ActiveServiceWalErrorV1::WrongPin);
                }
            }
            _ => return Err(ActiveServiceWalErrorV1::InvalidRecord),
        }
        let post = if value.get("verifiedPostHex") == V::Null {
            None
        } else {
            Some(crate::source_compact_index_v2::SourceIndexHistoryPinV1::from_canonical_scalar_bytes(namespace,&actual_fact_hex_bytes(&value.get("verifiedPostHex"))?).map_err(|_|ActiveServiceWalErrorV1::WrongPin)?)
        };
        if post.as_ref().is_some_and(|post| {
            post.storage.namespace != pending.original.storage.namespace
                || post.storage.persistent_identity != pending.original.storage.persistent_identity
                || post.storage.canonical_catalog != pending.original.storage.canonical_catalog
        }) {
            return Err(ActiveServiceWalErrorV1::WrongPin);
        }
        Ok(Self { value, post })
    }
    pub fn observed_outcome(&self) -> V {
        self.value.get("actualOutcome")
    }
    pub fn expected_verified_post(
        &self,
    ) -> Option<&crate::source_compact_index_v2::SourceIndexHistoryPinV1> {
        self.post.as_ref()
    }
}
