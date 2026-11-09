//! TradeLayer node for Bitcoin: observes blocks from a Bitcoin Core RPC,
//! admits the TradeLayer candidates of each block (`admission`), applies them
//! to the durable economic chain, rolls back on a reorg, and serves the
//! legacy listener HTTP API on loopback between blocks. See docs/NODE.md.
//!
//! Exit codes: 0 stopped cleanly (stop file or --max-blocks), 2 usage,
//! 3 config, 4 store (initialize, restore, stale head), 5 service (bind or
//! serve), 6 observer (quarantined, paused, store error), 7 admission or
//! alignment refusal (the block is not applied), 8 economic apply or rollback
//! failure, 9 head log write failure.
use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tradelayer_core::admission::{AdmissionNetwork, ParentOutputSource};
use tradelayer_core::admission_rpc::RpcPrevoutSource;
use tradelayer_core::bitcoin_inclusion::BitcoinPowProfile;
use tradelayer_core::bitcoin_witness_binding::BitcoinWitnessRules;
use tradelayer_core::bootstrap::BootstrapControl;
use tradelayer_core::chain::{BlockId, Tip};
use tradelayer_core::client::{RpcAuth, RpcClient, RpcValue};
use tradelayer_core::daemon::{
    Daemon, DaemonError, ObservationStore, ObserverConfig, ObserverMode, StepOutcome,
};
use tradelayer_core::economic_chain::{DurableEconomicChain, DurableEconomicError};
use tradelayer_core::listener_config::{
    head_token, parse_config, parse_expected_head, MAX_CONFIG_BYTES,
};
use tradelayer_core::network_profile::{TradeLayerMainnet, BITCOIN_TESTNET4_GENESIS};
use tradelayer_core::node_config::{
    apply_source_config, commitments_for_bootstrap, fresh_chain, parse_source_config,
    MAX_SOURCE_CONFIG_BYTES,
};
use tradelayer_core::node_ingest::{
    IngestError, NodeBinding, NodeIngest, NodeIngestConfig, PrevoutProvider, ZkIngest,
};
use tradelayer_core::node_runner::{
    head_log_sink, last_economic_head, observer_head_candidates, observer_head_sink, NodeError,
    NodeEvent, NodeRunner,
};
use tradelayer_core::node_service::{NodeHttpService, NodeStatus};
use tradelayer_core::runtime::{block_id_text, parse_block_id, RpcObservationClient};
use tradelayer_core::storage::Recovery;
use tradelayer_core::zk_verify::ZkNovaAdapter;

/// The proof verifier of `ZkVerifyV1`, used only when the source config enables the policy.
static ZK_VERIFIER: ZkNovaAdapter = ZkNovaAdapter;

const USAGE: &str = "tradelayer-node --root ABS_DIR --network BTC|BTC-TESTNET4 --rpc-address IP:PORT [--cookie-file PATH] --config ABS_FILE --source-config ABS_FILE --bind 127.0.0.1:PORT --stop-file ABS_PATH --head-log ABS_PATH (--initialize | [--economic-head-log ABS_PATH | --economic-expected-head SEQ:HEIGHT:HASH:COMMITMENT] [--resume]) [--anchor-height H --anchor-hash HASH] [--max-blocks N] [--rpc-timeout-ms MS] [--timeout-ms MS]\n       tradelayer-node --print-bootstrap-commitments --config ABS_FILE --collateral-seed-atoms N";
const FLAGS: [&str; 2] = ["--initialize", "--resume"];
const KNOWN: [&str; 17] = [
    "--root",
    "--network",
    "--rpc-address",
    "--cookie-file",
    "--config",
    "--source-config",
    "--bind",
    "--stop-file",
    "--head-log",
    "--economic-head-log",
    "--economic-expected-head",
    "--anchor-height",
    "--anchor-hash",
    "--max-blocks",
    "--rpc-timeout-ms",
    "--timeout-ms",
    "--collateral-seed-atoms",
];
const DEFAULT_RPC_TIMEOUT_MS: u64 = 5000;
const DEFAULT_SERVICE_TIMEOUT_MS: u64 = 2000;

