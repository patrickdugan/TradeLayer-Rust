//! Native codec benchmark. Run in release mode:
//!
//!   cargo run --release --example bench_codec -- [--iters N] [--block-iters M] [--runs R] [--warmup W]
//!
//! Prints one JSON object per line. Workloads and inputs match `wasm/bench/bench_wasm.mjs` and
//! `wasm/bench/bench_legacy.cjs`:
//!
//! * `decode_corpus`: every row of `fixtures/legacy_txdecoder_historical_vectors.tsv` (1447
//!   source-decoder cases, throws included) is decoded `N` times.
//! * `block_parse`: each fixture block is parsed (header hash, txids, merkle root) `M` times.
//!
//! Runtimes: `native-core` calls the core functions directly; `native-abi` runs the same JSON
//! request/response handler that the wasm module exports. Each run executes the whole workload
//! once; the first `W` runs are discarded as warm-up and the reported statistic is the median of
//! `R` further runs. Every output is verified against the fixture before timing starts.

use std::hint::black_box;
use std::time::Instant;
use tradelayer_core::legacy_txdecoder_audit::{
    source_decode_historical_method_unverified, HistoricalDecoderError,
};
use tradelayer_core::raw_block_ingest::UnverifiedRawBlockView;
use tradelayer_core::read_only::decode_hex;
use tradelayer_wasm::handle;
use tradelayer_wasm::json::quote;

const HISTORICAL: &str = include_str!("../../fixtures/legacy_txdecoder_historical_vectors.tsv");
const BLOCKS: [(&str, &str); 2] = [
    (
        "synthetic-4tx",
        include_str!("../../fixtures/raw_block_ingest_synthetic.hex"),
    ),
    (
        "review-block",
        include_str!("../../fixtures/tx_parser_review_block.hex"),
    ),
];

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

enum Expected {
    Value(String),
    Throw { name: String, message: String },
}

struct Row {
    method: String,
    body: String,
    expected: Expected,
}

fn corpus() -> Vec<Row> {
    HISTORICAL
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|line| {
            let f: Vec<&str> = line.split('\t').collect();
            let expected = if f[3] == "ok" {
                Expected::Value(unhex(f[4]))
            } else {
                Expected::Throw {
                    name: f[4].to_owned(),
                    message: unhex(f[5]),
                }
            };
            Row {
                method: f[0].to_owned(),
                body: unhex(f[2]),
                expected,
            }
        })
        .collect()
}

struct Args {
    iters: usize,
    block_iters: usize,
    runs: usize,
    warmup: usize,
}

fn args() -> Args {
    let mut a = Args {
        iters: 20,
        block_iters: 500,
        runs: 7,
        warmup: 3,
    };
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < argv.len() {
        let value = argv.get(i + 1).and_then(|v| v.parse::<usize>().ok());
        match (argv[i].as_str(), value) {
            ("--iters", Some(v)) => a.iters = v,
            ("--block-iters", Some(v)) => a.block_iters = v,
            ("--runs", Some(v)) => a.runs = v,
            ("--warmup", Some(v)) => a.warmup = v,
            (other, _) => panic!("bad argument {other}"),
        }
        i += 2;
    }
    a
}

fn report(runtime: &str, workload: &str, ops: usize, check: &str, mut times: Vec<f64>) {
    let runs = times.clone();
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = times[times.len() / 2];
    let list: Vec<String> = runs.iter().map(|t| format!("{t:.3}")).collect();
    println!(
        r#"{{"runtime":"{runtime}","workload":"{workload}","ops":{ops},"check":{},"median_ms":{median:.3},"min_ms":{:.3},"runs_ms":[{}]}}"#,
        quote(check),
        times[0],
        list.join(",")
    );
}

fn measure(a: &Args, mut once: impl FnMut()) -> Vec<f64> {
    for _ in 0..a.warmup {
        once();
    }
    (0..a.runs)
        .map(|_| {
            let t = Instant::now();
            once();
            t.elapsed().as_secs_f64() * 1000.0
        })
        .collect()
}

fn decode_direct(row: &Row) -> usize {
    match source_decode_historical_method_unverified(&row.method, &row.body) {
        Ok(body) => body.diagnostic_json().map_or(0, |s| s.len()),
        Err(HistoricalDecoderError::SourceThrow { .. }) => 1,
        Err(_) => 2,
    }
}

