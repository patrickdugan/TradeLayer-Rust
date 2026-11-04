//! Codec and raw-block operations, checked against the core's frozen fixture corpora.
use tradelayer_core::client::RpcValue;
use tradelayer_wasm::handle;

const WIRE: &str = include_str!("../../fixtures/wire_vectors.tsv");
const ROUNDTRIP: &str = include_str!("../../fixtures/wire_roundtrip_vectors.tsv");
const HISTORICAL: &str = include_str!("../../fixtures/legacy_txdecoder_historical_vectors.tsv");
const SYNTHETIC_BLOCK: &str = include_str!("../../fixtures/raw_block_ingest_synthetic.hex");
const REVIEW_BLOCK: &str = include_str!("../../fixtures/tx_parser_review_block.hex");

fn call(request: &str) -> RpcValue {
    let text = String::from_utf8(handle(request.as_bytes())).unwrap();
    RpcValue::parse(&text).unwrap_or_else(|_| panic!("response is not JSON: {text}"))
}

fn str_of(value: &RpcValue, name: &str) -> String {
    value.field(name).unwrap().unwrap().as_string().unwrap()
}

fn raw_of(value: &RpcValue, name: &str) -> String {
    value.field(name).unwrap().unwrap().as_json().to_owned()
}

fn is_ok(value: &RpcValue) -> bool {
    raw_of(value, "ok") == "true"
}

fn quote(text: &str) -> String {
    RpcValue::string(text).unwrap().as_json().to_owned()
}

fn unhex(text: &str) -> String {
    if text == "-" {
        return String::new();
    }
    String::from_utf8(
        (0..text.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
            .collect(),
    )
    .unwrap()
}

fn rows(text: &'static str) -> impl Iterator<Item = Vec<&'static str>> {
    text.lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| line.split('\t').collect())
}

fn rev_hex(hex: &str) -> String {
    (0..hex.len())
        .step_by(2)
        .rev()
        .map(|i| &hex[i..i + 2])
        .collect()
}

fn string_array(value: &RpcValue, name: &str) -> Vec<String> {
    value
        .field(name)
        .unwrap()
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_string().unwrap())
        .collect()
}

#[test]
fn wire_frame_and_typed_parse_match_wire_vectors() {
    let mut ok = 0;
    for row in rows(WIRE) {
        let (wire, type_id, body, status) = (row[1], row[2], row[3], row[4]);
        let frame = call(&format!(
            r#"{{"op":"wire_frame","payload":{}}}"#,
            quote(wire)
        ));
        assert!(is_ok(&frame), "{wire}: {frame:?}");
        assert_eq!(raw_of(&frame, "type"), type_id, "{wire}");
        assert_eq!(str_of(&frame, "body"), body, "{wire}");
        let typed = call(&format!(
            r#"{{"op":"wire_parse","payload":{}}}"#,
            quote(wire)
        ));
        if status == "ok" {
            assert!(is_ok(&typed), "{wire}: {typed:?}");
            ok += 1;
        } else {
            assert!(!is_ok(&typed), "{wire} must not parse as typed");
        }
    }
    assert!(ok >= 8, "{ok} typed rows parsed");
}

