use tradelayer_core::issuance_intent::JsonValue;
use tradelayer_core::validity_source::{SourceException, SourceOutcome, SourceValue as V};
// Lossless borrowed-slice test transport: no production limits changed.
fn json_span(bytes: &[u8], start: usize) -> usize {
    match bytes[start] {
        b'"' => {
            let mut at = start + 1;
            loop {
                match bytes[at] {
                    b'"' => return at + 1,
                    b'\\' => at += 2,
                    _ => at += 1,
                }
            }
        }
        b'[' | b'{' => {
            let close = if bytes[start] == b'[' { b']' } else { b'}' };
            let mut at = start + 1;
            loop {
                match bytes[at] {
                    byte if byte == close => return at + 1,
                    b'"' | b'[' | b'{' => at = json_span(bytes, at),
                    b']' | b'}' => panic!("mismatched frozen JSON container"),
                    _ => at += 1,
                }
            }
        }
        _ => {
            let mut at = start;
            while at < bytes.len()
                && !matches!(bytes[at], b',' | b']' | b'}' | b' ' | b'\t' | b'\r' | b'\n')
            {
                at += 1;
            }
            assert!(at > start);
            at
        }
    }
}
fn json_parts(raw: &str) -> Vec<&str> {
    let raw = raw.trim();
    assert!(raw.starts_with('[') && raw.ends_with(']'));
    let mut at = 1;
    let mut output = vec![];
    while at < raw.len() - 1 {
        while raw.as_bytes()[at].is_ascii_whitespace() {
            at += 1;
        }
        if raw.as_bytes()[at] == b']' {
            break;
        }
        let end = json_span(raw.as_bytes(), at);
        output.push(&raw[at..end]);
        at = end;
        while raw.as_bytes()[at].is_ascii_whitespace() {
            at += 1;
        }
        if raw.as_bytes()[at] == b']' {
            break;
        }
        assert_eq!(raw.as_bytes()[at], b',');
        at += 1;
    }
    output
}
fn json_string(raw: &str) -> String {
    match JsonValue::parse(raw).unwrap() {
        JsonValue::String(value) => value,
        _ => panic!("frozen JSON string required"),
    }
}
fn untag(raw: &str) -> V {
    let parts = json_parts(raw);
    match json_string(parts[0]).as_str() {
        "object" => V::Object(
            json_parts(parts[1])
                .into_iter()
                .map(|pair| {
                    let pair = json_parts(pair);
                    (json_string(pair[0]), untag(pair[1]))
                })
                .collect(),
        ),
        "array" => V::Array(json_parts(parts[1]).into_iter().map(untag).collect()),
        "undefined" => V::Undefined,
        "null" => V::Null,
        "promise" => V::Promise,
        "string" => V::String(json_string(parts[1])),
        "boolean" => {
            assert!(matches!(parts[1], "true" | "false"));
            V::Bool(parts[1] == "true")
        }
        "number" => {
            let bits = json_string(parts[1]);
            match bits.as_str() {
                "NaN" => V::number(f64::NAN),
                "Infinity" => V::number(f64::INFINITY),
                "-Infinity" => V::number(f64::NEG_INFINITY),
                "-0" => V::number(-0.),
                _ => V::Number(u64::from_str_radix(&bits, 16).unwrap()),
            }
        }
        "bignumber" => V::BigNumber(json_string(parts[1])),
        tag => panic!("unmapped selected source value {tag}"),
    }
}

pub fn records() -> Vec<V> {
    include_str!("../fixtures/orderbook_lifecycle_helpers.jsonl")
        .lines()
        .map(untag)
        .collect()
}
pub fn items(value: V) -> Vec<V> {
    let V::Array(v) = value else {
        panic!("array: {value:?}")
    };
    v
}
pub fn text(value: V) -> String {
    let V::String(v) = value else {
        panic!("text: {value:?}")
    };
    v
}
pub fn source_outcome(record: &V) -> SourceOutcome {
    let outcome = record.get("outcome");
    let error = outcome.get("thrown");
    if error == V::Undefined {
        SourceOutcome::Returned(outcome.get("returned"))
    } else {
        SourceOutcome::Thrown(SourceException {
            name: text(error.get("name")),
            message: text(error.get("message")),
        })
    }
}
pub fn source_rows(record: &V, collection: &str) -> Vec<V> {
    items(record.get("after").get("tables"))
        .iter()
        .find(|t| t.get("name") == V::String(collection.into()))
        .map_or(vec![], |t| items(t.get("rows")))
}
pub fn assert_value(actual: &V, expected: &V, path: &str) {
    match (actual, expected) {
        // SourceValue's BigNumber scalar denotes its exact decimal value;
        // source diagnostics use valueOf's exponent notation for the tick.
        (V::BigNumber(a), V::BigNumber(b)) => assert_eq!(
            a.parse::<tradelayer_core::decimal::Decimal>().unwrap(),
            b.parse::<tradelayer_core::decimal::Decimal>().unwrap(),
            "{path}"
        ),
        (V::Array(a), V::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{path}");
            for (i, (x, y)) in a.iter().zip(b).enumerate() {
                assert_value(x, y, &format!("{path}/{i}"));
            }
        }
        (V::Object(a), V::Object(b)) => {
            assert_eq!(a.len(), b.len(), "{path}");
            for ((ak, av), (bk, bv)) in a.iter().zip(b) {
                assert_eq!(ak, bk, "{path}");
                assert_value(av, bv, &format!("{path}/{ak}"));
            }
        }
        _ => assert_eq!(actual, expected, "{path}"),
    }
}