fn decode_corpus(a: &Args, rows: &[Row]) {
    // Verification pass (untimed): every direct result equals the fixture.
    let (mut ok, mut thrown, mut bytes) = (0usize, 0usize, 0usize);
    for row in rows {
        match (
            source_decode_historical_method_unverified(&row.method, &row.body),
            &row.expected,
        ) {
            (Ok(got), Expected::Value(want)) => {
                let json = got.diagnostic_json().unwrap();
                assert_eq!(&json, want, "{}", row.method);
                ok += 1;
                bytes += json.len();
            }
            (
                Err(HistoricalDecoderError::SourceThrow { name, message }),
                Expected::Throw {
                    name: n,
                    message: m,
                },
            ) => {
                assert_eq!(
                    (name, message.as_str()),
                    (n.as_str(), m.as_str()),
                    "{}",
                    row.method
                );
                thrown += 1;
            }
            (other, _) => panic!("{}: {other:?}", row.method),
        }
    }
    let check = format!("ok={ok},throw={thrown},bytes={bytes}");
    let ops = rows.len() * a.iters;

    let direct = measure(a, || {
        let mut acc = 0usize;
        for _ in 0..a.iters {
            for row in rows {
                acc += decode_direct(black_box(row));
            }
        }
        black_box(acc);
    });
    report("native-core", "decode_corpus", ops, &check, direct);

    let requests: Vec<Vec<u8>> = rows
        .iter()
        .map(|r| {
            format!(
                r#"{{"op":"decode_body","method":{},"body":{}}}"#,
                quote(&r.method),
                quote(&r.body)
            )
            .into_bytes()
        })
        .collect();
    for (req, row) in requests.iter().zip(rows) {
        let resp = String::from_utf8(handle(req)).unwrap();
        match &row.expected {
            Expected::Value(want) => assert!(resp.ends_with(&format!(r#""value":{want}}}"#))),
            Expected::Throw { .. } => assert!(resp.starts_with(r#"{"ok":false"#)),
        }
    }
    let abi = measure(a, || {
        let mut acc = 0usize;
        for _ in 0..a.iters {
            for req in &requests {
                acc += handle(black_box(req)).len();
            }
        }
        black_box(acc);
    });
    report("native-abi", "decode_corpus", ops, &check, abi);
}

fn block_direct(raw: &[u8]) -> usize {
    let view = UnverifiedRawBlockView::parse(raw, None).unwrap();
    let index = view.inspect_serial().unwrap();
    index.ordered_preindex().transaction_count() + usize::from(index.header_hash_raw()[0])
}

fn block_parse(a: &Args) {
    let mut check = Vec::new();
    let blocks: Vec<(String, String)> = BLOCKS
        .iter()
        .map(|(name, hex)| (name.to_string(), hex.trim().to_owned()))
        .collect();
    for (name, hex) in &blocks {
        let req = format!(r#"{{"op":"block_parse","hex":"{hex}"}}"#);
        let resp = String::from_utf8(handle(req.as_bytes())).unwrap();
        assert!(resp.starts_with(r#"{"ok":true"#), "{name}: {resp}");
        let field = |key: &str| -> String {
            let at = resp.find(key).unwrap() + key.len();
            resp[at..]
                .split([',', '}'])
                .next()
                .unwrap()
                .trim_matches('"')
                .to_owned()
        };
        check.push(format!(
            "{name}:{}:{}",
            field(r#""hash":"#),
            field(r#""txCount":"#)
        ));
    }
    let check = check.join(";");
    let ops = blocks.len() * a.block_iters;

    let direct = measure(a, || {
        let mut acc = 0usize;
        for _ in 0..a.block_iters {
            for (_, hex) in &blocks {
                let raw = decode_hex(black_box(hex)).unwrap();
                acc += block_direct(&raw);
            }
        }
        black_box(acc);
    });
    report("native-core", "block_parse", ops, &check, direct);

    let requests: Vec<Vec<u8>> = blocks
        .iter()
        .map(|(_, hex)| format!(r#"{{"op":"block_parse","hex":"{hex}"}}"#).into_bytes())
        .collect();
    let abi = measure(a, || {
        let mut acc = 0usize;
        for _ in 0..a.block_iters {
            for req in &requests {
                acc += handle(black_box(req)).len();
            }
        }
        black_box(acc);
    });
    report("native-abi", "block_parse", ops, &check, abi);
}

fn main() {
    let a = args();
    let rows = corpus();
    println!(
        r#"{{"meta":{{"rows":{},"iters":{},"block_iters":{},"runs":{},"warmup":{},"os":"{}","arch":"{}"}}}}"#,
        rows.len(),
        a.iters,
        a.block_iters,
        a.runs,
        a.warmup,
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    decode_corpus(&a, &rows);
    block_parse(&a);
}
