//! Loopback listener HTTP service over one restored economic store.
//!
//! Serves the library's `EconomicHttpService` (the legacy tradelayer.js
//! listener routes) on a loopback address until a stop file appears. It never
//! applies chain data: authenticated admission is false and the store is served
//! exactly as restored. See docs/LISTENERD.md.
//!
//! Exit codes: 0 stopped cleanly, 2 usage, 3 config, 4 store, 5 runtime.
use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tradelayer_core::bootstrap::BootstrapControl;
use tradelayer_core::client::RpcValue;
use tradelayer_core::economic_chain::DurableEconomicChain;
use tradelayer_core::http_service::ServeOutcome;
use tradelayer_core::listener_config::{
    head_token, parse_config, parse_expected_head, render_config, MAX_CONFIG_BYTES,
};
use tradelayer_core::listener_demo::{seed, DEMO_LABEL};
use tradelayer_core::service::ServiceError;
use tradelayer_core::service_runtime::EconomicHttpService;
use tradelayer_core::storage::Recovery;

const USAGE: &str = "tradelayer-listenerd --root ABS_DIR --bind 127.0.0.1:PORT --expected-head SEQ:HEIGHT:HASH:COMMITMENT --config ABS_FILE --stop-file ABS_PATH [--timeout-ms MS]\n       tradelayer-listenerd --demo-seed ABS_DIR [--config ABS_FILE]";
const DEFAULT_TIMEOUT_MS: u64 = 2000;
const DEMO_CONFIG_NAME: &str = "listenerd.conf";

enum Failure {
    Usage(String),
    Config(String),
    Store(String),
    Runtime(String),
}
impl Failure {
    fn code(&self) -> i32 {
        match self {
            Self::Usage(_) => 2,
            Self::Config(_) => 3,
            Self::Store(_) => 4,
            Self::Runtime(_) => 5,
        }
    }
    fn message(&self) -> &str {
        match self {
            Self::Usage(m) | Self::Config(m) | Self::Store(m) | Self::Runtime(m) => m,
        }
    }
}

fn main() {
    if let Err(failure) = run() {
        eprintln!("tradelayer-listenerd: {}", failure.message());
        std::process::exit(failure.code());
    }
}

fn run() -> Result<(), Failure> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help") {
        println!("{USAGE}");
        return Ok(());
    }
    let values = parse_args(&args)?;
    if values.contains_key("--demo-seed") {
        for key in values.keys() {
            if !["--demo-seed", "--config"].contains(&key.as_str()) {
                return Err(Failure::Usage(format!(
                    "{key} is not valid with --demo-seed; {USAGE}"
                )));
            }
        }
        return demo(&values);
    }
    serve(&values)
}

fn parse_args(args: &[String]) -> Result<BTreeMap<String, String>, Failure> {
    const KNOWN: [&str; 7] = [
        "--root",
        "--bind",
        "--expected-head",
        "--config",
        "--stop-file",
        "--timeout-ms",
        "--demo-seed",
    ];
    if args.len() > 14 || args.iter().any(|a| a.len() > 4096) {
        return Err(Failure::Usage("argument limit exceeded".into()));
    }
    let mut values = BTreeMap::new();
    let mut index = 0;
    while index < args.len() {
        let key = &args[index];
        if !KNOWN.contains(&key.as_str()) {
            return Err(Failure::Usage(format!("unknown option {key}; {USAGE}")));
        }
        index += 1;
        let value = args
            .get(index)
            .ok_or_else(|| Failure::Usage(format!("missing value for {key}")))?;
        if values.insert(key.clone(), value.clone()).is_some() {
            return Err(Failure::Usage(format!("duplicate option {key}")));
        }
        index += 1;
    }
    Ok(values)
}

fn absolute(values: &BTreeMap<String, String>, key: &str) -> Result<PathBuf, Failure> {
    let path = PathBuf::from(
        values
            .get(key)
            .ok_or_else(|| Failure::Usage(format!("missing {key}; {USAGE}")))?,
    );
    if !path.is_absolute() {
        return Err(Failure::Usage(format!("{key} must be an absolute path")));
    }
    Ok(path)
}

fn quote(text: &str) -> Result<String, Failure> {
    RpcValue::string(text)
        .map(|v| v.as_json().to_owned())
        .map_err(|_| Failure::Runtime("output text exceeds the JSON string limit".into()))
}

