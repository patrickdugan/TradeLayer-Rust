//! Frames for a ZkVerifyV1 staging run on Bitcoin testnet4 (`ops/zk-staging/README.md`).
//!
//! The staged program is the cubic test circuit (`fixtures/zk_nova/cubic_3steps.vk_nogens.hex`)
//! registered with layout 2, verifier version 1 and a 1000 ms budget, as in `tests/zk_node.rs`.
//!
//! ```text
//! cargo run --release --example zk_stage_frames -- binding \
//!     --anchor-hash HEX --sender ADDR --expiry HEIGHT
//! cargo run --release --example zk_stage_frames -- frames \
//!     --anchor-hash HEX --admin ADDR --other ADDR --expiry HEIGHT --code-hash HEX \
//!     --proof-meta FILE --proof FILE --out FILE
//! cargo run --release --example zk_stage_frames -- check-tx --frames FILE --signed FILE
//! ```
//!
//! `binding` prints the binding element `z0[0]` that the proof for `--sender` must start from.
//! `frames` checks that the proof starts from the admin's binding element and verifies it with
//! `ZkNovaAdapter`, then writes one TSV row per frame: `round`, `name`, the receipt reason the node
//! must record, and the frame in hex. Every frame is sent by the admin, which the node bootstrap
//! funds with the collateral property that pays the type-36 fee. Round 1 activates type 36; round 2
//! (registration and three proofs) must land in a later block, since the registry refuses type 36
//! in its activation block (`SameBlockActivation`).
//!
//! `check-tx` reads the signed transactions `ops/zk-staging/stage.py` built (`round`, `name`,
//! `txid`, `expect`, raw hex) and checks each with the node's own parser, `read_only::inspect_transaction`:
//! the txid matches, there is exactly one carrier, it is a legacy `"tl"` carrier holding a type-36
//! family frame, and the frame is byte for byte the one `frames` wrote under that name.

use std::collections::BTreeMap;
use std::process::ExitCode;

use tradelayer_core::ledger::AccountId;
use tradelayer_core::network_profile::TradeLayerMainnet;
use tradelayer_core::read_only::{inspect_transaction, CarrierFormat};
use tradelayer_core::runtime::parse_block_id;
use tradelayer_core::zk_nova::RegisteredNovaKey;
use tradelayer_core::zk_verify::{
    binding_element, program_id_of, zk_reason_rejected, ZkActivatePayload, ZkBindingContext,
    ZkEffect, ZkNodeFrame, ZkNovaAdapter, ZkRegisterPayload, ZkReject, ZkRejectReason, ZkSystem,
    ZkVerifier, ZkVerifyPayload, ZK_REASON_VERIFIED,
};

