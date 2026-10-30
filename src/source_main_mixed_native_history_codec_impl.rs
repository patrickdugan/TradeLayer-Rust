pub const MIXED_NATIVE_HISTORY_EXPECTATION_PROFILE: &str =
    "TL-MAIN-MIXED-NATIVE-HISTORY-EXPECTATION-13-1";
impl MixedNativeAckHistoryPinV1 {
    /// Persist independently of the WAL whose bytes this expectation checks.
    /// The bytes contain metadata expectations only, never a source state.
    pub fn canonical_expectation_bytes(&self) -> Result<Vec<u8>> {
        let trace = self.segmented.trace();
        bytes(&obj(&[
            ("profile", s(MIXED_NATIVE_HISTORY_EXPECTATION_PROFILE)),
            ("wal", actual_fact_wal_value(self.segmented.wal())),
            (
                "trace",
                obj(&[
                    ("profile", s(&trace.profile)),
                    ("bytes", s(trace.bytes.to_string())),
                    ("commits", s(trace.commits.to_string())),
                    ("chain", s(hex(&trace.chain_sha256d))),
                ]),
            ),
            ("actions", s(self.segmented.actions().to_string())),
            ("head", s(hex(&self.segmented.head()))),
            ("sourceHead", self.segmented.source_head().clone()),
            (
                "latestSegment",
                self.segmented
                    .latest_segment()
                    .map_or(V::Null, ActiveServiceWalBlobV1::value),
            ),
            (
                "metadataNamespace",
                s(self
                    .metadata_namespace
                    .path(Path::new(""))
                    .to_string_lossy()),
            ),
            (
                "nativePostHex",
                s(hex(&self
                    .native_post
                    .canonical_scalar_bytes()
                    .map_err(|_| ActiveServiceWalErrorV1::WrongPin)?)),
            ),
        ]))
    }
    /// A strict expected-pin decoder, not a writer/permit/financial importer.
    /// Actual-file full trace/KV/row verification is still mandatory at open.
    pub fn from_canonical_expectation_bytes(
        wal_namespace: &Namespace,
        metadata_namespace: &Namespace,
        raw: &[u8],
    ) -> Result<Self> {
        let value = actual_fact_parse_bounded(raw)?;
        history_keys(
            &value,
            &[
                "profile",
                "wal",
                "trace",
                "actions",
                "head",
                "sourceHead",
                "latestSegment",
                "metadataNamespace",
                "nativePostHex",
            ],
            &[],
        )?;
        if value.get("profile") != s(MIXED_NATIVE_HISTORY_EXPECTATION_PROFILE)
            || value.get("metadataNamespace")
                != s(metadata_namespace.path(Path::new("")).to_string_lossy())
        {
            return Err(ActiveServiceWalErrorV1::WrongContext);
        }
        mode4_namespace(wal_namespace, metadata_namespace)?;
        let encoded = value.get("wal");
        history_keys(
            &encoded,
            &[
                "profile",
                "root",
                "prefixProfile",
                "namespace",
                "file",
                "sequence",
                "kv",
            ],
            &[],
        )?;
        let kv = encoded.get("kv");
        history_keys(
            &kv,
            &["profile", "keys", "bytes", "complete", "sha256d"],
            &[],
        )?;
        let root = PathBuf::from(history_text(&encoded.get("root"))?);
        let file = PathBuf::from(history_text(&encoded.get("file"))?);
        if encoded.get("profile") != s(PROFILE)
            || encoded.get("prefixProfile") != s(crate::storage::VERIFIED_PREFIX_OPEN_PROFILE)
            || encoded.get("namespace") != s(wal_namespace.path(Path::new("")).to_string_lossy())
            || !root.is_absolute()
            || !file.is_absolute()
            || kv.get("profile") != s(COMPLETE_KV_RECEIPT_PROFILE)
            || kv.get("complete") != V::Bool(true)
        {
            return Err(ActiveServiceWalErrorV1::WrongPin);
        }
        let wal = ActiveServiceWalPinV1 {
            profile: PROFILE.into(),
            root,
            prefix: VerifiedJournalPrefixV1 {
                profile: crate::storage::VERIFIED_PREFIX_OPEN_PROFILE.into(),
                namespace: wal_namespace.clone(),
                canonical_file: file,
                physical_sequence: history_count(&encoded.get("sequence"))?,
                content: JournalCompleteKvReceiptV1 {
                    profile: COMPLETE_KV_RECEIPT_PROFILE.into(),
                    keys: history_count(&kv.get("keys"))?,
                    bytes: history_count(&kv.get("bytes"))?,
                    chain_sha256d: history_hash(&kv.get("sha256d"))?,
                    complete: true,
                },
            },
        };
        if actual_fact_wal_value(&wal) != encoded {
            return Err(ActiveServiceWalErrorV1::WrongPin);
        }
        let trace = segmented_trace(&value.get("trace"))?;
        if trace.commits != wal.prefix.physical_sequence {
            return Err(ActiveServiceWalErrorV1::WrongPin);
        }
        let source_head = value.get("sourceHead");
        segment_source_head(&source_head)?;
        let latest_segment = match value.get("latestSegment") {
            V::Null => None,
            ref v => Some(history_blob(v)?),
        };
        let native_post =
            crate::source_compact_index_v2::SourceIndexHistoryPinV1::from_canonical_scalar_bytes(
                metadata_namespace,
                &actual_fact_hex_bytes(&value.get("nativePostHex"))?,
            )
            .map_err(|_| ActiveServiceWalErrorV1::WrongPin)?;
        let pin = Self {
            segmented: SegmentedActiveHistoryPinV1 {
                wal,
                trace,
                actions: history_count(&value.get("actions"))?,
                head: history_hash(&value.get("head"))?,
                source_head,
                latest_segment,
            },
            metadata_namespace: metadata_namespace.clone(),
            native_post,
        };
        if pin.canonical_expectation_bytes()?.as_slice() != raw {
            return Err(ActiveServiceWalErrorV1::InvalidRecord);
        }
        Ok(pin)
    }
}
