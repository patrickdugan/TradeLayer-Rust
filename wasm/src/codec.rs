//! Wire codec and raw-block operations (stateless).
//!
//! Operations (request field `op`):
//!
//! | op | request fields | response members |
//! |---|---|---|
//! | `version` | none | `crate` |
//! | `wire_frame` | `payload` | `type`, `body` |
//! | `wire_parse` | `payload` | `type`, `kind` and the typed fields (strict subset) |
//! | `decode` | `payload` | `type`, `method`, `value` (legacy txDecoder semantics) |
//! | `decode_body` | `method`, `body` | `method`, `value` |
//! | `encode` | `method`, `input` | `method`, `encoded` (`null` when the source returns `undefined`) |
//! | `tx_inspect` | `hex` | `txid`, `wtxid`, counts |
//! | `block_parse` | `hex`, optional `expectedHash` | header fields, `txids`, merkle root |
//!
//! `value` is the core's tagged diagnostic JSON: `["number","<f64 bits hex|NaN|Infinity>"]`,
//! `["string",s]`, `["object",[[key,value],...]]` and so on, so JavaScript `undefined`, NaN and
//! signed zero survive. Hashes in responses use display order (byte-reversed, as block
//! explorers print them). None of these operations authenticates a transaction or a block.

use crate::json::{array, quote, Obj, OpError, Res};
use tradelayer_core::client::RpcValue;
use tradelayer_core::issuance_intent::JsonValue;
use tradelayer_core::legacy_txdecoder_audit::{
    source_decode_historical_method_unverified, source_decode_historical_type_unverified,
    HistoricalDecodedBody, HistoricalDecoderError,
};
use tradelayer_core::legacy_txencoder_audit::{
    source_encode_method_unverified, SourceEncoderReturn,
};
use tradelayer_core::raw_block_ingest::UnverifiedRawBlockView;
use tradelayer_core::read_only::{decode_hex, inspect_transaction};
use tradelayer_core::wire::{parse_frame, parse_typed, ParsedTransaction};

pub fn handle(op: &str, req: &RpcValue) -> Res<Obj> {
    match op {
        "version" => Ok(Obj::new().str("crate", env!("CARGO_PKG_VERSION"))),
        "wire_frame" => wire_frame(req),
        "wire_parse" => wire_parse(req),
        "decode" => decode(req),
        "decode_body" => decode_body(req),
        "encode" => encode(req),
        "tx_inspect" => tx_inspect(req),
        "block_parse" => block_parse(req),
        other => Err(OpError::new("UnknownOp", format!("unknown op `{other}`"))),
    }
}

fn wire_frame(req: &RpcValue) -> Res<Obj> {
    let payload = crate::json::str_field(req, "payload")?;
    let frame = parse_frame(&payload).map_err(OpError::from_debug)?;
    Ok(Obj::new()
        .num("type", frame.tx_type.as_u8())
        .str("body", frame.body))
}

fn wire_parse(req: &RpcValue) -> Res<Obj> {
    let payload = crate::json::str_field(req, "payload")?;
    let frame = parse_frame(&payload).map_err(OpError::from_debug)?;
    let parsed = parse_typed(&payload).map_err(OpError::from_debug)?;
    Ok(typed_members(
        Obj::new().num("type", frame.tx_type.as_u8()),
        &parsed,
    ))
}

fn typed_members(obj: Obj, parsed: &ParsedTransaction) -> Obj {
    match parsed {
        ParsedTransaction::Activate {
            types,
            code_hash_base36,
        } => obj
            .str("kind", "activate")
            .raw("types", array(types.iter().map(|t| t.as_u8().to_string())))
            .str("codeHashBase36", code_hash_base36),
        ParsedTransaction::Issue {
            initial_tokens,
            ticker,
            whitelist_raw,
            managed,
            backup_address,
            nft,
            colored_coin_hybrid,
            procedural_type,
        } => obj
            .str("kind", "issue")
            .str("initialTokens", &initial_tokens.to_string())
            .str("ticker", ticker)
            .str("whitelistRaw", whitelist_raw)
            .bool("managed", *managed)
            .str("backupAddress", backup_address)
            .bool("nft", *nft)
            .bool("coloredCoinHybrid", *colored_coin_hybrid)
            .raw(
                "proceduralType",
                procedural_type.map_or_else(|| "null".to_owned(), |v| v.to_string()),
            ),
        ParsedTransaction::Send {
            recipient,
            property_id,
            amount_atoms,
        } => obj
            .str("kind", "send")
            .str("recipient", recipient)
            .num("propertyId", property_id)
            .str("amountAtoms", &amount_atoms.to_string()),
        ParsedTransaction::SendAll { recipient } => {
            obj.str("kind", "send_all").str("recipient", recipient)
        }
        ParsedTransaction::Grant {
            property_id,
            amount_atoms,
            recipient,
            legacy_hash,
            dlc_template_id,
            dlc_contract_id,
            settlement_state,
            dlc_hash,
        } => obj
            .str("kind", "grant")
            .num("propertyId", property_id)
            .str("amountAtoms", &amount_atoms.to_string())
            .str("recipient", recipient)
            .str("legacyHash", legacy_hash)
            .str("dlcTemplateId", dlc_template_id)
            .str("dlcContractId", dlc_contract_id)
            .str("settlementState", settlement_state)
            .str("dlcHash", dlc_hash),
        ParsedTransaction::Redeem {
            property_id,
            amount_atoms,
            dlc_template_id,
            dlc_contract_id,
            settlement_state,
        } => obj
            .str("kind", "redeem")
            .num("propertyId", property_id)
            .str("amountAtoms", &amount_atoms.to_string())
            .str("dlcTemplateId", dlc_template_id)
            .str("dlcContractId", dlc_contract_id)
            .str("settlementState", settlement_state),
    }
}

