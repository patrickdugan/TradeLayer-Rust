use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener};
use std::sync::mpsc::{self, Receiver};
use std::thread::JoinHandle;
use std::time::Duration;
use tradelayer_core::client::*;
use tradelayer_core::network_profile::TradeLayerMainnet;

fn server(responses: Vec<String>) -> (SocketAddr, Receiver<String>, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (tx, rx) = mpsc::channel();
    let handle = std::thread::spawn(move || {
        for response in responses {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut data = Vec::new();
            let mut total = None;
            loop {
                let mut chunk = [0u8; 4096];
                let n = socket.read(&mut chunk).unwrap();
                assert!(n > 0);
                data.extend_from_slice(&chunk[..n]);
                if total.is_none() {
                    if let Some(end) = data.windows(4).position(|w| w == b"\r\n\r\n") {
                        let head = std::str::from_utf8(&data[..end]).unwrap();
                        let length = head
                            .lines()
                            .find_map(|line| line.strip_prefix("Content-Length: "))
                            .unwrap()
                            .parse::<usize>()
                            .unwrap();
                        total = Some(end + 4 + length);
                    }
                }
                if total.is_some_and(|total| data.len() >= total) {
                    break;
                }
            }
            tx.send(String::from_utf8(data).unwrap()).unwrap();
            let _ = socket.write_all(response.as_bytes());
        }
    });
    (address, rx, handle)
}
fn http(status: u16, body: &str) -> String {
    format!("HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",body.len())
}
fn reply(id: u64, value: &str) -> String {
    http(
        200,
        &format!("{{\"result\":{value},\"error\":null,\"id\":{id}}}"),
    )
}
fn client(address: SocketAddr, wallet: Option<String>) -> RpcClient {
    RpcClient::new(
        address,
        RpcAuth::Basic {
            username: "user".into(),
            password: "secret".into(),
        },
        wallet,
        Duration::from_secs(2),
        TradeLayerMainnet::Litecoin,
    )
    .unwrap()
}

#[test]
fn source_wrappers_preserve_wallet_paths_default_verbose_and_ignored_block_hash() {
    let (address, rx, handle) = server(vec![
        reply(1, "null"),
        reply(2, "[]"),
        reply(3, "[]"),
        reply(4, "null"),
    ]);
    let mut client = client(address, Some("desk/a b?#雪".into()));
    client
        .call_source(
            SourceRpcMethod::GetRawTransaction,
            &[
                RpcValue::from_string("tx"),
                RpcValue::boolean(false),
                RpcValue::from_string("ignored"),
            ],
        )
        .unwrap();
    client
        .call_source(SourceRpcMethod::ListUnspent, &[RpcValue::unsigned(6)])
        .unwrap();
    client
        .call_source(SourceRpcMethod::ListUnspentLower, &[RpcValue::unsigned(99)])
        .unwrap();
    client
        .call_source(SourceRpcMethod::GetNewAddress, &[])
        .unwrap();
    let requests = rx.iter().collect::<Vec<_>>();
    handle.join().unwrap();
    assert!(requests[0].starts_with("POST / HTTP/1.1\r\n"));
    assert!(requests[0].contains("Authorization: Basic dXNlcjpzZWNyZXQ="));
    assert!(requests[0].ends_with("\"params\":[\"tx\",false]}"));
    assert!(requests[1].starts_with("POST /wallet/desk%2Fa%20b%3F%23%E9%9B%AA HTTP/1.1\r\n"));
    assert!(requests[1].ends_with("\"params\":[6]}"));
    assert!(requests[2].ends_with("\"params\":[]}"));
    assert!(requests[3].starts_with("POST / HTTP/1.1\r\n"));
    assert_eq!(
        SourceRpcMethod::GetRawTransaction
            .source_params(&[RpcValue::null()])
            .unwrap()[1],
        RpcValue::boolean(true)
    );
}

