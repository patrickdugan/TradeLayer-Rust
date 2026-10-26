use std::collections::BTreeSet;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tradelayer_core::bootstrap::*;
use tradelayer_core::bootstrap_storage::*;
use tradelayer_core::client::{RpcAuth, RpcClient, RpcError};
use tradelayer_core::contract_registry::ContractId;
use tradelayer_core::economic::{EconomicConfig, SettlementPolicy};
use tradelayer_core::economic_chain::{
    DurableEconomicChain, DurableEconomicError, EconomicChain, EconomicChainConfig,
};
use tradelayer_core::ledger::{AccountId, Ledger, Operation, PropertyId, PropertyKind, TxId};
use tradelayer_core::network_profile::TradeLayerMainnet;
use tradelayer_core::runtime::RpcClientFactory;
use tradelayer_core::storage::{Journal, Namespace, Recovery, StorageError};
use tradelayer_core::tally::FeeRoute;

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "tl-runtime-{}-{}",
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
                    .starts_with("tl-runtime-")
        );
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn account(text: &str) -> AccountId {
    text.try_into().unwrap()
}
fn property() -> PropertyId {
    7.try_into().unwrap()
}
fn config(network: TradeLayerMainnet) -> EconomicChainConfig {
    EconomicChainConfig {
        economics: EconomicConfig {
            native_entitlements: None,
            insurance: None,
            source_consensus: None,
            network,
            contract: ContractId::try_from(1).unwrap(),
            collateral: property(),
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
        admin: account("admin"),
        permitted_spot_properties: BTreeSet::from([property()]),
        rollback_limit: 4,
    }
}
fn ledger() -> Ledger {
    let mut ledger = Ledger::new();
    ledger
        .apply(
            TxId::from_bytes([1; 32]),
            Operation::CreateProperty {
                id: property(),
                issuer: account("alice"),
                kind: PropertyKind::Fixed,
                initial_atoms: 10_000_000_000,
            },
        )
        .unwrap();
    ledger
}
fn mock_rpc(
    network: TradeLayerMainnet,
    replies: Vec<String>,
) -> (RpcClient, std::thread::JoinHandle<()>) {
    let (endpoint, handle) = mock_endpoint(replies);
    let client = RpcClient::new(
        endpoint,
        RpcAuth::Basic {
            username: "u".into(),
            password: "p".into(),
        },
        None,
        Duration::from_secs(2),
        network,
    )
    .unwrap();
    (client, handle)
}
fn mock_endpoint(replies: Vec<String>) -> (std::net::SocketAddr, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let handle = std::thread::spawn(move || {
        for (index, result) in replies.into_iter().enumerate() {
            let deadline = Instant::now() + Duration::from_secs(5);
            let (mut socket, _) = loop {
                match listener.accept() {
                    Ok(value) => break value,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(Instant::now() < deadline, "expected RPC did not arrive");
                        std::thread::sleep(Duration::from_millis(2));
                    }
                    Err(error) => panic!("mock accept: {error}"),
                }
            };
            socket.set_nonblocking(false).unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut data = Vec::new();
            loop {
                let mut buf = [0u8; 4096];
                let count = socket.read(&mut buf).unwrap();
                assert!(count > 0);
                data.extend_from_slice(&buf[..count]);
                if let Some(end) = data.windows(4).position(|w| w == b"\r\n\r\n") {
                    let header = std::str::from_utf8(&data[..end]).unwrap();
                    let length: usize = header
                        .lines()
                        .find_map(|line| line.strip_prefix("Content-Length: "))
                        .unwrap()
                        .parse()
                        .unwrap();
                    if data.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            let body = format!(
                "{{\"result\":{result},\"error\":null,\"id\":{}}}",
                index + 1
            );
            socket
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
                        body.len()
                    )
                    .as_bytes(),
                )
                .unwrap();
        }
    });
    (endpoint, handle)
}
fn identity_replies(network: TradeLayerMainnet) -> Vec<String> {
    let version = match network {
        TradeLayerMainnet::Bitcoin => "/Satoshi:1/",
        TradeLayerMainnet::Litecoin => "/LitecoinCore:1/",
        TradeLayerMainnet::Dogecoin => "/Shibetoshi:1/",
    };
    let genesis = network
        .genesis()
        .0
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    vec![
        format!("{{\"subversion\":\"{version}\"}}"),
        "{\"chain\":\"main\"}".into(),
        format!("\"{genesis}\""),
    ]
}

