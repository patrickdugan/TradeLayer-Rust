//! Source startup compatibility and an explicitly repaired local coordinator.
//!
//! A ready local runtime owns its client and complete restored state. Readiness
//! checks neither authenticate a blockchain nor manufacture a trusted adapter.

use crate::chain::BlockId;
use crate::network_profile::TradeLayerMainnet;
use crate::storage::Namespace;
use core::future::Future;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Instant;

pub const SOURCE_COMMIT: &str = "dca0888b827c5bc28fa9189bc9e1d507f5c5670c";
pub const SOURCE_LF_SHA256: &str =
    "55c7dd6da798a58dc57d34ad75be357f9811574db5e6f384620055f966447cc0";
pub const LEGACY_CLIENT_POLL_MS: u64 = 150;
pub const LEGACY_IGNORED_TIMEOUT_MS: u64 = 5000;
pub const LEGACY_POST_CLIENT_DELAY_MS: u64 = 500;

/// Source passes undefined and null through to Database.init distinctly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LegacyChainValue {
    Undefined,
    Null,
    Name(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyClient<C> {
    pub handle: C,
    pub chain: LegacyChainValue,
}

/// Literal source behavior: the captured client cannot change, timeout is
/// ignored, and client.chain is never examined. A false value waits forever
/// unless the caller's delay fails or the entire future is dropped.
pub async fn legacy_wait_for_client_chain<F, W>(
    client_present: bool,
    _timeout_ms: u64,
    interval_ms: u64,
    mut delay: W,
) -> Result<(), String>
where
    W: FnMut(u64) -> F,
    F: Future<Output = Result<(), String>>,
{
    if client_present {
        return Ok(());
    }
    loop {
        delay(interval_ms).await?;
    }
}

/// Literal exported initialize ordering. A present client proceeds even when
/// its chain is undefined. This API intentionally retains the source's wait
/// defect; use BootstrapCoordinator for cancellable functional startup.
pub async fn legacy_initialize<C, D, G, GF, W, WF, I, IF>(
    get_client: G,
    mut delay: W,
    initialize_database: I,
) -> Result<(C, D), String>
where
    G: FnOnce() -> GF,
    GF: Future<Output = Result<Option<LegacyClient<C>>, String>>,
    W: FnMut(u64) -> WF,
    WF: Future<Output = Result<(), String>>,
    I: FnOnce(LegacyChainValue) -> IF,
    IF: Future<Output = Result<D, String>>,
{
    let client = get_client().await?;
    legacy_wait_for_client_chain(
        client.is_some(),
        LEGACY_IGNORED_TIMEOUT_MS,
        LEGACY_CLIENT_POLL_MS,
        &mut delay,
    )
    .await?;
    // A missing client can only produce Err or remain pending in the helper.
    let client = client.expect("literal false-client wait never returns success");
    delay(LEGACY_POST_CLIENT_DELAY_MS).await?;
    let database = initialize_database(client.chain).await?;
    Ok((client.handle, database))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigError {
    RootMustBeAbsolute,
    RootMustExist,
    UnknownCategory,
}

impl core::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for ConfigError {}

/// Explicit mainnet selection. No environment fallback, discovery heuristic,
/// implicit directory creation, or unsupported testnet profile is introduced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootstrapConfig {
    network: TradeLayerMainnet,
    root: PathBuf,
    namespace: Namespace,
}

impl BootstrapConfig {
    pub fn new(
        network: TradeLayerMainnet,
        root: PathBuf,
        category: &str,
    ) -> Result<Self, ConfigError> {
        if !root.is_absolute() {
            return Err(ConfigError::RootMustBeAbsolute);
        }
        if !root.is_dir() {
            return Err(ConfigError::RootMustExist);
        }
        let namespace =
            Namespace::for_mainnet(network, category).map_err(|_| ConfigError::UnknownCategory)?;
        Ok(Self {
            network,
            root,
            namespace,
        })
    }
    pub const fn network(&self) -> TradeLayerMainnet {
        self.network
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn namespace(&self) -> &Namespace {
        &self.namespace
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    Cancelled,
    DeadlineExceeded,
}

/// Cancellation is shared across clones. Blocking adapters must check this
/// control inside their own I/O loops; the coordinator cannot interrupt a
/// synchronous system call. A deadline uses the monotonic Instant clock.
#[derive(Debug, Clone)]
pub struct BootstrapControl {
    cancelled: Arc<AtomicBool>,
    deadline: Option<Instant>,
}

impl BootstrapControl {
    pub fn new(deadline: Option<Instant>) -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            deadline,
        }
    }
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
    pub const fn deadline(&self) -> Option<Instant> {
        self.deadline
    }
    pub fn check(&self) -> Result<(), StopReason> {
        if self.cancelled.load(Ordering::Acquire) {
            return Err(StopReason::Cancelled);
        }
        if self
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            return Err(StopReason::DeadlineExceeded);
        }
        Ok(())
    }
}
impl Default for BootstrapControl {
    fn default() -> Self {
        Self::new(None)
    }
}

