# tradelayer-core 0.1.0-rc.1

RC-1 is the `tradelayer-core` library at the commit that carries this file. It is a release candidate when every gate below passes on that commit. Each gate names the command that decides it.

## Gates

| # | Gate | Command | Result on the RC commit |
|---|---|---|---|
| 1 | Formatting, strict Clippy, every test target, wallet-order oracle | `bash scripts/ci-preflight.sh --full` (needs `python3` on PATH and the rustup `cargo` proxy ahead of the toolchain directory) | see `fixtures/reference_fidelity/rc-0.1.0-rc.1-validation.json` |
| 2 | Contract-trading state parity with the pinned JS on seeded multi-block histories | `cargo test --locked --test source_contract_runtime` and `node scripts/source_contract_history_vectors.cjs --check --module-root <tradelayer.js root>` | 6/6 Rust tests; JS regeneration byte-identical under BigNumber 9.3.1 and the 9.1.2 source lock |
| 3 | Real child-process crash recovery | part of gate 1: `actual_child_crash_boundaries_recover_retry_and_support_witnesses`, `real_child_crashes_recover_only_before_intermediate_or_prepared_after`, `actual_process_exit_matrix_generation_current_and_retention`, `active_wal_real_exit_matrix` | pass in gate 1 |
| 4 | Zero third-party crates, offline, pinned toolchain | `python3 scripts/check_rust_supply_chain.py` | PASS: one first-party package, zero third-party crates |
| 5 | Real-block scan CPU and peak memory recorded | `scripts/bench_stage8d6_real_blocks.py` on the testnet4 corpus | `docs/STAGE8D6_HASHMAP_PROMOTION_AND_REAL_BLOCK_BENCH.md`, Windows and Linux tables |
| 6 | GitHub CI green on the RC commit | `ci.yml`, `kani.yml`, `lean.yml` on a pull request or `main` | runs only on `main` or a pull request |

## Gate 2 in detail

`scripts/source_contract_history_vectors.cjs` applies anchored transforms to the unchanged composed driver `scripts/source_contract_execution_vectors.cjs` and builds three seeded 15-block histories for one perpetual contract: an oracle price random walk (90 to 110), one to three random buy or sell orders per block from two funded actors, a queue drain every block and a fee flush every fifth block. The genuine pinned modules (types, logic, orderbook, clearing, contract registry, margin map, tally, oracle, insurance, IOU, trade history) execute every action over the fixture database. After each of the 171 actions the driver records the outcome and the complete state: every persisted collection, the clearing, oracle, insurance and tally caches, and module instances. The result is `fixtures/source_contract_history_vectors.tsv.gz`.

`tests/source_contract_runtime.rs` replays the same commands through `SourceContractRuntime` under the GlobalClearingV2, OptionLifecycleV4 and ProofLifecycleV6 policies and compares the full state after every action. Changing one digit of one balance in one recorded observation fails the comparison at that action and field.

The genuine source throws `ContractRegistry.isNativeContract is not a function` from the fee flush in each history (twice per history). The Rust runtime reproduces that throw and its partial state, as it does for the existing `matched-fee-flush` scene.

## Scope of RC-1

RC-1 covers the library's ledger, contract-trading, clearing, wire codec, index and durable-store paths as exercised by gates 1 to 5. These are separate work items, outside RC-1:

- Authenticated chain admission: parent UTXO/all-input checks and complete-block proof composition.
- A production daemon that owns the Ledger; `docs/DAEMON.md` describes the current observation service.
- Mainnet real-block measurements; the recorded corpus is testnet4.
- Power-loss durability on Windows, where directory fsync is a no-op (`src/storage.rs`).
- The mock oracle-abuse exercise against the separate UTXORef consumer repository.
- Reproducing NeDB executor scheduling: NeDB is not consensus (`CLAUDE_HANDOFF.md`, "Startup ordering").