#[test]
fn real_rpc_bootstrap_restores_the_sole_economic_owner_on_all_mainnets() {
    for network in [
        TradeLayerMainnet::Bitcoin,
        TradeLayerMainnet::Litecoin,
        TradeLayerMainnet::Dogecoin,
    ] {
        let directory = Directory::new();
        let cfg = config(network);
        let chain = EconomicChain::new_unverified(cfg.clone(), ledger()).unwrap();
        let owner = DurableEconomicChain::initialize(&directory.0, chain, cfg.anchor()).unwrap();
        let expected = owner.head().unwrap();
        drop(owner);
        let bootstrap = BootstrapConfig::new(network, directory.0.clone(), "clearing").unwrap();
        let (client, handle) = mock_rpc(network, identity_replies(network));
        let mut clients = RpcClientFactory::new(client);
        let mut stores = EconomicStoreFactory::new(EconomicRestoreConfig {
            chain: cfg.clone(),
            expected_head: expected,
            recovery: Recovery::RejectIncompleteTail,
        });
        let ready = BootstrapCoordinator::default()
            .start(
                bootstrap.clone(),
                &mut clients,
                &mut stores,
                &BootstrapControl::default(),
            )
            .unwrap();
        assert_eq!(ready.state().head().unwrap(), expected);
        assert_eq!(
            ready
                .state()
                .chain()
                .unwrap()
                .state()
                .ledger()
                .available_atoms(property(), &account("alice")),
            10_000_000_000
        );
        assert!(matches!(
            Journal::open_existing(
                &directory.0,
                Namespace::for_mainnet(network, "clearing").unwrap(),
                Recovery::RejectIncompleteTail
            ),
            Err(StorageError::Locked)
        ));
        assert!(clients
            .acquire(&bootstrap, &BootstrapControl::default())
            .is_err());
        drop(ready);
        handle.join().unwrap();
        let (reopened, _) =
            DurableEconomicChain::load(&directory.0, cfg, expected, Recovery::RejectIncompleteTail)
                .unwrap();
        assert_eq!(reopened.head().unwrap(), expected);
    }
}

#[test]
fn restore_mismatch_missing_store_and_wrong_scope_never_initialize_a_chain() {
    let directory = Directory::new();
    let network = TradeLayerMainnet::Bitcoin;
    let cfg = config(network);
    let owner = DurableEconomicChain::initialize(
        &directory.0,
        EconomicChain::new_unverified(cfg.clone(), ledger()).unwrap(),
        cfg.anchor(),
    )
    .unwrap();
    let expected = owner.head().unwrap();
    drop(owner);
    let bootstrap = BootstrapConfig::new(network, directory.0.clone(), "clearing").unwrap();
    let restore = EconomicRestoreConfig {
        chain: cfg.clone(),
        expected_head: expected,
        recovery: Recovery::RejectIncompleteTail,
    };
    let factory = EconomicStoreFactory::new(restore.clone());
    let wrong = BootstrapConfig::new(network, directory.0.clone(), "persistence").unwrap();
    assert!(matches!(
        factory.open_checked(&wrong, &BootstrapControl::default()),
        Err(RestoreAdapterError::ScopeMismatch)
    ));
    let mut altered = restore;
    altered.expected_head.sequence += 1;
    let mut store = EconomicStoreFactory::new(altered)
        .open_checked(&bootstrap, &BootstrapControl::default())
        .unwrap();
    assert!(matches!(
        store.restore_checked(&bootstrap, &BootstrapControl::default()),
        Err(RestoreAdapterError::Economic(
            DurableEconomicError::StaleHead
        ))
    ));
    assert!(matches!(
        store.restore_checked(&bootstrap, &BootstrapControl::default()),
        Err(RestoreAdapterError::AlreadyAttempted)
    ));
    let empty = Directory::new();
    let missing = BootstrapConfig::new(network, empty.0.clone(), "clearing").unwrap();
    let mut store = factory
        .open_checked(&missing, &BootstrapControl::default())
        .unwrap();
    assert!(store
        .restore_checked(&missing, &BootstrapControl::default())
        .is_err());
    assert!(!missing.namespace().path(&empty.0).exists());
    DurableEconomicChain::load(&directory.0, cfg, expected, Recovery::RejectIncompleteTail)
        .unwrap();
}