#[test]
fn raw_json_keeps_large_block_hex_exact_integers_and_unicode() {
    let hex = "ab".repeat(160_000);
    let (address, rx, handle) = server(vec![
        reply(1, &format!("\"{hex}\"")),
        reply(2, "18446744073709551615"),
    ]);
    let mut client = client(address, None);
    assert_eq!(
        client
            .call(
                "getblock",
                &[RpcValue::from_string("hash"), RpcValue::unsigned(0)],
                false
            )
            .unwrap()
            .as_string()
            .unwrap(),
        hex
    );
    assert_eq!(
        client
            .call("getblockcount", &[], false)
            .unwrap()
            .as_u64()
            .unwrap(),
        u64::MAX
    );
    assert_eq!(rx.iter().count(), 2);
    handle.join().unwrap();
    assert_eq!(
        RpcValue::parse(r#"{"雪":"\ud83d\ude80","n":9007199254740993}"#)
            .unwrap()
            .field("雪")
            .unwrap()
            .unwrap()
            .as_string()
            .unwrap(),
        "🚀"
    );
    assert_eq!(
        RpcValue::string("a\n\"\\\t雪")
            .unwrap()
            .as_string()
            .unwrap(),
        "a\n\"\\\t雪"
    );
}

#[test]
fn json_bounds_and_ambiguous_or_malformed_envelopes_reject() {
    for text in [
        "",
        "[1,]",
        "{\"a\":1,}",
        "01",
        "+1",
        "NaN",
        "1 2",
        "truex",
        "\"\\ud800\"",
        "\"\\udc00\"",
        "\"\\x00\"",
        "{\"id\":1,\"\\u0069d\":2}",
    ] {
        assert!(RpcValue::parse(text).is_err(), "{text}");
    }
    let deep = format!(
        "{}0{}",
        "[".repeat(MAX_RPC_DEPTH + 1),
        "]".repeat(MAX_RPC_DEPTH + 1)
    );
    assert_eq!(RpcValue::parse(&deep), Err(RpcError::Capacity));
    assert_eq!(
        RpcValue::parse(&" ".repeat(MAX_RPC_BODY + 1)),
        Err(RpcError::Capacity)
    );
    assert_eq!(
        RpcValue::parse(&format!("[{}]", "0,".repeat(MAX_RPC_NODES))),
        Err(RpcError::Capacity)
    );
    let errors = [
        (reply(2, "null"), RpcError::WrongResponseId),
        (
            http(200, r#"{"result":0,"id":1,"id":1}"#),
            RpcError::InvalidResponse,
        ),
        (
            http(200, r#"{"id":1,"error":null}"#),
            RpcError::InvalidResponse,
        ),
        (http(401, "unauthorized"), RpcError::HttpStatus(401)),
    ];
    for (response, expected) in errors {
        let (address, rx, handle) = server(vec![response]);
        assert_eq!(
            client(address, None).call("getblockcount", &[], false),
            Err(expected)
        );
        rx.recv().unwrap();
        handle.join().unwrap();
    }
}

#[test]
fn remote_core_errors_remain_distinct_and_calls_are_not_retried() {
    let (address, rx, handle) = server(vec![http(
        500,
        r#"{"result":null,"error":{"code":-28,"message":"Loading block index"},"id":1}"#,
    )]);
    let error = client(address, None)
        .call("getblockcount", &[], false)
        .unwrap_err();
    assert_eq!(
        error,
        RpcError::Remote {
            code: -28,
            message: "Loading block index".into()
        }
    );
    assert_eq!(error.to_string(), "Core RPC error -28");
    assert_eq!(rx.iter().count(), 1);
    handle.join().unwrap();
}

#[test]
fn chunked_responses_work_and_framing_ambiguity_truncation_and_overflow_reject() {
    let body = r#"{"result":"ok","error":null,"id":1}"#;
    let chunked=format!("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{:x};test=yes\r\n{}\r\n0\r\nX-Test: yes\r\n\r\n",body.len(),body);
    let (address, rx, handle) = server(vec![chunked]);
    assert_eq!(
        client(address, None)
            .call("getblockcount", &[], false)
            .unwrap()
            .as_string()
            .unwrap(),
        "ok"
    );
    rx.recv().unwrap();
    handle.join().unwrap();
    for response in [
        "HTTP/1.1 200 OK\r\nContent-Length: 3\r\nContent-Length: 3\r\n\r\n{}".to_owned(),
        "HTTP/1.1 200 OK\r\nContent-Length: 3\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n"
            .into(),
        "HTTP/1.1 200 OK\r\nContent-Length: 10\r\n\r\n{}".into(),
        "HTTP/1.1 200 OK\r\nContent-Encoding: gzip\r\nContent-Length: 2\r\n\r\n{}".into(),
        "HTTP/1.1 200 OK\r\n\r\n{}".into(),
        format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
            MAX_RPC_BODY + 1
        ),
        "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nffffffffffffffff\r\n".into(),
        format!(
            "HTTP/1.1 200 OK\r\nX-Huge: {}\r\n\r\n",
            "a".repeat(MAX_RPC_HEADERS)
        ),
    ] {
        let (address, rx, handle) = server(vec![response]);
        assert!(client(address, None)
            .call("getblockcount", &[], false)
            .is_err());
        rx.recv().unwrap();
        handle.join().unwrap();
    }
}

#[test]
fn actual_slow_response_hits_absolute_deadline() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let handle = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        let mut bytes = [0; 1024];
        let _ = socket.read(&mut bytes);
        std::thread::sleep(Duration::from_millis(100));
        let _ = socket.write_all(reply(1, "0").as_bytes());
    });
    let mut client = RpcClient::new(
        address,
        RpcAuth::Basic {
            username: "u".into(),
            password: "p".into(),
        },
        None,
        Duration::from_millis(20),
        TradeLayerMainnet::Bitcoin,
    )
    .unwrap();
    assert_eq!(
        client.call("getblockcount", &[], false),
        Err(RpcError::Timeout)
    );
    handle.join().unwrap();
}

