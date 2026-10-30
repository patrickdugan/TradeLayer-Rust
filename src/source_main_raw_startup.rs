//! Typed raw startup foundation. No legacy Invoke namespace, replay/head escape,
//! financial snapshot input or owner cloning. Prefix completion is not Main success.
use crate::source_compact_index_v2::SourceCompactIndex;
use crate::source_contract_replay::{SourceContractReplay, SourceContractReplayHead};
use crate::source_contract_runtime::{NativeMainStartupIoStateV1, SourceContractRuntimePolicy};
use crate::source_main_native_owners::NativeMainExpectation;
use crate::source_main_startup_prefix::MainStartupPrefixV1;
pub use crate::source_main_startup_prefix::{
    StartupCommandV1, StartupErrorV1, StartupStopV1, StartupThrowV1,
};
use crate::validity_source::{SourceRuleError, SourceValue as V};

pub const PROFILE: &str = "TL-MAIN-TYPED-RAW-STARTUP-FOUNDATION-13-1";
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeStartupIoErrorV1 {
    pub name: String,
    pub message: String,
    pub code: V,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeStartupDatastoreErrorV1 {
    pub category: String,
    pub error: NativeStartupIoErrorV1,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeStartupInputsV1 {
    pub chain: String,
    pub test: bool,
    /// Recorded own client fields (or actual null/undefined), separate from the
    /// recorded SDK getChain/getTest data capability bound by chain/test above.
    pub main_client: V,
    /// Recorded SDK own fields, separate from getChain/getTest RPC facts.
    pub sdk_client_fields: V,
    pub automatic_client_error: Option<NativeStartupIoErrorV1>,
    pub global_client_error: Option<NativeStartupIoErrorV1>,
    pub datastore_error: Option<NativeStartupDatastoreErrorV1>,
}
pub struct RejectedNativeRawStartupV1 {
    pub error: SourceRuleError,
    financial: SourceContractReplay,
    index: SourceCompactIndex,
}
impl core::fmt::Debug for RejectedNativeRawStartupV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RejectedNativeRawStartupV1")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}
impl RejectedNativeRawStartupV1 {
    /// Only preflight refusals reach this type; no source owner was mutated.
    pub fn into_owners(self) -> (SourceContractReplay, SourceCompactIndex) {
        (self.financial, self.index)
    }
}
pub struct SourceMainRawStartupV1 {
    financial: SourceContractReplay,
    index: SourceCompactIndex,
    prefix: Option<MainStartupPrefixV1>,
    io: NativeMainStartupIoStateV1,
    foundation: Vec<u8>,
    foundation_head: SourceContractReplayHead,
    origin: NativeMainExpectation,
    inputs: NativeStartupInputsV1,
    generation: u64,
    commands: Vec<StartupCommandV1>,
    start_error: Option<SourceRuleError>,
}
impl SourceMainRawStartupV1 {
    pub fn begin(
        mut financial: SourceContractReplay,
        mut index: SourceCompactIndex,
        expected: &NativeMainExpectation,
        inputs: NativeStartupInputsV1,
        generation: u64,
    ) -> Result<Self, Box<RejectedNativeRawStartupV1>> {
        let checked = (|| {
            if financial.head() != expected.financial_head
                || financial.runtime().policy() != SourceContractRuntimePolicy::MainOrchestrationV12
                || index.native_namespace() != &expected.namespace
                || index.snapshot().map_err(|_| SourceRuleError::Backend)? != expected.metadata
            {
                return Err(SourceRuleError::InvalidInput);
            }
            let namespace = crate::storage::Namespace::new(&inputs.chain, inputs.test, "txIndex")
                .map_err(|_| SourceRuleError::InvalidInput)?;
            if namespace != expected.namespace {
                return Err(SourceRuleError::InvalidInput);
            }
            financial
                .runtime()
                .native_main_raw_startup_preflight(&inputs)?;
            financial.checkpoint().map_err(|_| SourceRuleError::Backend)
        })();
        let foundation = match checked {
            Ok(value) => value,
            Err(error) => {
                return Err(Box::new(RejectedNativeRawStartupV1 {
                    error,
                    financial,
                    index,
                }))
            }
        };
        let mut io = NativeMainStartupIoStateV1::new(inputs.clone());
        let begun = financial
            .native_main_runtime_mut()
            .native_main_raw_startup_begin(&mut index, &mut io, generation);
        let (prefix, start_error) = match begun {
            Ok(value) => (Some(value), None),
            Err(error) => (None, Some(error)),
        };
        Ok(Self {
            financial,
            index,
            prefix,
            io,
            foundation,
            foundation_head: expected.financial_head,
            origin: expected.clone(),
            inputs,
            generation,
            commands: vec![],
            start_error,
        })
    }
    pub fn profile(&self) -> &'static str {
        PROFILE
    }
    /// The unchanged pre-startup foundation, never a current replay checkpoint.
    pub fn foundation_checkpoint(&self) -> &[u8] {
        &self.foundation
    }
    pub fn foundation_head(&self) -> SourceContractReplayHead {
        self.foundation_head
    }
    pub fn origin(&self) -> &NativeMainExpectation {
        &self.origin
    }
    pub fn raw_inputs(&self) -> &NativeStartupInputsV1 {
        &self.inputs
    }
    pub fn raw_commands(&self) -> &[StartupCommandV1] {
        &self.commands
    }
    pub fn next_timer(&self) -> Option<(u64, u64, &'static str)> {
        self.prefix.as_ref()?.next_timer()
    }
    pub fn pending_clients(&self) -> Vec<(&'static str, u64)> {
        self.prefix
            .as_ref()
            .map_or_else(Vec::new, |p| p.pending_clients())
    }
    pub fn stop(&self) -> StartupStopV1 {
        self.prefix.as_ref().map_or_else(
            || {
                StartupStopV1::Local(
                    self.start_error
                        .clone()
                        .unwrap_or(SourceRuleError::InvalidInput),
                )
            },
            |p| p.stop(),
        )
    }
    pub fn apply(&mut self, command: StartupCommandV1) -> Result<StartupStopV1, StartupErrorV1> {
        if self.commands.len() >= 1024 {
            return Err(StartupErrorV1::Capacity);
        }
        let prefix = self.prefix.as_mut().ok_or(StartupErrorV1::AlreadyStopped)?;
        let result = self
            .financial
            .native_main_runtime_mut()
            .native_main_raw_startup_apply(
                &mut self.index,
                &mut self.io,
                prefix,
                command.clone(),
            )?;
        self.commands.push(command);
        Ok(result)
    }
    pub fn financial_state(&self) -> Result<V, SourceRuleError> {
        self.financial.runtime().native_main_state_value()
    }
    pub fn effects(&self) -> &[V] {
        self.financial.runtime().native_main_row_effects()
    }
    pub fn visit_canonical_fields(
        &self,
        visitor: &mut impl FnMut(&str, &V) -> Result<(), SourceRuleError>,
    ) -> Result<(), SourceRuleError> {
        visitor("entry/profile", &V::String(PROFILE.into()))?;
        visitor("entry/generation", &V::String(self.generation.to_string()))?;
        visitor(
            "foundation/commands",
            &V::String(self.foundation_head.commands.to_string()),
        )?;
        visitor(
            "origin/identity",
            &V::String(
                self.index
                    .native_store_context()
                    .map_err(|_| SourceRuleError::Backend)?
                    .0,
            ),
        )?;
        visitor("raw/mainClient", &self.inputs.main_client)?;
        visitor("raw/sdkClientFields", &self.inputs.sdk_client_fields)?;
        self.io.visit_fields(visitor)?;
        if let Some(prefix) = &self.prefix {
            prefix.visit_fields(visitor)?;
        }
        if let Some(error) = &self.start_error {
            visitor("startError", &V::String(format!("{error:?}")))?;
        }
        Ok(())
    }
}
#[cfg(test)]
#[path = "source_main_raw_startup_tests.rs"]
mod tests;

include!("source_main_raw_startup_suffix_impl.rs");

include!("source_main_raw_startup_live_impl.rs");

include!("source_main_startup_nested_impl.rs");