enum Failure {
    Usage(String),
    Config(String),
    Store(String),
    Service(String),
    Observer(String),
    Ingest(String),
    Economic(String),
    HeadLog(String),
}
impl Failure {
    fn code(&self) -> i32 {
        match self {
            Self::Usage(_) => 2,
            Self::Config(_) => 3,
            Self::Store(_) => 4,
            Self::Service(_) => 5,
            Self::Observer(_) => 6,
            Self::Ingest(_) => 7,
            Self::Economic(_) => 8,
            Self::HeadLog(_) => 9,
        }
    }
    fn message(&self) -> &str {
        match self {
            Self::Usage(m)
            | Self::Config(m)
            | Self::Store(m)
            | Self::Service(m)
            | Self::Observer(m)
            | Self::Ingest(m)
            | Self::Economic(m)
            | Self::HeadLog(m) => m,
        }
    }
}
impl From<NodeError> for Failure {
    fn from(error: NodeError) -> Self {
        let text = error.to_string();
        match error {
            NodeError::Ingest(IngestError::Economic(_)) => Self::Economic(text),
            NodeError::Ingest(IngestError::Observer(_)) => Self::Observer(text),
            NodeError::Ingest(_) => Self::Ingest(text),
            NodeError::Service(_) => Self::Service(text),
            NodeError::Observer(_) | NodeError::Quarantined(_) | NodeError::NotObserving(_) => {
                Self::Observer(text)
            }
            NodeError::HeadLog(_) => Self::HeadLog(text),
        }
    }
}

fn main() {
    if let Err(failure) = run() {
        eprintln!("tradelayer-node: {}", failure.message());
        std::process::exit(failure.code());
    }
}

fn quote(text: &str) -> Result<String, Failure> {
    RpcValue::string(text)
        .map(|v| v.as_json().to_owned())
        .map_err(|_| Failure::Service("output text exceeds the JSON string limit".into()))
}
fn tip_text(tip: Tip) -> String {
    format!("{}:{}", tip.height, block_id_text(tip.id))
}
fn emit(line: &str) {
    use std::io::Write;
    println!("{line}");
    let _ = std::io::stdout().flush();
}

struct Args(BTreeMap<String, String>);
impl Args {
    fn parse(args: &[String]) -> Result<Self, Failure> {
        if args.len() > 40 || args.iter().any(|a| a.len() > 4096) {
            return Err(Failure::Usage("argument limit exceeded".into()));
        }
        let mut values = BTreeMap::new();
        let mut index = 0;
        while index < args.len() {
            let key = &args[index];
            let flag = FLAGS.contains(&key.as_str()) || key == "--print-bootstrap-commitments";
            if !flag && !KNOWN.contains(&key.as_str()) {
                return Err(Failure::Usage(format!("unknown option {key}; {USAGE}")));
            }
            let value = if flag {
                "true".to_owned()
            } else {
                index += 1;
                args.get(index)
                    .ok_or_else(|| Failure::Usage(format!("missing value for {key}")))?
                    .clone()
            };
            if values.insert(key.clone(), value).is_some() {
                return Err(Failure::Usage(format!("duplicate option {key}")));
            }
            index += 1;
        }
        Ok(Self(values))
    }
    fn has(&self, key: &str) -> bool {
        self.0.contains_key(key)
    }
    fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(String::as_str)
    }
    fn required(&self, key: &str) -> Result<&str, Failure> {
        self.get(key)
            .ok_or_else(|| Failure::Usage(format!("missing {key}; {USAGE}")))
    }
    fn absolute(&self, key: &str) -> Result<PathBuf, Failure> {
        let path = PathBuf::from(self.required(key)?);
        if !path.is_absolute() {
            return Err(Failure::Usage(format!("{key} must be an absolute path")));
        }
        Ok(path)
    }
    fn number(&self, key: &str, default: u64) -> Result<u64, Failure> {
        match self.get(key) {
            None => Ok(default),
            Some(text) => text
                .parse()
                .map_err(|_| Failure::Usage(format!("{key} must be an unsigned number"))),
        }
    }
}

