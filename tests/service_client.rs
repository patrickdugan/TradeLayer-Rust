use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::{Duration, Instant};
use tradelayer_core::bootstrap::BootstrapControl;
use tradelayer_core::client::RpcValue;
use tradelayer_core::http_service::HttpServiceClient;
use tradelayer_core::issuance_intent::{JsonObject, JsonValue};
use tradelayer_core::service::*;
use tradelayer_core::service_client::*;

fn unhex(text: &str) -> String {
    if text == "-" {
        return String::new();
    }
    String::from_utf8(
        text.as_bytes()
            .chunks_exact(2)
            .map(|pair| u8::from_str_radix(core::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect(),
    )
    .unwrap()
}
fn obj(value: &JsonValue) -> &JsonObject {
    match value {
        JsonValue::Object(value) => value,
        _ => panic!("object"),
    }
}
fn array(value: &JsonValue) -> &[JsonValue] {
    match value {
        JsonValue::Array(value) => value,
        _ => panic!("array"),
    }
}
fn string(value: &JsonValue) -> &str {
    match value {
        JsonValue::String(value) => value,
        _ => panic!("string"),
    }
}
fn field<'a>(value: &'a JsonValue, name: &str) -> &'a JsonValue {
    obj(value).get(name).unwrap()
}
fn str_field<'a>(value: &'a JsonValue, name: &str) -> &'a str {
    string(field(value, name))
}
fn json(text: &str) -> JsonValue {
    JsonValue::parse(text).unwrap()
}
fn response(scenario: &JsonValue) -> Result<ServiceResponse, ServiceError> {
    let s = obj(scenario);
    if s.get("failure") == Some(&JsonValue::from("transport")) {
        return Err(ServiceError::Transport(std::io::ErrorKind::ConnectionReset));
    }
    let status = match s.get("status") {
        Some(JsonValue::Number(value)) => value.as_f64() as u16,
        _ => 200,
    };
    let body = if let Some(JsonValue::String(wire)) = s.get("wire") {
        wire.clone()
    } else {
        s.get("data")
            .cloned()
            .unwrap_or_else(|| json("{\"ok\":true}"))
            .to_json()
            .unwrap()
    };
    ServiceResponse::text(status, body)
}
struct Recording {
    calls: Vec<ServiceRequest>,
    response: Result<ServiceResponse, ServiceError>,
    cancel: bool,
}
impl Recording {
    fn new(response: Result<ServiceResponse, ServiceError>) -> Self {
        Self {
            calls: vec![],
            response,
            cancel: false,
        }
    }
}
impl ServiceTransport for Recording {
    fn send(
        &mut self,
        request: &ServiceRequest,
        control: &BootstrapControl,
    ) -> Result<ServiceResponse, ServiceError> {
        self.calls.push(request.clone());
        if self.cancel {
            control.cancel();
        }
        self.response.clone()
    }
}
fn same_data(actual: &ClientData, expected: &JsonValue) {
    match actual {
        ClientData::Json(value) => assert_eq!(json(value.as_json()), *expected),
        ClientData::Text(value) => assert_eq!(JsonValue::String(value.clone()), *expected),
    }
}
fn same_request(request: &ServiceRequest, expected: &JsonValue) {
    assert_eq!(request.path, str_field(expected, "path"));
    assert_eq!(
        request.method,
        if str_field(expected, "method") == "GET" {
            ServiceMethod::Get
        } else {
            ServiceMethod::Post
        }
    );
    assert_eq!(
        request.body_present,
        field(expected, "body_present") == &JsonValue::Bool(true)
    );
    assert_eq!(json(request.body.as_json()), *field(expected, "body"));
    let query: BTreeMap<_, _> = obj(field(expected, "query"))
        .iter()
        .map(|(key, value)| (key.to_owned(), string(value).to_owned()))
        .collect();
    assert_eq!(request.query, query);
}

