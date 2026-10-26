//! Restoration-only bootstrap adapters for the complete durable state owners.
//! No checkpoint initialization, fallback genesis, or authentication is implied.

use crate::bootstrap::{
    BootstrapConfig, BootstrapControl, BootstrapStore, BootstrapStoreFactory, RestoredState,
    StopReason,
};
use crate::chain::Tip;
use crate::durable_iou::{DurableFundedIou, DurableIouError, DurableIouHead};
use crate::economic_chain::{
    DurableEconomicChain, DurableEconomicError, EconomicChainConfig, EconomicHead,
};
use crate::iou_ledger::PayoutAtomPolicy;
use crate::network_profile::TradeLayerMainnet;
use crate::persistence::{PersistenceConfig, PersistenceError, PersistentJointChain};
use crate::storage::{Namespace, Recovery, RecoveryReport, StorageError};

#[derive(Debug)]
pub enum RestoreAdapterError {
    ScopeMismatch,
    AlreadyAttempted,
    Stopped(StopReason),
    Namespace(StorageError),
    FundedIou(DurableIouError),
    Joint(PersistenceError),
    Economic(DurableEconomicError),
}
impl core::fmt::Display for RestoreAdapterError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for RestoreAdapterError {}

fn scope(
    config: &BootstrapConfig,
    network: TradeLayerMainnet,
    category: &str,
) -> Result<(), RestoreAdapterError> {
    let expected =
        Namespace::for_mainnet(network, category).map_err(RestoreAdapterError::Namespace)?;
    if config.network() != network || config.namespace() != &expected {
        return Err(RestoreAdapterError::ScopeMismatch);
    }
    Ok(())
}
fn check(control: &BootstrapControl) -> Result<(), RestoreAdapterError> {
    control.check().map_err(RestoreAdapterError::Stopped)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FundedIouRestoreConfig {
    pub network: TradeLayerMainnet,
    pub atom_policy: PayoutAtomPolicy,
    /// Mandatory exact comparison token, obtained independently of this load.
    pub expected_head: DurableIouHead,
    pub recovery: Recovery,
}

#[derive(Debug, Clone)]
pub struct FundedIouStoreFactory {
    expected: FundedIouRestoreConfig,
}
impl FundedIouStoreFactory {
    pub const fn new(expected: FundedIouRestoreConfig) -> Self {
        Self { expected }
    }
    pub const fn expected(&self) -> &FundedIouRestoreConfig {
        &self.expected
    }
    /// Preparation validates scope and does not open a file or create a path.
    /// The one-shot store performs complete owner restoration in restore.
    pub fn open_checked(
        &self,
        config: &BootstrapConfig,
        control: &BootstrapControl,
    ) -> Result<FundedIouRestoreStore, RestoreAdapterError> {
        check(control)?;
        scope(config, self.expected.network, "iou")?;
        Ok(FundedIouRestoreStore {
            bound: config.clone(),
            expected: self.expected,
            attempted: false,
            recovery_report: None,
        })
    }
}
impl BootstrapStoreFactory for FundedIouStoreFactory {
    type Store = FundedIouRestoreStore;
    fn open(
        &mut self,
        config: &BootstrapConfig,
        control: &BootstrapControl,
    ) -> Result<Self::Store, String> {
        self.open_checked(config, control)
            .map_err(|error| error.to_string())
    }
}

#[derive(Debug)]
pub struct FundedIouRestoreStore {
    bound: BootstrapConfig,
    expected: FundedIouRestoreConfig,
    attempted: bool,
    recovery_report: Option<RecoveryReport>,
}
impl FundedIouRestoreStore {
    pub const fn recovery_report(&self) -> Option<RecoveryReport> {
        self.recovery_report
    }
    pub fn restore_checked(
        &mut self,
        config: &BootstrapConfig,
        control: &BootstrapControl,
    ) -> Result<RestoredState<DurableFundedIou>, RestoreAdapterError> {
        check(control)?;
        scope(config, self.expected.network, "iou")?;
        if config != &self.bound {
            return Err(RestoreAdapterError::ScopeMismatch);
        }
        if self.attempted {
            return Err(RestoreAdapterError::AlreadyAttempted);
        }
        self.attempted = true;
        let restored = DurableFundedIou::open_existing(
            config.root(),
            self.expected.network,
            self.expected.atom_policy,
            self.expected.recovery,
            Some(self.expected.expected_head),
        );
        check(control)?;
        let (owner, report) = restored.map_err(RestoreAdapterError::FundedIou)?;
        self.recovery_report = Some(report);
        check(control)?;
        let network = owner
            .state()
            .map_err(RestoreAdapterError::FundedIou)?
            .network();
        check(control)?;
        Ok(RestoredState {
            state: owner,
            network,
            genesis: network.genesis(),
        })
    }
}
impl BootstrapStore for FundedIouRestoreStore {
    type State = DurableFundedIou;
    fn restore(
        &mut self,
        config: &BootstrapConfig,
        control: &BootstrapControl,
    ) -> Result<RestoredState<Self::State>, String> {
        self.restore_checked(config, control)
            .map_err(|error| error.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JointRestoreConfig {
    /// Independent expected anchor/admin/permitted set/retention and network.
    pub persistence: PersistenceConfig,
    pub expected_tip: Tip,
    pub recovery: Recovery,
}

#[derive(Debug, Clone)]
pub struct JointStoreFactory {
    expected: JointRestoreConfig,
}
impl JointStoreFactory {
    pub fn new(expected: JointRestoreConfig) -> Self {
        Self { expected }
    }
    pub fn expected(&self) -> &JointRestoreConfig {
        &self.expected
    }
    pub fn open_checked(
        &self,
        config: &BootstrapConfig,
        control: &BootstrapControl,
    ) -> Result<JointRestoreStore, RestoreAdapterError> {
        check(control)?;
        scope(config, self.expected.persistence.network, "persistence")?;
        Ok(JointRestoreStore {
            bound: config.clone(),
            expected: self.expected.clone(),
            attempted: false,
            recovery_report: None,
        })
    }
}
impl BootstrapStoreFactory for JointStoreFactory {
    type Store = JointRestoreStore;
    fn open(
        &mut self,
        config: &BootstrapConfig,
        control: &BootstrapControl,
    ) -> Result<Self::Store, String> {
        self.open_checked(config, control)
            .map_err(|error| error.to_string())
    }
}

#[derive(Debug)]
pub struct JointRestoreStore {
    bound: BootstrapConfig,
    expected: JointRestoreConfig,
    attempted: bool,
    recovery_report: Option<RecoveryReport>,
}
impl JointRestoreStore {
    pub const fn recovery_report(&self) -> Option<RecoveryReport> {
        self.recovery_report
    }
    pub fn restore_checked(
        &mut self,
        config: &BootstrapConfig,
        control: &BootstrapControl,
    ) -> Result<RestoredState<PersistentJointChain>, RestoreAdapterError> {
        check(control)?;
        scope(config, self.expected.persistence.network, "persistence")?;
        if config != &self.bound {
            return Err(RestoreAdapterError::ScopeMismatch);
        }
        if self.attempted {
            return Err(RestoreAdapterError::AlreadyAttempted);
        }
        self.attempted = true;
        let restored = PersistentJointChain::load(
            config.root(),
            self.expected.persistence.clone(),
            self.expected.expected_tip,
            self.expected.recovery,
        );
        check(control)?;
        let (owner, report) = restored.map_err(RestoreAdapterError::Joint)?;
        self.recovery_report = Some(report);
        check(control)?;
        let network = owner.state().map_err(RestoreAdapterError::Joint)?.network();
        check(control)?;
        Ok(RestoredState {
            state: owner,
            network,
            genesis: network.genesis(),
        })
    }
}
impl BootstrapStore for JointRestoreStore {
    type State = PersistentJointChain;
    fn restore(
        &mut self,
        config: &BootstrapConfig,
        control: &BootstrapControl,
    ) -> Result<RestoredState<Self::State>, String> {
        self.restore_checked(config, control)
            .map_err(|error| error.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EconomicRestoreConfig {
    pub chain: EconomicChainConfig,
    pub expected_head: EconomicHead,
    pub recovery: Recovery,
}
#[derive(Debug, Clone)]
pub struct EconomicStoreFactory {
    expected: EconomicRestoreConfig,
}
impl EconomicStoreFactory {
    pub fn new(expected: EconomicRestoreConfig) -> Self {
        Self { expected }
    }
    pub fn expected(&self) -> &EconomicRestoreConfig {
        &self.expected
    }
    pub fn open_checked(
        &self,
        config: &BootstrapConfig,
        control: &BootstrapControl,
    ) -> Result<EconomicRestoreStore, RestoreAdapterError> {
        check(control)?;
        scope(config, self.expected.chain.economics.network, "clearing")?;
        self.expected
            .chain
            .validate()
            .map_err(|e| RestoreAdapterError::Economic(DurableEconomicError::Economic(e)))?;
        Ok(EconomicRestoreStore {
            bound: config.clone(),
            expected: self.expected.clone(),
            attempted: false,
            recovery_report: None,
        })
    }
}
impl BootstrapStoreFactory for EconomicStoreFactory {
    type Store = EconomicRestoreStore;
    fn open(
        &mut self,
        config: &BootstrapConfig,
        control: &BootstrapControl,
    ) -> Result<Self::Store, String> {
        self.open_checked(config, control)
            .map_err(|error| error.to_string())
    }
}
/// Restoration configuration/report only; the returned runtime retains the sole
/// durable owner and its OS lock. No Ledger or chain copy remains in this store.
#[derive(Debug)]
pub struct EconomicRestoreStore {
    bound: BootstrapConfig,
    expected: EconomicRestoreConfig,
    attempted: bool,
    recovery_report: Option<RecoveryReport>,
}
impl EconomicRestoreStore {
    pub fn recovery_report(&self) -> Option<RecoveryReport> {
        self.recovery_report
    }
    pub fn restore_checked(
        &mut self,
        config: &BootstrapConfig,
        control: &BootstrapControl,
    ) -> Result<RestoredState<DurableEconomicChain>, RestoreAdapterError> {
        check(control)?;
        scope(config, self.expected.chain.economics.network, "clearing")?;
        if config != &self.bound {
            return Err(RestoreAdapterError::ScopeMismatch);
        }
        if self.attempted {
            return Err(RestoreAdapterError::AlreadyAttempted);
        }
        self.attempted = true;
        let restored = DurableEconomicChain::load(
            config.root(),
            self.expected.chain.clone(),
            self.expected.expected_head,
            self.expected.recovery,
        );
        check(control)?;
        let (owner, report) = restored.map_err(RestoreAdapterError::Economic)?;
        self.recovery_report = Some(report);
        let network = owner
            .chain()
            .map_err(RestoreAdapterError::Economic)?
            .config()
            .economics
            .network;
        check(control)?;
        Ok(RestoredState {
            state: owner,
            network,
            genesis: network.genesis(),
        })
    }
}
impl BootstrapStore for EconomicRestoreStore {
    type State = DurableEconomicChain;
    fn restore(
        &mut self,
        config: &BootstrapConfig,
        control: &BootstrapControl,
    ) -> Result<RestoredState<Self::State>, String> {
        self.restore_checked(config, control)
            .map_err(|error| error.to_string())
    }
}
