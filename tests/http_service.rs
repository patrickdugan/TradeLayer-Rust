use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::net::{Shutdown, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tradelayer_core::bootstrap::BootstrapControl;
use tradelayer_core::client::RpcValue;
use tradelayer_core::contract_registry::ContractId;
use tradelayer_core::economic::{EconomicConfig, SettlementPolicy};
use tradelayer_core::economic_chain::{DurableEconomicChain, EconomicChain, EconomicChainConfig};
use tradelayer_core::http_service::{HttpServiceClient, HttpServiceServer, ServeOutcome};
use tradelayer_core::ledger::{AccountId, Ledger, Operation, PropertyId, PropertyKind, TxId};
use tradelayer_core::network_profile::TradeLayerMainnet;
use tradelayer_core::service::*;
use tradelayer_core::service_client::{
    ClientData, ClientMethod, ClientOutcome, ClientRouting, ServiceClient, SourceClientKind,
};
use tradelayer_core::service_runtime::EconomicHttpService;
use tradelayer_core::storage::{Journal, Namespace, Recovery, StorageError};
use tradelayer_core::tally::FeeRoute;

fn json(text: &str) -> RpcValue {
    RpcValue::parse(text).unwrap()
}
fn request(method: ServiceMethod, path: &str, body: &str) -> ServiceRequest {
    ServiceRequest::new(method, path, BTreeMap::new(), json(body)).unwrap()
}
fn serve<F>(server: &HttpServiceServer, control: &BootstrapControl, handler: &mut F) -> ServeOutcome
where
    F: FnMut(&ServiceRequest) -> Result<ServiceResponse, ServiceError>,
{
    let end = Instant::now() + Duration::from_secs(5);
    loop {
        let outcome = server.serve_one(control, handler).unwrap();
        if outcome != ServeOutcome::Idle {
            return outcome;
        }
        assert!(Instant::now() < end, "client did not connect");
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn real_http_preserves_query_utf8_integer_lexemes_and_absent_post_bodies() {
    let server =
        HttpServiceServer::bind("127.0.0.1:0".parse().unwrap(), Duration::from_secs(2)).unwrap();
    let endpoint = server.local_addr().unwrap();
    for absent in [false, true] {
        let mut req = request(
            ServiceMethod::Post,
            "/Echo/a b/é",
            r#"{"atoms":340282366920938463463374607431768211455,"escaped":"\"\\"}"#,
        );
        req.query.insert("a+b".into(), " x+y &/?%é".into());
        if absent {
            req = req.without_body();
        }
        let expected = req.clone();
        let client = std::thread::spawn(move || {
            HttpServiceClient::new(endpoint, Duration::from_secs(2))
                .unwrap()
                .send(&req, &BootstrapControl::default())
                .unwrap()
        });
        assert_eq!(
            serve(&server, &BootstrapControl::default(), &mut |got| {
                assert_eq!(got.path, expected.path);
                assert_eq!(got.query, expected.query);
                assert_eq!(got.body_present, !absent);
                if absent {
                    assert_eq!(got.body.as_json(), "{}");
                } else {
                    assert_eq!(got.body, expected.body);
                }
                ServiceResponse::json(201, got.body.clone())
            }),
            ServeOutcome::Responded { status: 201 }
        );
        let response = client.join().unwrap();
        assert_eq!(response.status, 201);
        assert_eq!(response.content_type, ServiceContentType::Json);
        assert_eq!(
            response.body,
            if absent {
                "{}"
            } else {
                expected.body.as_json()
            }
        );
    }
}

#[test]
fn malformed_framing_authority_paths_and_body_limits_never_reach_backend() {
    let server =
        HttpServiceServer::bind("127.0.0.1:0".parse().unwrap(), Duration::from_secs(2)).unwrap();
    let endpoint = server.local_addr().unwrap();
    let cases=[
        ("GET /x HTTP/1.1\r\n\r\n",400),
        ("GET /x HTTP/1.1\r\nHost: external.example\r\n\r\n",400),
        ("GET /x HTTP/1.1\r\nHost: {HOST}\r\nHost: {HOST}\r\n\r\n",400),
        ("POST /x HTTP/1.1\r\nHost: {HOST}\r\nContent-Length: 0\r\nContent-Length: 0\r\n\r\n",400),
        ("POST /x HTTP/1.1\r\nHost: {HOST}\r\nContent-Length: 0\r\nTransfer-Encoding: chunked\r\n\r\n",400),
        ("POST /x HTTP/1.1\r\nHost: {HOST}\r\nExpect: 100-continue\r\n\r\n",400),
        ("GET /x HTTP/1.1\r\nHost: {HOST}\r\n folded: value\r\n\r\n",400),
        ("GET /x HTTP/1.1\r\nHost: {HOST}\r\nX-A: a\nb\r\n\r\n",400),
        ("GET /a%2fb HTTP/1.1\r\nHost: {HOST}\r\n\r\n",400),
        ("GET /a%5cb HTTP/1.1\r\nHost: {HOST}\r\n\r\n",400),
        ("GET /a%00b HTTP/1.1\r\nHost: {HOST}\r\n\r\n",400),
        ("GET /a%ff HTTP/1.1\r\nHost: {HOST}\r\n\r\n",400),
        ("GET /x?a=1&a=2 HTTP/1.1\r\nHost: {HOST}\r\n\r\n",400),
        ("GET /x#fragment HTTP/1.1\r\nHost: {HOST}\r\n\r\n",400),
        ("POST /x HTTP/1.1\r\nHost: {HOST}\r\nContent-Type: text/plain\r\nContent-Length: 2\r\n\r\n{}",400),
        ("POST /x HTTP/1.1\r\nHost: {HOST}\r\nContent-Type: application/json\r\nContent-Length: 4\r\n\r\nnull",400),
        ("POST /x HTTP/1.1\r\nHost: {HOST}\r\nContent-Type: application/json\r\nContent-Length: 1048577\r\n\r\n",413),
    ];
    for (wire, status) in cases {
        let wire = wire.replace("{HOST}", &endpoint.to_string());
        let client = std::thread::spawn(move || {
            let mut stream = TcpStream::connect(endpoint).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            stream.write_all(wire.as_bytes()).unwrap();
            stream.shutdown(Shutdown::Write).unwrap();
            let mut response = String::new();
            stream.read_to_string(&mut response).unwrap();
            response
        });
        assert_eq!(
            serve(&server, &BootstrapControl::default(), &mut |_| panic!(
                "malformed request reached backend"
            )),
            ServeOutcome::Responded { status }
        );
        let response = client.join().unwrap();
        assert!(
            response.starts_with(&format!("HTTP/1.1 {status} ")),
            "{response}"
        );
    }
}

#[test]
fn a_stalled_body_cancellation_stops_the_same_server_step_without_a_callback() {
    let server =
        HttpServiceServer::bind("127.0.0.1:0".parse().unwrap(), Duration::from_secs(5)).unwrap();
    let endpoint = server.local_addr().unwrap();
    let control = BootstrapControl::default();
    let cancel = control.clone();
    let client = std::thread::spawn(move || {
        let mut stream = TcpStream::connect(endpoint).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        stream.write_all(format!("POST /x HTTP/1.1\r\nHost: {endpoint}\r\nContent-Type: application/json\r\nContent-Length: 10\r\n\r\n").as_bytes()).unwrap();
        std::thread::sleep(Duration::from_millis(20));
        cancel.cancel();
        let mut reply = Vec::new();
        stream.read_to_end(&mut reply).unwrap();
        assert!(reply.is_empty());
    });
    let started = Instant::now();
    assert_eq!(
        serve(&server, &control, &mut |_| panic!(
            "cancelled body reached backend"
        )),
        ServeOutcome::Stopped
    );
    assert!(started.elapsed() < Duration::from_secs(2));
    client.join().unwrap();
    assert_eq!(
        serve(&server, &control, &mut |_| panic!(
            "stopped server dispatched"
        )),
        ServeOutcome::Stopped
    );
}

#[test]
fn caller_limits_and_loopback_bind_fail_before_socket_work() {
    assert!(HttpServiceServer::bind("0.0.0.0:0".parse().unwrap(), Duration::from_secs(1)).is_err());
    assert!(HttpServiceServer::bind("[::]:0".parse().unwrap(), Duration::from_secs(1)).is_err());
    assert!(
        HttpServiceClient::new("127.0.0.1:0".parse().unwrap(), Duration::from_secs(1)).is_err()
    );
    let mut client =
        HttpServiceClient::new("127.0.0.1:1".parse().unwrap(), Duration::from_secs(1)).unwrap();
    let control = BootstrapControl::default();
    control.cancel();
    assert_eq!(
        client.send(&request(ServiceMethod::Get, "/x", "{}"), &control),
        Err(ServiceError::Cancelled)
    );
    let mut req = request(ServiceMethod::Get, "/x", "{}");
    req.query.insert("x".into(), "é".repeat(3000));
    assert_eq!(
        client.send(&req, &BootstrapControl::default()),
        Err(ServiceError::Capacity)
    );
}

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "tl-service-http-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        assert!(
            self.0.is_absolute()
                && self.0.starts_with(std::env::temp_dir())
                && self
                    .0
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("tl-service-http-")
        );
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn config(network: TradeLayerMainnet) -> EconomicChainConfig {
    EconomicChainConfig {
        economics: EconomicConfig {
            network,
            contract: ContractId::try_from(1).unwrap(),
            collateral: PropertyId::try_from(7).unwrap(),
            inverse: false,
            notional: "1".parse().unwrap(),
            initial_margin_per_contract: "10".parse().unwrap(),
            fee_route: FeeRoute::Native(1),
            policy: SettlementPolicy::ConservingV1,
            funding: None,
            marking: None,
            liquidation: None,
            amm: None,
            contract_binding: None,
        },
        admin: AccountId::try_from("admin").unwrap(),
        permitted_spot_properties: BTreeSet::from([PropertyId::try_from(7).unwrap()]),
        rollback_limit: 4,
    }
}

#[test]
fn concrete_http_clients_query_the_original_locked_owner_and_return_exact_custody() {
    use tradelayer_core::issuance_intent::JsonValue;
    let directory = Directory::new();
    let network = TradeLayerMainnet::Bitcoin;
    let cfg = config(network);
    let mut ledger = Ledger::new();
    let amount = 123456789012345678901234567890_u128;
    ledger
        .apply(
            TxId::from_bytes([1; 32]),
            Operation::CreateProperty {
                id: PropertyId::try_from(7).unwrap(),
                issuer: AccountId::try_from("alice").unwrap(),
                kind: PropertyKind::Fixed,
                initial_atoms: amount,
            },
        )
        .unwrap();
    let owner = DurableEconomicChain::initialize(
        &directory.0,
        EconomicChain::new_unverified(cfg.clone(), ledger).unwrap(),
        cfg.anchor(),
    )
    .unwrap();
    let expected = owner.head().unwrap();
    let before = owner.chain().unwrap().state().snapshot_bytes().unwrap();
    let service = EconomicHttpService::bind(
        owner,
        "127.0.0.1:0".parse().unwrap(),
        Duration::from_secs(2),
    )
    .unwrap();
    let endpoint = service.local_addr().unwrap();
    assert!(matches!(
        Journal::open_existing(
            &directory.0,
            Namespace::for_mainnet(network, "clearing").unwrap(),
            Recovery::RejectIncompleteTail
        ),
        Err(StorageError::Locked)
    ));
    let client = std::thread::spawn(move || {
        let transport = HttpServiceClient::new(endpoint, Duration::from_secs(2)).unwrap();
        let mut client = ServiceClient::new(
            SourceClientKind::WalletInterface,
            ClientRouting::RepairedServiceV1,
            transport,
        );
        let initialized = client
            .call(ClientMethod::InitMain, &[], &BootstrapControl::default())
            .unwrap();
        let properties = client
            .call(
                ClientMethod::ListProperties,
                &[],
                &BootstrapControl::default(),
            )
            .unwrap();
        let balances = client
            .call(
                ClientMethod::GetAllBalancesForAddress,
                &[JsonValue::String("alice".into())],
                &BootstrapControl::default(),
            )
            .unwrap();
        (initialized, properties, balances)
    });
    for _ in 0..3 {
        let until = Instant::now() + Duration::from_secs(5);
        loop {
            if matches!(
                service.serve_one(&BootstrapControl::default()).unwrap(),
                ServeOutcome::Responded { status: 200 }
            ) {
                break;
            }
            assert!(Instant::now() < until);
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    let (initialized, properties, balances) = client.join().unwrap();
    let ClientOutcome::Response(initialized) = initialized else {
        panic!("wallet init response");
    };
    let initialized = json(&initialized.body);
    assert_eq!(
        initialized.field("initialized").unwrap().unwrap().as_json(),
        "true"
    );
    assert_eq!(
        initialized
            .field("scope")
            .unwrap()
            .unwrap()
            .as_string()
            .unwrap(),
        "economic_read_service"
    );
    assert_eq!(
        initialized
            .field("daemonStarted")
            .unwrap()
            .unwrap()
            .as_json(),
        "false"
    );
    assert_eq!(
        initialized
            .field("authenticated_admission")
            .unwrap()
            .unwrap()
            .as_json(),
        "false"
    );
    let get = |value| match value {
        ClientOutcome::Data(ClientData::Json(data)) => data,
        _ => panic!("expected JSON"),
    };
    let properties = get(properties);
    let balances = get(balances);
    assert!(properties.as_json().contains(&amount.to_string()));
    assert!(balances.as_json().contains(&format!("\"{amount}\"")));
    assert!(balances.as_json().contains("alice") || balances.as_json().contains("availableAtoms"));
    assert_eq!(service.economic_head().unwrap(), expected);
    assert_eq!(
        service
            .economic_chain()
            .unwrap()
            .state()
            .snapshot_bytes()
            .unwrap(),
        before
    );
    let (_, owner) = service.into_parts();
    assert_eq!(owner.head().unwrap(), expected);
    drop(owner);
    let (owner, _) =
        DurableEconomicChain::load(&directory.0, cfg, expected, Recovery::RejectIncompleteTail)
            .unwrap();
    assert_eq!(owner.head().unwrap(), expected);
}
