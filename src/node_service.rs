//! Loopback read service that owns the node's durable economic chain.
//!
//! It serves the same legacy listener routes as `service_runtime::
//! EconomicHttpService`, with two differences the node needs. The owner can be
//! lent out for one block apply and put back without closing the listening
//! socket (`with_owner`), and `runtime.status` reports the node's admission
//! state instead of the fixed `authenticated_admission:false` of a restored
//! store. `endpoint = None` keeps the owner without a listener, for tests.

use crate::bootstrap::BootstrapControl;
use crate::chain::{BlockId, Tip};
use crate::client::RpcValue;
use crate::economic_chain::{DurableEconomicChain, EconomicChain, EconomicHead};
use crate::http_service::{HttpServiceServer, ServeOutcome};
use crate::node_ingest::ADMISSION_SCOPE;
use crate::runtime::block_id_text;
use crate::service::{ServiceBackend, ServiceCall, ServiceError};
use crate::service_view::EconomicReadBackend;
use std::net::SocketAddr;
use std::time::Duration;

type Result<T> = core::result::Result<T, ServiceError>;

/// What `runtime.status` says about the node, beyond the economic state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeStatus {
    /// True when the economic tip is a block this process admitted through
    /// `admission::admit_block`. A store restored without an admitted block
    /// makes no admission claim.
    pub authenticated_admission: bool,
    /// Blocks admitted and applied by this process.
    pub admitted_blocks: u64,
    pub authenticated_candidates: u64,
    pub refused_candidates: u64,
    pub unrepresentable_candidates: u64,
    pub applied_candidates: u64,
    pub rolled_back_blocks: u64,
    /// The observed tip economic height 0 stands for.
    pub height_base: Tip,
    /// The observer's selected tip, when known.
    pub observed_tip: Option<Tip>,
}
impl NodeStatus {
    pub const fn new(height_base: Tip) -> Self {
        Self {
            authenticated_admission: false,
            admitted_blocks: 0,
            authenticated_candidates: 0,
            refused_candidates: 0,
            unrepresentable_candidates: 0,
            applied_candidates: 0,
            rolled_back_blocks: 0,
            height_base,
            observed_tip: None,
        }
    }
    fn tip_text(tip: Tip) -> String {
        format!("{}:{}", tip.height, block_id_text(tip.id))
    }
    /// The JSON members, without braces.
    pub fn members(&self) -> String {
        let observed = self.observed_tip.map_or_else(
            || "null".to_owned(),
            |t| format!("\"{}\"", Self::tip_text(t)),
        );
        format!(
            "\"authenticated_admission\":{},\"admission_scope\":\"{ADMISSION_SCOPE}\",\"admitted_blocks\":{},\"authenticated_candidates\":{},\"refused_candidates\":{},\"unrepresentable_candidates\":{},\"applied_candidates\":{},\"rolled_back_blocks\":{},\"height_base\":\"{}\",\"observed_tip\":{observed}",
            self.authenticated_admission,
            self.admitted_blocks,
            self.authenticated_candidates,
            self.refused_candidates,
            self.unrepresentable_candidates,
            self.applied_candidates,
            self.rolled_back_blocks,
            Self::tip_text(self.height_base),
        )
    }
}

pub struct NodeHttpService {
    server: Option<HttpServiceServer>,
    owner: DurableEconomicChain,
    expected: EconomicHead,
    status: NodeStatus,
}
impl NodeHttpService {
    pub fn new(
        owner: DurableEconomicChain,
        endpoint: Option<SocketAddr>,
        timeout: Duration,
        status: NodeStatus,
    ) -> Result<Self> {
        EconomicReadBackend::from_durable(&owner)?;
        let expected = owner
            .head()
            .map_err(|_| ServiceError::Backend("durable_health"))?;
        let server = endpoint
            .map(|endpoint| HttpServiceServer::bind(endpoint, timeout))
            .transpose()?;
        Ok(Self {
            server,
            owner,
            expected,
            status,
        })
    }
    fn check(&self) -> Result<()> {
        if self
            .owner
            .chain()
            .map_err(|_| ServiceError::Backend("durable_health"))?
            .tip()
            != self.expected.tip
        {
            return Err(ServiceError::Backend("economic_head_changed"));
        }
        Ok(())
    }
    pub fn local_addr(&self) -> Result<Option<SocketAddr>> {
        self.server
            .as_ref()
            .map(HttpServiceServer::local_addr)
            .transpose()
    }
    pub fn economic_head(&self) -> Result<EconomicHead> {
        self.check()?;
        Ok(self.expected)
    }
    pub fn economic_chain(&self) -> Result<&EconomicChain> {
        self.check()?;
        self.owner
            .chain()
            .map_err(|_| ServiceError::Backend("durable_health"))
    }
    pub fn status(&self) -> NodeStatus {
        self.status
    }
    pub fn set_status(&mut self, status: NodeStatus) {
        self.status = status;
    }
    /// Lends the owner to `change`, then records the head it leaves behind.
    /// The listening socket stays open. A closure that leaves the owner
    /// unhealthy makes the next read fail with `durable_health`.
    pub fn with_owner<T>(
        &mut self,
        change: impl FnOnce(&mut DurableEconomicChain) -> T,
    ) -> Result<T> {
        self.check()?;
        let out = change(&mut self.owner);
        self.expected = self
            .owner
            .head()
            .map_err(|_| ServiceError::Backend("durable_health"))?;
        Ok(out)
    }
    pub fn serve_one(&self, control: &BootstrapControl) -> Result<ServeOutcome> {
        let Some(server) = &self.server else {
            return Ok(ServeOutcome::Idle);
        };
        self.check()?;
        let outcome = server.serve_one(control, &mut |request| {
            let mut backend = ReadyBackend {
                view: EconomicReadBackend::from_durable(&self.owner)?,
                head: self.expected,
                status: self.status,
            };
            crate::service_routes::dispatch(request, &mut backend)
        });
        self.check()?;
        outcome
    }
    pub fn into_owner(self) -> DurableEconomicChain {
        self.owner
    }
}

struct ReadyBackend<'a> {
    view: EconomicReadBackend<'a>,
    head: EconomicHead,
    status: NodeStatus,
}
impl ServiceBackend for ReadyBackend<'_> {
    fn call(&mut self, call: ServiceCall) -> Result<RpcValue> {
        if matches!(call.target, "runtime.initialize" | "runtime.status") {
            if !call.arguments.is_empty() {
                return Err(ServiceError::InvalidRequest);
            }
            let state = self.view.call(ServiceCall {
                target: "runtime.status",
                arguments: vec![],
            })?;
            let head = format!(
                "{}:{}:{}:{}",
                self.head.sequence,
                self.head.tip.height,
                block_id_text(self.head.tip.id),
                block_id_text(BlockId(self.head.commitment))
            );
            let quoted = RpcValue::string(&head).map_err(|_| ServiceError::Capacity)?;
            let body = format!(
                "{{\"initialized\":true,\"scope\":\"node\",\"daemonStarted\":true,{},\"economicHead\":{},\"state\":{}}}",
                self.status.members(),
                quoted.as_json(),
                state.as_json()
            );
            return RpcValue::parse(&body).map_err(|_| ServiceError::Backend("status_json"));
        }
        self.view.call(call)
    }
}
