// Explicit same-process native-file recovery. No source/action execution retry.
struct ActiveWalNativeRecovery {
    raw: Vec<u8>,
    prior: Option<ActiveServiceWalBlobV1>,
    replacement: Option<Box<crate::source_compact_index_v2::NativePreparedValidityWriteV1>>,
    mapping: Option<crate::source_compact_index_v2::NativePreparedValiditySupersessionV1>,
    installed: bool,
}
struct ActiveServiceWithoutNativeIndex {
    financial: NativeMainFinancialWithoutIndexV1,
    namespace: crate::storage::Namespace,
    frame: Option<Box<NativeMainServiceContinuationV1>>,
    source_start_error: Option<SourceRuleError>,
    origin: NativeMainSessionExpectation,
    head: NativeMainActiveServiceHeadV1,
    history: Vec<NativeMainActiveServiceReceiptV1>,
    pending_observation: Option<NativeMainActiveServicePendingV1>,
    attempted_write: Option<V>,
    observation_limit: usize,
    inline_profile: bool,
}
pub struct SourceMainNativeDurableServiceNoIndexV1 {
    // Private controller retains actual WAL plan/intent/history; it is never
    // exposed while its moved financial owner lives in `service`.
    controller: SourceMainNativeDurableActiveServiceV1,
    service: ActiveServiceWithoutNativeIndex,
}
pub struct RejectedNativeMainActiveNoIndexV1 {
    pub cause: String,
    controller: SourceMainNativeDurableActiveServiceV1,
}
impl RejectedNativeMainActiveNoIndexV1 {
    pub fn into_controller(self) -> SourceMainNativeDurableActiveServiceV1 {
        self.controller
    }
}
impl core::fmt::Debug for RejectedNativeMainActiveNoIndexV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RejectedNativeMainActiveNoIndexV1")
            .field("cause", &self.cause)
            .finish_non_exhaustive()
    }
}
pub struct RejectedNativeMainActiveReopenV1 {
    pub cause: String,
    owner: SourceMainNativeDurableServiceNoIndexV1,
}
impl RejectedNativeMainActiveReopenV1 {
    pub fn into_owner(self) -> SourceMainNativeDurableServiceNoIndexV1 {
        self.owner
    }
}
impl core::fmt::Debug for RejectedNativeMainActiveReopenV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RejectedNativeMainActiveReopenV1")
            .field("cause", &self.cause)
            .finish_non_exhaustive()
    }
}
impl SourceMainNativeDurableActiveServiceV1 {
    pub fn create(
        owner: SourceMainNativeSessionV13,
        root: impl AsRef<std::path::Path>,
        namespace: crate::storage::Namespace,
        filesystem: crate::source_contract_replay::source_durable::SourceDurableFilesystem,
        observation_limit: usize,
    ) -> Result<Self, Box<RejectedNativeMainActiveWalV1>> {
        Self::create_internal(
            owner,
            root,
            namespace,
            filesystem,
            observation_limit,
            false,
            None,
        )
    }
    /// Explicit enhanced entry: mode and required origin pin are sealed in the
    /// immutable WAL context BEFORE any action; old create bytes stay unchanged.
    pub fn create_with_verified_history(
        owner: SourceMainNativeSessionV13,
        root: impl AsRef<std::path::Path>,
        namespace: crate::storage::Namespace,
        filesystem: crate::source_contract_replay::source_durable::SourceDurableFilesystem,
        observation_limit: usize,
    ) -> Result<Self, Box<RejectedNativeMainActiveWalV1>> {
        Self::create_internal(
            owner,
            root,
            namespace,
            filesystem,
            observation_limit,
            true,
            None,
        )
    }
    fn history_pin_bytes(index: &SourceCompactIndex) -> ActiveWalResult<Vec<u8>> {
        let pin = index
            .capture_verified_history_pin()
            .map_err(|e| active_wal_backend(e.to_string()))?;
        let mut fields =
            NativeFieldCollector::new("Native13/ActiveWalVerifiedHistory/v1", true, 1024 * 1024);
        pin.visit_canonical_fields(&mut |p, v| fields.emit(p, v))
            .map_err(active_wal_backend)?;
        Ok(fields.frames.expect("retained actual history pin"))
    }
    fn capture_history_bytes(&self) -> ActiveWalResult<Vec<u8>> {
        Self::history_pin_bytes(self.index())
    }
    /// Current original-physical mode is authoritative; absent required blobs
    /// can never downgrade an enhanced history to the old profile.
    pub fn read_checked_history_entry(
        &self,
        receipt: &NativeMainActiveWalReceiptV1,
    ) -> ActiveWalResult<V> {
        if !self.history.iter().any(|known| known == receipt) {
            return Err(NativeMainActiveWalErrorV1::WrongAction);
        }
        let raw = self.wal.read_blob(&receipt.entry)?;
        let entry = crate::property_catalog::parse_diagnostic_value(
            std::str::from_utf8(&raw).map_err(active_wal_backend)?,
        )
        .map_err(active_wal_backend)?;
        if entry.get("profile") != s(self.profile()) {
            return Err(NativeMainActiveWalErrorV1::WrongAction);
        }
        for name in ["verifiedHistoryBefore", "verifiedHistoryPost"] {
            let value = entry.get(name);
            if self.history_enabled {
                let V::String(bytes) = value.get("bytes") else {
                    return Err(NativeMainActiveWalErrorV1::WrongAction);
                };
                let count = bytes.parse::<u64>().map_err(active_wal_backend)?;
                let V::String(hash) = value.get("sha256") else {
                    return Err(NativeMainActiveWalErrorV1::WrongAction);
                };
                if hash.len() != 64
                    || !hash.bytes().all(|b| b.is_ascii_hexdigit())
                    || count.to_string() != bytes
                {
                    return Err(NativeMainActiveWalErrorV1::WrongAction);
                }
                let mut sha256 = [0; 32];
                for (i, byte) in sha256.iter_mut().enumerate() {
                    *byte = u8::from_str_radix(&hash[i * 2..i * 2 + 2], 16)
                        .map_err(active_wal_backend)?;
                }
                self.wal.read_blob(&ActiveServiceWalBlobV1 {
                    bytes: count,
                    sha256,
                })?;
            } else if value != V::Undefined {
                return Err(NativeMainActiveWalErrorV1::WrongAction);
            }
        }
        Ok(entry)
    }
    pub fn into_no_native_index(
        mut self,
    ) -> Result<SourceMainNativeDurableServiceNoIndexV1, Box<RejectedNativeMainActiveNoIndexV1>>
    {
        if self.phase != NativeMainActiveWalPhaseV1::NativeRecoveryRequired
            || self
                .active_service()
                .is_none_or(|s| s.pending_write().is_none())
            || !self.pending.as_ref().is_some_and(|p| {
                p.executed
                    && matches!(p.action, NativeMainActiveServiceActionV1::AppendPending)
                    && p.plan.is_none()
            })
        {
            return Err(Box::new(RejectedNativeMainActiveNoIndexV1 {
                cause: "unresolved actual native append required".into(),
                controller: self,
            }));
        }
        let ActiveWalOwner::Active(service) = self.owner.take().expect("checked actual owner")
        else {
            unreachable!("checked active")
        };
        let SourceMainNativeActiveServiceV1 {
            owner,
            frame,
            source_start_error,
            origin,
            head,
            history,
            pending_observation,
            attempted_write,
            observation_limit,
            inline_profile,
        } = *service;
        let SourceMainNativeSessionV13 {
            financial,
            index,
            foundation,
            foundation_head,
            origin_rows,
            binding,
            origin_physical,
            head: base_head,
            history: base_history,
            halted,
        } = owner;
        let namespace = index.native_namespace().clone();
        drop(index);
        Ok(SourceMainNativeDurableServiceNoIndexV1 {
            controller: self,
            service: ActiveServiceWithoutNativeIndex {
                financial: NativeMainFinancialWithoutIndexV1 {
                    financial,
                    foundation,
                    foundation_head,
                    origin_rows,
                    binding,
                    origin_physical,
                    head: base_head,
                    history: base_history,
                    halted,
                },
                namespace,
                frame,
                source_start_error,
                origin,
                head,
                history,
                pending_observation,
                attempted_write,
                observation_limit,
                inline_profile,
            },
        })
    }
    fn advance_native_recovery(&mut self) -> ActiveWalResult<()> {
        match self.phase {
            NativeMainActiveWalPhaseV1::NativeSupersessionIntent => {
                self.write_barrier("NativeSupersessionIntent")?;
                self.after_native_supersession_intent()?;
            }
            NativeMainActiveWalPhaseV1::NativeSupersessionAppend => {
                // One native attempt only. Any error, including a post-write
                // observation error, requires another sealed file reopen.
                self.phase = NativeMainActiveWalPhaseV1::NativeRecoveryRequired;
                let Some(ActiveWalOwner::Active(service)) = self.owner.as_mut() else {
                    return Err(NativeMainActiveWalErrorV1::WrongAction);
                };
                let frame = service
                    .frame
                    .as_mut()
                    .ok_or(NativeMainActiveWalErrorV1::WrongAction)?;
                // Native append only: original Main action, decoder, mark and
                // financial phase are never called again.
                let stop = service
                    .owner
                    .financial
                    .native_main_runtime_mut()
                    .native_main_service_append(&mut service.owner.index, frame);
                if stop != NativeMainServiceStopV1::AwaitNativeWrite {
                    self.phase = NativeMainActiveWalPhaseV1::NativeRecoveryRequired;
                    return Err(NativeMainActiveWalErrorV1::NativeRecoveryRequired);
                }
                let write = frame
                    .pending_write()
                    .ok_or(NativeMainActiveWalErrorV1::WrongAction)?;
                service
                    .owner
                    .index
                    .resolve_prepared_validity_post(write)
                    .map_err(|e| active_wal_backend(e.to_string()))?;
                self.pending
                    .as_mut()
                    .expect("retained action")
                    .append_context = Some(write.durable_read_context());
                self.phase = NativeMainActiveWalPhaseV1::Observe;
            }
            _ => return Err(NativeMainActiveWalErrorV1::WrongAction),
        }
        Ok(())
    }
    fn install_native_replacement(&mut self) -> ActiveWalResult<()> {
        let p = self
            .pending
            .as_mut()
            .ok_or(NativeMainActiveWalErrorV1::NoAction)?;
        let recovery = p
            .native_recovery
            .as_mut()
            .ok_or(NativeMainActiveWalErrorV1::WrongAction)?;
        if recovery.installed {
            return Err(NativeMainActiveWalErrorV1::WrongAction);
        }
        let replacement = recovery
            .replacement
            .take()
            .ok_or(NativeMainActiveWalErrorV1::WrongAction)?;
        let mapping = recovery
            .mapping
            .as_ref()
            .ok_or(NativeMainActiveWalErrorV1::WrongAction)?;
        let Some(ActiveWalOwner::Active(service)) = self.owner.as_mut() else {
            return Err(NativeMainActiveWalErrorV1::WrongAction);
        };
        let frame = service
            .frame
            .as_mut()
            .ok_or(NativeMainActiveWalErrorV1::WrongAction)?;
        if let Err((error, replacement)) = service
            .owner
            .financial
            .native_main_runtime_mut()
            .native_main_service_install_supersession(frame, replacement, mapping)
        {
            recovery.replacement = Some(replacement);
            return Err(active_wal_backend(error.to_string()));
        }
        recovery.installed = true;
        service.attempted_write = Some(
            frame
                .pending_write()
                .expect("installed real plan")
                .durable_read_context(),
        );
        Ok(())
    }
}
impl SourceMainNativeDurableServiceNoIndexV1 {
    pub fn profile(&self) -> &'static str {
        "TL-MAIN-ACTIVE-NATIVE-RECOVERY-13-1"
    }
    pub fn financial_runtime(&self) -> &SourceContractRuntime {
        self.service.financial.financial.runtime()
    }
    pub fn effects(&self) -> &[V] {
        self.financial_runtime().native_main_row_effects()
    }
    pub fn pending_write(&self) -> &crate::source_compact_index_v2::NativePreparedValidityWriteV1 {
        self.service
            .frame
            .as_ref()
            .and_then(|f| f.pending_write())
            .expect("retained actual plan")
    }
    pub fn try_reopen(
        self,
        root: &std::path::Path,
        candidate: crate::source_compact_index_v2::NativePreparedValidityCandidateV1,
        recovery: crate::storage::Recovery,
    ) -> Result<SourceMainNativeDurableActiveServiceV1, Box<RejectedNativeMainActiveReopenV1>> {
        use crate::source_compact_index_v2::NativePreparedValidityCandidateV1 as Candidate;
        let checked: ActiveWalResult<(SourceCompactIndex, ActiveWalNativeRecovery)> = (|| {
            let (index, observation) = SourceCompactIndex::reopen_prepared_validity(
                root,
                self.service.namespace.clone(),
                self.pending_write(),
                candidate,
                recovery,
            )
            .map_err(|e| active_wal_backend(e.to_string()))?;
            let mut proof = obj(&[
                ("profile", s(self.profile())),
                (
                    "candidate",
                    s(match candidate {
                        Candidate::Before => "Before",
                        Candidate::Intermediate => "Intermediate",
                        Candidate::Post => "Post",
                    }),
                ),
                (
                    "snapshot",
                    physical_value(
                        &physical(&index).map_err(|e| active_wal_backend(format!("{e:?}")))?,
                    ),
                ),
                (
                    "discardedBytes",
                    s(&observation.recovery_report().discarded_bytes.to_string()),
                ),
            ]);
            let previous = self
                .controller
                .pending
                .as_ref()
                .and_then(|p| p.native_recovery.as_ref())
                .map(|p| ActiveServiceWalBlobV1::of(&p.raw))
                .transpose()?;
            proof
                .insert(
                    "previousRecovery",
                    previous.as_ref().map_or(V::Null, |p| p.value()),
                )
                .map_err(active_wal_backend)?;

            let pin = index
                .capture_verified_history_pin()
                .map_err(|e| active_wal_backend(e.to_string()))?;
            let mut fields = NativeFieldCollector::new(
                "Native13/ActiveWalVerifiedHistory/v1",
                true,
                1024 * 1024,
            );
            pin.visit_canonical_fields(&mut |p, v| fields.emit(p, v))
                .map_err(active_wal_backend)?;
            proof
                .insert(
                    "verifiedReopenedHistory",
                    s(&hex(fields.frames.as_deref().expect("retained"))),
                )
                .map_err(active_wal_backend)?;
            let (replacement, mapping) = if candidate == Candidate::Post {
                index
                    .resolve_prepared_validity_post(self.pending_write())
                    .map_err(|e| active_wal_backend(e.to_string()))?;
                (None, None)
            } else {
                let (replacement, mapping) = index
                    .supersede_prepared_validity(self.pending_write(), &observation)
                    .map_err(|e| active_wal_backend(e.to_string()))?;
                let binding = index
                    .require_actual_native_supersession(
                        self.pending_write(),
                        &observation,
                        &replacement,
                        &mapping,
                    )
                    .map_err(|e| active_wal_backend(e.to_string()))?;
                let candidates=crate::source_main_native_intent_store::NativeWriteIntentCandidatesV1::from_prepared_append(replacement.append_plan(),replacement.durable_read_context()).map_err(|e|active_wal_backend(e.to_string()))?;
                proof
                    .insert("binding", binding)
                    .map_err(active_wal_backend)?;
                proof
                    .insert(
                        "replacementCandidates",
                        candidates.independently_retained_facts().clone(),
                    )
                    .map_err(active_wal_backend)?;
                proof
                    .insert(
                        "replacementMutationsSha256",
                        s(&hex(&sha256(candidates.exact_prepared_mutations()))),
                    )
                    .map_err(active_wal_backend)?;
                (Some(Box::new(replacement)), Some(mapping))
            };
            Ok((
                index,
                ActiveWalNativeRecovery {
                    raw: active_wal_bytes(&proof)?,
                    prior: previous,
                    replacement,
                    mapping,
                    installed: false,
                },
            ))
        })();
        let (index, native_recovery) = match checked {
            Ok(value) => value,
            Err(error) => {
                return Err(Box::new(RejectedNativeMainActiveReopenV1 {
                    cause: error.to_string(),
                    owner: self,
                }))
            }
        };
        let Self {
            mut controller,
            service,
        } = self;
        let ActiveServiceWithoutNativeIndex {
            financial,
            namespace: _,
            frame,
            source_start_error,
            origin,
            head,
            history,
            pending_observation,
            attempted_write,
            observation_limit,
            inline_profile,
        } = service;
        let NativeMainFinancialWithoutIndexV1 {
            financial,
            foundation,
            foundation_head,
            origin_rows,
            binding,
            origin_physical,
            head: base_head,
            history: base_history,
            halted,
        } = financial;
        let service = SourceMainNativeActiveServiceV1 {
            owner: SourceMainNativeSessionV13 {
                financial,
                index,
                foundation,
                foundation_head,
                origin_rows,
                binding,
                origin_physical,
                head: base_head,
                history: base_history,
                halted,
            },
            frame,
            source_start_error,
            origin,
            head,
            history,
            pending_observation,
            attempted_write,
            observation_limit,
            inline_profile,
        };
        controller.owner = Some(ActiveWalOwner::Active(Box::new(service)));
        controller
            .pending
            .as_mut()
            .expect("retained original action")
            .native_recovery = Some(native_recovery);
        controller.phase = if candidate == Candidate::Post {
            NativeMainActiveWalPhaseV1::Observe
        } else {
            NativeMainActiveWalPhaseV1::NativeSupersessionIntent
        };
        Ok(controller)
    }
}
