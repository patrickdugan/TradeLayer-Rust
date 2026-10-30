/// Explicit completed ProcessRows history only. Pins are integrity expectations
/// for actual original files. They are not source/financial outcome inputs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeMainHistoricalRowsExpectationV1 {
    pub archive: NativeMainArchivePin,
    pub current: crate::source_compact_index_v2::SourceIndexHistoryPinV1,
    /// Each command retains its actual Before and every successful native Post.
    /// Rejected/no-match source calls do not advance this physical sequence.
    pub commands: Vec<Vec<crate::source_compact_index_v2::SourceIndexHistoryPinV1>>,
}
pub struct RejectedNativeMainHistoricalRowsV1 {
    pub error: NativeMainSessionError,
    index: SourceCompactIndex,
    financial: Option<SourceContractReplay>,
    actual_prefix: Vec<NativeMainSessionReceipt>,
}
impl core::fmt::Debug for RejectedNativeMainHistoricalRowsV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RejectedNativeMainHistoricalRowsV1")
            .field("error", &self.error)
            .field("executed_commands", &self.actual_prefix.len())
            .finish_non_exhaustive()
    }
}
impl RejectedNativeMainHistoricalRowsV1 {
    pub fn index(&self) -> &SourceCompactIndex {
        &self.index
    }
    pub fn financial_runtime(&self) -> Option<&SourceContractRuntime> {
        self.financial.as_ref().map(SourceContractReplay::runtime)
    }
    pub fn actual_prefix(&self) -> &[NativeMainSessionReceipt] {
        &self.actual_prefix
    }
}
impl SourceMainNativeSessionV13 {
    /// Rebuild ONE actual financial owner from raw foundation commands and raw
    /// completed row calls. Metadata is read from original historical frames;
    /// the uniquely owned current Post index is never written or replaced.
    /// Catchup/service/action replay and unavailable generations are refused.
    pub fn reexecute_historical_rows(
        bytes: &[u8],
        expected: &NativeMainHistoricalRowsExpectationV1,
        index: SourceCompactIndex,
    ) -> Result<Self, Box<RejectedNativeMainHistoricalRowsV1>> {
        let NativeMainHistoricalColdPartsV1 {
            financial,
            index,
            foundation,
            foundation_head,
            origin_rows,
            binding,
            origin_physical,
            head,
            history,
        } = Self::rebuild_historical_rows_prefix(bytes, expected, &expected.current, index)?;
        Ok(Self {
            financial,
            index,
            foundation,
            foundation_head,
            origin_rows,
            binding,
            origin_physical,
            head,
            history,
            halted: false,
        })
    }
    fn rebuild_historical_rows_prefix(
        bytes: &[u8],
        expected: &NativeMainHistoricalRowsExpectationV1,
        actual_post: &crate::source_compact_index_v2::SourceIndexHistoryPinV1,
        index: SourceCompactIndex,
    ) -> Result<NativeMainHistoricalColdPartsV1, Box<RejectedNativeMainHistoricalRowsV1>> {
        Self::rebuild_historical_rows_prefix_with_provider(
            bytes,
            expected,
            actual_post,
            index,
            &NativeMainOriginalHistoryProviderV1,
        )
    }
    fn rebuild_historical_rows_prefix_with_provider(
        bytes: &[u8],
        expected: &NativeMainHistoricalRowsExpectationV1,
        actual_post: &crate::source_compact_index_v2::SourceIndexHistoryPinV1,
        mut index: SourceCompactIndex,
        provider: &impl NativeMainActualHistoryProviderV1,
    ) -> Result<NativeMainHistoricalColdPartsV1, Box<RejectedNativeMainHistoricalRowsV1>> {
        let mut financial = None;
        let mut history = vec![];
        let attempted = (|| {
            let archive = inspect_archive(bytes, &expected.archive)?;
            if index
                .capture_verified_history_pin()
                .map_err(cold_history_error)?
                != *actual_post
                || cold_prefix_physical(&expected.current) != expected.archive.current
                || archive.entries.len() != expected.commands.len()
                || archive.entries.iter().any(|(command, _)| {
                    !matches!(command, NativeMainSessionCommand::ProcessRows { .. })
                })
            {
                return Err(NativeMainSessionError::WrongPhysicalBinding);
            }
            // Independently verify the historical completed endpoint before
            // constructing the one financial owner. It can precede actual Post.
            provider
                .read(&mut index, &expected.current)
                .map_err(cold_history_error)?;
            let origin_pin = expected
                .commands
                .first()
                .and_then(|pins| pins.first())
                .unwrap_or(&expected.current);
            let origin = provider
                .read(&mut index, origin_pin)
                .map_err(cold_history_error)?;
            let origin_physical = NativePhysicalBinding {
                persistent_identity: origin_pin.storage.persistent_identity.clone(),
                canonical_catalog: origin_pin.storage.canonical_catalog.clone(),
                metadata: origin.pin().clone(),
            };
            if origin_physical != expected.archive.origin
                || logical_read(&origin_pin.rows) != archive.origin_rows
            {
                return Err(NativeMainSessionError::WrongPhysicalBinding);
            }
            financial = Some(
                SourceContractReplay::from_checkpoint(
                    &archive.foundation,
                    expected.archive.foundation,
                )
                .map_err(|e| NativeMainSessionError::Backend(e.to_string()))?,
            );
            let owner = financial.as_mut().expect("created exactly once");
            if owner.runtime().policy() != SourceContractRuntimePolicy::MainOrchestrationV12
                || !owner.runtime().collection("txIndex").is_empty()
            {
                return Err(NativeMainSessionError::InvalidFoundation);
            }
            let state = owner
                .runtime()
                .state_value()
                .map_err(|e| NativeMainSessionError::Backend(e.to_string()))?;
            let client = state.get("mainProduction").get("client");
            let binding = obj(&[
                ("profile", s(STORE_BINDING_PROFILE)),
                ("chain", client.get("chain")),
                ("test", client.get("test")),
                ("context", s(&origin_physical.metadata.context)),
                ("metadataProfile", s(&origin_physical.metadata.profile)),
                ("sessionProfile", s(PROFILE)),
            ]);
            let V::String(chain) = client.get("chain") else {
                return Err(NativeMainSessionError::InvalidFoundation);
            };
            let V::Bool(test) = client.get("test") else {
                return Err(NativeMainSessionError::InvalidFoundation);
            };
            if binding != archive.binding
                || crate::storage::Namespace::new(&chain, test, "txIndex")
                    .map_err(|_| NativeMainSessionError::InvalidFoundation)?
                    != *index.native_namespace()
            {
                return Err(NativeMainSessionError::InvalidFoundation);
            }
            let mut seed = PROFILE.as_bytes().to_vec();
            seed.extend_from_slice(&owner.head().commands.to_be_bytes());
            seed.extend_from_slice(&owner.head().commitment);
            seed.extend_from_slice(
                &owner
                    .native_main_financial_commitment()
                    .map_err(|e| NativeMainSessionError::Backend(e.to_string()))?,
            );
            frame(&mut seed, &binding)?;
            frame(&mut seed, &logical_read(&origin_pin.rows))?;
            let mut head = NativeMainSessionHead {
                commands: 0,
                commitment: sha256(&seed),
            };
            owner
                .native_main_runtime_mut()
                .native_main_bind(binding.clone())
                .map_err(|e| NativeMainSessionError::Backend(e.to_string()))?;
            let mut previous = origin_physical.clone();
            for (sequence, ((command, recorded), pins)) in archive
                .entries
                .into_iter()
                .zip(&expected.commands)
                .enumerate()
            {
                let mut playback = provider
                    .playback(&mut index, pins)
                    .map_err(cold_history_error)?;
                let before = NativePhysicalBinding {
                    persistent_identity: pins[0].storage.persistent_identity.clone(),
                    canonical_catalog: pins[0].storage.canonical_catalog.clone(),
                    metadata: playback.current().clone(),
                };
                let post_pin = pins.last().expect("nonempty verified playback");
                let after = NativePhysicalBinding {
                    persistent_identity: post_pin.storage.persistent_identity.clone(),
                    canonical_catalog: post_pin.storage.canonical_catalog.clone(),
                    metadata: post_pin.metadata.clone(),
                };
                if before != previous
                    || archive.physical_entries[sequence]
                        != (physical_value(&before), physical_value(&after))
                {
                    return Err(NativeMainSessionError::WrongPhysicalBinding);
                }
                let NativeMainSessionCommand::ProcessRows { rows, height, live } = &command else {
                    unreachable!("preflighted")
                };
                let actual = owner.native_main_runtime_mut().native_main_historical_rows(
                    &mut playback,
                    rows.clone(),
                    height.clone(),
                    *live,
                );
                let returned = actual.result.map_err(|error| {
                    NativeMainSessionError::Backend(format!("historical source stopped: {error}"))
                })?;
                if actual.storage_stop.is_some()
                    || !playback.complete()
                    || playback.current() != &after.metadata
                {
                    return Err(NativeMainSessionError::WrongPhysicalBinding);
                }
                let effects = owner.native_main_runtime_mut().take_effects();
                let mut receipt = NativeMainSessionReceipt {
                    profile: RECEIPT_PROFILE.into(),
                    sequence: head.commands,
                    command,
                    status: NativeMainSessionStatus::Completed(returned),
                    grouping: None,
                    validity: actual.validity,
                    effects,
                    financial_commitment: owner
                        .native_main_financial_commitment()
                        .map_err(|e| NativeMainSessionError::Backend(e.to_string()))?,
                    after_rows: Some(post_pin.rows.clone()),
                    before,
                    after: Some(after.clone()),
                    previous_head: head.commitment,
                    head: [0; 32],
                };
                let encoded = logical_receipt_bytes(&receipt)?;
                let mut raw = head.commitment.to_vec();
                raw.extend_from_slice(&encoded);
                receipt.head = sha256(&raw);
                head = NativeMainSessionHead {
                    commands: head.commands + 1,
                    commitment: receipt.head,
                };
                history.push(receipt);
                if encoded != recorded {
                    return Err(NativeMainSessionError::Backend(
                        "historical source receipt mismatch".into(),
                    ));
                }
                previous = after;
            }
            provider
                .read(&mut index, &expected.current)
                .map_err(cold_history_error)?;
            if head != expected.archive.head
                || previous != expected.archive.current
                || index
                    .capture_verified_history_pin()
                    .map_err(cold_history_error)?
                    != *actual_post
            {
                return Err(NativeMainSessionError::WrongHead);
            }
            Ok((
                archive.foundation,
                binding,
                origin_pin.rows.clone(),
                origin_physical,
                head,
            ))
        })();
        match attempted {
            Err(error) => Err(Box::new(RejectedNativeMainHistoricalRowsV1 {
                error,
                index,
                financial,
                actual_prefix: history,
            })),
            Ok((foundation, binding, origin_rows, origin_physical, head)) => {
                Ok(NativeMainHistoricalColdPartsV1 {
                    financial: financial.expect("actual foundation was reexecuted"),
                    index,
                    foundation,
                    foundation_head: expected.archive.foundation,
                    origin_rows,
                    binding,
                    origin_physical,
                    head,
                    history,
                })
            }
        }
    }
}
fn cold_history_error(
    error: crate::source_compact_index_v2::CompactIndexError,
) -> NativeMainSessionError {
    NativeMainSessionError::Backend(error.to_string())
}
