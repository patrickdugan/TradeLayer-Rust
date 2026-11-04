//! Read-only admission report for one observed block. Prints the outcome of
//! authenticating each TradeLayer candidate; nothing is applied to a ledger.
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;
use tradelayer_core::admission::{
    admit_block, AdmissionContext, AdmissionNetwork, CandidateOutcome,
};
use tradelayer_core::admission_rpc::RpcPrevoutSource;
use tradelayer_core::bitcoin_inclusion::BitcoinPowProfile;
use tradelayer_core::bitcoin_witness_binding::BitcoinWitnessRules;
use tradelayer_core::bootstrap::BootstrapControl;
use tradelayer_core::chain::BlockId;
use tradelayer_core::client::{RpcAuth, RpcClient, RpcValue};
use tradelayer_core::network_profile::TradeLayerMainnet;
use tradelayer_core::read_only::{decode_hex, hex_bytes};
use tradelayer_core::runtime::{block_id_text, parse_block_id};

const USAGE: &str = "tl_admit --network BTC|BTC-TESTNET4 --rpc-address IP:PORT [--cookie-file PATH] (--height HEIGHT | --block-hash HASH) [--rpc-timeout-ms MS]";
const FLAGS: [&str; 6] = [
    "--network",
    "--rpc-address",
    "--cookie-file",
    "--height",
    "--block-hash",
    "--rpc-timeout-ms",
];

fn main() {
    if let Err(error) = run() {
        eprintln!("tl_admit: {error}");
        std::process::exit(1);
    }
}

fn quote(text: &str) -> Result<String, String> {
    RpcValue::string(text)
        .map(|value| value.as_json().to_owned())
        .map_err(|e| e.to_string())
}

fn run() -> Result<(), String> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.iter().any(|arg| arg == "--help") {
        println!("{USAGE}");
        return Ok(());
    }
    if args.len() > 16 || args.iter().any(|a| a.len() > 4096) {
        return Err("Argument limit exceeded".into());
    }
    let mut values = BTreeMap::new();
    let mut index = 0;
    while index < args.len() {
        let key = &args[index];
        if !FLAGS.contains(&key.as_str()) {
            return Err(format!("Unknown option {key}"));
        }
        index += 1;
        let value = args
            .get(index)
            .ok_or_else(|| format!("Missing value for {key}"))?
            .clone();
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
    let testnet4 = match required("--network")? {
        "BTC" => false,
        "BTC-TESTNET4" => true,
        _ => return Err("Unknown network".into()),
    };
    let rpc_address = required("--rpc-address")?
        .parse()
        .map_err(|_| "Invalid literal RPC address")?;
    let timeout = match values.get("--rpc-timeout-ms") {
        Some(text) => text.parse().map_err(|_| "Invalid timeout")?,
        None => 5000,
    };
    let auth = match values.get("--cookie-file") {
        Some(path) => RpcAuth::Cookie(PathBuf::from(path)),
        None => RpcAuth::from_env(
            &std::env::vars()
                .filter(|(key, _)| ["RPC_USER", "RPC_PASS"].contains(&key.as_str()))
                .collect(),
        ),
    };
    let mut client = RpcClient::new(
        rpc_address,
        auth,
        None,
        Duration::from_millis(timeout),
        TradeLayerMainnet::Bitcoin,
    )
    .map_err(|e| e.to_string())?;
    if testnet4 {
        client
            .verify_testnet4_controlled(&BootstrapControl::default())
            .map_err(|e| e.to_string())?;
    } else {
        client.verify_network().map_err(|e| e.to_string())?;
    }
    let hash = match (values.get("--height"), values.get("--block-hash")) {
        (Some(height), None) => {
            let height: u64 = height.parse().map_err(|_| "Invalid height")?;
            let text = client
                .call("getblockhash", &[RpcValue::unsigned(height)], false)
                .and_then(|v| v.as_string())
                .map_err(|e| e.to_string())?;
            parse_block_id(&text).map_err(|_| "Invalid block hash from node")?
        }
        (None, Some(text)) => parse_block_id(text).map_err(|_| "Invalid --block-hash")?,
        _ => return Err(format!("Choose --height or --block-hash; {USAGE}")),
    };
    let hash_json = RpcValue::string(&block_id_text(hash)).map_err(|e| e.to_string())?;
    let header = client
        .call(
            "getblockheader",
            &[hash_json.clone(), RpcValue::boolean(true)],
            false,
        )
        .map_err(|e| e.to_string())?;
    let height = header
        .field("height")
        .map_err(|e| e.to_string())?
        .ok_or("Header without height")?
        .as_u64()
        .map_err(|e| e.to_string())?;
    let parent = match header
        .field("previousblockhash")
        .map_err(|e| e.to_string())?
    {
        Some(text) => parse_block_id(&text.as_string().map_err(|e| e.to_string())?)
            .map_err(|_| "Invalid parent hash from node")?,
        None => BlockId([0; 32]),
    };
    let raw_hex = client
        .call("getblock", &[hash_json, RpcValue::unsigned(0)], false)
        .and_then(|v| v.as_string())
        .map_err(|e| e.to_string())?;
    let raw_block = decode_hex(&raw_hex).map_err(|_| "Invalid raw block hex from node")?;
    let mut source = RpcPrevoutSource::fetch(&mut client, hash).map_err(|e| e.to_string())?;
    let context = AdmissionContext {
        height,
        block: hash,
        parent,
        pow: BitcoinPowProfile::MainnetLike,
        rules: BitcoinWitnessRules::SegwitActive,
        network: if testnet4 {
            AdmissionNetwork::Testnet
        } else {
            AdmissionNetwork::Mainnet
        },
    };
    let outcomes = admit_block(&raw_block, &context, &mut source).map_err(|e| e.to_string())?;
    let authenticated = outcomes.iter().filter(|o| o.is_authenticated()).count();
    let mut rows = Vec::with_capacity(outcomes.len());
    for outcome in &outcomes {
        let detail = match outcome {
            CandidateOutcome::Authenticated {
                sender, payload, ..
            } => format!(
                "\"status\":\"authenticated\",\"sender\":{},\"parent_amount_satoshis\":{},\"parent_amount_signed\":{},\"payload\":{}",
                quote(&sender.address)?,
                sender.parent_amount_satoshis,
                sender.parent_amount_is_signed,
                quote(&hex_bytes(&payload.payload))?
            ),
            CandidateOutcome::Refused { reason, .. } => format!(
                "\"status\":\"refused\",\"reason\":{}",
                quote(&format!("{reason:?}"))?
            ),
        };
        rows.push(format!(
            "{{\"txid\":\"{}\",\"position\":{},{detail}}}",
            hex_bytes(&outcome.txid_display()),
            outcome.position()
        ));
    }
    println!(
        "{{\"block\":\"{}\",\"height\":{height},\"candidates\":{},\"authenticated\":{authenticated},\"refused\":{},\"outcomes\":[{}],\"applied_to_ledger\":false}}",
        block_id_text(hash),
        outcomes.len(),
        outcomes.len() - authenticated,
        rows.join(",")
    );
    Ok(())
}
