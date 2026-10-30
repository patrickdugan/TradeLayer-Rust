//! Explicit policy13 owner loan. No live Journal or financial mirror in Core.
//! The frozen legacy foundation is never represented as the current head.
use crate::hash256::sha256;
use crate::source_compact_index_v2::{
    MetadataSnapshot, RowStreamReceipt, SourceCompactIndex, StreamedValidityEvaluation,
};
use crate::source_contract_replay::{
    SourceContractCommandOutcome, SourceContractReplay, SourceContractReplayHead,
};
use crate::source_contract_runtime::{SourceContractRuntime, SourceContractRuntimePolicy};
use crate::source_main_native_owners::{NativeMainOwners, STORE_BINDING_PROFILE};
use crate::validity_source::{DependencyCall, DependencyReturn, SourceRuleError, SourceValue as V};
use std::path::PathBuf;
pub const PROFILE: &str = "TL-MAIN-NATIVE-OWNER-SESSION-13";
pub const RECEIPT_PROFILE: &str = "TL-MAIN-NATIVE-COMPLETE-READ-13-1";
const MAX_COMMANDS: usize = 4096;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeMainSessionHead {
    pub commands: u64,
    pub commitment: [u8; 32],
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativePhysicalBinding {
    pub persistent_identity: String,
    pub canonical_catalog: PathBuf,
    pub metadata: MetadataSnapshot,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeMainSessionExpectation {
    pub session: NativeMainSessionHead,
    pub physical: NativePhysicalBinding,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeMainSessionCommand {
    ConstructConsensusFromIndex {
        start: V,
    },
    /// Original bounded caller array; never a partial native query result.
    ProcessRows {
        rows: V,
        height: V,
        live: bool,
    },
    /// Acts on the same financial owner; escaped txIndex access refuses.
    Invoke(DependencyCall),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeMainSessionStatus {
    Completed(DependencyReturn),
    Suspended {
        error: SourceRuleError,
        consumed_rows: u64,
    },
    RecoveryRequired {
        cause: String,
        consumed_rows: u64,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeMainSessionReceipt {
    pub profile: String,
    pub sequence: u64,
    pub command: NativeMainSessionCommand,
    pub status: NativeMainSessionStatus,
    pub grouping: Option<RowStreamReceipt>,
    pub validity: Vec<StreamedValidityEvaluation>,
    pub effects: Vec<V>,
    pub financial_commitment: [u8; 32],
    pub after_rows: Option<RowStreamReceipt>,
    pub before: NativePhysicalBinding,
    pub after: Option<NativePhysicalBinding>,
    pub previous_head: [u8; 32],
    pub head: [u8; 32],
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeMainSessionError {
    WrongHead,
    WrongPhysicalBinding,
    Halted,
    Capacity,
    InvalidFoundation,
    Backend(String),
}
pub struct RejectedNativeMainSession {
    pub error: NativeMainSessionError,
    owners: NativeMainOwners,
}
impl core::fmt::Debug for RejectedNativeMainSession {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RejectedNativeMainSession")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}
impl RejectedNativeMainSession {
    pub fn into_owners(self) -> NativeMainOwners {
        self.owners
    }
}
/// Both actual owners stay private. No into_replay or legacy checkpoint API
/// exposes a changed native financial owner with a frozen legacy head.
pub struct SourceMainNativeSessionV13 {
    financial: SourceContractReplay,
    index: SourceCompactIndex,
    foundation: Vec<u8>,
    foundation_head: SourceContractReplayHead,
    origin_rows: RowStreamReceipt,
    binding: V,
    origin_physical: NativePhysicalBinding,
    head: NativeMainSessionHead,
    history: Vec<NativeMainSessionReceipt>,
    halted: bool,
}
impl core::fmt::Debug for SourceMainNativeSessionV13 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SourceMainNativeSessionV13")
            .field("head", &self.head)
            .field("halted", &self.halted)
            .finish_non_exhaustive()
    }
}
fn s(value: &str) -> V {
    V::String(value.into())
}
fn obj(fields: &[(&str, V)]) -> V {
    V::Object(
        fields
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
    )
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn frame(bytes: &mut Vec<u8>, value: &V) -> Result<(), NativeMainSessionError> {
    let text = value
        .diagnostic_json()
        .map_err(|e| NativeMainSessionError::Backend(e.to_string()))?;
    let len = u32::try_from(text.len()).map_err(|_| NativeMainSessionError::Capacity)?;
    bytes.extend_from_slice(&len.to_be_bytes());
    bytes.extend_from_slice(text.as_bytes());
    Ok(())
}
fn complete(index: &SourceCompactIndex) -> Result<RowStreamReceipt, NativeMainSessionError> {
    let pin = index
        .snapshot()
        .map_err(|e| NativeMainSessionError::Backend(e.to_string()))?;
    let mut cursor = index
        .read_generation(&pin)
        .map_err(|e| NativeMainSessionError::Backend(e.to_string()))?
        .cursor();
    while cursor
        .next_row()
        .map_err(|e| NativeMainSessionError::Backend(e.to_string()))?
        .is_some()
    {}
    let receipt = cursor.receipt();
    if !receipt.complete {
        return Err(NativeMainSessionError::Backend(
            "incomplete native read".into(),
        ));
    }
    Ok(receipt)
}
fn physical(index: &SourceCompactIndex) -> Result<NativePhysicalBinding, NativeMainSessionError> {
    let (persistent_identity, canonical_catalog) = index
        .native_store_context()
        .map_err(|e| NativeMainSessionError::Backend(e.to_string()))?;
    Ok(NativePhysicalBinding {
        persistent_identity,
        canonical_catalog,
        metadata: index
            .snapshot()
            .map_err(|e| NativeMainSessionError::Backend(e.to_string()))?,
    })
}
fn logical_read(receipt: &RowStreamReceipt) -> V {
    obj(&[
        ("profile", s(&receipt.profile)),
        ("rows", s(&receipt.rows.to_string())),
        ("chain", s(&hex(&receipt.chain_sha256d))),
        ("complete", V::Bool(receipt.complete)),
    ])
}
impl NativeMainSessionCommand {
    pub fn value(&self) -> V {
        match self {
            Self::ConstructConsensusFromIndex { start } => obj(&[
                ("kind", s("ConstructConsensusFromIndex")),
                ("start", start.clone()),
            ]),
            Self::Invoke(call) => obj(&[
                ("kind", s("Invoke")),
                ("target", s(&call.target)),
                ("args", V::Array(call.arguments.clone())),
            ]),
            Self::ProcessRows { rows, height, live } => obj(&[
                ("kind", s("ProcessRows")),
                ("rows", rows.clone()),
                ("height", height.clone()),
                ("live", V::Bool(*live)),
            ]),
        }
    }
}
impl NativeMainSessionStatus {
    pub fn value(&self) -> V {
        match self {
            Self::Completed(returned) => obj(&[
                ("kind", s("Completed")),
                (
                    "source",
                    SourceContractCommandOutcome::Dependency(returned.clone()).value(),
                ),
            ]),
            Self::Suspended {
                error,
                consumed_rows,
            } => obj(&[
                ("kind", s("Suspended")),
                ("error", s(&format!("{error:?}"))),
                ("consumedRows", s(&consumed_rows.to_string())),
            ]),
            Self::RecoveryRequired {
                cause,
                consumed_rows,
            } => obj(&[
                ("kind", s("RecoveryRequired")),
                ("cause", s(cause)),
                ("consumedRows", s(&consumed_rows.to_string())),
            ]),
        }
    }
}
fn logical_receipt_bytes(
    receipt: &NativeMainSessionReceipt,
) -> Result<Vec<u8>, NativeMainSessionError> {
    let mut bytes = RECEIPT_PROFILE.as_bytes().to_vec();
    bytes.extend_from_slice(&receipt.sequence.to_be_bytes());
    frame(&mut bytes, &receipt.command.value())?;
    frame(&mut bytes, &receipt.status.value())?;
    frame(
        &mut bytes,
        &receipt.grouping.as_ref().map_or(V::Null, logical_read),
    )?;
    bytes.extend_from_slice(&(receipt.validity.len() as u64).to_be_bytes());
    for evaluation in &receipt.validity {
        let query = &evaluation.query;
        let update = evaluation.update.as_ref().map_or(V::Null, |observation| {
            obj(&[
                ("target", s(&observation.call.target)),
                ("arguments", V::Array(observation.call.arguments.clone())),
                (
                    "returned",
                    SourceContractCommandOutcome::Dependency(observation.returned.clone()).value(),
                ),
            ])
        });
        let outcome = match &evaluation.outcome {
            crate::validity_source::SourceOutcome::Returned(v) => obj(&[("returned", v.clone())]),
            crate::validity_source::SourceOutcome::Thrown(e) => {
                obj(&[("name", s(&e.name)), ("message", s(&e.message))])
            }
        };
        frame(
            &mut bytes,
            &obj(&[
                ("profile", s(&evaluation.profile)),
                ("predicate", s(&query.predicate)),
                ("visited", s(&query.visited_rows.to_string())),
                ("matched", s(&query.matched_rows.to_string())),
                ("chain", s(&hex(&query.chain_sha256d))),
                ("complete", V::Bool(query.complete)),
                ("update", update),
                ("outcome", outcome),
            ]),
        )?;
    }
    bytes.extend_from_slice(&(receipt.effects.len() as u64).to_be_bytes());
    for effect in &receipt.effects {
        frame(&mut bytes, effect)?;
    }
    bytes.extend_from_slice(&receipt.financial_commitment);
    frame(
        &mut bytes,
        &receipt.after_rows.as_ref().map_or(V::Null, logical_read),
    )?;
    Ok(bytes)
}
impl SourceMainNativeSessionV13 {
    pub fn adopt(mut owners: NativeMainOwners) -> Result<Self, Box<RejectedNativeMainSession>> {
        let prepared = (|| {
            let expected = owners
                .expectation()
                .map_err(|e| NativeMainSessionError::Backend(format!("{e:?}")))?;
            let (financial, index) = owners
                .loan(&expected)
                .map_err(|e| NativeMainSessionError::Backend(format!("{e:?}")))?;
            if financial.runtime().policy() != SourceContractRuntimePolicy::MainOrchestrationV12 {
                return Err(NativeMainSessionError::InvalidFoundation);
            }
            let foundation = financial
                .checkpoint()
                .map_err(|e| NativeMainSessionError::Backend(e.to_string()))?;
            let origin_rows = complete(index)?;
            let state = financial
                .runtime()
                .state_value()
                .map_err(|e| NativeMainSessionError::Backend(e.to_string()))?;
            let client = state.get("mainProduction").get("client");
            let binding = obj(&[
                ("profile", s(STORE_BINDING_PROFILE)),
                ("chain", client.get("chain")),
                ("test", client.get("test")),
                ("context", s(&expected.metadata.context)),
                ("metadataProfile", s(&expected.metadata.profile)),
                ("sessionProfile", s(PROFILE)),
            ]);
            physical(index)?;
            let foundation_head = financial.head();
            let mut seed = PROFILE.as_bytes().to_vec();
            seed.extend_from_slice(&foundation_head.commands.to_be_bytes());
            seed.extend_from_slice(&foundation_head.commitment);
            seed.extend_from_slice(
                &financial
                    .native_main_financial_commitment()
                    .map_err(|e| NativeMainSessionError::Backend(e.to_string()))?,
            );
            frame(&mut seed, &binding)?;
            frame(&mut seed, &logical_read(&origin_rows))?;
            Ok((
                foundation,
                origin_rows,
                binding,
                foundation_head,
                sha256(&seed),
                physical(index)?,
            ))
        })();
        let (foundation, origin_rows, binding, foundation_head, commitment, origin_physical) =
            match prepared {
                Ok(v) => v,
                Err(error) => return Err(Box::new(RejectedNativeMainSession { error, owners })),
            };
        let expected = owners.expectation().expect("unchanged preflighted owners");
        if let Err(error) = owners
            .loan(&expected)
            .expect("unchanged preflight")
            .0
            .native_main_runtime_mut()
            .native_main_bind(binding.clone())
        {
            return Err(Box::new(RejectedNativeMainSession {
                error: NativeMainSessionError::Backend(error.to_string()),
                owners,
            }));
        }
        let (financial, index) = owners.into_owners();
        let head = NativeMainSessionHead {
            commands: 0,
            commitment,
        };
        Ok(Self {
            financial,
            index,
            foundation,
            foundation_head,
            origin_rows,
            binding,
            origin_physical,
            head,
            history: vec![],
            halted: false,
        })
    }
    pub fn head(&self) -> NativeMainSessionHead {
        self.head.clone()
    }
    pub fn foundation_head(&self) -> SourceContractReplayHead {
        self.foundation_head
    }
    /// Explicitly the pre-native source-command foundation, never current.
    pub fn foundation_checkpoint(&self) -> &[u8] {
        &self.foundation
    }
    pub fn origin_rows(&self) -> &RowStreamReceipt {
        &self.origin_rows
    }
    pub fn origin_physical(&self) -> &NativePhysicalBinding {
        &self.origin_physical
    }
    pub fn logical_binding(&self) -> &V {
        &self.binding
    }
    pub fn financial_runtime(&self) -> &SourceContractRuntime {
        self.financial.runtime()
    }
    pub fn require_authenticated_admission(&self) -> Result<(), SourceRuleError> {
        Err(SourceRuleError::UnauthenticatedInputAndState)
    }
    pub fn index(&self) -> &SourceCompactIndex {
        &self.index
    }
    pub fn history(&self) -> &[NativeMainSessionReceipt] {
        &self.history
    }
    pub fn expectation(&self) -> Result<NativeMainSessionExpectation, NativeMainSessionError> {
        Ok(NativeMainSessionExpectation {
            session: self.head(),
            physical: physical(&self.index)?,
        })
    }
    pub fn apply(
        &mut self,
        expected: &NativeMainSessionExpectation,
        command: NativeMainSessionCommand,
    ) -> Result<&NativeMainSessionReceipt, NativeMainSessionError> {
        if self.halted {
            return Err(NativeMainSessionError::Halted);
        }
        if expected.session != self.head {
            return Err(NativeMainSessionError::WrongHead);
        }
        let before = physical(&self.index)?;
        if before != expected.physical {
            return Err(NativeMainSessionError::WrongPhysicalBinding);
        }
        if self.history.len() >= MAX_COMMANDS {
            return Err(NativeMainSessionError::Capacity);
        }
        command
            .value()
            .validate()
            .map_err(|e| NativeMainSessionError::Backend(e.to_string()))?;
        let mut ordinary_effects = None;
        let (result, grouping, validity, consumed, storage_stop) = match &command {
            NativeMainSessionCommand::ConstructConsensusFromIndex { start } => {
                let result = self
                    .financial
                    .native_main_runtime_mut()
                    .native_main_construct(&mut self.index, start.clone());
                (
                    result.result,
                    result.grouping,
                    result.validity,
                    result.consumed_rows,
                    result.storage_stop,
                )
            }
            NativeMainSessionCommand::ProcessRows { rows, height, live } => {
                let result = self.financial.native_main_runtime_mut().native_main_rows(
                    &mut self.index,
                    rows.clone(),
                    height.clone(),
                    *live,
                );
                (
                    result.result,
                    result.grouping,
                    result.validity,
                    result.consumed_rows,
                    result.storage_stop,
                )
            }
            NativeMainSessionCommand::Invoke(call) => {
                let receipt = self
                    .financial
                    .apply(
                        crate::source_contract_replay::SourceContractCommand::Invoke(call.clone()),
                    )
                    .map_err(|error| NativeMainSessionError::Backend(error.to_string()))?;
                ordinary_effects = Some(receipt.effects);
                let result = match receipt.outcome {
                    SourceContractCommandOutcome::Dependency(returned) => Ok(returned),
                    SourceContractCommandOutcome::LocalError(error) => Err(error),
                    _ => return Err(NativeMainSessionError::InvalidFoundation),
                };
                (result, None, vec![], 0, None)
            }
        };
        let effects = ordinary_effects
            .unwrap_or_else(|| self.financial.native_main_runtime_mut().take_effects());
        let after = physical(&self.index);
        let after_rows = complete(&self.index);
        let status = if let Some(cause) = storage_stop {
            NativeMainSessionStatus::RecoveryRequired {
                cause,
                consumed_rows: consumed,
            }
        } else if let Err(error) = &after {
            NativeMainSessionStatus::RecoveryRequired {
                cause: format!("{error:?}"),
                consumed_rows: consumed,
            }
        } else if let Err(error) = &after_rows {
            NativeMainSessionStatus::RecoveryRequired {
                cause: format!("{error:?}"),
                consumed_rows: consumed,
            }
        } else {
            match result {
                Ok(returned) => NativeMainSessionStatus::Completed(returned),
                Err(error) => NativeMainSessionStatus::Suspended {
                    error,
                    consumed_rows: consumed,
                },
            }
        };
        self.halted = !matches!(status, NativeMainSessionStatus::Completed(_));
        let financial_commitment = match self.financial.native_main_financial_commitment() {
            Ok(head) => head,
            Err(error) => {
                self.halted = true;
                return Err(NativeMainSessionError::Backend(format!(
                    "accepted prefix needs recovery: {error}"
                )));
            }
        };
        let mut receipt = NativeMainSessionReceipt {
            profile: RECEIPT_PROFILE.into(),
            sequence: self.head.commands,
            command,
            status,
            grouping,
            validity,
            effects,
            financial_commitment,
            after_rows: after_rows.ok(),
            before,
            after: after.ok(),
            previous_head: self.head.commitment,
            head: [0; 32],
        };
        let encoded = match logical_receipt_bytes(&receipt) {
            Ok(bytes) => bytes,
            Err(error) => {
                self.halted = true;
                return Err(error);
            }
        };
        let mut bytes = self.head.commitment.to_vec();
        bytes.extend_from_slice(&encoded);
        receipt.head = sha256(&bytes);
        self.head = NativeMainSessionHead {
            commands: self.head.commands + 1,
            commitment: receipt.head,
        };
        self.history.push(receipt);
        Ok(self.history.last().unwrap())
    }
}
include!("source_main_native_checkpoint_impl.rs");
include!("source_main_native_historical_replay_impl.rs");
include!("source_main_checked_history_provider_impl.rs");
include!("source_main_native_cold_prefix_impl.rs");
include!("source_main_native_cold_active_reexecution_impl.rs");
include!("source_main_mode4_source_reexecution_impl.rs");
include!("source_main_native_cold_active_observation_impl.rs");
include!("source_main_native_step_session_impl.rs");
include!("source_main_native_step_segment_impl.rs");

include!("source_main_native_row_session_impl.rs");
include!("source_main_native_completed_bytes_impl.rs");
include!("source_main_native_completed_bounded_codec_impl.rs");
include!("source_main_native_completed_chain_impl.rs");
include!("source_main_native_completed_publication_impl.rs");
include!("source_main_native_completed_install_impl.rs");
include!("source_main_native_completed_fields_impl.rs");
include!("source_main_native_no_index_impl.rs");
include!("source_main_native_supersession_controller_impl.rs");
include!("source_main_native_prefix_ack_controller_impl.rs");
include!("source_main_native_no_control_store_impl.rs");
include!("source_main_native_service_session_impl.rs");
include!("source_main_inline_service_session_impl.rs");
include!("source_main_retained_caller_session_impl.rs");
include!("source_main_retained_file_command_impl.rs");
include!("source_main_retained_file_session_impl.rs");
include!("source_main_automatic_file_session_impl.rs");
include!("source_main_retained_consensus_session_impl.rs");
include!("source_main_fresh_input_session_impl.rs");
include!("source_main_coercive_sync_session_impl.rs");
include!("source_main_null_sync_session_impl.rs");
include!("source_main_retained_caller_start_wal_impl.rs");
include!("source_main_retained_caller_selected_dispatch_impl.rs");
include!("source_main_retained_caller_selected_wal_impl.rs");
include!("source_main_retained_caller_selected_no_index_impl.rs");
include!("source_main_retained_caller_selected_replacement_impl.rs");
include!("source_main_retained_caller_selected_escrow_impl.rs");
#[cfg(test)]
#[path = "source_main_inline_service_tests.rs"]
mod native_inline_service_tests;
include!("source_main_active_service_action_codec_impl.rs");
include!("source_main_active_service_durable_impl.rs");
include!("source_main_active_service_recovery_impl.rs");
include!("source_main_service_escrow_append_impl.rs");
include!("source_main_actual_fact_append_session_impl.rs");
include!("source_main_actual_fact_controller_impl.rs");
include!("source_main_actual_fact_controller_driver_impl.rs");
include!("source_main_actual_fact_recovery_owner_impl.rs");
include!("source_main_actual_fact_replacement_impl.rs");
include!("source_main_active_escrow_controller_impl.rs");
include!("source_main_inline_wal_impl.rs");
include!("source_main_mode4_trace_pin_impl.rs");
include!("source_main_inline_wal_recovery_impl.rs");
include!("source_main_inline_wal_install_impl.rs");
#[cfg(test)]
#[path = "source_main_active_service_action_codec_tests.rs"]
mod native_active_service_action_codec_tests;
#[cfg(test)]
#[path = "source_main_active_service_durable_tests.rs"]
mod native_active_service_durable_tests;

#[cfg(test)]
#[path = "source_main_native_service_tests.rs"]
mod native_main_service_tests;
#[cfg(test)]
#[path = "source_main_native_row_session_tests.rs"]
mod native_row_session_tests;

#[cfg(test)]
#[path = "source_main_native_catchup_session_tests.rs"]
mod native_catchup_session_tests;

#[cfg(test)]
#[path = "source_main_native_no_index_tests.rs"]
mod native_main_no_index_tests;

#[cfg(test)]
#[path = "source_main_native_no_control_store_tests.rs"]
mod native_main_no_control_store_tests;

#[cfg(test)]
#[path = "source_main_native_completed_publication_tests.rs"]
mod completed_publication_tests;

#[cfg(test)]
include!("source_main_native_completed_guard_test_impl.rs");

include!("source_main_native_completed_controller_impl.rs");
#[cfg(test)]
#[path = "source_main_native_completed_recovery_tests.rs"]
mod completed_recovery_tests;

#[cfg(test)]
#[path = "source_main_native_historical_replay_tests.rs"]
mod native_historical_replay_tests;

#[cfg(test)]
#[path = "source_main_retained_caller_tests.rs"]
mod retained_caller_tests;

#[cfg(test)]
#[path = "source_main_retained_consensus_tests.rs"]
mod retained_consensus_tests;

#[cfg(test)]
#[path = "source_main_retained_caller_start_wal_tests.rs"]
mod retained_caller_start_wal_tests;

#[cfg(test)]
#[path = "source_main_retained_caller_selected_wal_tests.rs"]
mod retained_caller_selected_wal_tests;

#[cfg(test)]
#[path = "source_main_retained_file_command_tests.rs"]
mod retained_file_command_tests;
#[cfg(test)]
#[path = "source_main_retained_file_tests.rs"]
mod retained_file_tests;

#[cfg(test)]
#[path = "source_main_automatic_file_tests.rs"]
mod automatic_native_file_tests;

#[cfg(test)]
#[path = "source_main_fresh_input_core_tests.rs"]
mod fresh_rpc_input_tests;
