//! Moved-owner/preflight foundation. This does not yet claim Runtime V13
//! dispatch, durable replay or automatic Main collection binding.
use crate::source_compact_index_v2::{MetadataSnapshot, SourceCompactIndex};
use crate::source_contract_replay::{SourceContractReplay, SourceContractReplayHead};
use crate::source_contract_runtime::SourceContractRuntimePolicy;
use crate::storage::Namespace;
use crate::validity_source::SourceValue as V;

pub const OWNER_PROFILE: &str = "TL-NATIVE-MAIN-MOVED-OWNERS-FOUNDATION-1";
pub const STORE_BINDING_PROFILE: &str = "TL-NATIVE-MAIN-TXINDEX-V13-1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeMainExpectation {
    pub financial_head: SourceContractReplayHead,
    pub namespace: Namespace,
    pub metadata: MetadataSnapshot,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NativeMainOwnerError {
    WrongFinancialHead,
    WrongNamespace,
    WrongMetadataSnapshot,
    CoreIndexMirrorPresent,
    UnsupportedFoundationPolicy,
    MissingOrMismatchedMainClient,
    Store(String),
}
pub struct RejectedNativeMainOwners {
    pub error: NativeMainOwnerError,
    financial: SourceContractReplay,
    index: SourceCompactIndex,
}
impl core::fmt::Debug for RejectedNativeMainOwners {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RejectedNativeMainOwners")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}
impl RejectedNativeMainOwners {
    pub fn into_owners(self) -> (SourceContractReplay, SourceCompactIndex) {
        (self.financial, self.index)
    }
}
/// Owns both actual owners; there is no Clone and no live Journal in Core.
pub struct NativeMainOwners {
    financial: SourceContractReplay,
    index: SourceCompactIndex,
}
impl core::fmt::Debug for NativeMainOwners {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("NativeMainOwners")
            .field("financial_head", &self.financial.head())
            .field("metadata", &self.index.snapshot())
            .finish()
    }
}
impl NativeMainOwners {
    pub fn adopt(
        financial: SourceContractReplay,
        index: SourceCompactIndex,
        expected: &NativeMainExpectation,
    ) -> Result<Self, Box<RejectedNativeMainOwners>> {
        if let Err(error) = Self::preflight(&financial, &index, expected) {
            return Err(Box::new(RejectedNativeMainOwners {
                error,
                financial,
                index,
            }));
        }
        Ok(Self { financial, index })
    }
    fn preflight(
        financial: &SourceContractReplay,
        index: &SourceCompactIndex,
        expected: &NativeMainExpectation,
    ) -> Result<(), NativeMainOwnerError> {
        if financial.head() != expected.financial_head {
            return Err(NativeMainOwnerError::WrongFinancialHead);
        }
        if financial.runtime().policy() != SourceContractRuntimePolicy::MainOrchestrationV12 {
            return Err(NativeMainOwnerError::UnsupportedFoundationPolicy);
        }
        if !financial.runtime().collection("txIndex").is_empty() {
            return Err(NativeMainOwnerError::CoreIndexMirrorPresent);
        }
        if index.native_namespace() != &expected.namespace {
            return Err(NativeMainOwnerError::WrongNamespace);
        }
        let actual = index
            .snapshot()
            .map_err(|e| NativeMainOwnerError::Store(format!("{e:?}")))?;
        if actual != expected.metadata {
            return Err(NativeMainOwnerError::WrongMetadataSnapshot);
        }
        let state = financial
            .runtime()
            .state_value()
            .map_err(|e| NativeMainOwnerError::Store(e.to_string()))?;
        let client = state.get("mainProduction").get("client");
        let (V::String(chain), V::Bool(test)) = (client.get("chain"), client.get("test")) else {
            return Err(NativeMainOwnerError::MissingOrMismatchedMainClient);
        };
        let detected = Namespace::new(&chain, test, "txIndex")
            .map_err(|_| NativeMainOwnerError::MissingOrMismatchedMainClient)?;
        if detected != expected.namespace {
            return Err(NativeMainOwnerError::MissingOrMismatchedMainClient);
        }
        Ok(())
    }
    pub fn financial(&self) -> &SourceContractReplay {
        &self.financial
    }
    pub fn index(&self) -> &SourceCompactIndex {
        &self.index
    }
    pub fn expectation(&self) -> Result<NativeMainExpectation, NativeMainOwnerError> {
        Ok(NativeMainExpectation {
            financial_head: self.financial.head(),
            namespace: self.index.native_namespace().clone(),
            metadata: self
                .index
                .snapshot()
                .map_err(|e| NativeMainOwnerError::Store(e.to_string()))?,
        })
    }
    /// Future V13 Runtime integration consumes this unique scoped loan. The
    /// callback is crate-private so this foundation is not a public fake bridge.
    pub(crate) fn loan(
        &mut self,
        expected: &NativeMainExpectation,
    ) -> Result<(&mut SourceContractReplay, &mut SourceCompactIndex), NativeMainOwnerError> {
        Self::preflight(&self.financial, &self.index, expected)?;
        Ok((&mut self.financial, &mut self.index))
    }
    pub fn into_owners(self) -> (SourceContractReplay, SourceCompactIndex) {
        (self.financial, self.index)
    }
}