fn decoder_error(error: HistoricalDecoderError) -> OpError {
    match error {
        HistoricalDecoderError::SourceThrow { name, message } => {
            OpError::new("SourceThrow", format!("{name}: {message}"))
        }
        other => OpError::from_debug(other),
    }
}

fn decoded(obj: Obj, body: &HistoricalDecodedBody) -> Res<Obj> {
    let value = body.diagnostic_json().map_err(decoder_error)?;
    Ok(obj.str("method", body.method()).raw("value", value))
}

/// `payload` is a full `tl<type><body>` string; the Types route table picks the decoder.
fn decode(req: &RpcValue) -> Res<Obj> {
    let payload = crate::json::str_field(req, "payload")?;
    let frame = parse_frame(&payload).map_err(OpError::from_debug)?;
    let type_id = frame.tx_type.as_u8();
    let body =
        source_decode_historical_type_unverified(type_id, frame.body).map_err(decoder_error)?;
    decoded(Obj::new().num("type", type_id), &body)
}

/// `method` is a txDecoder export name, `body` the text after the type selector.
fn decode_body(req: &RpcValue) -> Res<Obj> {
    let method = crate::json::str_field(req, "method")?;
    let body = crate::json::str_field(req, "body")?;
    let result =
        source_decode_historical_method_unverified(&method, &body).map_err(decoder_error)?;
    decoded(Obj::new(), &result)
}

/// `input` is the JSON argument object of the pinned txEncoder method.
fn encode(req: &RpcValue) -> Res<Obj> {
    let method = crate::json::str_field(req, "method")?;
    let input_raw = crate::json::field(req, "input")?;
    let input = JsonValue::parse(input_raw.as_json())
        .map_err(|e| OpError::bad(&format!("field `input` is not core JSON: {e:?}")))?;
    let out = source_encode_method_unverified(&method, &input).map_err(OpError::from_debug)?;
    let encoded = match out {
        SourceEncoderReturn::Undefined => "null".to_owned(),
        other => quote(other.source_string().unwrap_or_default()),
    };
    Ok(Obj::new().str("method", &method).raw("encoded", encoded))
}

const HEX: &[u8; 16] = b"0123456789abcdef";

/// Lowercase hex of `bytes`, byte-reversed (display order) when `reversed`.
fn hex_of(bytes: &[u8], reversed: bool) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    let mut push = |b: u8| {
        out.push(char::from(HEX[usize::from(b >> 4)]));
        out.push(char::from(HEX[usize::from(b & 15)]));
    };
    if reversed {
        bytes.iter().rev().for_each(|b| push(*b));
    } else {
        bytes.iter().for_each(|b| push(*b));
    }
    out
}

fn hash_field(req: &RpcValue, name: &str) -> Res<Option<[u8; 32]>> {
    let Ok(Some(value)) = req.field(name) else {
        return Ok(None);
    };
    let text = value
        .as_string()
        .map_err(|_| OpError::bad(&format!("field `{name}` is not a string")))?;
    let mut bytes: [u8; 32] = decode_hex(&text)
        .map_err(OpError::from_debug)?
        .try_into()
        .map_err(|_| OpError::bad(&format!("field `{name}` is not 32 bytes")))?;
    bytes.reverse(); // display order in, hashing order out
    Ok(Some(bytes))
}

fn tx_inspect(req: &RpcValue) -> Res<Obj> {
    let raw = decode_hex(&crate::json::str_field(req, "hex")?).map_err(OpError::from_debug)?;
    let tx = inspect_transaction(&raw).map_err(OpError::from_debug)?;
    Ok(Obj::new()
        .str("txid", &hex_of(&tx.txid, true))
        .str("wtxid", &hex_of(&tx.wtxid, true))
        .bool("hasWitness", tx.has_witness)
        .num("inputs", tx.input_count)
        .num("outputs", tx.output_count)
        .num("opReturnOutputs", tx.op_return_count)
        .num("bytes", raw.len()))
}

fn block_parse(req: &RpcValue) -> Res<Obj> {
    let raw = decode_hex(&crate::json::str_field(req, "hex")?).map_err(OpError::from_debug)?;
    let expected = hash_field(req, "expectedHash")?;
    let view = UnverifiedRawBlockView::parse(&raw, expected).map_err(OpError::from_debug)?;
    let index = view.inspect_serial().map_err(OpError::from_debug)?;
    let header = &raw[..80];
    let word = |at: usize| {
        u32::from_le_bytes([header[at], header[at + 1], header[at + 2], header[at + 3]])
    };
    let pre = index.ordered_preindex();
    let txids = array(
        pre.ordered()
            .iter()
            .map(|row| quote(&hex_of(&row.observation.txid, true))),
    );
    Ok(Obj::new()
        .str("hash", &hex_of(&index.header_hash_raw(), true))
        .num("version", word(0) as i32)
        .str("prevHash", &hex_of(&header[4..36], true))
        .str("merkleRoot", &hex_of(&index.txid_merkle_root_raw(), true))
        .bool(
            "merkleMatchesHeader",
            index.txid_merkle_root_raw() == view.header_merkle_root_raw(),
        )
        .num("time", word(68))
        .num("bits", word(72))
        .num("nonce", word(76))
        .num("txCount", view.transaction_count())
        .num("weight", index.block_weight())
        .num("strippedBytes", index.stripped_block_bytes())
        .num("opReturnOutputs", pre.op_return_output_count())
        .raw("txids", txids))
}