/// Metadata reported by a caller-owned client; a genesis match is an identity
/// consistency check, not header consensus, UTXO validation or authentication.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClaimedNodeIdentity {
    pub network: TradeLayerMainnet,
    pub test: bool,
    pub genesis: BlockId,
}

pub trait BootstrapClient {
    fn identity(&mut self, control: &BootstrapControl) -> Result<ClaimedNodeIdentity, String>;
}

/// Returned handles must release acquired resources in Drop. An adapter that
/// fails before returning a handle must release its own partial acquisitions.
pub trait BootstrapClientFactory {
    type Client: BootstrapClient;
    fn acquire(
        &mut self,
        config: &BootstrapConfig,
        control: &BootstrapControl,
    ) -> Result<Self::Client, String>;
}

/// State should be the complete health-gated durable owner, not a cloned raw
/// ledger detached from its journal and uncertain-write/poison state.
#[derive(Debug)]
pub struct RestoredState<S> {
    pub state: S,
    pub network: TradeLayerMainnet,
    pub genesis: BlockId,
}

pub trait BootstrapStore {
    type State;
    /// Implementations must restore one internally coherent checkpoint using
    /// independently configured anchor/tip/policy, validate its invariants and
    /// retain the sole durable state owner. This interface supplies no policy
    /// that could replace those checks with caller metadata.
    fn restore(
        &mut self,
        config: &BootstrapConfig,
        control: &BootstrapControl,
    ) -> Result<RestoredState<Self::State>, String>;
}

