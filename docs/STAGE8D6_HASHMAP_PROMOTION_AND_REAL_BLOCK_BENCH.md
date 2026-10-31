# Stage 8D6: promote the HashMap-only TXID reducer for one-team CPU preindex

Baseline: merged `TradeLayer-Rust main@c7a37eb90b1ea335051bfc3cb7712116b372cfb7`. This is a narrow successor to exploratory [PR #139](https://github.com/patrickdugan/TradeLayer-Rust/pull/139). Other draft CPU schedulers and UTXO research remain independent.

## Change and trust boundary

The opt-in, block-wide `UnverifiedRawBlockView::inspect_parallel_one_team` now preallocates a standard, randomly seeded `HashMap<[u8; 32], usize>` for duplicate-TXID detection. The workers still hash and classify immutable prepared transactions using the already merged dynamic atomic microbatch scheduler. The coordinator still joins all workers and processes results *in their original block order*, maintaining the original first/second duplicate indices, earliest scan errors, carrier counts and candidate ordering.

`inspect_parallel_one_team_tree_reference` retains the original `BTreeMap` reduction for benchmarks and parity testing. The **default** `inspect_parallel`, serial path, historic owned-transaction APIs, Merkle algorithm, optional dependency waves and trusted/consensus denials remain unchanged. For a one-worker call, both one-team routes intentionally use the original serial code. The in-place Merkle experiment from #139 is excluded: its measured isolated gain was negligible and is irrelevant to this patch.

The extra memory of the preallocated `HashMap` is bounded by `MAX_BLOCK_TRANSACTIONS` after checking the existing `MAX_BLOCK_INPUT_BYTES` limit. The one-team scanner already allocates one result slot per transaction. The standard `HashMap` uses a process-random seed and does not depend on iteration order for any consensus-relevant decision. This produces *unverified observations*, never Bitcoin chain/UTXO/script validation or TradeLayer economic state admission.

## Prior synthetic findings

A [green Stage8D5 counterbalanced run](https://github.com/patrickdugan/TradeLayer-Rust/actions/runs/40242371422) gave original one-team vs HashMap-only prepared-only median times:

| TX / SegWit / workers | BTreeMap | HashMap only |
| --- | ---: | ---: |
| 1,000 / 0% / 1 | 2.075 ms | 2.083 ms |
| 4,000 / 50% / 4 | 7.785 ms | 7.157 ms |
| 10,000 / 95% / 8 | 22.325 ms | 20.724 ms |

These GitHub-hosted synthetic results identify the candidate, not real Bitcoin Core throughput. Stage8D6 independently measures its narrow promotion against the retained reference with alternating variant order. There is no CI speed threshold.

## Real-block CPU and peak-memory harness

`tradelayer-real-block-reduce-bench` reads a *local* Bitcoin Core `getblock(blockhash,0)` hexadecimal file, checks raw syntax and locally recomputed TXID Merkle root using the existing `UnverifiedRawBlockView`. It times **prepared-only CPU scanning** for one of `tree` or `hash`; the block fetch, disk read and first structural parse are excluded from reported scan times. One variant per process permits Linux `/proc/self/status` `VmHWM` peak-RSS measurement, including parsing and allocation. It prints identity and sample count for audit, and explicitly refuses full consensus claims.

`scripts/bench_stage8d6_real_blocks.py` launches independent processes for both reducers, alternates their order across paired trials, validates header identity and transaction count, and reports their median scan times and median process-high-water memory as JSON Lines, one per source block. All inputs are operator-supplied local files; the scripts perform no RPC and require no secrets.

Example on a host that already runs Bitcoin Core:

```sh
# Choose heights using YOUR local node. Select varied transaction counts and
# witness/OP_RETURN mixes; heights below are examples, not claimed TX counts.
for height in 700000 800000 850000; do
  hash="$(bitcoin-cli getblockhash "$height")"
  bitcoin-cli getblock "$hash" 0 > "/tmp/bitcoin-$height.hex"
done

cargo build --all-targets --release --frozen
python3 scripts/bench_stage8d6_real_blocks.py --workers 8 \
  --repeats 5 --trials 5 \
  /tmp/bitcoin-700000.hex /tmp/bitcoin-800000.hex /tmp/bitcoin-850000.hex
```

Each `tree`/ `hash` invocation has the same parsed block and result-slot budget. The printed peak RSS is *whole-process* high-water memory, including startup, input bytes and layout parsing. Evaluate median and tail latency across several real blocks on a controlled CPU; GitHub runner measurements can shift with noise and host contention. Do not infer that reading a raw block from a file authenticates active chain membership or that hashing speed predicts script validation throughput.

A frozen historical **Bitcoin mainnet block 1** header and coinbase already in `fixtures/` are assembled by `scripts/make_stage8d6_mainnet_block1.py` for a fully offline, public-source CI smoke test. The script independently verifies the coinbase TXID equals the published header Merkle. That genuine one-transaction block checks the input format and safety boundary; it is too small to evaluate parallel speedup. Representative recent blocks remain an explicit operator-side measurement requirement.

## Gates before merging

The new Rust regression module compares the HashMap-only path with the original one-team reducer and windowed/serial reference across worker counts 1/2/4/8/16, windows 1/7/64/256/512, 0/50/95%-SegWit blocks, 10k TX stress and duplicate first/second indices. It also checks the source historical block-1 fixture, invalid configurations and all permanent-denial gates.

Require frozen formatting, Clippy, tests, independent release build, strict aggregate check and historical block-1 smoke on the exact PR head. **Keep draft until a repeated, same-host real-block corpus documents CPU latency distribution and peak RSS**, and a reviewer accepts the memory tradeoff. The default windowed and serial paths stay as published.

## Same-host real-block corpus: testnet4 (2026-10-31)

Seven testnet4 blocks were exported with `getblock <hash> 0` from a local Bitcoin Core 31.1 node and measured on one host: AMD Ryzen AI 9 365 (10 cores / 20 threads), 31.1 GB RAM, Windows 11, release build with Rust 1.90.0 MSVC. The node was stopped before measuring. Each row is the median of 5 paired process trials with alternating variant order, each trial taking the median of 5 prepared-only scans. Peak memory is the child's Windows peak working set (`K32GetProcessMemoryInfo` on the exited child's handle), the counterpart of Linux `VmHWM`. Raw rows, including every trial's scan time: `fixtures/bench/stage8d6_testnet4_corpus.jsonl`.

| Height | TX | Raw bytes | Workers | BTreeMap ms | HashMap ms | Tree/Hash | Peak WS KiB (tree / hash) |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 72200 | 5,715 | 1,640,282 | 8 | 6.911 | 6.070 | 1.139 | 12,140 / 12,296 |
| 66700 | 3,856 | 1,358,585 | 8 | 4.992 | 4.565 | 1.094 | 10,812 / 10,568 |
| 74800 | 3,337 | 1,465,566 | 8 | 4.392 | 3.894 | 1.128 | 10,848 / 10,732 |
| 55700 | 2,516 | 681,665 | 8 | 3.125 | 2.784 | 1.123 | 8,044 / 8,024 |
| 69500 | 1,009 | 418,373 | 8 | 1.555 | 1.466 | 1.061 | 6,752 / 6,748 |
| 30500 | 296 | 69,736 | 8 | 0.797 | 0.817 | 0.976 | 5,468 / 5,472 |
| 95000 | 152 | 67,509 | 8 | 0.625 | 0.628 | 0.995 | 5,448 / 5,452 |
| 72200 | 5,715 | 1,640,282 | 16 | 6.185 | 5.422 | 1.141 | 12,148 / 12,320 |
| 66700 | 3,856 | 1,358,585 | 16 | 4.487 | 4.137 | 1.085 | 10,584 / 10,620 |
| 74800 | 3,337 | 1,465,566 | 16 | 4.129 | 3.698 | 1.117 | 10,744 / 10,724 |
| 55700 | 2,516 | 681,665 | 16 | 3.112 | 2.838 | 1.097 | 8,164 / 8,144 |
| 69500 | 1,009 | 418,373 | 16 | 1.795 | 1.652 | 1.087 | 6,868 / 6,872 |
| 30500 | 296 | 69,736 | 16 | 1.080 | 1.090 | 0.991 | 5,492 / 5,484 |
| 95000 | 152 | 67,509 | 16 | 0.932 | 0.933 | 0.999 | 5,480 / 5,500 |

On blocks of 1,000 or more transactions the BTreeMap reference takes 6 to 14% longer than the HashMap reducer at both worker counts. Below 300 transactions the two are within 2.5% of each other. Peak working set is the same for both reducers (largest difference 2.3%) and grows from 5.4 MB to 12.3 MB with block size. Going from 8 to 16 workers speeds up only the three largest blocks and slows the other four.

Running the corpus found a defect: the bench decoded the block with `read_only::decode_hex`, which is bounded by one raw transaction (1,000,000 bytes), so every block above 1 MB raw failed with `InvalidHex`. It now decodes up to `MAX_UNVERIFIED_RAW_BLOCK_BYTES` (4,000,000). CI's block-1 smoke (215 bytes) could not catch this. The harness also records the Windows peak working set and every trial's scan time.

### Same corpus under Linux (WSL2)

The same seven files and the same commit, built with Rust 1.90.0 for `x86_64-unknown-linux-gnu` inside WSL2 (kernel 6.18.54.1-microsoft-standard-WSL2, 20 vCPUs, 19 GB) on the same host, measured with the unchanged Linux path: peak memory is each process's own `VmHWM`. Raw rows: `fixtures/bench/stage8d6_testnet4_corpus_wsl.jsonl`.

| Height | TX | Workers | BTreeMap ms | HashMap ms | Tree/Hash | VmHWM KiB (tree / hash) |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 72200 | 5,715 | 8 | 7.006 | 6.757 | 1.037 | 9,916 / 9,572 |
| 66700 | 3,856 | 8 | 5.111 | 4.905 | 1.042 | 8,344 / 8,244 |
| 74800 | 3,337 | 8 | 5.196 | 4.774 | 1.088 | 8,308 / 8,124 |
| 55700 | 2,516 | 8 | 3.968 | 3.850 | 1.031 | 5,808 / 5,512 |
| 69500 | 1,009 | 8 | 2.174 | 1.972 | 1.102 | 4,464 / 4,388 |
| 30500 | 296 | 8 | 1.344 | 1.204 | 1.116 | 3,092 / 3,064 |
| 95000 | 152 | 8 | 1.078 | 1.130 | 0.954 | 3,064 / 2,932 |
| 72200 | 5,715 | 16 | 7.925 | 7.240 | 1.095 | 10,040 / 9,632 |
| 66700 | 3,856 | 16 | 5.873 | 5.547 | 1.059 | 8,340 / 8,080 |
| 74800 | 3,337 | 16 | 5.216 | 5.090 | 1.025 | 8,552 / 7,996 |
| 55700 | 2,516 | 16 | 4.196 | 4.131 | 1.016 | 5,916 / 5,500 |
| 69500 | 1,009 | 16 | 2.816 | 2.769 | 1.017 | 4,600 / 4,476 |
| 30500 | 296 | 16 | 2.138 | 2.102 | 1.017 | 3,156 / 3,148 |
| 95000 | 152 | 16 | 1.828 | 1.755 | 1.042 | 3,052 / 3,012 |

Under Linux the BTreeMap reference takes 2 to 12% longer than the HashMap reducer on every block except the 152-transaction block at 8 workers, where it is 5% shorter. The HashMap reducer's peak RSS is lower in all 14 rows, by 0.3 to 7.6%, and peak RSS ranges from 2.9 MB to 10.0 MB. Sixteen workers are slower than eight on every block. Scan times under WSL2 are higher than the Windows-native run on the same CPU for every block, so the two tables compare reducers within one OS, not the two OSes against each other.

These are testnet4 blocks. Their transaction and witness mix is not mainnet's, and the scan measures TXID hashing and duplicate detection, not script validation or state application.
