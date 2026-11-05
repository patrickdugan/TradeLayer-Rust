//! Native benchmark of the economic playground: K blocks of market-maker requoting plus random
//! taker flow through `handle`, the same JSON path the wasm export runs. `playground/bench.mjs`
//! replays the identical schedule through the wasm; compare the `checksum` fields.
//!
//!   cargo run --release --example bench_playground -- [blocks=1000] [seed=1] [marks]

use std::time::Instant;
use tradelayer_core::client::RpcValue;
use tradelayer_wasm::handle;

const ATOMS: u64 = 100_000_000;
const LEVELS: u64 = 3;
const SIZE: &str = "0.5";
const TRADERS: u32 = 4;
const HEIGHT0: u64 = 1_000_000;

/// mulberry32; `playground/bench.mjs` implements the same generator.
struct Rng(u32);
impl Rng {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x6D2B_79F5);
        let mut t = self.0;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
        f64::from(t ^ (t >> 14)) / 4_294_967_296.0
    }
}

fn cents(c: u64) -> String {
    format!("{}.{:02}", c / 100, c % 100)
}

fn call(request: &str) -> RpcValue {
    let text = String::from_utf8(handle(request.as_bytes())).expect("utf8");
    let value = RpcValue::parse(&text).expect("json");
    let ok = value.field("ok").unwrap().unwrap();
    if ok.as_json() != "true" {
        let state = String::from_utf8(handle(br#"{"op":"pg_state","session":"bench","fills":0}"#))
            .unwrap_or_default();
        panic!(
            "request failed: {text}
{request}
{state}"
        );
    }
    value
}

/// One block of ops for block `k`, advancing the generator and the price walk.
fn schedule(rng: &mut Rng, price: &mut f64) -> String {
    *price *= 1.0 + (rng.next() - 0.5) * 2.0 * 0.001;
    let price_1e4 = (*price * 10_000.0 + 0.5).floor() as u64;
    let mid = (price_1e4 + 50) / 100;
    let mut ops = vec![
        format!(r#"{{"t":"oracle","price_1e4":{price_1e4}}}"#),
        r#"{"t":"cancel_all","account":"mm"}"#.to_owned(),
    ];
    for i in 0..LEVELS {
        let off = mid * (5 + 5 * i) / 10_000;
        ops.push(format!(
            r#"{{"t":"order","account":"mm","side":"buy","qty":"{SIZE}","price":"{}","post":true}}"#,
            cents(mid - off)
        ));
        ops.push(format!(
            r#"{{"t":"order","account":"mm","side":"sell","qty":"{SIZE}","price":"{}","post":true}}"#,
            cents(mid + off)
        ));
    }
    if rng.next() < 0.6 {
        let trader = (rng.next() * f64::from(TRADERS)).floor() as u32;
        let buy = rng.next() < 0.5;
        let qty = format!("0.{}", 1 + (rng.next() * 3.0).floor() as u64);
        let side = if buy { "buy" } else { "sell" };
        if rng.next() < 0.5 {
            ops.push(format!(
                r#"{{"t":"order","account":"t{trader}","side":"{side}","qty":"{qty}","market":true,"price":"{}"}}"#,
                cents(mid)
            ));
        } else {
            let px = if buy {
                mid * 10_020 / 10_000
            } else {
                mid * 9_980 / 10_000
            };
            ops.push(format!(
                r#"{{"t":"order","account":"t{trader}","side":"{side}","qty":"{qty}","price":"{}"}}"#,
                cents(px)
            ));
        }
    }
    ops.join(",")
}

fn main() {
    let mut args = std::env::args().skip(1);
    let blocks: u64 = args.next().and_then(|a| a.parse().ok()).unwrap_or(1000);
    let seed: u32 = args.next().and_then(|a| a.parse().ok()).unwrap_or(1);
    let marks = args.next().is_some_and(|a| a == "marks");
    let mut accounts = vec![format!(
        r#"{{"name":"mm","tusd_atoms":{}}}"#,
        5_000_000 * ATOMS
    )];
    for t in 0..TRADERS {
        accounts.push(format!(
            r#"{{"name":"t{t}","tusd_atoms":{}}}"#,
            500_000 * ATOMS
        ));
    }
    call(&format!(
        r#"{{"op":"pg_new","id":"bench","anchor_height":{HEIGHT0},"price_1e4":1000000000,"marks":{marks},"accounts":[{}]}}"#,
        accounts.join(",")
    ));
    let mut rng = Rng(seed);
    let mut price = 100_000.0_f64;
    let mut times = Vec::with_capacity(blocks as usize);
    let mut fills = 0u64;
    for k in 0..blocks {
        let ops = schedule(&mut rng, &mut price);
        let request = format!(
            r#"{{"op":"pg_block","session":"bench","height":{},"ops":[{ops}]}}"#,
            HEIGHT0 + k
        );
        let start = Instant::now();
        let out = call(&request);
        times.push(start.elapsed().as_secs_f64() * 1000.0);
        fills += out.field("fill_count").unwrap().unwrap().as_u64().unwrap();
    }
    let state = call(r#"{"op":"pg_state","session":"bench","fills":0}"#);
    let mm_position = state
        .field("positions")
        .unwrap()
        .unwrap()
        .as_array()
        .unwrap()
        .into_iter()
        .find(|p| p.field("account").unwrap().unwrap().as_string().unwrap() == "mm")
        .map_or_else(
            || "0".to_owned(),
            |p| p.field("contracts").unwrap().unwrap().as_string().unwrap(),
        );
    let net = state
        .field("net_contracts")
        .unwrap()
        .unwrap()
        .as_string()
        .unwrap();
    let total_ms: f64 = times.iter().sum();
    let mut sorted = times.clone();
    sorted.sort_by(f64::total_cmp);
    let median = sorted[sorted.len() / 2];
    let first_tenth: f64 =
        times[..times.len() / 10].iter().sum::<f64>() / (times.len() / 10).max(1) as f64;
    let last_tenth: f64 = times[times.len() - times.len() / 10..].iter().sum::<f64>()
        / (times.len() / 10).max(1) as f64;
    let windows: Vec<String> = [100usize, 300, 500, 700, 1000]
        .into_iter()
        .filter(|&n| n <= times.len())
        .map(|n| {
            let mean = times[n - 100..n].iter().sum::<f64>() / 100.0;
            format!("\"{n}\":{mean:.2}")
        })
        .collect();
    println!(
        "{{\"window_mean_ms\":{{{}}},\"runtime\":\"native\",\"blocks\":{blocks},\"seed\":{seed},\"marks\":{marks},\"median_ms\":{median:.3},\"mean_ms\":{:.3},\"first_tenth_ms\":{first_tenth:.3},\"last_tenth_ms\":{last_tenth:.3},\"blocks_per_s_median\":{:.1},\"blocks_per_s_total\":{:.1},\"fills\":{fills},\"checksum\":{{\"fill_total\":{},\"mm_contracts\":\"{mm_position}\",\"net\":\"{net}\",\"price_1e4\":{}}}}}",
        windows.join(","),
        total_ms / blocks as f64,
        1000.0 / median,
        blocks as f64 / (total_ms / 1000.0),
        state.field("fill_total").unwrap().unwrap().as_json(),
        state
            .field("oracle")
            .unwrap()
            .unwrap()
            .field("price_1e4")
            .unwrap()
            .unwrap()
            .as_json(),
    );
}