#[test]
fn full_unchanged_source_corpus_matches_requests_and_results() {
    let mut count = 0;
    let mut source_only = 0;
    for line in include_str!("../fixtures/service_client_vectors.tsv")
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        let f: Vec<_> = line.split('\t').collect();
        count += 1;
        let kind = if f[0] == "express" {
            SourceClientKind::InterfaceExpress
        } else {
            SourceClientKind::WalletInterface
        };
        let args = json(&unhex(f[3]));
        let scenario = json(&unhex(f[4]));
        let calls = json(&unhex(f[6]));
        let expected = json(&unhex(f[7]));
        let mut client = ServiceClient::new(
            kind,
            ClientRouting::PinnedSource,
            Recording::new(response(&scenario)),
        );
        let actual = client.call_by_name(f[2], array(&args), &BootstrapControl::default());
        if f[5] != "supported" {
            source_only += 1;
            assert_eq!(
                actual.unwrap_err(),
                if f[5] == "raw-url-only" {
                    ClientError::UnsupportedSourcePath
                } else {
                    ClientError::UnsupportedSourceQuery
                },
                "{}",
                f[1]
            );
            assert!(client.into_transport().calls.is_empty());
            continue;
        }
        match str_field(&expected, "kind") {
            "undefined" => match actual.unwrap() {
                ClientOutcome::Undefined { swallowed } => assert_eq!(
                    swallowed.is_some(),
                    field(&expected, "swallowed") == &JsonValue::Bool(true),
                    "{}",
                    f[1]
                ),
                other => panic!("{} {other:?}", f[1]),
            },
            "data" => match actual.unwrap() {
                ClientOutcome::Data(data) => same_data(&data, field(&expected, "value")),
                other => panic!("{} {other:?}", f[1]),
            },
            "response" => match actual.unwrap() {
                ClientOutcome::Response(response) => {
                    assert_eq!(response.status, 200);
                    same_data(
                        &ClientData::Json(RpcValue::parse(&response.body).unwrap()),
                        field(&expected, "body"),
                    );
                }
                other => panic!("{} {other:?}", f[1]),
            },
            "number" => match actual.unwrap() {
                ClientOutcome::Number(value) => {
                    let bits = str_field(&expected, "bits");
                    if bits == "nan" {
                        assert!(value.as_f64().is_nan(), "{}", f[1]);
                    } else {
                        assert_eq!(
                            value.bits(),
                            u64::from_str_radix(bits, 16).unwrap(),
                            "{}",
                            f[1]
                        );
                    }
                }
                other => panic!("{} {other:?}", f[1]),
            },
            "throw" => {
                let error = actual.unwrap_err();
                if f[2] == "notAnExport" {
                    assert_eq!(error, ClientError::UnknownMethod);
                } else if field(&expected, "original") == &JsonValue::Bool(true) {
                    assert!(
                        matches!(
                            error,
                            ClientError::Failure(
                                ClientFailure::Transport(_) | ClientFailure::Http { .. }
                            )
                        ),
                        "{} {error:?}",
                        f[1]
                    );
                } else {
                    match error {
                        ClientError::Failure(ClientFailure::SourceThrow { name, message }) => {
                            assert_eq!(name, str_field(&expected, "name"), "{}", f[1]);
                            assert_eq!(message, str_field(&expected, "message"), "{}", f[1]);
                        }
                        other => panic!("{} {other:?}", f[1]),
                    }
                }
            }
            other => panic!("{other}"),
        }
        let recorded = client.into_transport();
        assert_eq!(recorded.calls.len(), array(&calls).len(), "{}", f[1]);
        for (actual, expected) in recorded.calls.iter().zip(array(&calls)) {
            same_request(actual, field(expected, "request"));
        }
    }
    assert_eq!((count, source_only), (213, 10));
}

#[test]
fn every_export_is_available_only_on_its_actual_source_object() {
    assert_eq!(
        ClientMethod::ALL
            .iter()
            .filter(|method| method.supported_by(SourceClientKind::InterfaceExpress))
            .count(),
        16
    );
    assert_eq!(
        ClientMethod::ALL
            .iter()
            .filter(|method| method.supported_by(SourceClientKind::WalletInterface))
            .count(),
        19
    );
    let archived = include_str!("../fixtures/legacy_interface_express_source.txt");
    assert_eq!(
        archived
            .matches("async getContractPositionForAddressAndContractId(")
            .count(),
        2
    );
    for method in ClientMethod::ALL {
        assert_eq!(ClientMethod::from_name(method.name()), Some(method));
    }
    assert_eq!(ClientMethod::from_name("getcontractinfo"), None);
    let mut client = ServiceClient::new(
        SourceClientKind::InterfaceExpress,
        ClientRouting::PinnedSource,
        Recording::new(response(&json("{}"))),
    );
    assert_eq!(
        client.call(
            ClientMethod::GetInitialMargin,
            &[],
            &BootstrapControl::default()
        ),
        Err(ClientError::UnknownMethod)
    );
    assert!(client.into_transport().calls.is_empty());
}

