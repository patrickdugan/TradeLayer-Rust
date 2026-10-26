//! Concrete adapters connecting Core RPC and complete durable bootstrap owners.
use crate::bootstrap::{
    BootstrapClient, BootstrapClientFactory, BootstrapConfig, BootstrapControl, ClaimedNodeIdentity,
};
use crate::chain::{BlockId, Tip};
use crate::client::RpcClient;
use crate::client::{RpcError, RpcValue};
use crate::daemon::{
    Daemon, DaemonError, DaemonStatus, ObservationHead, ObservationStore, StepOutcome,
};
use crate::daemon::{ObservationRpc, ObservationRpcError};
use crate::economic_chain::{
    DurableEconomicChain, DurableEconomicError, EconomicChain, EconomicHead,
};

#[derive(Debug)]
pub struct RpcObservationClient {
    client: RpcClient,
    control: BootstrapControl,
}
impl RpcObservationClient {
    pub fn new(client: RpcClient, control: BootstrapControl) -> Self {
        Self { client, control }
    }
    pub fn into_client(self) -> RpcClient {
        self.client
    }
    fn query(
        &mut self,
        method: &str,
        params: &[RpcValue],
    ) -> Result<RpcValue, ObservationRpcError> {
        self.client
            .call_controlled(method, params, false, &self.control)
            .map_err(observation_error)
    }
}
fn observation_error(error: RpcError) -> ObservationRpcError {
    match error {
        RpcError::Timeout
        | RpcError::Cancelled
        | RpcError::Transport(_)
        | RpcError::Remote { code: -28, .. } => ObservationRpcError::Transient,
        RpcError::WrongChain | RpcError::WrongGenesis => ObservationRpcError::WrongIdentity,
        RpcError::Remote { .. } => ObservationRpcError::Unavailable,
        _ => ObservationRpcError::InvalidResponse,
    }
}
pub fn parse_block_id(text: &str) -> Result<BlockId, ObservationRpcError> {
    if text.len() != 64 || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(ObservationRpcError::InvalidResponse);
    }
    let mut bytes = [0u8; 32];
    for (slot, pair) in bytes.iter_mut().zip(text.as_bytes().chunks_exact(2)) {
        *slot = ((pair[0] as char)
            .to_digit(16)
            .ok_or(ObservationRpcError::InvalidResponse)?
            * 16
            + (pair[1] as char)
                .to_digit(16)
                .ok_or(ObservationRpcError::InvalidResponse)?) as u8;
    }
    Ok(BlockId(bytes))
}
pub fn block_id_text(id: BlockId) -> String {
    id.0.iter().map(|b| format!("{b:02x}")).collect()
}
impl ObservationRpc for RpcObservationClient {
    fn verify_identity(&mut self) -> Result<ClaimedNodeIdentity, ObservationRpcError> {
        self.client
            .verify_network_controlled(&self.control)
            .map_err(observation_error)?;
        let network = self.client.expected_network();
        Ok(ClaimedNodeIdentity {
            network,
            test: false,
            genesis: network.genesis(),
        })
    }
    fn reported_tip(&mut self) -> Result<Tip, ObservationRpcError> {
        let info = self.query("getblockchaininfo", &[])?;
        let get = |name| {
            info.field(name)
                .map_err(observation_error)?
                .ok_or(ObservationRpcError::InvalidResponse)
        };
        let height = get("blocks")?.as_u64().map_err(observation_error)?;
        let hash = get("bestblockhash")?
            .as_string()
            .map_err(observation_error)?;
        Ok(Tip {
            height,
            id: parse_block_id(&hash)?,
        })
    }
    fn block_hash(&mut self, height: u64) -> Result<BlockId, ObservationRpcError> {
        parse_block_id(
            &self
                .query("getblockhash", &[RpcValue::unsigned(height)])?
                .as_string()
                .map_err(observation_error)?,
        )
    }
    fn raw_block(&mut self, hash: BlockId) -> Result<Vec<u8>, ObservationRpcError> {
        let hex = self
            .query(
                "getblock",
                &[
                    RpcValue::string(&block_id_text(hash)).map_err(observation_error)?,
                    RpcValue::unsigned(0),
                ],
            )?
            .as_string()
            .map_err(observation_error)?;
        if hex.len() % 2 != 0
            || hex.len() / 2 > crate::raw_block_ingest::MAX_UNVERIFIED_RAW_BLOCK_BYTES
            || !hex.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(ObservationRpcError::InvalidResponse);
        }
        let mut bytes = Vec::with_capacity(hex.len() / 2);
        for pair in hex.as_bytes().chunks_exact(2) {
            bytes.push(
                (((pair[0] as char)
                    .to_digit(16)
                    .ok_or(ObservationRpcError::InvalidResponse)?
                    << 4)
                    + (pair[1] as char)
                        .to_digit(16)
                        .ok_or(ObservationRpcError::InvalidResponse)?) as u8,
            );
        }
        Ok(bytes)
    }
}

