use tradelayer_core::bitvm_bundle::{BitvmError, Environment};
use tradelayer_core::bitvm_cache::*;
use tradelayer_core::issuance_intent::{JsonNumber, JsonObject, JsonValue};
fn unhex(raw: &str) -> String {
    if raw == "-" {
        return String::new();
    }
    String::from_utf8(
        raw.as_bytes()
            .chunks_exact(2)
            .map(|p| u8::from_str_radix(core::str::from_utf8(p).unwrap(), 16).unwrap())
            .collect(),
    )
    .unwrap()
}
fn obj(raw: &str) -> JsonObject {
    JsonObject::parse(raw).unwrap()
}
fn object(v: &JsonValue) -> &JsonObject {
    match v {
        JsonValue::Object(o) => o,
        _ => panic!("object"),
    }
}
fn string<'a>(o: &'a JsonObject, k: &str) -> &'a str {
    match o.get(k).unwrap() {
        JsonValue::String(s) => s,
        _ => panic!("string"),
    }
}
fn env(o: &JsonObject) -> Environment {
    o.iter()
        .map(|(k, v)| {
            (
                k.to_owned(),
                match v {
                    JsonValue::String(s) => s.clone(),
                    _ => panic!("env"),
                },
            )
        })
        .collect()
}
fn crc(bytes: &mut [u8]) {
    let n = bytes.len() - 4;
    let mut crc = !0u32;
    for &b in &bytes[..n] {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb88320 & (0u32.wrapping_sub(crc & 1)));
        }
    }
    bytes[n..].copy_from_slice(&(!crc).to_be_bytes());
}
#[test]
fn complete_source_lifecycle_identity_and_helper_vectors() {
    let mut environment = Environment::new();
    let mut state = BitvmCacheRegistry::new(environment.clone()).unwrap();
    let mut count = 0;
    for line in include_str!("../fixtures/bitvm_cache_vectors.tsv")
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
    {
        count += 1;
        let f: Vec<_> = line.split('\t').collect();
        let input = obj(&unhex(f[2]));
        let context = obj(&unhex(f[3]));
        let at = f[4].parse().unwrap();
        let expected = obj(&unhex(f[5]));
        if f[1] == "boundary-created-nonfinite" {
            assert_eq!(expected.get("ok"), Some(&JsonValue::Bool(true)));
            let before = state.clone();
            assert_eq!(state.open(&input, &context, at), Err(BitvmError::NonFinite));
            assert_eq!(state, before);
            continue;
        }
        if f[1].starts_with("trace-") {
            assert_eq!(expected.get("ok"), Some(&JsonValue::Bool(false)));
            assert_eq!(state.records().count(), 0);
            continue;
        }
        let before = state.clone();
        let actual: Result<JsonValue, BitvmError> = match f[0] {
            "reset" => {
                environment = env(&input);
                state = BitvmCacheRegistry::new(environment.clone()).unwrap();
                Ok(JsonValue::Null)
            }
            "blocks" => state.challenge_blocks(input.get("raw")).map(|n| {
                if n.is_finite() {
                    JsonValue::Number(JsonNumber::from_f64(n).unwrap())
                } else {
                    JsonValue::Null
                }
            }),
            "id" => BitvmCacheRegistry::build_cache_id(&input).map(JsonValue::String),
            "open" => state.open(&input, &context, at).map(JsonValue::Object),
            "get" => Ok(state
                .get(string(&input, "id"))
                .cloned()
                .map_or(JsonValue::Null, JsonValue::Object)),
            op => {
                let id = string(&input, "id");
                let detail = object(input.get("detail").unwrap());
                match op {
                    "challenge" => state.challenge(id, detail, at),
                    "finalize" => state.finalize(id, detail, at),
                    "resolve" => state.resolve(id, detail, at),
                    _ => panic!("op"),
                }
                .map(JsonValue::Object)
            }
        };
        if expected.get("ok") == Some(&JsonValue::Bool(true)) {
            assert_eq!(&actual.unwrap(), expected.get("value").unwrap(), "{}", f[1]);
        } else {
            assert_eq!(
                actual.unwrap_err(),
                BitvmError::Source(string(&expected, "error").into()),
                "{}",
                f[1]
            );
            assert_eq!(state, before);
        }
        let rows = JsonValue::Array(
            state
                .records()
                .map(|(_, v)| JsonValue::Object(v.clone()))
                .collect(),
        );
        assert_eq!(
            rows,
            JsonValue::parse(&unhex(f[6])).unwrap(),
            "rows {}",
            f[1]
        );
        let bytes = state.checkpoint_bytes().unwrap();
        assert_eq!(
            BitvmCacheRegistry::from_checkpoint_bytes(&bytes, &environment).unwrap(),
            state,
            "replay {}",
            f[1]
        );
    }
    assert_eq!(count, 52);
}
#[test]
fn checkpoint_rejects_rechecksummed_state_tampering_and_wrong_configuration() {
    let environment = Environment::new();
    let mut state = BitvmCacheRegistry::new(environment.clone()).unwrap();
    state
        .open(
            &obj("{\"cacheId\":\"a\",\"propertyId\":7,\"amount\":2}"),
            &obj("{\"block\":10}"),
            1,
        )
        .unwrap();
    let bytes = state.checkpoint_bytes().unwrap();
    let mut damaged = bytes.clone();
    let pattern = b"\"status\":\"PENDING\"";
    let at = damaged
        .windows(pattern.len())
        .rposition(|p| p == pattern)
        .unwrap();
    damaged[at + 10] = b'R';
    crc(&mut damaged);
    assert!(BitvmCacheRegistry::from_checkpoint_bytes(&damaged, &environment).is_err());
    assert_eq!(
        BitvmCacheRegistry::from_checkpoint_bytes(
            &bytes,
            &Environment::from([("TL_BITVM_CHALLENGE_BLOCKS".into(), "6".into())])
        ),
        Err(BitvmError::ConfigMismatch)
    );
    assert_eq!(
        BitvmCacheRegistry::from_checkpoint_bytes(&bytes, &environment).unwrap(),
        state
    );
}
#[test]
fn failed_and_nonfinite_writes_preserve_complete_metadata() {
    let mut state = BitvmCacheRegistry::new(Environment::new()).unwrap();
    state
        .open(
            &obj("{\"cacheId\":\"a\",\"propertyId\":7,\"amount\":2}"),
            &obj("{}"),
            1,
        )
        .unwrap();
    let before = state.clone();
    assert!(state
        .challenge("a", &obj("{\"challengeBondAmount\":-1}"), 2)
        .is_err());
    assert_eq!(state, before);
    assert_eq!(
        state.open(
            &obj("{\"cacheId\":\"b\",\"propertyId\":7,\"amount\":2}"),
            &obj("{\"block\":\"bad\"}"),
            2
        ),
        Err(BitvmError::NonFinite)
    );
    assert_eq!(state, before);
    assert!(state
        .open(
            &obj("{\"cacheId\":\"b\",\"propertyId\":7,\"amount\":2}"),
            &obj("{}"),
            9_007_199_254_740_992
        )
        .is_err());
    assert_eq!(state, before);
    let long = obj(&format!(
        "{{\"cacheId\":\"{}\",\"propertyId\":7,\"amount\":2}}",
        "x".repeat(1025)
    ));
    assert_eq!(state.open(&long, &obj("{}"), 2), Err(BitvmError::Capacity));
    assert_eq!(state, before);
}