#[test]
fn named_repairs_restore_missing_params_without_changing_source_mode() {
    use ClientMethod::*;
    let wallet = SourceClientKind::WalletInterface;
    let repair = ClientRouting::RepairedServiceV1;
    let source = ClientRouting::PinnedSource;
    let init = prepare_request(wallet, repair, InitMain, &[]).unwrap();
    assert_eq!(json(init.body.as_json()), json("{\"test\":false}"));
    assert_eq!(
        json(
            prepare_request(wallet, repair, InitMain, &[JsonValue::Bool(true)])
                .unwrap()
                .body
                .as_json()
        ),
        json("{\"test\":true}")
    );
    assert_eq!(
        json(
            prepare_request(wallet, source, InitMain, &[JsonValue::Bool(false)])
                .unwrap()
                .body
                .as_json()
        ),
        json("{\"test\":true}")
    );
    for invalid in [
        JsonValue::Null,
        json("0"),
        JsonValue::from("false"),
        json("{}"),
    ] {
        assert_eq!(
            prepare_request(wallet, repair, InitMain, &[invalid]),
            Err(ClientError::InvalidArguments)
        );
    }
    let p =
        |method, args: &str| prepare_request(wallet, repair, method, array(&json(args))).unwrap();
    let order = p(
        GetOrderBook,
        "[{\"propertyId1\":7,\"propertyId2\":8,\"ignored\":true}]",
    );
    assert_eq!(order.path, "/tl_getOrderbook");
    assert_eq!(
        json(order.body.as_json()),
        json("{\"propertyId1\":7,\"propertyId2\":8}")
    );
    assert!(matches!(
        prepare_request(wallet, source, GetOrderBook, array(&json("[{}]"))),
        Err(ClientError::Failure(ClientFailure::SourceThrow {
            name: "ReferenceError",
            ..
        }))
    ));
    let position = p(
        GetContractPositionForAddressAndContractId,
        "[{\"address\":\"a/b?c#d%\\\\e\",\"contractId\":1}]",
    );
    assert_eq!(position.path, "/tl_contractPosition");
    assert_eq!(position.query.get("address").unwrap(), "a/b?c#d%\\e");
    assert!(prepare_request(
        wallet,
        source,
        GetContractPositionForAddressAndContractId,
        array(&json("[{\"address\":\"alice\",\"contractId\":1}]"))
    )
    .unwrap()
    .query
    .is_empty());
    let column = p(
        GetColumn,
        "[{\"channel\":\"c\",\"addressA\":\"a\",\"addressB\":\"b\"}]",
    );
    assert_eq!(column.method, ServiceMethod::Post);
    assert_eq!(
        json(column.body.as_json()),
        json("{\"channelAddress\":\"c\",\"newCommitAddress\":\"a\",\"cpAddress\":\"b\"}")
    );
    let balance = p(GetAllBalancesForAddress, "[\"alice\"]");
    assert_eq!(
        json(balance.body.as_json()),
        json("{\"address\":\"alice\"}")
    );
    let oracle = p(GetOracleHistory, "[{\"oracleId\":3}]");
    assert_eq!(oracle.query.get("contractId"), Some(&"3".to_owned()));
    let express = SourceClientKind::InterfaceExpress;
    let spaces = [JsonValue::from("a ")];
    assert_eq!(
        prepare_request(express, source, GetClearingHistory, &spaces)
            .unwrap()
            .path,
        "/clearingHistory/a"
    );
    assert_eq!(
        prepare_request(express, repair, GetClearingHistory, &spaces)
            .unwrap()
            .path,
        "/clearingHistory/a "
    );
    assert_eq!(
        prepare_request(
            express,
            source,
            GetClearingHistory,
            &[JsonValue::from(". ")]
        ),
        Err(ClientError::UnsupportedSourcePath)
    );
    assert_eq!(
        prepare_request(express, repair, GetWalletPositions, &[]),
        Err(ClientError::InvalidArguments)
    );
    let positions = prepare_request(
        express,
        repair,
        GetWalletPositions,
        &[JsonValue::from("alice")],
    )
    .unwrap();
    assert_eq!(positions.query.get("address"), Some(&"alice".into()));
    for method in ClientMethod::ALL {
        let args = match method {
            InitMain => json("[]"),
            GetOrderBook => json("[{\"propertyId1\":7,\"propertyId2\":8}]"),
            GetContractOrderBook | GetContractTradeHistory | GetFundingHistory => {
                json("[{\"contractId\":1}]")
            }
            GetContractPositionForAddressAndContractId => {
                json("[{\"address\":\"alice\",\"contractId\":1}]")
            }
            GetTradeHistory => json("[{\"propertyId1\":7,\"propertyId2\":8}]"),
            GetOracleHistory => json("[{\"oracleId\":1}]"),
            GetColumn => json("[{\"channel\":\"c\",\"addressA\":\"a\",\"addressB\":\"b\"}]"),
            _ => json("[1,100]"),
        };
        if method.supported_by(wallet) {
            let request = prepare_request(wallet, repair, method, array(&args)).unwrap();
            request.validate().unwrap();
        }
    }
}