#[test]
fn wire_parse_send_fields() {
    let r = call(r#"{"op":"wire_parse","payload":"tl20;alice;5;2"}"#);
    assert_eq!(str_of(&r, "kind"), "send");
    assert_eq!(str_of(&r, "recipient"), "alice");
    assert_eq!(raw_of(&r, "propertyId"), "5");
    assert_eq!(str_of(&r, "amountAtoms"), "200000000");
    let r = call(r#"{"op":"wire_parse","payload":"tl21;alice"}"#);
    assert_eq!(str_of(&r, "kind"), "send_all");
}

#[test]
fn typed_errors_carry_core_variant_names() {
    let r = call(r#"{"op":"wire_frame","payload":"xx20"}"#);
    assert_eq!(str_of(&r, "code"), "InvalidMarker");
    let r = call(r#"{"op":"decode_body","method":"decodeNope","body":""}"#);
    assert_eq!(str_of(&r, "code"), "MissingMethod");
    let r = call(r#"{"op":"encode","method":"encodeNope","input":{}}"#);
    assert_eq!(str_of(&r, "code"), "UnknownSourceMethod");
    let r = call(r#"{"op":"block_parse","hex":"00"}"#);
    assert_eq!(str_of(&r, "code"), "TruncatedBlockHeader");
    let r = call(r#"{"op":"block_parse","hex":"zz"}"#);
    assert_eq!(str_of(&r, "code"), "InvalidHex");
    let r = call(r#"{"op":"encode","method":"encodeSend"}"#);
    assert_eq!(str_of(&r, "code"), "BadRequest");
}

#[test]
fn decode_routes_by_type_selector() {
    let r = call(r#"{"op":"decode","payload":"tl20;alice;5;2"}"#);
    assert!(is_ok(&r), "{r:?}");
    assert_eq!(raw_of(&r, "type"), "2");
    assert_eq!(str_of(&r, "method"), "decodeSend");
    // Type 34 has no exported decoder in the pinned Types dispatcher.
    let r = call(r#"{"op":"decode","payload":"tlyabc"}"#);
    assert_eq!(str_of(&r, "code"), "MissingMethod");
}

#[test]
fn full_historical_decoder_corpus_matches_through_the_abi() {
    let mut count = 0;
    for row in rows(HISTORICAL) {
        let (method, case) = (row[0], row[1]);
        let body = unhex(row[2]);
        let r = call(&format!(
            r#"{{"op":"decode_body","method":{},"body":{}}}"#,
            quote(method),
            quote(&body)
        ));
        match row[3] {
            "ok" => {
                assert!(is_ok(&r), "{method}/{case}: {r:?}");
                assert_eq!(raw_of(&r, "value"), unhex(row[4]), "{method}/{case}");
            }
            "throw" => {
                assert_eq!(str_of(&r, "code"), "SourceThrow", "{method}/{case}");
                assert_eq!(
                    str_of(&r, "message"),
                    format!("{}: {}", row[4], unhex(row[5])),
                    "{method}/{case}"
                );
            }
            other => panic!("unknown status {other}"),
        }
        count += 1;
    }
    assert_eq!(count, 1447);
}

#[test]
fn encoder_and_decoder_roundtrip_corpus_through_the_abi() {
    let (mut encoded, mut decoded, mut undefined) = (0, 0, 0);
    for row in rows(ROUNDTRIP) {
        let name = row[1];
        if row[2] == "direct" && row[6] == "exact" {
            let input = unhex(row[5]);
            let r = call(&format!(
                r#"{{"op":"encode","method":{},"input":{input}}}"#,
                quote(row[4])
            ));
            match row[7] {
                "ok" => {
                    assert!(is_ok(&r), "{name}: {r:?}");
                    let expected = RpcValue::parse(&unhex(row[8]))
                        .unwrap()
                        .as_string()
                        .unwrap();
                    assert_eq!(str_of(&r, "encoded"), expected, "{name}");
                    encoded += 1;
                }
                "undefined" => {
                    assert_eq!(raw_of(&r, "encoded"), "null", "{name}");
                    undefined += 1;
                }
                _ => {}
            }
        }
        if row[12] == "ok" {
            let r = call(&format!(
                r#"{{"op":"decode_body","method":{},"body":{}}}"#,
                quote(row[10]),
                quote(&unhex(row[11]))
            ));
            assert!(is_ok(&r), "{name}: {r:?}");
            assert_eq!(raw_of(&r, "value"), unhex(row[13]), "{name}");
            decoded += 1;
        }
    }
    assert!(encoded >= 40, "encoded {encoded}");
    assert!(decoded >= 40, "decoded {decoded}");
    assert_eq!(undefined, 1);
}

#[test]
fn synthetic_block_hash_txids_and_merkle_root() {
    let hex = SYNTHETIC_BLOCK.trim();
    let r = call(&format!(r#"{{"op":"block_parse","hex":"{hex}"}}"#));
    assert!(is_ok(&r), "{r:?}");
    // Display order is the byte reverse of the hashing-order digests in raw_block_ingest.rs.
    assert_eq!(
        str_of(&r, "hash"),
        rev_hex("09667b766e1bccf918412b65f62966d7a2803613592fed34ba9d27ac608eb0c0")
    );
    assert_eq!(
        str_of(&r, "merkleRoot"),
        rev_hex("760e6d8636fd5304a65989b461a27a88f48545bc1cc6e2b4c2016a638d2e8a9c")
    );
    assert_eq!(raw_of(&r, "merkleMatchesHeader"), "true");
    assert_eq!(raw_of(&r, "txCount"), "4");
    let expected = [
        "8647c745029df431acac0ff4f8a91ff00d6194ef87a705946f784af9f213bffa",
        "49f3b25cef04ddad2b71d1dbfa5cf1b5b2f0bc39ae73eaa395de79e0a17c4af3",
        "f878d360c2a1b812ef5aa5b2706f0f56aa34dba00b4f40fe07000571f00208b1",
        "4f61a8c818de93316dc5b69b57d8fdae42c774056908109397d8381e7e8dcfac",
    ]
    .map(rev_hex);
    assert_eq!(string_array(&r, "txids"), expected);
    // The expected header hash is checked when supplied.
    let good = str_of(&r, "hash");
    let r2 = call(&format!(
        r#"{{"op":"block_parse","hex":"{hex}","expectedHash":"{good}"}}"#
    ));
    assert!(is_ok(&r2), "{r2:?}");
    let bad = "00".repeat(32);
    let r3 = call(&format!(
        r#"{{"op":"block_parse","hex":"{hex}","expectedHash":"{bad}"}}"#
    ));
    assert_eq!(str_of(&r3, "code"), "RequestedHeaderHashMismatch");
}

#[test]
fn tx_inspect_reports_the_bitcoin_block_1_coinbase_txid() {
    let tx = include_str!("../../fixtures/bitcoin_mainnet_block1_coinbase.hex").trim();
    let r = call(&format!(r#"{{"op":"tx_inspect","hex":"{tx}"}}"#));
    assert!(is_ok(&r), "{r:?}");
    assert_eq!(
        str_of(&r, "txid"),
        "0e3e2357e806b6cdb1f70b54c3a3a17b6714ee1f0e68bebb44a74b1efd512098"
    );
    assert_eq!(raw_of(&r, "inputs"), "1");
    assert_eq!(raw_of(&r, "outputs"), "1");
    assert_eq!(raw_of(&r, "hasWitness"), "false");
    // A whole block is not one transaction.
    let block = SYNTHETIC_BLOCK.trim();
    let r = call(&format!(r#"{{"op":"tx_inspect","hex":"{block}"}}"#));
    assert!(!is_ok(&r));
}

#[test]
fn review_block_parses_and_is_internally_consistent() {
    let hex = REVIEW_BLOCK.trim();
    let r = call(&format!(r#"{{"op":"block_parse","hex":"{hex}"}}"#));
    assert!(is_ok(&r), "{r:?}");
    assert_eq!(raw_of(&r, "merkleMatchesHeader"), "true");
    let n: usize = raw_of(&r, "txCount").parse().unwrap();
    assert_eq!(string_array(&r, "txids").len(), n);
    assert!(n > 1);
}
