# tradelayer-wasm

Raw-ABI WebAssembly binding of `tradelayer-core`. One JSON request goes in, one JSON response comes out. The module has no imports, runs in browsers and in Node 18 or later, and the core crate stays free of third-party dependencies. The same handler (`tradelayer_wasm::handle`) runs natively in tests and benchmarks.

This package covers the stateless codec and raw-block operations. Operations whose name starts with `pg_` belong to the economic playground and are documented in [PLAYGROUND.md](PLAYGROUND.md).

## ABI

Exports of `tradelayer_wasm.wasm`:

| export | meaning |
|---|---|
| `tl_alloc(len) -> ptr` | allocate a request buffer |
| `tl_free(ptr, len)` | release a buffer from `tl_alloc` or `tl_call` |
| `tl_call(ptr, len) -> ptr` | run one request; result is `[len: u32 LE][json bytes]`, released with `tl_free(ptr, 4 + len)` |

`js/tradelayer.mjs` wraps this: `const tl = await loadTradeLayer(bytes); tl.call({ op: 'version' })`.

Responses are `{"ok":true,...}` or `{"ok":false,"code":"...","message":"..."}`. `code` is the core error's variant name (`InvalidMarker`, `TruncatedBlockHeader`, `RequestedHeaderHashMismatch`, ...), `BadRequest` for malformed requests, `UnknownOp`, or `SourceThrow` when the pinned legacy decoder throws (message is `Name: text`). Requests above 16 MiB are refused.

## Codec and block operations

| op | request | response |
|---|---|---|
| `version` | none | `crate` |
| `wire_frame` | `payload` (`tl` + base-36 type + body) | `type`, `body` |
| `wire_parse` | `payload` | `type`, `kind` (`activate`, `issue`, `send`, `send_all`, `grant`, `redeem`) and the typed fields; amounts are decimal strings in atoms |
| `decode` | `payload` | `type`, `method`, `value`: legacy txDecoder semantics, decoder picked by the Types route table |
| `decode_body` | `method` (a txDecoder export), `body` (text after the type selector) | `method`, `value` |
| `encode` | `method` (a txEncoder export), `input` (JSON arguments) | `method`, `encoded` (`null` where the source returns `undefined`) |
| `tx_inspect` | `hex` (one raw transaction) | `txid`, `wtxid`, `hasWitness`, `inputs`, `outputs`, `opReturnOutputs`, `bytes` |
| `block_parse` | `hex` (raw block), optional `expectedHash` | `hash`, `version`, `prevHash`, `merkleRoot`, `merkleMatchesHeader`, `time`, `bits`, `nonce`, `txCount`, `weight`, `strippedBytes`, `opReturnOutputs`, `txids` |

`value` is the core's tagged diagnostic JSON, for example `["number","<f64 bits as hex>"]`, `["string","x"]`, `["object",[["key",value],...]]`, `["undefined"]`, so `undefined`, NaN and signed zero survive. Hashes are in display order (byte-reversed, as block explorers print them). `decode`, `decode_body`, `encode`, `wire_parse` and `block_parse` observe bytes only: none of them authenticates a transaction or a block (no proof of work, scripts, UTXOs or signatures).

Examples:

```js
tl.call({ op: 'decode', payload: 'tl20;alice;5;2' });          // type 2, decodeSend
tl.call({ op: 'wire_parse', payload: 'tl20;alice;5;2' });      // kind "send", amountAtoms "200000000"
tl.call({ op: 'block_parse', hex: blockHex });                  // hash, txids, merkleRoot
```

## Tests

`tests/codec.rs` runs the core's frozen corpora through the ABI: all 1447 historical txDecoder cases (values and exact throws), the encoder and decoder rows of `wire_roundtrip_vectors.tsv`, the 16 rows of `wire_vectors.tsv`, and the block and transaction hashes of `raw_block_ingest_synthetic.hex`, `tx_parser_review_block.hex` and the Bitcoin block 1 coinbase.

## Build

Run cargo in WSL: the mingw linker on Windows fails on this cdylib.

```sh
cargo test --locked --offline                                              # in wasm/
cargo clippy --locked --offline --all-targets -- -D warnings
cargo clippy --locked --offline --lib --target wasm32-unknown-unknown -- -D warnings
bash wasm/scripts/build-wasm.sh /mnt/c/path/to/out                         # release wasm
```

`build-wasm.sh` builds with `+1.90.0` for `wasm32-unknown-unknown` and copies `tradelayer_wasm.wasm` to the output directory. The directory must be an absolute POSIX path (`/mnt/c/...`); a `C:/...` path is relative inside WSL and would land inside the crate, so the script refuses it. `TL_TARGET_DIR` sets the cargo target directory. A cold release build took 20 minutes (single-threaded LTO); the module is 537,362 bytes.

## Benchmarks

Workloads, identical inputs for every runtime:

- `decode_corpus`: all 1447 rows of `fixtures/legacy_txdecoder_historical_vectors.tsv` (1419 decode, 28 throw), each decoded 5 times per run, so 7235 decodes. Output is the tagged JSON; every runtime verifies its output against the fixture before timing, and `summarize.mjs` refuses to merge runs whose output checksum differs.
- `block_parse`: the two fixture blocks `raw_block_ingest_synthetic.hex` (4 transactions, 370 bytes) and `tx_parser_review_block.hex` (11 transactions, 27,609 bytes), each parsed 500 times per run (1000 parses): header hash, all txids (SHA-256d of every transaction), merkle root and the header comparison.

Protocol: 3 warm-up runs, then 7 timed runs of the whole workload; the table reports the median and the minimum. Runtimes:

