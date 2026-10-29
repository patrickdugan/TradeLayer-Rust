// Only Store is absent. The same Replay and healthy native index remain in
// the original owner; no replacement financial state or dummy Store exists.
use crate::source_contract_replay::source_durable::SourceDurableFilesystem;
#[derive(Clone, Copy)]
enum NativeMainControlPostOperationV1 {
    PrefixAck,
    SupersedingIntent,
}
pub struct SourceMainNativeRowNoControlStoreV1 {
    owner: SourceMainNativeSessionV13,
    permit: NativeCommandIntentPermitV1,
    frame: Option<NativeMainOwnedFrameV1>,
    source_start_error: Option<SourceRuleError>,
    receipt_budget: usize,
    command: NativeMainSessionCommand,
    reopened: Option<NativePreparedValidityRecoveryObservationV1>,
    supersession: Option<NativeMainPreparedRowSupersessionV1>,
    superseding_active: bool,
    prefix_ack: Option<NativePreparedPrefixAckV1>,
    prefix_ack_recovered: Option<NativeIntentStorePinV1>,
    operation: NativeMainControlPostOperationV1,
}
pub struct RejectedNativeMainNoControlStoreV1 {
    pub cause: String,
    session: SourceMainNativeRowSessionV1,
}
impl RejectedNativeMainNoControlStoreV1 {
    pub fn into_session(self) -> SourceMainNativeRowSessionV1 {
        self.session
    }
}
impl core::fmt::Debug for RejectedNativeMainNoControlStoreV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RejectedNativeMainNoControlStoreV1")
            .field("cause", &self.cause)
            .finish_non_exhaustive()
    }
}
pub struct RejectedNativeMainControlPostOpenV1 {
    pub cause: String,
    owner: SourceMainNativeRowNoControlStoreV1,
}
impl RejectedNativeMainControlPostOpenV1 {
    pub fn owner(&self) -> &SourceMainNativeRowNoControlStoreV1 {
        &self.owner
    }
    pub fn into_owner(self) -> SourceMainNativeRowNoControlStoreV1 {
        self.owner
    }
}
impl core::fmt::Debug for RejectedNativeMainControlPostOpenV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RejectedNativeMainControlPostOpenV1")
            .field("cause", &self.cause)
            .finish_non_exhaustive()
    }
}
impl SourceMainNativeRowSessionV1 {
    pub fn into_no_control_store(
        self,
    ) -> Result<SourceMainNativeRowNoControlStoreV1, Box<RejectedNativeMainNoControlStoreV1>> {
        let operation = match self.pc {
            NativeMainRowControllerPcV1::PrefixAckAttempted if self.prefix_ack.is_some() => {
                NativeMainControlPostOperationV1::PrefixAck
            }
            NativeMainRowControllerPcV1::SupersessionStoreAttempted
                if self.supersession.is_some() =>
            {
                NativeMainControlPostOperationV1::SupersedingIntent
            }
            _ => {
                return Err(Box::new(RejectedNativeMainNoControlStoreV1 {
                    cause: "an actual retained control sync attempt is required".into(),
                    session: self,
                }))
            }
        };
        let before = match operation {
            NativeMainControlPostOperationV1::PrefixAck => self
                .prefix_ack
                .as_ref()
                .unwrap()
                .independently_expected_before(),
            NativeMainControlPostOperationV1::SupersedingIntent => self
                .supersession
                .as_ref()
                .unwrap()
                .intent
                .independently_expected_before(),
        };
        if self.store.pin().ok().as_ref() == Some(before) {
            return Err(Box::new(RejectedNativeMainNoControlStoreV1 { cause: "healthy independently known Before must retry exact prepared operation on the same Store".into(), session: self }));
        }
        let Self {
            owner,
            store,
            permit,
            frame,
            source_start_error,
            pc: _,
            receipt_budget,
            command,
            reopened,
            supersession,
            superseding_active,
            prefix_ack,
            prefix_ack_recovered,
        } = self;
        // Metadata index, financial Replay and source frame never leave owner.
        // Only the poisoned/ambiguous control Store lock is released.
        drop(store);
        Ok(SourceMainNativeRowNoControlStoreV1 {
            owner,
            permit,
            frame,
            source_start_error,
            receipt_budget,
            command,
            reopened,
            supersession,
            superseding_active,
            prefix_ack,
            prefix_ack_recovered,
            operation,
        })
    }
    pub fn resume_recovered_control_post(&mut self) -> NativeMainRowStopV1 {
        match self.pc {
            NativeMainRowControllerPcV1::PrefixAckSynced => {
                self.native_main_resume_acknowledged_source()
            }
            NativeMainRowControllerPcV1::SupersessionIntentSynced => {
                self.native_main_install_synced_supersession()
            }
            _ => {
                NativeMainRowStopV1::StorageStopped("verified synced control Post required".into())
            }
        }
    }
}
impl SourceMainNativeRowNoControlStoreV1 {
    pub fn visit_canonical_fields(
        &self,
        visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
    ) -> Result<(), SourceRuleError> {
        visitor("profile", &s("TL-MAIN-NO-CONTROL-STORE-13-1"))?;
        visitor("controlStorePresent", &V::Bool(false))?;
        visitor(
            "operation",
            &s(match self.operation {
                NativeMainControlPostOperationV1::PrefixAck => "PrefixAck",
                NativeMainControlPostOperationV1::SupersedingIntent => "SupersedingIntent",
            }),
        )?;
        native_main_supersession_store_pin(
            "expectedControlPost",
            self.independently_expected_control_post(),
            visitor,
        )?;
        native_main_supersession_store_pin(
            "retainedControlBefore",
            self.permit.independently_retained_pin(),
            visitor,
        )?;
        match &self.command {
            NativeMainSessionCommand::ProcessRows { rows, height, live } => {
                visitor("command/kind", &s("ProcessRows"))?;
                crate::source_contract_runtime::native_main_row_visit_value(
                    "command/rows",
                    rows,
                    visitor,
                )?;
                crate::source_contract_runtime::native_main_row_visit_value(
                    "command/height",
                    height,
                    visitor,
                )?;
                visitor("command/live", &V::Bool(*live))?;
            }
            NativeMainSessionCommand::ConstructConsensusFromIndex { start } => {
                visitor("command/kind", &s("ConstructConsensusFromIndex"))?;
                crate::source_contract_runtime::native_main_row_visit_value(
                    "command/start",
                    start,
                    visitor,
                )?;
            }
            _ => unreachable!("admitted raw source command"),
        }
        let financial = self
            .owner
            .financial
            .native_main_financial_commitment()
            .map_err(|_| SourceRuleError::Backend)?;
        visitor("currentFinancialCommitment", &s(&hex(&financial)))?;
        crate::source_contract_runtime::native_main_row_visit_value(
            "currentNativePhysical",
            &physical_value(&physical(&self.owner.index).map_err(|_| SourceRuleError::Backend)?),
            visitor,
        )?;
        visitor(
            "retainedHistoricalHead/commands",
            &s(&self.owner.head.commands.to_string()),
        )?;
        visitor(
            "retainedHistoricalHead/commitment",
            &s(&hex(&self.owner.head.commitment)),
        )?;
        visitor(
            "retainedFoundation/bytes",
            &s(&self.owner.foundation.len().to_string()),
        )?;
        visitor(
            "retainedFoundation/sha256",
            &s(&hex(&sha256(&self.owner.foundation))),
        )?;
        visitor(
            "retainedFoundationHead/commands",
            &s(&self.owner.foundation_head.commands.to_string()),
        )?;
        visitor(
            "retainedFoundationHead/commitment",
            &s(&hex(&self.owner.foundation_head.commitment)),
        )?;
        crate::source_contract_runtime::native_main_row_visit_value(
            "retainedBinding",
            &self.owner.binding,
            visitor,
        )?;
        crate::source_contract_runtime::native_main_row_visit_value(
            "retainedOriginPhysical",
            &physical_value(&self.owner.origin_physical),
            visitor,
        )?;
        crate::source_contract_runtime::native_main_row_visit_value(
            "retainedOriginRows",
            &logical_read(&self.owner.origin_rows),
            visitor,
        )?;
        visitor("retainedHalted", &V::Bool(self.owner.halted))?;
        visitor(
            "retainedHistory/count",
            &s(&self.owner.history.len().to_string()),
        )?;
        for (at, receipt) in self.owner.history.iter().enumerate() {
            let raw = logical_receipt_bytes(receipt).map_err(|_| SourceRuleError::Backend)?;
            visitor(
                &format!("retainedHistory/{at}/bytes"),
                &s(&raw.len().to_string()),
            )?;
            visitor(
                &format!("retainedHistory/{at}/sha256"),
                &s(&hex(&sha256(&raw))),
            )?;
        }
        if let Some(frame) = &self.frame {
            frame.visit_canonical_fields(&mut |path, value| {
                visitor(&format!("frame/{path}"), value)
            })?;
        }
        native_main_reopen_fields(self.reopened.as_ref(), visitor)?;
        native_main_supersession_fields(
            self.supersession.as_ref(),
            self.superseding_active,
            visitor,
        )?;
        native_main_prefix_ack_fields(
            self.prefix_ack.as_ref(),
            self.prefix_ack_recovered.as_ref(),
            visitor,
        )?;
        visitor("receiptBudget", &s(&self.receipt_budget.to_string()))?;
        visitor(
            "sourceStartError",
            &self
                .source_start_error
                .as_ref()
                .map_or(V::Null, |error| s(&format!("{error:?}"))),
        )?;
        visitor("effects/count", &s(&self.effects().len().to_string()))?;
        for (at, effect) in self.effects().iter().enumerate() {
            crate::source_contract_runtime::native_main_row_visit_value(
                &format!("effects/{at}"),
                effect,
                visitor,
            )?;
        }
        Ok(())
    }
    pub fn financial_runtime(&self) -> &SourceContractRuntime {
        self.owner.financial.runtime()
    }
    pub fn index(&self) -> &SourceCompactIndex {
        &self.owner.index
    }
    pub fn effects(&self) -> &[V] {
        self.owner.financial.runtime().native_main_row_effects()
    }
    pub fn independently_expected_control_post(&self) -> &NativeIntentStorePinV1 {
        match self.operation {
            NativeMainControlPostOperationV1::PrefixAck => self
                .prefix_ack
                .as_ref()
                .unwrap()
                .independently_expected_after(),
            NativeMainControlPostOperationV1::SupersedingIntent => self
                .supersession
                .as_ref()
                .unwrap()
                .intent
                .independently_expected_after(),
        }
    }
    pub fn try_reopen_synced_post(
        mut self,
        root: &Path,
        namespace: Namespace,
        filesystem: SourceDurableFilesystem,
    ) -> Result<SourceMainNativeRowSessionV1, Box<RejectedNativeMainControlPostOpenV1>> {
        let opened = match self.operation {
            NativeMainControlPostOperationV1::PrefixAck => {
                NativeIntentStoreV1::open_synced_native_prefix_ack(
                    root,
                    namespace,
                    self.prefix_ack.as_ref().unwrap(),
                    filesystem,
                )
            }
            NativeMainControlPostOperationV1::SupersedingIntent => {
                NativeIntentStoreV1::open_synced_superseding_intent(
                    root,
                    namespace,
                    &self.supersession.as_ref().unwrap().intent,
                    filesystem,
                )
            }
        };
        let store = match opened {
            Ok(store) => store,
            Err(error) => {
                return Err(Box::new(RejectedNativeMainControlPostOpenV1 {
                    cause: error.to_string(),
                    owner: self,
                }))
            }
        };
        let rebound = match self.operation {
            NativeMainControlPostOperationV1::PrefixAck => store.recover_synced_native_prefix_ack(
                self.prefix_ack.as_ref().unwrap(),
                &mut self.permit,
            ),
            NativeMainControlPostOperationV1::SupersedingIntent => store
                .recover_synced_superseding_native_intent(
                    &self.supersession.as_ref().unwrap().intent,
                    &mut self.permit,
                ),
        };
        if let Err(error) = rebound {
            drop(store);
            return Err(Box::new(RejectedNativeMainControlPostOpenV1 {
                cause: error.to_string(),
                owner: self,
            }));
        }
        let Self {
            owner,
            permit,
            frame,
            source_start_error,
            receipt_budget,
            command,
            reopened,
            supersession,
            superseding_active,
            prefix_ack,
            mut prefix_ack_recovered,
            operation,
        } = self;
        let pc = match operation {
            NativeMainControlPostOperationV1::PrefixAck => {
                prefix_ack_recovered = Some(
                    prefix_ack
                        .as_ref()
                        .unwrap()
                        .independently_expected_after()
                        .clone(),
                );
                NativeMainRowControllerPcV1::PrefixAckSynced
            }
            NativeMainControlPostOperationV1::SupersedingIntent => {
                NativeMainRowControllerPcV1::SupersessionIntentSynced
            }
        };
        Ok(SourceMainNativeRowSessionV1 {
            owner,
            store,
            permit,
            frame,
            source_start_error,
            pc,
            receipt_budget,
            command,
            reopened,
            supersession,
            superseding_active,
            prefix_ack,
            prefix_ack_recovered,
        })
    }
}