pub trait BootstrapStoreFactory {
    type Store: BootstrapStore;
    fn open(
        &mut self,
        config: &BootstrapConfig,
        control: &BootstrapControl,
    ) -> Result<Self::Store, String>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootstrapStage {
    AcquireClient,
    IdentifyNetwork,
    OpenStore,
    RestoreState,
    PublishReady,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootstrapStatus {
    Idle,
    Starting(BootstrapStage),
    Failed(BootstrapStage),
    Ready,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BootstrapError {
    AlreadyStarted,
    ResetRequiresFailure,
    Stopped {
        stage: BootstrapStage,
        reason: StopReason,
    },
    Adapter {
        stage: BootstrapStage,
        message: String,
    },
    NodeIdentityMismatch,
    StateIdentityMismatch,
}
impl core::fmt::Display for BootstrapError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for BootstrapError {}

/// All handles are private until successful return. The coordinator retains no
/// copied economic state. Accessing an owned durable state's methods continues
/// to enforce that owner's health/recovery policy after startup.
#[derive(Debug)]
pub struct ReadyLocalRuntime<C, D, S> {
    config: BootstrapConfig,
    node: ClaimedNodeIdentity,
    state: S,
    store: D,
    client: C,
}

pub type StartedRuntime<C, D> = ReadyLocalRuntime<C, D, <D as BootstrapStore>::State>;
impl<C, D, S> ReadyLocalRuntime<C, D, S> {
    pub fn config(&self) -> &BootstrapConfig {
        &self.config
    }
    pub const fn claimed_node_identity(&self) -> ClaimedNodeIdentity {
        self.node
    }
    pub fn client_mut(&mut self) -> &mut C {
        &mut self.client
    }
    pub fn store_mut(&mut self) -> &mut D {
        &mut self.store
    }
    pub fn state(&self) -> &S {
        &self.state
    }
    pub fn state_mut(&mut self) -> &mut S {
        &mut self.state
    }
    pub fn into_parts(self) -> (C, D, S) {
        (self.client, self.store, self.state)
    }
}

/// One explicit startup owner. `&mut self` serializes attempts in safe Rust;
/// storage adapters retain their own OS locks for cross-owner exclusion.
#[derive(Debug)]
pub struct BootstrapCoordinator {
    status: BootstrapStatus,
}
impl Default for BootstrapCoordinator {
    fn default() -> Self {
        Self {
            status: BootstrapStatus::Idle,
        }
    }
}

impl BootstrapCoordinator {
    pub const fn status(&self) -> BootstrapStatus {
        self.status
    }

    /// A failed attempt requires explicit reset; this does not reverse disk
    /// recovery or guarantee that an adapter's failed write was absent.
    pub fn reset_after_failure(&mut self) -> Result<(), BootstrapError> {
        if !matches!(self.status, BootstrapStatus::Failed(_)) {
            return Err(BootstrapError::ResetRequiresFailure);
        }
        self.status = BootstrapStatus::Idle;
        Ok(())
    }

    pub fn start<C: BootstrapClientFactory, D: BootstrapStoreFactory>(
        &mut self,
        config: BootstrapConfig,
        clients: &mut C,
        stores: &mut D,
        control: &BootstrapControl,
    ) -> Result<StartedRuntime<C::Client, D::Store>, BootstrapError> {
        if self.status != BootstrapStatus::Idle {
            return Err(BootstrapError::AlreadyStarted);
        }
        let mut attempt = Attempt::new(&mut self.status);
        attempt.check(control)?;
        let mut client = clients
            .acquire(&config, control)
            .map_err(|message| attempt.adapter_error(message, control))?;
        attempt.check(control)?;
        attempt.stage(BootstrapStage::IdentifyNetwork);
        let node = client
            .identity(control)
            .map_err(|message| attempt.adapter_error(message, control))?;
        attempt.check(control)?;
        if node.network != config.network || node.test || node.genesis != config.network.genesis() {
            return Err(BootstrapError::NodeIdentityMismatch);
        }
        attempt.stage(BootstrapStage::OpenStore);
        attempt.check(control)?;
        let mut store = stores
            .open(&config, control)
            .map_err(|message| attempt.adapter_error(message, control))?;
        attempt.check(control)?;
        attempt.stage(BootstrapStage::RestoreState);
        let restored = store
            .restore(&config, control)
            .map_err(|message| attempt.adapter_error(message, control))?;
        attempt.check(control)?;
        if restored.network != config.network || restored.genesis != config.network.genesis() {
            return Err(BootstrapError::StateIdentityMismatch);
        }
        attempt.stage(BootstrapStage::PublishReady);
        // This final check is the publication linearization point. A cancel
        // arriving afterward does not retroactively revoke a returned owner.
        attempt.check(control)?;
        let ready = ReadyLocalRuntime {
            config,
            node,
            client,
            store,
            state: restored.state,
        };
        attempt.ready();
        Ok(ready)
    }
}

/// RAII marks the attempt failed even when an adapter unwinds. Handles remain
/// local and are dropped during ordinary error propagation or panic unwinding.
struct Attempt<'a> {
    status: &'a mut BootstrapStatus,
    stage: BootstrapStage,
    ready: bool,
}
impl<'a> Attempt<'a> {
    fn new(status: &'a mut BootstrapStatus) -> Self {
        let stage = BootstrapStage::AcquireClient;
        *status = BootstrapStatus::Starting(stage);
        Self {
            status,
            stage,
            ready: false,
        }
    }
    fn stage(&mut self, stage: BootstrapStage) {
        self.stage = stage;
        *self.status = BootstrapStatus::Starting(stage);
    }
    fn check(&self, control: &BootstrapControl) -> Result<(), BootstrapError> {
        control.check().map_err(|reason| BootstrapError::Stopped {
            stage: self.stage,
            reason,
        })
    }
    fn adapter_error(&self, message: String, control: &BootstrapControl) -> BootstrapError {
        if let Err(reason) = control.check() {
            return BootstrapError::Stopped {
                stage: self.stage,
                reason,
            };
        }
        BootstrapError::Adapter {
            stage: self.stage,
            message,
        }
    }
    fn ready(&mut self) {
        *self.status = BootstrapStatus::Ready;
        self.ready = true;
    }
}
impl Drop for Attempt<'_> {
    fn drop(&mut self) {
        if !self.ready {
            *self.status = BootstrapStatus::Failed(self.stage);
        }
    }
}
