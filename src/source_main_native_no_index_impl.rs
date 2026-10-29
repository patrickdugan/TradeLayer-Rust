// A real type-state move, never a dummy Journal beside a financial owner.
use crate::source_compact_index_v2::{
    NativePreparedValidityCandidateV1, NativePreparedValidityRecoveryObservationV1,
};
use crate::storage::{Namespace, Recovery};
use std::path::Path;

struct NativeMainFinancialWithoutIndexV1 {
    financial: SourceContractReplay,
    foundation: Vec<u8>,
    foundation_head: SourceContractReplayHead,
    origin_rows: RowStreamReceipt,
    binding: V,
    origin_physical: NativePhysicalBinding,
    head: NativeMainSessionHead,
    history: Vec<NativeMainSessionReceipt>,
    halted: bool,
}
pub struct SourceMainNativeRowNoIndexV1 {
    owner: NativeMainFinancialWithoutIndexV1,
    namespace: Namespace,
    store: NativeIntentStoreV1,
    permit: NativeCommandIntentPermitV1,
    frame: Option<NativeMainOwnedFrameV1>,
    source_start_error: Option<SourceRuleError>,
    pc: NativeMainRowControllerPcV1,
    receipt_budget: usize,
    command: NativeMainSessionCommand,
    reopened: Option<NativePreparedValidityRecoveryObservationV1>,
    supersession: Option<NativeMainPreparedRowSupersessionV1>,
    superseding_active: bool,
    prefix_ack: Option<NativePreparedPrefixAckV1>,
    prefix_ack_recovered: Option<NativeIntentStorePinV1>,
}
pub struct RejectedNativeMainNoIndexLoanV1 {
    pub cause: String,
    session: SourceMainNativeRowSessionV1,
}
impl RejectedNativeMainNoIndexLoanV1 {
    pub fn into_session(self) -> SourceMainNativeRowSessionV1 {
        self.session
    }
}
impl core::fmt::Debug for RejectedNativeMainNoIndexLoanV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RejectedNativeMainNoIndexLoanV1")
            .field("cause", &self.cause)
            .finish_non_exhaustive()
    }
}
pub struct RejectedNativeMainReopenV1 {
    pub cause: String,
    owner: SourceMainNativeRowNoIndexV1,
}
impl RejectedNativeMainReopenV1 {
    pub fn into_owner(self) -> SourceMainNativeRowNoIndexV1 {
        self.owner
    }
    pub fn owner(&self) -> &SourceMainNativeRowNoIndexV1 {
        &self.owner
    }
}
impl core::fmt::Debug for RejectedNativeMainReopenV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RejectedNativeMainReopenV1")
            .field("cause", &self.cause)
            .finish_non_exhaustive()
    }
}
impl SourceMainNativeRowSessionV1 {
    pub fn into_no_index(
        self,
    ) -> Result<SourceMainNativeRowNoIndexV1, Box<RejectedNativeMainNoIndexLoanV1>> {
        if !matches!(
            self.pc,
            NativeMainRowControllerPcV1::NativeIntent
                | NativeMainRowControllerPcV1::NativeAppendAttempted
                | NativeMainRowControllerPcV1::NativePost
        ) || self.pending_write().is_none()
        {
            return Err(Box::new(RejectedNativeMainNoIndexLoanV1 {
                cause: "actual unresolved native write intent required".into(),
                session: self,
            }));
        }
        let Self {
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
        } = self;
        let SourceMainNativeSessionV13 {
            financial,
            index,
            foundation,
            foundation_head,
            origin_rows,
            binding,
            origin_physical,
            head,
            history,
            halted,
        } = owner;
        let namespace = index.native_namespace().clone();
        // Drop ONLY the old mutable index. Original immutable readers may stay
        // in the source frame, but no replacement mutable owner exists yet.
        drop(index);
        Ok(SourceMainNativeRowNoIndexV1 {
            owner: NativeMainFinancialWithoutIndexV1 {
                financial,
                foundation,
                foundation_head,
                origin_rows,
                binding,
                origin_physical,
                head,
                history,
                halted,
            },
            namespace,
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
    pub fn reopened_observation(&self) -> Option<&NativePreparedValidityRecoveryObservationV1> {
        self.reopened.as_ref()
    }
}
impl SourceMainNativeRowNoIndexV1 {
    pub fn financial_runtime(&self) -> &SourceContractRuntime {
        self.owner.financial.runtime()
    }
    pub fn effects(&self) -> &[V] {
        self.owner.financial.runtime().native_main_row_effects()
    }
    pub fn pending_write(&self) -> &crate::source_compact_index_v2::NativePreparedValidityWriteV1 {
        self.frame
            .as_ref()
            .and_then(|frame| frame.pending_write())
            .expect("linear unresolved native await")
    }
    pub fn store_pin(&self) -> Result<NativeIntentStorePinV1, String> {
        self.store.pin().map_err(|error| error.to_string())
    }
    pub fn command(&self) -> &NativeMainSessionCommand {
        &self.command
    }
    pub fn try_reopen(
        self,
        root: &Path,
        candidate: NativePreparedValidityCandidateV1,
        recovery: Recovery,
    ) -> Result<SourceMainNativeRowSessionV1, Box<RejectedNativeMainReopenV1>> {
        // One explicit candidate, with the old mutable index already dropped.
        // Neither discovery nor a source/native append is performed here.
        let opened = SourceCompactIndex::reopen_prepared_validity(
            root,
            self.namespace.clone(),
            self.pending_write(),
            candidate,
            recovery,
        );
        let (index, observation) = match opened {
            Ok(opened) => opened,
            Err(error) => {
                return Err(Box::new(RejectedNativeMainReopenV1 {
                    cause: error.to_string(),
                    owner: self,
                }))
            }
        };
        let Self {
            owner,
            store,
            permit,
            frame,
            source_start_error,
            receipt_budget,
            command,
            supersession,
            superseding_active,
            prefix_ack,
            prefix_ack_recovered,
            namespace: _,
            pc: _,
            reopened: _,
        } = self;
        let NativeMainFinancialWithoutIndexV1 {
            financial,
            foundation,
            foundation_head,
            origin_rows,
            binding,
            origin_physical,
            head,
            history,
            halted,
        } = owner;
        Ok(SourceMainNativeRowSessionV1 {
            owner: SourceMainNativeSessionV13 {
                financial,
                index,
                foundation,
                foundation_head,
                origin_rows,
                binding,
                origin_physical,
                head,
                history,
                halted,
            },
            store,
            permit,
            frame,
            source_start_error,
            pc: if candidate == NativePreparedValidityCandidateV1::Post {
                NativeMainRowControllerPcV1::NativePost
            } else {
                NativeMainRowControllerPcV1::RecoveryBefore
            },
            receipt_budget,
            command,
            reopened: Some(observation),
            supersession,
            superseding_active,
            prefix_ack,
            prefix_ack_recovered,
        })
    }
    pub fn visit_canonical_fields(
        &self,
        visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
    ) -> Result<(), SourceRuleError> {
        visitor("profile", &s("TL-MAIN-ACTUAL-NO-INDEX-LOAN-13-1"))?;
        visitor("nativeOwnerPresent", &V::Bool(false))?;
        visitor(
            "namespace/path",
            &s(&self.namespace.path(Path::new("")).to_string_lossy()),
        )?;
        visitor(
            "retainedControllerPc",
            &s(match self.pc {
                NativeMainRowControllerPcV1::NativeIntent => "NativeIntent",
                NativeMainRowControllerPcV1::NativeAppendAttempted => "NativeAppendAttempted",
                NativeMainRowControllerPcV1::NativePost => "NativePost",
                _ => "InvalidLoan",
            }),
        )?;
        visitor("receiptBudget", &s(&self.receipt_budget.to_string()))?;
        visitor(
            "sourceStartError",
            &self
                .source_start_error
                .as_ref()
                .map_or(V::Null, |error| s(&format!("{error:?}"))),
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
            _ => unreachable!("checked actual command"),
        }
        if let Some(frame) = &self.frame {
            frame.visit_canonical_fields(&mut |path, value| {
                visitor(&format!("frame/{path}"), value)
            })?;
        }
        native_main_reopen_fields(self.reopened.as_ref(), visitor)?;
        if self.supersession.is_some() || self.superseding_active {
            native_main_supersession_fields(
                self.supersession.as_ref(),
                self.superseding_active,
                visitor,
            )?;
        }
        if self.prefix_ack_recovered.is_some() {
            native_main_prefix_ack_fields(
                self.prefix_ack.as_ref(),
                self.prefix_ack_recovered.as_ref(),
                visitor,
            )?;
        }
        let pin = self.permit.independently_retained_pin();
        visitor("controlPin/root", &s(&pin.canonical_root.to_string_lossy()))?;
        visitor("controlPin/context", &s(&hex(&pin.context)))?;
        visitor("controlPin/sequence", &s(&pin.sequence.to_string()))?;
        visitor("controlPin/state", &s(&hex(&pin.state_sha256)))?;
        let financial = self
            .owner
            .financial
            .native_main_financial_commitment()
            .map_err(|_| SourceRuleError::Backend)?;
        visitor("currentFinancialCommitment", &s(&hex(&financial)))?;
        visitor(
            "retainedFoundation/bytes",
            &s(&self.owner.foundation.len().to_string()),
        )?;
        visitor(
            "retainedFoundation/sha256",
            &s(&hex(&sha256(&self.owner.foundation))),
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
            "retainedFoundationHead/commands",
            &s(&self.owner.foundation_head.commands.to_string()),
        )?;
        visitor(
            "retainedFoundationHead/commitment",
            &s(&hex(&self.owner.foundation_head.commitment)),
        )?;
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
}
fn native_main_reopen_fields(
    observation: Option<&NativePreparedValidityRecoveryObservationV1>,
    visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
) -> Result<(), SourceRuleError> {
    visitor("reopen/present", &V::Bool(observation.is_some()))?;
    if let Some(observation) = observation {
        visitor("reopen/profile", &s(observation.profile()))?;
        visitor(
            "reopen/candidate",
            &s(&format!("{:?}", observation.candidate())),
        )?;
        let pin = observation.snapshot();
        visitor("reopen/snapshot/profile", &s(&pin.profile))?;
        visitor("reopen/snapshot/context", &s(&pin.context))?;
        visitor("reopen/snapshot/sequence", &s(&pin.sequence.to_string()))?;
        let publication = &pin.publication;
        visitor(
            "reopen/snapshot/generation",
            &s(&publication.generation.to_string()),
        )?;
        visitor(
            "reopen/snapshot/parent",
            &publication.parent.map_or(V::Null, |n| s(&n.to_string())),
        )?;
        visitor(
            "reopen/snapshot/physicalCreation",
            &s(&publication.physical_creation_sequence.to_string()),
        )?;
        visitor(
            "reopen/snapshot/logicalCreation",
            &s(&publication.creation_sequence.to_string()),
        )?;
        visitor(
            "reopen/snapshot/manifest",
            &s(&hex(&publication.manifest_sha256d)),
        )?;
        let content = observation.content();
        visitor("reopen/content/profile", &s(&content.profile))?;
        visitor("reopen/content/keys", &s(&content.keys.to_string()))?;
        visitor("reopen/content/bytes", &s(&content.bytes.to_string()))?;
        visitor("reopen/content/chain", &s(&hex(&content.chain_sha256d)))?;
        visitor("reopen/content/complete", &V::Bool(content.complete))?;
        visitor(
            "reopen/discardedBytes",
            &s(&observation.recovery_report().discarded_bytes.to_string()),
        )?;
    }
    Ok(())
}