#[test]
fn cancellation_deadline_and_resource_caps_precede_transport_and_override_swallow() {
    let args = vec![JsonValue::Null; MAX_ARGUMENTS + 1];
    let mut client = ServiceClient::new(
        SourceClientKind::InterfaceExpress,
        ClientRouting::PinnedSource,
        Recording::new(response(&json("{}"))),
    );
    assert_eq!(
        client.call(ClientMethod::InitMain, &args, &BootstrapControl::default()),
        Err(ClientError::Capacity)
    );
    let oversized = JsonValue::String("x".repeat(65_537));
    assert_eq!(
        client.call(
            ClientMethod::InitMain,
            &[oversized],
            &BootstrapControl::default()
        ),
        Err(ClientError::Capacity)
    );
    let control = BootstrapControl::default();
    control.cancel();
    assert_eq!(
        client.call(ClientMethod::InitMain, &[], &control),
        Err(ClientError::Failure(ClientFailure::Transport(
            ServiceError::Cancelled
        )))
    );
    let expired = BootstrapControl::new(Some(Instant::now()));
    assert_eq!(
        client.call(ClientMethod::InitMain, &[], &expired),
        Err(ClientError::Failure(ClientFailure::Transport(
            ServiceError::Timeout
        )))
    );
    assert!(client.into_transport().calls.is_empty());
    let mut transport = Recording::new(Err(ServiceError::Transport(
        std::io::ErrorKind::Interrupted,
    )));
    transport.cancel = true;
    let mut client = ServiceClient::new(
        SourceClientKind::InterfaceExpress,
        ClientRouting::PinnedSource,
        transport,
    );
    assert_eq!(
        client.call(ClientMethod::InitMain, &[], &BootstrapControl::default()),
        Err(ClientError::Failure(ClientFailure::Transport(
            ServiceError::Cancelled
        )))
    );
    let mut client = ServiceClient::new(
        SourceClientKind::WalletInterface,
        ClientRouting::PinnedSource,
        Recording::new(Ok(ServiceResponse {
            status: 200,
            content_type: ServiceContentType::Text,
            body: " ".repeat(MAX_SERVICE_RESPONSE + 1),
        })),
    );
    assert_eq!(
        client.call(
            ClientMethod::ListProperties,
            &[],
            &BootstrapControl::default()
        ),
        Err(ClientError::Capacity)
    );
    let mut client = ServiceClient::new(
        SourceClientKind::WalletInterface,
        ClientRouting::PinnedSource,
        Recording::new(Ok(ServiceResponse::text(
            200,
            "0".repeat(MAX_NUMBER_RESPONSE + 1),
        )
        .unwrap())),
    );
    assert_eq!(
        client.call(
            ClientMethod::GetInitialMargin,
            &[],
            &BootstrapControl::default()
        ),
        Err(ClientError::Capacity)
    );
}