| name | what runs |
|---|---|
| `native-core` | `examples/bench_codec.rs`: core functions called directly (for blocks: hex decode, parse, serial index) |
| `native-abi` | same example: the JSON `handle()` the wasm module exports, pre-built request bytes |
| `wasm-raw` | `bench/bench_wasm.mjs`: wasm module under Node through `loadTradeLayer`, `callRaw` with pre-built request strings (UTF-8 encode, call, decode included) |
| `wasm-call` | same: `call()` with `JSON.stringify` and `JSON.parse` in the loop |
| `legacy-js` | `bench/bench_legacy.cjs`: the pinned legacy `txDecoder.js` (tradelayer.js `dca0888b`) with its BigNumber, Base94 and Base256 sources in a Node VM, decode plus tagged `JSON.stringify` |
| `legacy-js-decode-only` | same, without serialization |

The legacy loading code is `bench/legacy_loader.cjs`, taken from `scripts/audit_legacy_txdecoder_source.cjs`; it reads the pinned sources from `fixtures/` and installs no packages.

There is no legacy JavaScript row for `block_parse`. The pinned `txIndex.js` obtains blocks through the Core RPC client (`client.getblock`) and has no raw-block, txid or merkle code that runs offline, so no equivalent workload exists and none was written for this table.

Commands (paths relative to the repository root):

```sh
# native, Windows (gnu toolchain; a bin links fine, only the cdylib fails)
cargo +1.90.0-x86_64-pc-windows-gnu build --release --example bench_codec   # in wasm/
$CARGO_TARGET_DIR/release/examples/bench_codec.exe --iters 5 --block-iters 500 --runs 7 --warmup 3 > native-windows.jsonl
# native, WSL
cargo +1.90.0 build --release --example bench_codec                          # in wasm/, inside WSL
$CARGO_TARGET_DIR/release/examples/bench_codec --iters 5 --block-iters 500 --runs 7 --warmup 3 > native-wsl.jsonl
# wasm and legacy under Node 24
node wasm/bench/bench_wasm.mjs out/tradelayer_wasm.wasm --iters 5 --block-iters 500 --runs 7 --warmup 3 > wasm.jsonl
node wasm/bench/bench_legacy.cjs --iters 5 --runs 7 --warmup 3 > legacy.jsonl
node wasm/bench/summarize.mjs native-windows.jsonl native-wsl.jsonl wasm.jsonl legacy.jsonl
```

### Results

Machine: AMD Ryzen AI 9 365 (20 logical cores), 31 GB RAM, Windows 11 Home 10.0.26200, WSL2 Ubuntu 24.04 (kernel 6.18.54.1-microsoft-standard-WSL2). Rust 1.90.0 for every target (`x86_64-pc-windows-gnu`, `x86_64-unknown-linux-gnu`, `wasm32-unknown-unknown`), release profile (`opt-level = 3`, LTO, `codegen-units = 1`, `panic = "abort"`). Node v24.19.0 (V8 13.6.233.17-node.51) on Windows. Where each number ran: `native (windows)` and both Node rows on Windows, `native (wsl)` inside WSL2. Run on 2026-11-05; the WSL load average was 1.9 when the runs started. The last column is relative to `native-core (windows)`.

`decode_corpus`: 7235 decodes per run.

| runtime | median ms | min ms | decodes/s | vs native-core (windows) |
|---|---:|---:|---:|---:|
| native-core (windows) | 62.82 | 57.77 | 115,176 | 1.00 |
| native-core (wsl) | 52.73 | 51.80 | 137,221 | 0.84 |
| native-abi (windows) | 88.00 | 85.96 | 82,216 | 1.40 |
| native-abi (wsl) | 70.13 | 65.30 | 103,161 | 1.12 |
| wasm-raw (Node 24) | 111.50 | 101.37 | 64,887 | 1.78 |
| wasm-call (Node 24) | 155.52 | 117.05 | 46,521 | 2.48 |
| legacy-js (Node 24) | 3934.03 | 3310.20 | 1,839 | 62.63 |
| legacy-js-decode-only (Node 24) | 3858.28 | 3553.57 | 1,875 | 61.42 |

`block_parse`: 1000 block parses per run.

| runtime | median ms | min ms | parses/s | vs native-core (windows) |
|---|---:|---:|---:|---:|
| native-core (windows) | 78.98 | 77.05 | 12,661 | 1.00 |
| native-core (wsl) | 94.70 | 87.74 | 10,559 | 1.20 |
| native-abi (windows) | 484.82 | 451.93 | 2,063 | 6.14 |
| native-abi (wsl) | 257.90 | 237.50 | 3,877 | 3.27 |
| wasm-raw (Node 24) | 717.24 | 705.09 | 1,394 | 9.08 |
| legacy JavaScript | no equivalent | | | |

Reading the numbers:

- The wasm module decodes the legacy corpus 35 times faster than the legacy JavaScript decoder (3934 ms against 111.5 ms, medians) and is 1.8 to 2.1 times slower than the same code compiled natively.
- The ABI rows for `block_parse` cost 3.3 to 6.1 times the direct core call, against 1.1 to 1.4 times for `decode_corpus`. A block request carries the block as one hex string (55 KB for the review block) and the handler runs the core JSON validator over the request text several times (request parse, `op`, `hex`, `expectedHash`) before hex decoding; decode requests are tens of bytes. The ABI takes JSON only, so the block bytes cannot be passed raw.
- A native run takes 50 to 100 ms, so the min and median columns differ by up to 15 percent on the native rows; the minimum shows the spread.