#[derive(Debug)]
pub enum ObservationRuntimeError {
    Economic(DurableEconomicError),
    Observer(DaemonError),
    NetworkMismatch,
    EconomicHeadChanged,
}
impl core::fmt::Display for ObservationRuntimeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for ObservationRuntimeError {}
/// Owns the already restored economic journal without exposing a mutation path
/// from RPC observations. The complete head is captured once; every step/query
/// checks journal health and tip without rehashing the unchanged checkpoint.
pub struct ObservedEconomicRuntime<R> {
    observer: Daemon<R>,
    economic: DurableEconomicChain,
    expected: EconomicHead,
}
impl<R: ObservationRpc> ObservedEconomicRuntime<R> {
    pub fn new(
        source: R,
        store: ObservationStore,
        economic: DurableEconomicChain,
    ) -> Result<Self, ObservationRuntimeError> {
        let expected = economic.head().map_err(ObservationRuntimeError::Economic)?;
        if store
            .config()
            .map_err(ObservationRuntimeError::Observer)?
            .network
            != economic
                .chain()
                .map_err(ObservationRuntimeError::Economic)?
                .config()
                .economics
                .network
        {
            return Err(ObservationRuntimeError::NetworkMismatch);
        }
        Ok(Self {
            observer: Daemon::new(source, store),
            economic,
            expected,
        })
    }
    fn check(&self) -> Result<(), ObservationRuntimeError> {
        if self
            .economic
            .chain()
            .map_err(ObservationRuntimeError::Economic)?
            .tip()
            != self.expected.tip
        {
            return Err(ObservationRuntimeError::EconomicHeadChanged);
        }
        Ok(())
    }
    pub fn step(
        &mut self,
        now_ms: u64,
        control: &BootstrapControl,
    ) -> Result<StepOutcome, ObservationRuntimeError> {
        self.check()?;
        let result = self
            .observer
            .step(now_ms, control)
            .map_err(ObservationRuntimeError::Observer)?;
        self.check()?;
        Ok(result)
    }
    pub fn status(&self) -> Result<DaemonStatus, ObservationRuntimeError> {
        self.check()?;
        self.observer
            .status()
            .map_err(ObservationRuntimeError::Observer)
    }
    pub fn observation_head(&self) -> Result<ObservationHead, ObservationRuntimeError> {
        self.check()?;
        self.observer
            .head()
            .map_err(ObservationRuntimeError::Observer)
    }
    pub fn economic_head(&self) -> Result<EconomicHead, ObservationRuntimeError> {
        self.check()?;
        Ok(self.expected)
    }
    pub fn economic_chain(&self) -> Result<&EconomicChain, ObservationRuntimeError> {
        self.check()?;
        self.economic
            .chain()
            .map_err(ObservationRuntimeError::Economic)
    }
    pub fn observation_store(&self) -> Result<&ObservationStore, ObservationRuntimeError> {
        self.check()?;
        self.observer
            .store()
            .head()
            .map_err(ObservationRuntimeError::Observer)?;
        Ok(self.observer.store())
    }
    pub fn into_parts(self) -> (Daemon<R>, DurableEconomicChain) {
        (self.observer, self.economic)
    }
}

impl BootstrapClient for RpcClient {
    fn identity(&mut self, control: &BootstrapControl) -> Result<ClaimedNodeIdentity, String> {
        self.verify_network_controlled(control)
            .map_err(|e| e.to_string())?;
        control.check().map_err(|e| format!("{e:?}"))?;
        let network = self.expected_network();
        Ok(ClaimedNodeIdentity {
            network,
            test: false,
            genesis: network.genesis(),
        })
    }
}

/// Moves the configured client into one startup attempt; this is not a global
/// singleton. Failure drops the acquired client along with partial resources.
#[derive(Debug)]
pub struct RpcClientFactory {
    client: Option<RpcClient>,
}
impl RpcClientFactory {
    pub fn new(client: RpcClient) -> Self {
        Self {
            client: Some(client),
        }
    }
}
impl BootstrapClientFactory for RpcClientFactory {
    type Client = RpcClient;
    fn acquire(
        &mut self,
        config: &BootstrapConfig,
        control: &BootstrapControl,
    ) -> Result<RpcClient, String> {
        control.check().map_err(|e| format!("{e:?}"))?;
        let client = self.client.as_ref().ok_or("RPC client already acquired")?;
        if client.expected_network() != config.network() {
            return Err("RPC/bootstrap network mismatch".into());
        }
        self.client
            .take()
            .ok_or("RPC client already acquired".into())
    }
}