#[test]
fn duplicate_json_fields_use_source_last_write_order_and_text_errors_stay_text() {
    let mut client = ServiceClient::new(
        SourceClientKind::WalletInterface,
        ClientRouting::PinnedSource,
        Recording::new(Ok(ServiceResponse::text(200, "{\"x\":1,\"x\":2}").unwrap())),
    );
    match client
        .call(
            ClientMethod::ListProperties,
            &[],
            &BootstrapControl::default(),
        )
        .unwrap()
    {
        ClientOutcome::Data(data) => same_data(&data, &json("{\"x\":2}")),
        other => panic!("{other:?}"),
    }
    let mut client = ServiceClient::new(
        SourceClientKind::WalletInterface,
        ClientRouting::PinnedSource,
        Recording::new(Ok(ServiceResponse::text(400, "bad address").unwrap())),
    );
    assert_eq!(
        client.call(
            ClientMethod::ListProperties,
            &[],
            &BootstrapControl::default()
        ),
        Err(ClientError::Failure(ClientFailure::Http {
            status: 400,
            data: ClientData::Text("bad address".into())
        }))
    );
}

fn read_request(stream: &mut std::net::TcpStream) -> String {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut bytes = Vec::new();
    loop {
        let mut chunk = [0u8; 4096];
        let n = stream.read(&mut chunk).unwrap();
        assert_ne!(n, 0);
        bytes.extend_from_slice(&chunk[..n]);
        if let Some(end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            let headers = core::str::from_utf8(&bytes[..end]).unwrap();
            let length = headers
                .lines()
                .find_map(|line| line.strip_prefix("Content-Length: "))
                .unwrap()
                .parse::<usize>()
                .unwrap();
            if bytes.len() >= end + 4 + length {
                return String::from_utf8(bytes).unwrap();
            }
        }
    }
}
#[test]
fn both_clients_bind_real_http_transport_with_absent_bodies_and_single_encoding() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let mut calls = Vec::new();
        for body in ["{\"ok\":true}", "\"-0\"", "{\"available\":7}"] {
            let (mut stream, _) = listener.accept().unwrap();
            calls.push(read_request(&mut stream));
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
        }
        calls
    });
    let transport = HttpServiceClient::new(endpoint, Duration::from_secs(2)).unwrap();
    let mut express = ServiceClient::new(
        SourceClientKind::InterfaceExpress,
        ClientRouting::PinnedSource,
        transport,
    );
    assert!(matches!(
        express
            .call(
                ClientMethod::ListProperties,
                &[],
                &BootstrapControl::default()
            )
            .unwrap(),
        ClientOutcome::Data(_)
    ));
    let mut wallet = ServiceClient::new(
        SourceClientKind::WalletInterface,
        ClientRouting::PinnedSource,
        express.into_transport(),
    );
    match wallet
        .call(
            ClientMethod::GetInitialMargin,
            &[JsonValue::from("x y&z%"), json("100")],
            &BootstrapControl::default(),
        )
        .unwrap()
    {
        ClientOutcome::Number(value) => assert_eq!(value.bits(), (-0.0f64).to_bits()),
        other => panic!("{other:?}"),
    }
    let mut repaired = ServiceClient::new(
        SourceClientKind::WalletInterface,
        ClientRouting::RepairedServiceV1,
        wallet.into_transport(),
    );
    assert!(matches!(
        repaired
            .call(
                ClientMethod::GetAllBalancesForAddress,
                &[JsonValue::from("alice")],
                &BootstrapControl::default()
            )
            .unwrap(),
        ClientOutcome::Data(_)
    ));
    let requests = server.join().unwrap();
    assert!(requests[0].starts_with("POST /listProperties HTTP/1.1\r\n"));
    assert!(requests[0].contains("Content-Length: 0\r\n"));
    assert!(requests[0].ends_with("\r\n\r\n"));
    assert!(requests[1]
        .starts_with("GET /tl_getInitMargin?contractId=x%20y%26z%25&price=100 HTTP/1.1\r\n"));
    assert!(requests[2].starts_with("POST /tl_getAllBalancesForAddress HTTP/1.1\r\n"));
    assert!(requests[2].ends_with("{\"address\":\"alice\"}"));
}
