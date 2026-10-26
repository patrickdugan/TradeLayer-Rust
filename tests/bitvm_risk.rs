use tradelayer_core::bitvm_bundle::{BitvmError, Environment};
use tradelayer_core::bitvm_risk::*;
use tradelayer_core::issuance_intent::{JsonObject, JsonValue};
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
fn string<'a>(o: &'a JsonObject, k: &str) -> &'a str {
    match o.get(k).unwrap() {
        JsonValue::String(s) => s,
        _ => panic!("string"),
    }
}
fn environment(o: &JsonObject) -> Environment {
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
fn complete_source_risk_windows_caps_repeats_and_failure_trace() {
    let mut env = Environment::new();
    let mut book = BitvmRiskBook::new(env.clone()).unwrap();
    let mut count = 0;
    for line in include_str!("../fixtures/bitvm_risk_vectors.tsv")
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
    {
        count += 1;
        let f: Vec<_> = line.split('\t').collect();
        let input = obj(&unhex(f[2]));
        let expected = obj(&unhex(f[5]));
        let at = f[4].parse().unwrap();
        if f[1] == "boundary-overflow" {
            assert_eq!(expected.get("ok"), Some(&JsonValue::Bool(true)));
            let before = book.clone();
            assert_eq!(book.on_cache_open(&input, at), Err(BitvmError::NonFinite));
            assert_eq!(book, before);
            continue;
        }
        if f[1].starts_with("trace-") {
            let JsonValue::Array(rows) = JsonValue::parse(&unhex(f[6])).unwrap() else {
                panic!("rows")
            };
            assert_eq!(rows.len(), 1);
            assert_eq!(book.records().count(), 0);
            continue;
        }
        let before = book.clone();
        let actual = match f[0] {
            "reset" => {
                env = environment(&input);
                book = BitvmRiskBook::new(env.clone()).unwrap();
                Ok(())
            }
            "onCacheOpen" => book.on_cache_open(&input, at),
            "onEscrowRelease" => book.on_escrow_release(&input, at),
            "onSweep" => book.on_sweep(&input, at),
            _ => panic!("op"),
        };
        if expected.get("ok") == Some(&JsonValue::Bool(true)) {
            actual.unwrap();
        } else {
            assert_eq!(
                actual.unwrap_err(),
                BitvmError::Source(string(&expected, "error").into()),
                "{}",
                f[1]
            );
            assert_eq!(book, before);
        }
        assert_eq!(
            JsonValue::Array(
                book.records()
                    .map(|(_, v)| JsonValue::Object(v.clone()))
                    .collect()
            ),
            JsonValue::parse(&unhex(f[6])).unwrap(),
            "rows {}",
            f[1]
        );
        let bytes = book.checkpoint_bytes().unwrap();
        assert_eq!(
            BitvmRiskBook::from_checkpoint_bytes(&bytes, &env).unwrap(),
            book
        );
    }
    assert_eq!(count, 48);
}
#[test]
fn staged_errors_and_binary64_overflow_never_partially_write() {
    let mut book = BitvmRiskBook::new(Environment::from([(
        "TL_BITVM_MAX_PENDING_ESCROW_PER_DLC".into(),
        "6".into(),
    )]))
    .unwrap();
    book.on_cache_open(&obj("{\"propertyId\":7,\"amount\":5,\"dlcRef\":\"a\"}"), 1)
        .unwrap();
    let before = book.clone();
    assert!(book
        .on_cache_open(&obj("{\"propertyId\":7,\"amount\":2,\"dlcRef\":\"a\"}"), 2)
        .is_err());
    assert_eq!(book, before);
    let mut huge = BitvmRiskBook::new(Environment::new()).unwrap();
    let event = obj("{\"propertyId\":7,\"amount\":1.7976931348623157e308}");
    huge.on_cache_open(&event, 1).unwrap();
    let before = huge.clone();
    assert_eq!(huge.on_cache_open(&event, 2), Err(BitvmError::NonFinite));
    assert_eq!(huge, before);
    assert_eq!(book.replay_policy(), "SourceRepeatCallsAtomicV1");
}
#[test]
fn checkpoint_replays_all_rows_and_rejects_rechecksummed_counter_fiction() {
    let env = Environment::new();
    let mut book = BitvmRiskBook::new(env.clone()).unwrap();
    book.on_cache_open(&obj("{\"propertyId\":7,\"amount\":3,\"dlcRef\":\"d\"}"), 1)
        .unwrap();
    let bytes = book.checkpoint_bytes().unwrap();
    let mut bad = bytes.clone();
    let pattern = b"\"pendingAmount\":3";
    let at = bad
        .windows(pattern.len())
        .rposition(|p| p == pattern)
        .unwrap();
    bad[at + pattern.len() - 1] = b'4';
    crc(&mut bad);
    assert!(BitvmRiskBook::from_checkpoint_bytes(&bad, &env).is_err());
    assert_eq!(
        BitvmRiskBook::from_checkpoint_bytes(
            &bytes,
            &Environment::from([("unused".into(), "value".into())])
        ),
        Err(BitvmError::ConfigMismatch)
    );
    assert_eq!(
        BitvmRiskBook::from_checkpoint_bytes(&bytes, &env).unwrap(),
        book
    );
}
#[test]
fn node_budget_before_growth_and_explicit_key_helpers() {
    let mut book = BitvmRiskBook::new(Environment::new()).unwrap();
    let input = obj(&format!(
        "{{\"propertyId\":0,\"junk\":[{}]}}",
        vec!["0"; 2000].join(",")
    ));
    let mut successes = 0;
    loop {
        let before = book.clone();
        match book.on_cache_open(&input, 1) {
            Ok(()) => {
                successes += 1;
                assert!(successes < 40)
            }
            Err(e) => {
                assert_eq!(e, BitvmError::Capacity);
                assert_eq!(book, before);
                break;
            }
        }
    }
    assert_eq!(
        escrow_global_key(7.0).unwrap(),
        "bitvm-risk-escrow-global-7"
    );
    assert_eq!(
        escrow_dlc_key(7.0, None).unwrap(),
        "bitvm-risk-escrow-dlc-7-default"
    );
    assert_eq!(
        window_key(
            &Environment::new(),
            7.0,
            Some(&JsonValue::String("bad".into()))
        )
        .unwrap(),
        "bitvm-risk-window-7-NaN"
    );
}