#[test]
fn cancelled_or_expired_control_stops_before_rpc_connection_or_store_open() {
    let mut client = RpcClient::new(
        "127.0.0.1:1".parse().unwrap(),
        RpcAuth::Basic {
            username: "u".into(),
            password: "p".into(),
        },
        None,
        Duration::from_secs(2),
        TradeLayerMainnet::Bitcoin,
    )
    .unwrap();
    let cancelled = BootstrapControl::default();
    cancelled.cancel();
    assert_eq!(
        client.verify_network_controlled(&cancelled),
        Err(RpcError::Cancelled)
    );
    let expired = BootstrapControl::new(Some(Instant::now() - Duration::from_millis(1)));
    assert_eq!(
        client.verify_network_controlled(&expired),
        Err(RpcError::Timeout)
    );
    let directory = Directory::new();
    let cfg = config(TradeLayerMainnet::Bitcoin);
    let bootstrap =
        BootstrapConfig::new(TradeLayerMainnet::Bitcoin, directory.0.clone(), "clearing").unwrap();
    let mut factory = RpcClientFactory::new(client);
    assert!(factory.acquire(&bootstrap, &cancelled).is_err());
    assert!(factory
        .acquire(&bootstrap, &BootstrapControl::default())
        .is_ok());
    assert!(!bootstrap.namespace().path(&directory.0).exists());
    assert!(cfg.validate().is_ok());
}

#[test]
fn loopback_raw_observation_commits_while_the_attached_economic_head_stays_fixed() {
    use tradelayer_core::chain::{BlockId, Tip};
    use tradelayer_core::daemon::{ObservationStore, ObserverConfig, StepOutcome};
    use tradelayer_core::runtime::{ObservedEconomicRuntime, RpcObservationClient};
    let directory = Directory::new();
    let network = TradeLayerMainnet::Bitcoin;
    let cfg = config(network);
    let economic = DurableEconomicChain::initialize(
        &directory.0,
        EconomicChain::new_unverified(cfg.clone(), ledger()).unwrap(),
        cfg.anchor(),
    )
    .unwrap();
    let economic_head = economic.head().unwrap();
    let header = include_str!("../fixtures/bitcoin_mainnet_genesis_block1_headers.txt")
        .lines()
        .nth(1)
        .unwrap();
    let raw = format!(
        "{header}01{}",
        include_str!("../fixtures/bitcoin_mainnet_block1_coinbase.hex").trim()
    );
    let bytes = tradelayer_core::read_only::decode_hex(&raw).unwrap();
    let mut hash = tradelayer_core::raw_block_ingest::UnverifiedRawBlockView::parse(&bytes, None)
        .unwrap()
        .header_hash_raw();
    hash.reverse();
    let id = BlockId(hash);
    let text = tradelayer_core::runtime::block_id_text(id);
    let genesis = tradelayer_core::runtime::block_id_text(network.genesis());
    let mut replies = identity_replies(network);
    replies.extend([
        format!("{{\"blocks\":1,\"bestblockhash\":\"{text}\"}}"),
        format!("\"{genesis}\""),
        format!("\"{text}\""),
        format!("\"{raw}\""),
        format!("\"{text}\""),
    ]);
    let (client, handle) = mock_rpc(network, replies);
    let control = BootstrapControl::default();
    let observer = ObservationStore::initialize(
        &directory.0,
        ObserverConfig::new(
            network,
            Tip {
                height: 0,
                id: network.genesis(),
            },
        )
        .unwrap(),
    )
    .unwrap();
    let mut runtime = ObservedEconomicRuntime::new(
        RpcObservationClient::new(client, control.clone()),
        observer,
        economic,
    )
    .unwrap();
    let mut committed = false;
    for now in 0..20 {
        if matches!(
            runtime.step(now, &control).unwrap(),
            StepOutcome::Recorded { .. }
        ) {
            committed = true;
            break;
        }
    }
    assert!(committed);
    handle.join().unwrap();
    assert_eq!(runtime.economic_head().unwrap(), economic_head);
    assert_eq!(
        runtime.economic_chain().unwrap().state().finalized_height(),
        None
    );
    assert_eq!(
        runtime.observation_head().unwrap().tip,
        Tip { height: 1, id }
    );
    assert_eq!(
        runtime.observation_store().unwrap().records().unwrap()[0].raw(),
        bytes
    );
    assert_eq!(
        runtime
            .economic_chain()
            .unwrap()
            .state()
            .ledger()
            .available_atoms(property(), &account("alice")),
        10_000_000_000
    );
}