const KEY_FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/fixtures/zk_nova/cubic_3steps.vk_nogens.hex"
);
const LAYOUT: u32 = 2;
const VERIFIER_VERSION: u32 = 1;
const MAX_VERIFY_MS: u32 = 1000;
/// Byte 4816 is `snark_primary.eval_W` in a cubic proof; flipping its low bit is corruption 0 of
/// the fixture metadata, which nova rejects as `InvalidSumcheckProof`.
const CORRUPT_OFFSET: usize = 4816;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("zk_stage_frames: {message}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let command = args
        .next()
        .ok_or("expected `binding`, `frames` or `check-tx`")?;
    if !matches!(command.as_str(), "binding" | "frames" | "check-tx") {
        return Err(format!("unknown command {command}"));
    }
    let mut options = BTreeMap::new();
    while let Some(flag) = args.next() {
        let value = args.next().ok_or(format!("{flag} needs a value"))?;
        if options.insert(flag.clone(), value).is_some() {
            return Err(format!("{flag} given twice"));
        }
    }
    let mut take = |name: &str| {
        options
            .remove(name)
            .ok_or_else(|| format!("missing {name}"))
    };
    if command == "check-tx" {
        let frames = read(&take("--frames")?)?;
        let signed = read(&take("--signed")?)?;
        finish(options)?;
        return check_transactions(&frames, &signed);
    }
    let anchor = parse_block_id(&take("--anchor-hash")?)
        .map_err(|_| "--anchor-hash must be 64 hex digits")?;
    let expiry: u64 = take("--expiry")?
        .parse()
        .map_err(|_| "--expiry must be a height")?;
    let ctx = ZkBindingContext {
        genesis: TradeLayerMainnet::Bitcoin.genesis().0,
        anchor: anchor.0,
    };
    let key = RegisteredNovaKey::from_vk_nogens(&unhex(&read(KEY_FIXTURE)?)?)
        .map_err(|e| format!("cubic key: {e:?}"))?
        .encode();
    let program_id = program_id_of(
        ZkSystem::Nova,
        &key,
        LAYOUT,
        ZkEffect::StatementReceipt,
        VERIFIER_VERSION,
        MAX_VERIFY_MS,
    );
    match command.as_str() {
        "binding" => {
            let sender = account(&take("--sender")?)?;
            finish(options)?;
            println!(
                "{}",
                hex(&binding_element(&ctx, &sender, expiry, &program_id))
            );
            Ok(())
        }
        "frames" => {
            let admin = account(&take("--admin")?)?;
            let other = account(&take("--other")?)?;
            let code_hash: [u8; 32] = unhex(&take("--code-hash")?)?
                .try_into()
                .map_err(|_| "--code-hash must be 32 bytes")?;
            let meta = read(&take("--proof-meta")?)?;
            let proof = unhex(&read(&take("--proof")?)?)?;
            let out = take("--out")?;
            finish(options)?;
            if admin == other {
                return Err("--other must differ from --admin".into());
            }
            let field = |name: &str| {
                meta.lines()
                    .find_map(|l| l.strip_prefix(&format!("{name}\t")))
                    .map(str::trim)
                    .ok_or(format!("proof metadata has no {name}"))
            };
            let z0: [u8; 32] = unhex(field("z0")?)?
                .try_into()
                .map_err(|_| "z0 must be 32 bytes")?;
            let zn: [u8; 32] = unhex(field("zn")?)?
                .try_into()
                .map_err(|_| "zn must be 32 bytes")?;
            let steps: u32 = field("num_steps")?
                .parse()
                .map_err(|_| "num_steps must be a u32")?;
            let own = binding_element(&ctx, &admin, expiry, &program_id);
            if z0 != own {
                return Err(format!(
                    "the proof starts from {} but the admin's binding element is {}",
                    hex(&z0),
                    hex(&own)
                ));
            }
            ZkNovaAdapter
                .verify(ZkSystem::Nova, &key, steps, &[z0, zn], &proof)
                .map_err(|e| format!("the staged proof does not verify: {e:?}"))?;
            let mut corrupt = proof.clone();
            *corrupt
                .get_mut(CORRUPT_OFFSET)
                .ok_or("proof shorter than a cubic proof")? ^= 1;
            if ZkNovaAdapter.verify(ZkSystem::Nova, &key, steps, &[z0, zn], &corrupt)
                != Err(ZkReject::InvalidProof)
            {
                return Err("the corrupted proof is not rejected as InvalidProof".into());
            }
            let verify = |inputs: [[u8; 32]; 2], body: &[u8]| {
                let mut enveloped = steps.to_be_bytes().to_vec();
                enveloped.extend_from_slice(body);
                ZkNodeFrame::Verify(ZkVerifyPayload {
                    system: ZkSystem::Nova.code(),
                    program_id,
                    public_inputs: inputs.to_vec(),
                    proof: enveloped,
                    expiry,
                })
            };
            let other_binding = binding_element(&ctx, &other, expiry, &program_id);
            let rows = [
                (
                    1,
                    "activate",
                    "zk:activated".to_owned(),
                    ZkNodeFrame::Activate(ZkActivatePayload { code_hash }),
                ),
                (
                    2,
                    "register",
                    "zk:registered".to_owned(),
                    ZkNodeFrame::Register(ZkRegisterPayload {
                        system: ZkSystem::Nova.code(),
                        program_id,
                        key_bytes: key.clone(),
                        public_input_layout: LAYOUT,
                        effect: ZkEffect::StatementReceipt.code(),
                        verifier_version: VERIFIER_VERSION,
                        max_verify_ms: MAX_VERIFY_MS,
                    }),
                ),
                (
                    2,
                    "verify-valid",
                    ZK_REASON_VERIFIED.to_owned(),
                    verify([z0, zn], &proof),
                ),
                (
                    2,
                    "verify-corrupt",
                    zk_reason_rejected(&ZkRejectReason::Verifier(ZkReject::InvalidProof)),
                    verify([z0, zn], &corrupt),
                ),
                (
                    2,
                    "verify-other-binding",
                    zk_reason_rejected(&ZkRejectReason::BindingMismatch),
                    verify([other_binding, zn], &proof),
                ),
            ];
            let mut text = String::from("# round\tname\texpect\tframe\n");
            for (round, name, expect, frame) in rows {
                let bytes = frame.encode();
                if ZkNodeFrame::decode(&bytes).as_ref() != Ok(&frame) {
                    return Err(format!("{name} does not round-trip"));
                }
                text.push_str(&format!("{round}\t{name}\t{expect}\t{}\n", hex(&bytes)));
            }
            std::fs::write(&out, text).map_err(|e| format!("{out}: {e}"))?;
            println!("program_id\t{}", hex(&program_id));
            println!("binding_admin\t{}", hex(&own));
            println!("binding_other\t{}", hex(&other_binding));
            println!("frames\t{out}");
            Ok(())
        }
        _ => unreachable!("the command was checked above"),
    }
}