#[test]
fn network_readiness_binds_family_mainnet_and_genesis() {
    for network in [
        TradeLayerMainnet::Bitcoin,
        TradeLayerMainnet::Litecoin,
        TradeLayerMainnet::Dogecoin,
    ] {
        let version = match network {
            TradeLayerMainnet::Bitcoin => "/Satoshi:1(litecoin)/",
            TradeLayerMainnet::Litecoin => "/LitecoinCore:1(bitcoin)/",
            TradeLayerMainnet::Dogecoin => "/Shibetoshi:1(bitcoin)/",
        };
        let genesis = network
            .genesis()
            .0
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let (address, rx, handle) = server(vec![
            reply(1, &format!("{{\"subversion\":\"{version}\"}}")),
            reply(2, r#"{"chain":"main"}"#),
            reply(3, &format!("\"{genesis}\"")),
        ]);
        let mut client = RpcClient::new(
            address,
            RpcAuth::Basic {
                username: "u".into(),
                password: "p".into(),
            },
            None,
            Duration::from_secs(2),
            network,
        )
        .unwrap();
        client.verify_network().unwrap();
        assert_eq!(rx.iter().count(), 3);
        handle.join().unwrap();
    }
    for (responses, expected) in [
        (
            vec![reply(1, r#"{"subversion":"/Dogecoin/"}"#)],
            RpcError::WrongChain,
        ),
        (
            vec![
                reply(1, r#"{"subversion":"/Litecoin/"}"#),
                reply(2, r#"{"chain":"test"}"#),
            ],
            RpcError::WrongChain,
        ),
        (
            vec![
                reply(1, r#"{"subversion":"/Litecoin/"}"#),
                reply(2, r#"{"chain":"main"}"#),
                reply(3, "\"bad\""),
            ],
            RpcError::WrongGenesis,
        ),
    ] {
        let (address, rx, handle) = server(responses);
        assert_eq!(client(address, None).verify_network(), Err(expected));
        rx.iter().for_each(drop);
        handle.join().unwrap();
    }
}

#[test]
fn auth_debug_redacts_and_malformed_credentials_and_methods_fail_before_connect() {
    let address = "127.0.0.1:1".parse().unwrap();
    assert!(format!(
        "{:?}",
        RpcAuth::Basic {
            username: "alice".into(),
            password: "secret".into()
        }
    )
    .contains("redacted"));
    for (username, password) in [("a:b", "p"), ("u", "p\r\nInjected")] {
        assert!(RpcClient::new(
            address,
            RpcAuth::Basic {
                username: username.into(),
                password: password.into()
            },
            None,
            Duration::from_secs(1),
            TradeLayerMainnet::Bitcoin
        )
        .is_err());
    }
    for method in ["", "getblock\r\nInjected", "/wallet/x", "get block"] {
        assert_eq!(
            client(address, None).call(method, &[], false),
            Err(RpcError::InvalidMethod)
        );
    }
}

#[test]
fn bootstrap_matches_source_valid_domain_and_rejects_invalid_socket_numbers() {
    let env = BTreeMap::new();
    let boot = RpcBootstrap::from_env(&env, "LTCTEST").unwrap();
    assert_eq!(boot.source_chain, "LTCTEST");
    assert_eq!(boot.port, 19332);
    assert!(!boot.locked);
    let env = BTreeMap::from([
        ("CHAIN".into(), "btc".into()),
        ("RPC_PORT".into(), "0x208d".into()),
        ("RPC_WALLET".into(), "w/a".into()),
    ]);
    let boot = RpcBootstrap::from_env(&env, "LTC").unwrap();
    assert_eq!(boot.source_chain, "BTC");
    assert_eq!(boot.port, 8333);
    assert!(boot.locked);
    assert_eq!(boot.wallet.as_deref(), Some("w/a"));
    for port in ["-1", "NaN", "Infinity", "65536", "1.5", "0"] {
        assert!(
            RpcBootstrap::from_env(&BTreeMap::from([("RPC_PORT".into(), port.into())]), "LTC")
                .is_err()
        );
    }
}

trait TestString {
    fn from_string(text: &str) -> RpcValue;
}

#[test]
fn pinned_source_bootstrap_dispatch_and_chain_vectors() {
    let mut count = 0;
    for line in include_str!("../fixtures/client_vectors.tsv")
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
    {
        let parts = line.split('\t').collect::<Vec<_>>();
        assert_eq!(parts.len(), 3);
        let input = RpcValue::parse(parts[1]).unwrap();
        let expected = RpcValue::parse(parts[2]).unwrap();
        let field = |value: &RpcValue, key: &str| value.field(key).unwrap().unwrap();
        match parts[0] {
            "B" => {
                // The source environment fixtures are string-only objects.
                let env = field(&input, "env");
                let mut map = BTreeMap::new();
                for key in [
                    "CHAIN",
                    "RPC_HOST",
                    "RPC_PORT",
                    "TIMEOUT_MS",
                    "AUTODETECT",
                    "RPC_WALLET",
                    "LTC_WALLET",
                    "WALLET_NAME",
                ] {
                    if let Some(value) = env.field(key).unwrap() {
                        map.insert(key.to_owned(), value.as_string().unwrap());
                    }
                }
                let boot = RpcBootstrap::from_env(
                    &map,
                    &field(&input, "defaultChain").as_string().unwrap(),
                )
                .unwrap();
                assert_eq!(
                    boot.source_chain,
                    field(&expected, "chain").as_string().unwrap()
                );
                assert_eq!(boot.host, field(&expected, "host").as_string().unwrap());
                assert_eq!(
                    u64::from(boot.port),
                    field(&expected, "port").as_u64().unwrap()
                );
                assert_eq!(
                    boot.timeout.as_millis() as u64,
                    field(&expected, "timeout").as_u64().unwrap()
                );
                assert_eq!(
                    boot.locked.to_string(),
                    field(&expected, "locked").as_json()
                );
                let wallet = field(&expected, "wallet");
                assert_eq!(
                    boot.wallet.map(|w| format!("/wallet/{w}")),
                    if wallet.as_json() == "null" {
                        None
                    } else {
                        Some(wallet.as_string().unwrap())
                    }
                );
            }
            "R" => {
                use SourceRpcMethod::*;
                let wrapper = field(&input, "wrapper").as_string().unwrap();
                let method = match wrapper.as_str() {
                    "getBlockchainInfo" => GetBlockchainInfo,
                    "getRawTransaction" => GetRawTransaction,
                    "getnewaddress" => GetNewAddress,
                    "sendtoaddress" => SendToAddress,
                    "getNetworkInfo" => GetNetworkInfo,
                    "getTransaction" => GetTransaction,
                    "getBlock" => GetBlock,
                    "getBlockHash" => GetBlockHash,
                    "createRawTransaction" => CreateRawTransaction,
                    "listUnspent" => ListUnspent,
                    "decoderawtransaction" => DecodeRawTransaction,
                    "signrawtransactionwithwallet" => SignRawTransactionWithWallet,
                    "dumpprivkey" => DumpPrivKey,
                    "sendrawtransaction" => SendRawTransaction,
                    "validateAddress" => ValidateAddress,
                    "getBlockCount" => GetBlockCount,
                    "loadWallet" => LoadWallet,
                    "listlabels" => ListLabels,
                    "getaddressesbylabel" => GetAddressesByLabel,
                    "getwalletinfo" => GetWalletInfo,
                    "listunspent" => ListUnspentLower,
                    _ => panic!("unmapped source wrapper {wrapper}"),
                };
                assert_eq!(
                    method.name(),
                    field(&expected, "method").as_string().unwrap()
                );
                assert_eq!(
                    method
                        .source_params(&field(&input, "args").as_array().unwrap())
                        .unwrap(),
                    field(&expected, "params").as_array().unwrap()
                );
                let wallet = field(&input, "wallet");
                let actual = if method.wallet_scoped() && wallet.as_json() != "null" {
                    Some(format!("/wallet/{}", wallet.as_string().unwrap()))
                } else {
                    None
                };
                let path = field(&expected, "path");
                assert_eq!(
                    actual,
                    if path.as_json() == "null" {
                        None
                    } else {
                        Some(path.as_string().unwrap())
                    }
                );
            }
            "C" => {
                let found = determine_chain_from_subversion(&input.as_string().unwrap());
                let found = found.ok().map(|network| match network {
                    TradeLayerMainnet::Bitcoin => "BTC",
                    TradeLayerMainnet::Litecoin => "LTC",
                    TradeLayerMainnet::Dogecoin => "DOGE",
                });
                assert_eq!(
                    found,
                    if expected.as_json() == "null" {
                        None
                    } else {
                        Some(match expected.as_string().unwrap().as_str() {
                            "BTC" => "BTC",
                            "LTC" => "LTC",
                            "DOGE" => "DOGE",
                            _ => panic!(),
                        })
                    }
                );
            }
            "OBS" => assert_eq!(field(&expected, "isInitializing").as_json(), "true"),
            other => panic!("unexpected vector {other}"),
        }
        count += 1;
    }
    assert_eq!(count, 68);
}

#[test]
fn cookie_credentials_are_read_per_request_and_do_not_leak_in_debug() {
    let path = std::env::temp_dir().join(format!("tl-rpc-cookie-test-{}", std::process::id()));
    std::fs::write(&path, b"__cookie__:first\n").unwrap();
    let (address, rx, handle) = server(vec![reply(1, "0"), reply(2, "1")]);
    let mut client = RpcClient::new(
        address,
        RpcAuth::Cookie(path.clone()),
        None,
        Duration::from_secs(2),
        TradeLayerMainnet::Bitcoin,
    )
    .unwrap();
    client.call("getblockcount", &[], false).unwrap();
    assert!(rx
        .recv()
        .unwrap()
        .contains("Authorization: Basic X19jb29raWVfXzpmaXJzdA=="));
    std::fs::write(&path, b"__cookie__:second\r\n").unwrap();
    client.call("getblockcount", &[], false).unwrap();
    assert!(rx
        .recv()
        .unwrap()
        .contains("Authorization: Basic X19jb29raWVfXzpzZWNvbmQ="));
    assert!(!format!("{client:?}").contains("first"));
    handle.join().unwrap();
    std::fs::remove_file(path).unwrap();
}
impl TestString for RpcValue {
    fn from_string(text: &str) -> RpcValue {
        RpcValue::string(text).unwrap()
    }
}