#[test]
fn cancellation_arriving_during_the_first_rpc_prevents_later_identity_requests() {
    let control = BootstrapControl::default();
    let cancel = control.clone();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let client = RpcClient::new(
        listener.local_addr().unwrap(),
        RpcAuth::Basic {
            username: "u".into(),
            password: "p".into(),
        },
        None,
        Duration::from_secs(2),
        TradeLayerMainnet::Bitcoin,
    )
    .unwrap();
    let handle = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        let mut data = [0u8; 2048];
        assert!(socket.read(&mut data).unwrap() > 0);
        cancel.cancel();
        let body = r#"{"id":1,"result":{"subversion":"/Satoshi:1/"},"error":null}"#;
        let _ = socket.write_all(
            format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            )
            .as_bytes(),
        );
    });
    use tradelayer_core::chain::Tip;
    use tradelayer_core::daemon::{ObservationStore, ObserverConfig, ObserverMode, StepOutcome};
    use tradelayer_core::runtime::{ObservedEconomicRuntime, RpcObservationClient};
    let directory = Directory::new();
    let network = TradeLayerMainnet::Bitcoin;
    let cfg = config(network);
    let economic = DurableEconomicChain::initialize(
        &directory.0,
        EconomicChain::new_unverified(cfg.clone(), ledger()).unwrap(),
        cfg.anchor(),
    )
    .unwrap();
    let observer = ObservationStore::initialize(
        &directory.0,
        ObserverConfig::new(
            network,
            Tip {
                height: 0,
                id: network.genesis(),
            },
        )
        .unwrap(),
    )
    .unwrap();
    let mut runtime = ObservedEconomicRuntime::new(
        RpcObservationClient::new(client, control.clone()),
        observer,
        economic,
    )
    .unwrap();
    let economic_head = runtime.economic_head().unwrap();
    assert_eq!(runtime.step(0, &control).unwrap(), StepOutcome::Stopped);
    assert_eq!(runtime.status().unwrap().mode, ObserverMode::Stopped);
    assert!(!runtime.status().unwrap().identity_checked);
    assert!(runtime
        .observation_store()
        .unwrap()
        .records()
        .unwrap()
        .is_empty());
    assert_eq!(runtime.economic_head().unwrap(), economic_head);
    handle.join().unwrap();
}