fn check_transactions(frames: &str, signed: &str) -> Result<(), String> {
    let rows = |text: &str| -> Vec<Vec<String>> {
        text.lines()
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .map(|l| l.split('\t').map(str::to_owned).collect())
            .collect()
    };
    let expected: BTreeMap<String, Vec<u8>> = rows(frames)
        .into_iter()
        .map(|r| match r.as_slice() {
            [_, name, _, frame] => Ok((name.clone(), unhex(frame)?)),
            _ => Err("frames rows have four columns".to_owned()),
        })
        .collect::<Result<_, _>>()?;
    let signed = rows(signed);
    if signed.is_empty() {
        return Err("no signed transactions".into());
    }
    for row in signed {
        let [_, name, txid, _, raw] = row.as_slice() else {
            return Err("signed rows have five columns".into());
        };
        let frame = expected
            .get(name)
            .ok_or(format!("{name} is not in the frames file"))?;
        let observation =
            inspect_transaction(&unhex(raw)?).map_err(|e| format!("{name}: {e:?}"))?;
        let mut display = observation.txid;
        display.reverse();
        if hex(&display) != *txid {
            return Err(format!(
                "{name}: txid {txid} does not match the raw transaction"
            ));
        }
        let [carrier] = observation.carriers.as_slice() else {
            return Err(format!(
                "{name}: {} carriers, the node admits exactly one",
                observation.carriers.len()
            ));
        };
        if carrier.format != CarrierFormat::LegacyAscii
            || !ZkNodeFrame::is_zk_frame(&carrier.payload)
        {
            return Err(format!("{name}: the carrier is not a type-36 family frame"));
        }
        if carrier.payload != *frame {
            return Err(format!("{name}: the carrier differs from the frame"));
        }
        println!("ok\t{name}\t{txid}\tvout {}", carrier.vout);
    }
    Ok(())
}

fn account(text: &str) -> Result<AccountId, String> {
    text.try_into()
        .map_err(|_| format!("{text} is not an account"))
}

fn finish(options: BTreeMap<String, String>) -> Result<(), String> {
    match options.keys().next() {
        Some(flag) => Err(format!("unknown option {flag}")),
        None => Ok(()),
    }
}

fn read(path: &str) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))
}

fn unhex(text: &str) -> Result<Vec<u8>, String> {
    let text = text.trim();
    if !text.len().is_multiple_of(2) {
        return Err("odd-length hex".into());
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).map_err(|_| "invalid hex".to_owned()))
        .collect()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
