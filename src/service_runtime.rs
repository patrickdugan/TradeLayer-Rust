//! An owned local read service over an already restored complete economic owner.
use crate::bootstrap::BootstrapControl;
use crate::economic_chain::{DurableEconomicChain, EconomicChain, EconomicHead};
use crate::http_service::{HttpServiceServer, ServeOutcome};
use crate::runtime::block_id_text;
use crate::service::{ServiceBackend, ServiceCall, ServiceError};
use crate::service_view::EconomicReadBackend;
use crate::{chain::BlockId, client::RpcValue};
use std::net::SocketAddr;
use std::time::Duration;

type Result<T> = core::result::Result<T, ServiceError>;
/// The service keeps the owner's journal lock. Network queries cannot apply or
/// rewind economic state; consuming this wrapper returns that same owner.
pub struct EconomicHttpService {
    server: HttpServiceServer,
    owner: DurableEconomicChain,
    expected: EconomicHead,
}
impl EconomicHttpService {
    pub fn bind(
        owner: DurableEconomicChain,
        endpoint: SocketAddr,
        timeout: Duration,
    ) -> Result<Self> {
        EconomicReadBackend::from_durable(&owner)?;
        let expected = owner
            .head()
            .map_err(|_| ServiceError::Backend("durable_health"))?;
        let server = HttpServiceServer::bind(endpoint, timeout)?;
        Ok(Self {
            server,
            owner,
            expected,
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
    pub fn local_addr(&self) -> Result<SocketAddr> {
        self.server.local_addr()
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
    pub fn serve_one(&self, control: &BootstrapControl) -> Result<ServeOutcome> {
        self.check()?;
        let outcome = self.server.serve_one(control, &mut |request| {
            let mut backend = ReadyBackend {
                view: EconomicReadBackend::from_durable(&self.owner)?,
                head: self.expected,
            };
            crate::service_routes::dispatch(request, &mut backend)
        });
        self.check()?;
        outcome
    }
    pub fn into_parts(self) -> (HttpServiceServer, DurableEconomicChain) {
        (self.server, self.owner)
    }
}
struct ReadyBackend<'a> {
    view: EconomicReadBackend<'a>,
    head: EconomicHead,
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
            let body=format!("{{\"initialized\":true,\"scope\":\"economic_read_service\",\"daemonStarted\":false,\"authenticated_admission\":false,\"economicHead\":{},\"state\":{}}}",quoted.as_json(),state.as_json());
            return RpcValue::parse(&body).map_err(|_| ServiceError::Backend("status_json"));
        }
        self.view.call(call)
    }
}