fn observe_cli(root: &std::path::Path, endpoint: &str, tail: &[&str]) -> std::process::Output {
    std::process::Command::new(env!("CARGO_BIN_EXE_tl_observe"))
        .arg("--root")
        .arg(root)
        .args([
            "--network",
            "BTC",
            "--rpc-address",
            endpoint,
            "--rpc-timeout-ms",
            "300",
        ])
        .args(tail)
        .env("RPC_USER", "test")
        .env("RPC_PASS", "private-cli-password")
        .output()
        .unwrap()
}
fn cli_status(output: &std::process::Output) -> tradelayer_core::client::RpcValue {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    tradelayer_core::client::RpcValue::parse(std::str::from_utf8(&output.stdout).unwrap().trim())
        .unwrap()
}
fn status_text(value: &tradelayer_core::client::RpcValue, field: &str) -> String {
    value.field(field).unwrap().unwrap().as_string().unwrap()
}
#[test]
fn cli_exact_restart_tokens_control_modes_and_cooperative_shutdown_are_durable() {
    use tradelayer_core::chain::Tip;
    use tradelayer_core::daemon::{
        ObservationHead, ObservationStore, ObserverConfig, ObserverMode,
    };
    use tradelayer_core::runtime::parse_block_id;
    let directory = Directory::new();
    let (endpoint, server) = mock_endpoint(identity_replies(TradeLayerMainnet::Bitcoin));
    let output = observe_cli(
        &directory.0,
        &endpoint.to_string(),
        &["--initialize", "--steps", "1"],
    );
    let initial = cli_status(&output);
    server.join().unwrap();
    assert_eq!(
        initial
            .field("identity_checked")
            .unwrap()
            .unwrap()
            .as_json(),
        "true"
    );
    assert_eq!(
        initial
            .field("authenticated_admission")
            .unwrap()
            .unwrap()
            .as_json(),
        "false"
    );
    let initial_head = status_text(&initial, "head");
    let paused = cli_status(&observe_cli(
        &directory.0,
        "127.0.0.1:1",
        &["--expected-head", &initial_head, "--pause"],
    ));
    assert_eq!(status_text(&paused, "mode"), "Paused");
    let paused_head = status_text(&paused, "head");
    assert_ne!(paused_head, initial_head);
    let stale = observe_cli(
        &directory.0,
        "127.0.0.1:1",
        &["--expected-head", &initial_head, "--steps", "0"],
    );
    assert!(!stale.status.success());
    assert!(String::from_utf8_lossy(&stale.stderr).contains("StaleHead"));
    let resumed = cli_status(&observe_cli(
        &directory.0,
        "127.0.0.1:1",
        &["--expected-head", &paused_head, "--resume"],
    ));
    assert_eq!(status_text(&resumed, "mode"), "Observing");
    let resume_head = status_text(&resumed, "head");
    let stop_file = directory.0.join("operator-stop");
    std::fs::write(&stop_file, b"stop").unwrap();
    let stopped = cli_status(&observe_cli(
        &directory.0,
        "127.0.0.1:1",
        &[
            "--expected-head",
            &resume_head,
            "--run",
            "--stop-file",
            stop_file.to_str().unwrap(),
        ],
    ));
    assert_eq!(status_text(&stopped, "mode"), "Stopped");
    assert_eq!(
        stopped
            .field("stored_blocks")
            .unwrap()
            .unwrap()
            .as_u64()
            .unwrap(),
        0
    );
    let token = status_text(&stopped, "head");
    let fields: Vec<_> = token.split(':').collect();
    let head = ObservationHead {
        sequence: fields[0].parse().unwrap(),
        tip: Tip {
            height: fields[1].parse().unwrap(),
            id: parse_block_id(fields[2]).unwrap(),
        },
        commitment: parse_block_id(fields[3]).unwrap().0,
    };
    let cfg = ObserverConfig::new(
        TradeLayerMainnet::Bitcoin,
        Tip {
            height: 0,
            id: TradeLayerMainnet::Bitcoin.genesis(),
        },
    )
    .unwrap();
    let (store, _) =
        ObservationStore::load(&directory.0, cfg, head, Recovery::RejectIncompleteTail).unwrap();
    assert_eq!(store.mode().unwrap(), ObserverMode::Stopped);
    assert_eq!(store.head().unwrap(), head);
    assert!(
        !Namespace::for_mainnet(TradeLayerMainnet::Bitcoin, "clearing")
            .unwrap()
            .path(&directory.0)
            .exists()
    );
}
#[test]
fn cli_invalid_arguments_never_create_a_store_or_expose_credentials() {
    let directory = Directory::new();
    for tail in [
        vec!["--initialize", "--reconcile", "invalid"],
        vec!["--initialize", "--reconcile", "3:not-a-hash"],
        vec!["--initialize", "--resume"],
        vec!["--initialize", "--run"],
        vec!["--initialize", "--steps", "1000001"],
        vec!["--initialize", "--workers", "0"],
        vec!["--initialize", "--initialize"],
        vec!["--expected-head", "invalid"],
    ] {
        let output = observe_cli(&directory.0, "127.0.0.1:1", &tail);
        assert!(!output.status.success(), "{tail:?}");
        assert!(
            std::fs::read_dir(&directory.0).unwrap().next().is_none(),
            "{tail:?}"
        );
        assert!(!String::from_utf8_lossy(&output.stderr).contains("private-cli-password"));
    }
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_tl_observe"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("--expected-head"));
}

#[test]
fn cancellation_during_a_blocked_read_is_reported_after_the_bounded_socket_returns() {
    for verify in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let control = BootstrapControl::default();
        let cancel = control.clone();
        let mut client = RpcClient::new(
            listener.local_addr().unwrap(),
            RpcAuth::Basic {
                username: "u".into(),
                password: "p".into(),
            },
            None,
            Duration::from_millis(150),
            TradeLayerMainnet::Bitcoin,
        )
        .unwrap();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut data = [0; 4096];
            assert!(socket.read(&mut data).unwrap() > 0);
            std::thread::sleep(Duration::from_millis(20));
            cancel.cancel();
            // Withhold the entire reply until the bounded client closes.
            while socket.read(&mut data).unwrap() != 0 {}
        });
        let started = Instant::now();
        let result = if verify {
            client.verify_network_controlled(&control)
        } else {
            client
                .call_controlled("getblockcount", &[], false, &control)
                .map(|_| ())
        };
        assert_eq!(result, Err(RpcError::Cancelled));
        assert!(started.elapsed() < Duration::from_secs(2));
        server.join().unwrap();
    }
}
