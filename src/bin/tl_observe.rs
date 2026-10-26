//! Explicit local observation service. RPC data never applies economic inputs.
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tradelayer_core::bootstrap::BootstrapControl;
use tradelayer_core::chain::Tip;
use tradelayer_core::client::{RpcAuth, RpcClient, RpcValue};
use tradelayer_core::daemon::{
    Daemon, ObservationHead, ObservationStore, ObserverConfig, ObserverMode,
};
use tradelayer_core::network_profile::TradeLayerMainnet;
use tradelayer_core::runtime::{block_id_text, parse_block_id, RpcObservationClient};
use tradelayer_core::storage::Recovery;

const USAGE:&str="tl_observe --root ABSOLUTE_DIR --network BTC|LTC|DOGE --rpc-address IP:PORT [--cookie-file PATH] (--initialize | --expected-head SEQUENCE:HEIGHT:HASH:COMMITMENT) [--anchor-height HEIGHT --anchor-hash HASH] [--steps COUNT | --run --stop-file PATH] [--rpc-timeout-ms MS] [--workers COUNT] [--stop|--pause|--resume|--reconcile HEIGHT:HASH]";
fn main() {
    if let Err(error) = run() {
        eprintln!("tl_observe: {error}");
        std::process::exit(1);
    }
}
fn run() -> Result<(), String> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.iter().any(|arg| arg == "--help") {
        println!("{USAGE}");
        return Ok(());
    }
    if args.len() > 40 || args.iter().any(|a| a.len() > 4096) {
        return Err("Argument limit exceeded".into());
    }
    let mut values = BTreeMap::new();
    let mut index = 0;
    while index < args.len() {
        let key = &args[index];
        let flag =
            ["--initialize", "--run", "--stop", "--pause", "--resume"].contains(&key.as_str());
        if ![
            "--root",
            "--network",
            "--rpc-address",
            "--cookie-file",
            "--expected-head",
            "--anchor-height",
            "--anchor-hash",
            "--steps",
            "--stop-file",
            "--rpc-timeout-ms",
            "--workers",
            "--reconcile",
            "--initialize",
            "--run",
            "--stop",
            "--pause",
            "--resume",
        ]
        .contains(&key.as_str())
        {
            return Err(format!("Unknown option {key}"));
        }
        let value = if flag {
            "true".to_owned()
        } else {
            index += 1;
            args.get(index)
                .ok_or_else(|| format!("Missing value for {key}"))?
                .clone()
        };
        if values.insert(key.clone(), value).is_some() {
            return Err(format!("Duplicate option {key}"));
        }
        index += 1;
    }
    let required = |key: &str| {
        values
            .get(key)
            .map(String::as_str)
            .ok_or_else(|| format!("Missing {key}; {USAGE}"))
    };
    let root = PathBuf::from(required("--root")?);
    if !root.is_absolute() || !root.is_dir() {
        return Err("--root must name an existing absolute directory".into());
    }
    let network = match required("--network")? {
        "BTC" => TradeLayerMainnet::Bitcoin,
        "LTC" => TradeLayerMainnet::Litecoin,
        "DOGE" => TradeLayerMainnet::Dogecoin,
        _ => return Err("Unknown mainnet".into()),
    };
    let rpc_address = required("--rpc-address")?
        .parse()
        .map_err(|_| "Invalid literal RPC address")?;
    let anchor_height = number(values.get("--anchor-height"), 0)?;
    let anchor_hash = match values.get("--anchor-hash") {
        Some(hash) => parse_block_id(hash).map_err(|_| "Invalid anchor hash")?,
        None if anchor_height == 0 => network.genesis(),
        None => return Err("Nonzero anchor requires --anchor-hash".into()),
    };
    let mut config = ObserverConfig::new(
        network,
        Tip {
            height: anchor_height,
            id: anchor_hash,
        },
    )
    .map_err(|e| e.to_string())?;
    config.scan.workers =
        usize::try_from(number(values.get("--workers"), config.scan.workers as u64)?)
            .map_err(|_| "Invalid workers")?;
    if values.contains_key("--initialize") == values.contains_key("--expected-head") {
        return Err("Choose exactly one of --initialize and --expected-head".into());
    }
    let run_forever = values.contains_key("--run");
    if run_forever && values.contains_key("--steps") {
        return Err("Choose --run or --steps".into());
    }
    let stop_file = values.get("--stop-file").map(PathBuf::from);
    if run_forever && stop_file.is_none() {
        return Err("--run requires --stop-file for cooperative shutdown".into());
    }
    if stop_file.as_ref().is_some_and(|p| !p.is_absolute()) {
        return Err("Stop file path must be absolute".into());
    }
    let steps = number(values.get("--steps"), 1)?;
    if steps > 1_000_000 {
        return Err("Step limit exceeded".into());
    }
    let operations = ["--pause", "--resume", "--stop", "--reconcile"]
        .iter()
        .filter(|key| values.contains_key(**key))
        .count();
    if operations > 1 {
        return Err("Choose at most one control operation".into());
    }
    if values.contains_key("--initialize")
        && (values.contains_key("--resume") || values.contains_key("--reconcile"))
    {
        return Err("Resume/reconcile require an existing store and expected head".into());
    }
    let reconciliation = values
        .get("--reconcile")
        .map(|target| -> Result<Tip, String> {
            let (height, hash) = target
                .split_once(':')
                .ok_or("Invalid reconciliation target")?;
            Ok(Tip {
                height: height
                    .parse()
                    .map_err(|_| "Invalid reconciliation height")?,
                id: parse_block_id(hash).map_err(|_| "Invalid reconciliation hash")?,
            })
        })
        .transpose()?;
    let auth = match values.get("--cookie-file") {
        Some(path) => RpcAuth::Cookie(PathBuf::from(path)),
        None => RpcAuth::from_env(
            &std::env::vars()
                .filter(|(key, _)| ["RPC_USER", "RPC_PASS"].contains(&key.as_str()))
                .collect(),
        ),
    };
    let timeout = number(values.get("--rpc-timeout-ms"), 5000)?;
    let client = RpcClient::new(
        rpc_address,
        auth,
        None,
        Duration::from_millis(timeout),
        network,
    )
    .map_err(|e| e.to_string())?;
    let control = BootstrapControl::default();
    let store = if values.contains_key("--initialize") {
        ObservationStore::initialize(&root, config).map_err(|e| e.to_string())?
    } else {
        let expected = parse_head(required("--expected-head")?)?;
        ObservationStore::load(&root, config, expected, Recovery::RejectIncompleteTail)
            .map_err(|e| e.to_string())?
            .0
    };
    let mut daemon = Daemon::new(RpcObservationClient::new(client, control.clone()), store);
    let expected = daemon.head().map_err(|e| e.to_string())?;
    if values.contains_key("--stop") {
        daemon.stop(expected).map_err(|e| e.to_string())?;
    }
    if values.contains_key("--pause") {
        daemon.pause(expected).map_err(|e| e.to_string())?;
    }
    if values.contains_key("--resume") {
        daemon.resume(expected).map_err(|e| e.to_string())?;
    }
    if let Some(tip) = reconciliation {
        daemon.reconcile(expected, tip).map_err(|e| e.to_string())?;
    }
    let start = Instant::now();
    let mut count = 0u64;
    while (run_forever || count < steps) && operations == 0 {
        if stop_file.as_ref().is_some_and(|p| p.exists()) {
            control.cancel();
        }
        let millis =
            u64::try_from(start.elapsed().as_millis()).map_err(|_| "Clock limit exceeded")?;
        let _ = daemon.step(millis, &control).map_err(|e| e.to_string())?;
        count += 1;
        let mode = daemon.status().map_err(|e| e.to_string())?.mode;
        if !matches!(mode, ObserverMode::Observing) {
            break;
        }
        if run_forever {
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    let status = daemon.status().map_err(|e| e.to_string())?;
    let head = format!(
        "{}:{}:{}:{}",
        status.head.sequence,
        status.head.tip.height,
        block_id_text(status.head.tip.id),
        block_id_text(tradelayer_core::chain::BlockId(status.head.commitment))
    );
    let quote = |text: &str| {
        RpcValue::string(text)
            .map(|v| v.as_json().to_owned())
            .map_err(|e| e.to_string())
    };
    println!("{{\"head\":{},\"mode\":{},\"phase\":{},\"stored_blocks\":{},\"stored_raw_bytes\":{},\"identity_checked\":{},\"authenticated_admission\":false}}",quote(&head)?,quote(&format!("{:?}",status.mode))?,quote(&format!("{:?}",status.phase))?,status.stored_blocks,status.stored_raw_bytes,status.identity_checked);
    Ok(())
}
fn number(value: Option<&String>, default: u64) -> Result<u64, String> {
    match value {
        Some(value) => value.parse().map_err(|_| "Invalid unsigned option".into()),
        None => Ok(default),
    }
}
fn parse_head(text: &str) -> Result<ObservationHead, String> {
    let parts = text.split(':').collect::<Vec<_>>();
    if parts.len() != 4 {
        return Err("Invalid expected head".into());
    }
    Ok(ObservationHead {
        sequence: parts[0].parse().map_err(|_| "Invalid sequence")?,
        tip: Tip {
            height: parts[1].parse().map_err(|_| "Invalid tip height")?,
            id: parse_block_id(parts[2]).map_err(|_| "Invalid tip hash")?,
        },
        commitment: parse_block_id(parts[3])
            .map_err(|_| "Invalid head commitment")?
            .0,
    })
}