fn demo(values: &BTreeMap<String, String>) -> Result<(), Failure> {
    let root = absolute(values, "--demo-seed")?;
    let config_path = match values.get("--config") {
        Some(_) => absolute(values, "--config")?,
        None => root.join(DEMO_CONFIG_NAME),
    };
    if config_path.exists() {
        return Err(Failure::Usage("demo config file already exists".into()));
    }
    let seeded = seed(&root).map_err(Failure::Store)?;
    let text = render_config(&seeded.config).map_err(|e| Failure::Config(e.to_string()))?;
    let text =
        format!("# {DEMO_LABEL}\n# Serve with the --expected-head printed by --demo-seed.\n{text}");
    write_new(&config_path, text.as_bytes())?;
    println!(
        "{{\"demo\":true,\"label\":{},\"root\":{},\"config\":{},\"expectedHead\":{},\"blocks\":{},\"fills\":{},\"authenticated_admission\":false}}",
        quote(DEMO_LABEL)?,
        quote(&root.to_string_lossy())?,
        quote(&config_path.to_string_lossy())?,
        quote(&head_token(seeded.head))?,
        seeded.blocks,
        seeded.fills
    );
    Ok(())
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), Failure> {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| Failure::Config(format!("config file {}: {e}", path.display())))?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|e| Failure::Config(format!("config file {}: {e}", path.display())))
}

fn serve(values: &BTreeMap<String, String>) -> Result<(), Failure> {
    let bind: SocketAddr = values
        .get("--bind")
        .ok_or_else(|| Failure::Usage(format!("missing --bind; {USAGE}")))?
        .parse()
        .map_err(|_| Failure::Usage("--bind must be a literal IP:PORT".into()))?;
    if !bind.ip().is_loopback() {
        return Err(Failure::Usage(
            "--bind must be a loopback address; this service is unauthenticated".into(),
        ));
    }
    let timeout_ms = match values.get("--timeout-ms") {
        None => DEFAULT_TIMEOUT_MS,
        Some(text) => text
            .parse::<u64>()
            .ok()
            .filter(|ms| (1..=60_000).contains(ms))
            .ok_or_else(|| Failure::Usage("--timeout-ms must be 1..=60000".into()))?,
    };
    let root = absolute(values, "--root")?;
    if !root.is_dir() {
        return Err(Failure::Usage(
            "--root must be an existing directory".into(),
        ));
    }
    let stop_file = absolute(values, "--stop-file")?;
    if stop_file.exists() {
        return Err(Failure::Usage(
            "--stop-file already exists; remove it before starting".into(),
        ));
    }
    let config_path = absolute(values, "--config")?;
    let config_len = std::fs::metadata(&config_path)
        .map_err(|e| Failure::Config(format!("config file {}: {e}", config_path.display())))?
        .len();
    if config_len > MAX_CONFIG_BYTES as u64 {
        return Err(Failure::Config(format!(
            "config file exceeds {MAX_CONFIG_BYTES} bytes"
        )));
    }
    let config_text = std::fs::read_to_string(&config_path)
        .map_err(|e| Failure::Config(format!("config file {}: {e}", config_path.display())))?;
    let config = parse_config(&config_text).map_err(|e| Failure::Config(e.to_string()))?;
    let expected = parse_expected_head(
        values
            .get("--expected-head")
            .ok_or_else(|| Failure::Usage(format!("missing --expected-head; {USAGE}")))?,
    )
    .map_err(|e| Failure::Usage(e.to_string()))?;
    let (owner, _) =
        DurableEconomicChain::load(&root, config, expected, Recovery::RejectIncompleteTail)
            .map_err(|e| Failure::Store(format!("cannot restore store: {e}")))?;
    let service = EconomicHttpService::bind(owner, bind, Duration::from_millis(timeout_ms))
        .map_err(|e| Failure::Runtime(format!("cannot start service: {e}")))?;
    let local = service
        .local_addr()
        .map_err(|e| Failure::Runtime(format!("local address: {e}")))?;
    let head = service
        .economic_head()
        .map_err(|e| Failure::Runtime(format!("store head: {e}")))?;
    println!(
        "{{\"event\":\"listening\",\"addr\":{},\"head\":{},\"authenticated_admission\":false}}",
        quote(&local.to_string())?,
        quote(&head_token(head))?
    );
    flush_stdout();

    let control = BootstrapControl::default();
    let started = Instant::now();
    let (mut served, mut dropped) = (0u64, 0u64);
    loop {
        if stop_file.exists() {
            break;
        }
        match service.serve_one(&control) {
            Ok(ServeOutcome::Idle) => std::thread::sleep(Duration::from_millis(2)),
            Ok(ServeOutcome::Responded { .. }) => served += 1,
            Ok(ServeOutcome::Stopped) => break,
            // One bad connection (reset, timeout, malformed framing) does not stop the service.
            Err(
                ServiceError::Transport(_)
                | ServiceError::Timeout
                | ServiceError::InvalidHttp
                | ServiceError::Cancelled,
            ) => {
                dropped += 1;
                std::thread::sleep(Duration::from_millis(2));
            }
            Err(error) => return Err(Failure::Runtime(format!("service failed: {error}"))),
        }
    }
    println!(
        "{{\"event\":\"stopped\",\"served\":{served},\"dropped\":{dropped},\"uptimeMs\":{}}}",
        started.elapsed().as_millis()
    );
    flush_stdout();
    Ok(())
}

fn flush_stdout() {
    use std::io::Write;
    let _ = std::io::stdout().flush();
}
