# Owned local HTTP API

`HttpServiceServer` serves the complete repaired route tables through an
explicit backend. `HttpServiceClient` implements their real TCP transport.
`EconomicHttpService` binds those routes to an already restored complete
`DurableEconomicChain`, keeping its OS lock and the sole Ledger in the owner.
It exposes immutable queries and returns that same owner when consumed.

```rust
use std::time::Duration;
use tradelayer_core::bootstrap::BootstrapControl;
use tradelayer_core::economic_chain::DurableEconomicChain;
use tradelayer_core::http_service::ServeOutcome;
use tradelayer_core::service::ServiceError;
use tradelayer_core::service_runtime::EconomicHttpService;

fn serve_existing(
    owner: DurableEconomicChain,
    control: BootstrapControl,
) -> Result<DurableEconomicChain, ServiceError> {
    let address = "127.0.0.1:3000".parse().map_err(|_| ServiceError::InvalidRequest)?;
    let service = EconomicHttpService::bind(owner, address, Duration::from_secs(5))?;
    loop {
        match service.serve_one(&control)? {
            ServeOutcome::Stopped => break,
            ServeOutcome::Idle => std::thread::sleep(Duration::from_millis(10)),
            ServeOutcome::Responded { .. } => {}
        }
    }
    Ok(service.into_parts().1)
}
```

The caller supplies an owner restored using independent configuration, full
expected head and explicit recovery policy. This API never infers configuration,
initializes replacement state or launches a node on import. The current library
is an embedding surface; deployment configuration and a service launcher remain
operator integration work.

`runtime.initialize` is idempotent readiness of the already restored read
service. Its response explicitly says `scope:economic_read_service`,
`daemonStarted:false` and `authenticated_admission:false`. It starts neither
indexing nor clearing. Network pause, wallet loading/signing/broadcast,
unavailable property/insurance lifecycles and unsigned procedural settlement
require separate actual capabilities. They return typed unavailability through
this read backend. Unknown parser/observer heights stay null rather than imply
sync; a known economic checkpoint does not authenticate a chain.

The server binds loopback only and checks Host against its numeric authority or
`localhost` with the same port. It supplies a local read API without remote
authentication or TLS. It accepts one connection per polling call, parses one
HTTP/1.1 request, sends one explicitly framed response and closes. The caller
owns polling, cancellation, scheduling and shutdown. This serial bounded runner
is not a concurrent production-throughput claim.

Request headers are capped at16KiB/64 fields, request targets at8192 bytes,
queries at128 distinct keys and JSON object bodies at1MiB. JSON responses are
capped at16MiB. Duplicate length/Host/type fields, folded headers, transfer
encoding, Expect, unsupported content encoding, ambiguous percent-encoded path
separators, invalid UTF8/control bytes and duplicate query keys reject before
backend dispatch. An absent body differs from explicit JSON null; null/array
bodies are outside the repaired parameter-object contract. Query decoding treats
plus as space; decoded path and query components are encoded once by the client.

One absolute per-request timeout covers the socket work and response parsing;
explicit caller deadlines can shorten it. Server reads use at most100ms waits
between control checks without reissuing a request. A client connect or blocked
write can remain pending up to its configured deadline; no immediate OS-level
interruption is claimed. Transport calls never retry a request. Cancellation is
checked before and after transport/dispatch and before publishing a response.
Idle or stopped polls do not replay economic history. Queries validate the
actual owner only after an accepted request.

The source clients, listeners and facade are separately characterized against
complete pinned source. See [SERVICE_CLIENT.md](SERVICE_CLIENT.md),
[SERVICE_ROUTES.md](SERVICE_ROUTES.md) and [SERVICE_VIEW.md](SERVICE_VIEW.md).
The useful API policy is explicitly named; broken source callbacks, undefined
imports, asynchronous races and Axios/Express coercions remain observations.
Actual large atom balances, locked custody categories and Decimal quantities
use exact JSON strings rather than lossy Number conversions.

Tests use real loopback TCP, both concrete service clients, malformed HTTP,
UTF8/query encoding, absent POST bodies, cancellation with a silent peer, exact
large balances, the original locked economic owner and restart at its unchanged
head. Source oracles additionally use genuine installed Axios and Express in
controlled dependency adapters. Deployment interoperability, authentication,
full historical stores and all economic admission gates remain open.
# Executed spot price extensions

`GET /spotPrice/:base/:quote?height=H` and
`GET /spotVwap/:base/:quote?startBlock=A&endBlock=B` query actual retained
executions in the original owner. They preserve direction, exact atom sums and
owner tip, with no source fallback. These are additional Rust read routes;
see [ECONOMIC_SPOT_PRICES.md](ECONOMIC_SPOT_PRICES.md) for bounds and provenance.