fn read_config(path: &Path, limit: usize) -> Result<String, Failure> {
    let length = std::fs::metadata(path)
        .map_err(|e| Failure::Config(format!("config file {}: {e}", path.display())))?
        .len();
    if length > limit as u64 {
        return Err(Failure::Config(format!(
            "config file {} exceeds {limit} bytes",
            path.display()
        )));
    }
    std::fs::read_to_string(path)
        .map_err(|e| Failure::Config(format!("config file {}: {e}", path.display())))
}

struct RpcPrevouts {
    client: RpcClient,
}
impl PrevoutProvider for RpcPrevouts {
    fn parent_source(&mut self, block: BlockId) -> Result<Box<dyn ParentOutputSource>, String> {
        RpcPrevoutSource::fetch(&mut self.client, block)
            .map(|source| Box::new(source) as Box<dyn ParentOutputSource>)
            .map_err(|e| e.to_string())
    }
}

fn observer_dir(root: &Path, anchor: Tip) -> PathBuf {
    root.join(format!(
        "observer-{:012}-{}",
        anchor.height,
        block_id_text(anchor.id)
    ))
}
fn observer_config(testnet4: bool, anchor: Tip) -> Result<ObserverConfig, DaemonError> {
    if testnet4 {
        ObserverConfig::testnet4(anchor)
    } else {
        ObserverConfig::new(TradeLayerMainnet::Bitcoin, anchor)
    }
}
/// Observer generation directories under `root`, newest anchor first.
fn observer_generations(root: &Path) -> Result<Vec<Tip>, Failure> {
    let mut found = Vec::new();
    let entries = std::fs::read_dir(root)
        .map_err(|e| Failure::Store(format!("root {}: {e}", root.display())))?;
    for entry in entries {
        let entry = entry.map_err(|e| Failure::Store(format!("root: {e}")))?;
        let name = entry.file_name();
        let Some(rest) = name.to_str().and_then(|n| n.strip_prefix("observer-")) else {
            continue;
        };
        let Some((height, hash)) = rest.split_once('-') else {
            continue;
        };
        if let (Ok(height), Ok(id)) = (height.parse::<u64>(), parse_block_id(hash)) {
            found.push(Tip { height, id });
        }
    }
    found.sort_by(|a, b| b.height.cmp(&a.height));
    Ok(found)
}

fn run() -> Result<(), Failure> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help") {
        println!("{USAGE}");
        return Ok(());
    }
    let args = Args::parse(&args)?;
    let config_path = args.absolute("--config")?;
    let mut config = parse_config(&read_config(&config_path, MAX_CONFIG_BYTES)?)
        .map_err(|e| Failure::Config(e.to_string()))?;
    if config.economics.network != TradeLayerMainnet::Bitcoin {
        return Err(Failure::Config(
            "tradelayer-node serves the Bitcoin network only".into(),
        ));
    }

    if args.has("--print-bootstrap-commitments") {
        let seed: u128 = args
            .required("--collateral-seed-atoms")?
            .parse()
            .map_err(|_| {
                Failure::Usage("--collateral-seed-atoms must be an unsigned number".into())
            })?;
        let (ledger, catalogs) =
            commitments_for_bootstrap(&config, seed).map_err(|e| Failure::Config(e.to_string()))?;
        emit(&format!(
            "{{\"collateral_seed_atoms\":{seed},\"bootstrap_commitment\":\"{}\",\"catalog_bootstrap_commitment\":\"{}\"}}",
            block_id_text(BlockId(ledger)),
            block_id_text(BlockId(catalogs))
        ));
        return Ok(());
    }

    let source = parse_source_config(&read_config(
        &args.absolute("--source-config")?,
        MAX_SOURCE_CONFIG_BYTES,
    )?)
    .map_err(|e| Failure::Config(e.to_string()))?;
    apply_source_config(&mut config, &source).map_err(|e| Failure::Config(e.to_string()))?;

    let testnet4 = match args.required("--network")? {
        "BTC" => false,
        "BTC-TESTNET4" => true,
        _ => {
            return Err(Failure::Usage(
                "--network must be BTC or BTC-TESTNET4".into(),
            ))
        }
    };
    let rpc_address: SocketAddr = args
        .required("--rpc-address")?
        .parse()
        .map_err(|_| Failure::Usage("--rpc-address must be a literal IP:PORT".into()))?;
    let bind: SocketAddr = args
        .required("--bind")?
        .parse()
        .map_err(|_| Failure::Usage("--bind must be a literal IP:PORT".into()))?;
    if !bind.ip().is_loopback() {
        return Err(Failure::Usage(
            "--bind must be a loopback address; this service is unauthenticated".into(),
        ));
    }
    let root = args.absolute("--root")?;
    if !root.is_dir() {
        return Err(Failure::Usage(
            "--root must be an existing directory".into(),
        ));
    }
    let stop_file = args.absolute("--stop-file")?;
    if stop_file.exists() {
        return Err(Failure::Usage(
            "--stop-file already exists; remove it before starting".into(),
        ));
    }
    let head_log = args.absolute("--head-log")?;
    let economic_log = match args.get("--economic-head-log") {
        Some(_) => args.absolute("--economic-head-log")?,
        None => {
            let mut name = head_log.clone().into_os_string();
            name.push(".economic");
            PathBuf::from(name)
        }
    };
    let initialize = args.has("--initialize");
    if initialize && (args.has("--economic-expected-head") || args.has("--resume")) {
        return Err(Failure::Usage(
            "--initialize takes no restart options".into(),
        ));
    }
    let max_blocks = args.number("--max-blocks", 0)?;
    let rpc_timeout = args.number("--rpc-timeout-ms", DEFAULT_RPC_TIMEOUT_MS)?;
    let service_timeout = args.number("--timeout-ms", DEFAULT_SERVICE_TIMEOUT_MS)?;
    if !(1..=60_000).contains(&service_timeout) {
        return Err(Failure::Usage("--timeout-ms must be 1..=60000".into()));
    }

    // The base tip is the economic chain's anchor. It is fixed when the store
    // is created and recorded in `node-base`; a restart reads it and refuses a
    // different one.
    let base_file = root.join("node-base");
    let requested = if args.has("--anchor-height") || args.has("--anchor-hash") || initialize {
        let anchor_height = args.number("--anchor-height", 0)?;
        Some(Tip {
            height: anchor_height,
            id: match args.get("--anchor-hash") {
                Some(hash) => parse_block_id(hash)
                    .map_err(|_| Failure::Usage("--anchor-hash must be 64 hex digits".into()))?,
                None if anchor_height == 0 && testnet4 => BITCOIN_TESTNET4_GENESIS,
                None if anchor_height == 0 => TradeLayerMainnet::Bitcoin.genesis(),
                None => {
                    return Err(Failure::Usage(
                        "a nonzero anchor requires --anchor-hash".into(),
                    ))
                }
            },
        })
    } else {
        None
    };
    let base = if initialize {
        requested.expect("initialize always requests a base")
    } else {
        let text = std::fs::read_to_string(&base_file)
            .map_err(|e| Failure::Store(format!("base record {}: {e}", base_file.display())))?;
        let stored = text
            .trim()
            .split_once(':')
            .and_then(|(height, hash)| {
                Some(Tip {
                    height: height.parse().ok()?,
                    id: parse_block_id(hash).ok()?,
                })
            })
            .ok_or_else(|| Failure::Store("base record is malformed".into()))?;
        if requested.is_some_and(|asked| asked != stored) {
            return Err(Failure::Usage(
                "--anchor-height/--anchor-hash differ from the store's recorded base".into(),
            ));
        }
        stored
    };
    // Heights are absolute and the economy's height-0 anchor is the Bitcoin
    // mainnet genesis, so testnet4 runs from a nonzero base.
    if base.height == 0 && testnet4 {
        return Err(Failure::Usage(
            "BTC-TESTNET4 needs --anchor-height above 0 with its --anchor-hash:              the economy's height-0 anchor is the Bitcoin mainnet genesis"
                .into(),
        ));
    }
    config.economics.anchor = base;
    config
        .validate()
        .map_err(|e| Failure::Config(format!("anchor {}: {e}", tip_text(base))))?;

    let auth = || match args.get("--cookie-file") {
        Some(path) => RpcAuth::Cookie(PathBuf::from(path)),
        None => RpcAuth::from_env(
            &std::env::vars()
                .filter(|(key, _)| ["RPC_USER", "RPC_PASS"].contains(&key.as_str()))
                .collect(),
        ),
    };
    let client = |auth| {
        RpcClient::new(
            rpc_address,
            auth,
            None,
            Duration::from_millis(rpc_timeout),
            TradeLayerMainnet::Bitcoin,
        )
        .map_err(|e| Failure::Config(format!("rpc client: {e}")))
    };
    let observer_client = client(auth())?;
    let mut prevout_client = client(auth())?;
    let control = BootstrapControl::default();
    if testnet4 {
        prevout_client
            .verify_testnet4_controlled(&control)
            .map_err(|e| Failure::Observer(format!("node identity: {e}")))?;
    } else {
        prevout_client
            .verify_network_controlled(&control)
            .map_err(|e| Failure::Observer(format!("node identity: {e}")))?;
    }

    // Economic store.
    let economic_root = root.join("economic");
    let owner = if initialize {
        std::fs::create_dir_all(&economic_root)
            .map_err(|e| Failure::Store(format!("economic root: {e}")))?;
        let chain = fresh_chain(config.clone(), source.collateral_seed_atoms)
            .map_err(|e| Failure::Config(e.to_string()))?;
        let tip = chain.tip();
        let owner = DurableEconomicChain::initialize(&economic_root, chain, tip)
            .map_err(|e| Failure::Store(format!("cannot initialize economic store: {e}")))?;
        let mut sink = head_log_sink(&economic_log, true)
            .map_err(|e| Failure::HeadLog(format!("{}: {e}", economic_log.display())))?;
        sink(owner.head().map_err(|e| Failure::Store(e.to_string()))?)
            .map_err(|e| Failure::HeadLog(e.to_string()))?;
        owner
    } else {
        let expected = match args.get("--economic-expected-head") {
            Some(text) => parse_expected_head(text).map_err(|e| Failure::Usage(e.to_string()))?,
            None => last_economic_head(&economic_log).map_err(Failure::Store)?,
        };
        let (owner, _) = DurableEconomicChain::load(
            &economic_root,
            config.clone(),
            expected,
            Recovery::RejectIncompleteTail,
        )
        .map_err(|e| match e {
            DurableEconomicError::StaleHead => Failure::Store(
                "StaleHead: the economic store's head differs from the head log; \
                 a crash between a commit and its log line needs --economic-expected-head"
                    .into(),
            ),
            other => Failure::Store(format!("cannot restore economic store: {other}")),
        })?;
        owner
    };

    // Observer store: the newest generation that loads.
    let store = if initialize {
        let dir = observer_dir(&root, base);
        std::fs::create_dir_all(&dir).map_err(|e| Failure::Store(format!("observer root: {e}")))?;
        let sink = observer_head_sink(&head_log, true)
            .map_err(|e| Failure::HeadLog(format!("{}: {e}", head_log.display())))?;
        ObservationStore::initialize_recorded(
            &dir,
            observer_config(testnet4, base).map_err(|e| Failure::Config(e.to_string()))?,
            Some(sink),
        )
        .map_err(|e| Failure::Store(format!("cannot initialize observer store: {e}")))?
    } else {
        let candidates = observer_head_candidates(&head_log).map_err(Failure::Store)?;
        let mut loaded = None;
        let mut last_error = String::from("no observer store found under --root");
        'generations: for anchor in observer_generations(&root)? {
            let config =
                observer_config(testnet4, anchor).map_err(|e| Failure::Config(e.to_string()))?;
            for expected in &candidates {
                match ObservationStore::load(
                    &observer_dir(&root, anchor),
                    config.clone(),
                    *expected,
                    Recovery::RejectIncompleteTail,
                ) {
                    Ok((mut store, _)) => {
                        let sink = observer_head_sink(&head_log, false).map_err(|e| {
                            Failure::HeadLog(format!("{}: {e}", head_log.display()))
                        })?;
                        store.record_heads(sink);
                        loaded = Some(store);
                        break 'generations;
                    }
                    Err(error) => last_error = format!("observer store: {error}"),
                }
            }
        }
        loaded.ok_or(Failure::Store(last_error))?
    };

    if initialize {
        use std::io::Write;
        let record = format!("{}:{}\n", base.height, block_id_text(base.id));
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&base_file)
            .map_err(|e| Failure::Store(format!("base record {}: {e}", base_file.display())))?;
        file.write_all(record.as_bytes())
            .and_then(|()| file.sync_all())
            .map_err(|e| Failure::Store(format!("base record {}: {e}", base_file.display())))?;
    }

    let source_client = if testnet4 {
        RpcObservationClient::testnet4(observer_client, control.clone())
    } else {
        RpcObservationClient::new(observer_client, control.clone())
    };
    let mut daemon = Daemon::new(source_client, store);
    match daemon
        .store()
        .mode()
        .map_err(|e| Failure::Observer(e.to_string()))?
    {
        ObserverMode::Observing => {}
        ObserverMode::Stopped => {
            let head = daemon
                .head()
                .map_err(|e| Failure::Observer(e.to_string()))?;
            daemon
                .resume(head)
                .map_err(|e| Failure::Observer(format!("resume: {e}")))?;
        }
        // The runner resolves fork quarantines itself.
        ObserverMode::Quarantined(
            tradelayer_core::daemon::QuarantineReason::ForkDetected
            | tradelayer_core::daemon::QuarantineReason::CursorAboveTip,
        ) => {}
        mode => {
            if !args.has("--resume") {
                return Err(Failure::Observer(format!(
                    "observer mode is {mode:?}; clear its cause and start with --resume"
                )));
            }
            let head = daemon
                .head()
                .map_err(|e| Failure::Observer(e.to_string()))?;
            daemon
                .resume(head)
                .map_err(|e| Failure::Observer(format!("resume: {e}")))?;
        }
    }

    let ingest = NodeIngest::new(NodeIngestConfig {
        binding: NodeBinding {
            network: TradeLayerMainnet::Bitcoin,
            base,
        },
        admission_network: if testnet4 {
            AdmissionNetwork::Testnet
        } else {
            AdmissionNetwork::Mainnet
        },
        pow: BitcoinPowProfile::MainnetLike,
        rules: BitcoinWitnessRules::SegwitActive,
        mode: source.block_mode,
    });
    let zk_enabled = source.zk_verify.is_some();
    let ingest = if zk_enabled {
        ingest.with_zk(ZkIngest::new(&ZK_VERIFIER))
    } else {
        ingest
    };
    let service = NodeHttpService::new(
        owner,
        Some(bind),
        Duration::from_millis(service_timeout),
        NodeStatus::new(base),
    )
    .map_err(|e| Failure::Service(format!("cannot start service: {e}")))?;
    let local = service
        .local_addr()
        .map_err(|e| Failure::Service(format!("local address: {e}")))?
        .ok_or_else(|| Failure::Service("service has no listener".into()))?;
    let economic_head = service
        .economic_head()
        .map_err(|e| Failure::Service(format!("store head: {e}")))?;
    let economic_sink = head_log_sink(&economic_log, false)
        .map_err(|e| Failure::HeadLog(format!("{}: {e}", economic_log.display())))?;
    let rotation_root = root.clone();
    let rotation_log = head_log.clone();
    let mut runner = NodeRunner::new(
        daemon,
        service,
        RpcPrevouts {
            client: prevout_client,
        },
        ingest,
    )
    .with_head_sink(economic_sink)
    .with_rotation(Box::new(move |anchor| {
        let dir = observer_dir(&rotation_root, anchor);
        std::fs::create_dir_all(&dir).map_err(|_| DaemonError::InvalidConfig)?;
        let sink = observer_head_sink(&rotation_log, false).map_err(DaemonError::HeadRecord)?;
        ObservationStore::initialize_recorded(&dir, observer_config(testnet4, anchor)?, Some(sink))
    }));

    emit(&format!(
        "{{\"event\":\"listening\",\"addr\":{},\"network\":{},\"economic_head\":{},\"height_base\":{},\"admission_scope\":\"{}\",\"authenticated_admission\":false}}",
        quote(&local.to_string())?,
        quote(if testnet4 { "BTC-TESTNET4" } else { "BTC" })?,
        quote(&head_token(economic_head))?,
        quote(&tip_text(base))?,
        tradelayer_core::node_ingest::ADMISSION_SCOPE,
    ));

    let start = Instant::now();
    let mut admitted_this_run = 0u64;
    let mut reason = "stop-file";
    'main: loop {
        if stop_file.exists() {
            break;
        }
        if max_blocks > 0 && admitted_this_run >= max_blocks {
            reason = "max-blocks";
            break;
        }
        let now = u64::try_from(start.elapsed().as_millis())
            .map_err(|_| Failure::Observer("clock limit exceeded".into()))?;
        let events = runner.step(now, &control)?;
        let mut wait_until = None;
        for event in &events {
            match event {
                NodeEvent::Applied(report) => {
                    admitted_this_run += 1;
                    let head = runner
                        .service()
                        .economic_head()
                        .map_err(|e| Failure::Service(e.to_string()))?;
                    let refused = report
                        .refused
                        .iter()
                        .map(|r| {
                            Ok(format!(
                                "{{\"txid\":\"{}\",\"position\":{},\"reason\":{}}}",
                                r.txid,
                                r.position,
                                quote(&r.reason)?
                            ))
                        })
                        .collect::<Result<Vec<_>, Failure>>()?
                        .join(",");
                    // The ZK count appears only when the policy is on.
                    let zk_items = if zk_enabled {
                        format!(",\"zk_items\":{}", report.zk_items)
                    } else {
                        String::new()
                    };
                    emit(&format!(
                        "{{\"event\":\"block\",\"observed\":{},\"economic_height\":{},\"candidates\":{},\"authenticated\":{},\"refused\":{},\"unrepresentable\":{},\"submitted\":{}{zk_items},\"accepted\":{},\"rejected\":{},\"already_processed\":{},\"refusals\":[{refused}],\"economic_head\":{}}}",
                        quote(&tip_text(report.observed))?,
                        report.economic_height,
                        report.candidates,
                        report.authenticated,
                        report.refused_total,
                        report.unrepresentable,
                        report.submitted,
                        report.source.accepted,
                        report.source.rejected,
                        report.source.already_processed,
                        quote(&head_token(head))?
                    ));
                }
                NodeEvent::Reorganized { from, to } => emit(&format!(
                    "{{\"event\":\"reorganized\",\"from\":{},\"to\":{}}}",
                    quote(&tip_text(*from))?,
                    quote(&tip_text(*to))?
                )),
                NodeEvent::Rotated { anchor } => emit(&format!(
                    "{{\"event\":\"observer_rotated\",\"anchor\":{}}}",
                    quote(&tip_text(*anchor))?
                )),
                NodeEvent::Observer(StepOutcome::Waiting { until_ms }) => {
                    wait_until = Some(*until_ms);
                }
                NodeEvent::Observer(StepOutcome::Stopped) => break 'main,
                NodeEvent::Observer(_) => {}
            }
        }
        runner.serve(&control)?;
        // Between blocks: answer requests until the observer's next poll.
        if let Some(until) = wait_until {
            loop {
                if stop_file.exists() {
                    break 'main;
                }
                let now = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
                if now >= until {
                    break;
                }
                runner.serve(&control)?;
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }
    let status = runner.service().status();
    let head = runner
        .service()
        .economic_head()
        .map_err(|e| Failure::Service(e.to_string()))?;
    emit(&format!(
        "{{\"event\":\"stopped\",\"reason\":\"{reason}\",\"admitted_blocks\":{},\"economic_head\":{},\"observed_tip\":{},\"authenticated_admission\":{},\"uptimeMs\":{}}}",
        status.admitted_blocks,
        quote(&head_token(head))?,
        status
            .observed_tip
            .map_or_else(|| Ok("null".to_owned()), |t| quote(&tip_text(t)))?,
        status.authenticated_admission,
        start.elapsed().as_millis()
    ));
    Ok(())
}
