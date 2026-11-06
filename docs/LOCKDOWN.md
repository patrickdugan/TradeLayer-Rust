# Code lockdown: tradelayer-core 0.1.0-rc.2 (Bitcoin launch)

Bitcoin is the launch chain. Litecoin keeps its parameters in this crate (TL Vest and the
Litecoin consensus code) and ships later as its own client. This file records what is frozen
at tag `v0.1.0-rc.2`, how each part was verified, and what is outside the freeze.

## In the freeze

| Area | What it does | Verification |
|---|---|---|
| RC-1 library | ledger, contract trading, clearing, wire codec, index and durable stores | RC-1 gates 1-6 (`docs/RC.md`) |
| Observer (`tl_observe`) | Bitcoin mainnet and testnet4 observation, head log, fork search before exit | `tests/runtime.rs`; testnet4 rehearsal: 17 drills PASS, 3 live reorgs auto-reconciled (`ops/rehearsal/README.md`) |
| Admission slice 1 (`src/admission.rs`, `tl_admit`) | senders authenticated by the input-0 signature (P2PKH, P2WPKH, SIGHASH_ALL) against claimed parent outputs; txid Merkle inclusion; BIP141 witness binding | `tests/admission.rs` 18/18, including a lying-source relabel test; 20 live testnet4 blocks |
| Node (`tradelayer-node`) | admits observed blocks, applies them through the source runner, rolls back on reorgs, serves the listener API on loopback | `tests/node_ingest.rs` 12/12; live testnet4 run on the witness box (`docs/NODE.md`) |
| Absolute heights | the economic anchor is the activation tip; rules see Bitcoin heights | `tests/economic_anchor.rs` 8/8; anchored differential tests |
| Incremental audits | `AuditPolicy::Periodic(64)`: live and delta checks every block, full audit every 64 blocks | `tests/economic_audit_policy.rs`: identical results and snapshot bytes under every policy |
| Bounded history | receipts archived to a hash-committed log; rollback window as floor state plus block inputs; internal legs by sequence watermark | `src/economic_history_tests.rs`, `tests/economic_chain_window.rs`, `src/economic_legs_tests.rs` |
| Listener API | `tradelayer-listenerd`; `tl_check_kyc` and `tl_check_commits` from TradeLayer state | `tests/listenerd.rs`, `tests/service_kyc_commits.rs` |
| WebAssembly binding (`wasm/`) | codec, block and economic playground ops for browsers and Node | `wasm_binding` CI job; `wasm/README.md` |
| Supply chain | zero third-party crates; the wasm binding is the one admitted first-party manifest | `scripts/check_rust_supply_chain.py`, 12 mutation tests |

## Verification of this tag

Run on the release commit (`release: tradelayer-core 0.1.0-rc.2`, whose tree this file
adds to without code changes):

- Full local pre-push check (`scripts/ci-preflight.sh --full`, Windows, Rust 1.90.0, the
  CI-identical legacy JS package set): 2,837 tests across 322 targets, 0 failures, together
  with formatting (batched rustfmt over every target root and the include lists), Clippy with
  warnings denied, doctests, the logic inventory and the supply-chain gate.
- `wasm/` in WSL: formatting, 16 tests, Clippy native and `wasm32-unknown-unknown`.
- GitHub Actions on `main`: Rust CI (`tests`, `release`, `executable_contract`, `wasm_binding`,
  `check`) and the Kani workflow; the tag is set only after both pass on this commit.
- Earlier commits of the freeze passed the same full check and Rust CI one by one: the
  incremental audits (2,790 tests), node and listener queries (2,810), bounded receipts
  (2,819), the economic anchor (2,830) and the rollback window (2,837).

After the tag, the desktop wallet ran `tradelayer-node` from this commit as its sidecar against a
local Bitcoin Core 31.1 testnet4 node at height 159,367 (tradelayer-wallet `feat/rust-port`
0b2a79a, `scripts/e2e-node-sidecar.mjs`): 14 of 14 checks passed. The node initialized at
tip - 12, caught up with `authenticated_admission: true`, reported `maxProcessedHeight` 159,367
(the absolute Core tip), answered `tl_check_kyc` and `tl_check_commits`, stopped on its stop file
in 727 ms and restarted from its store. Testnet4 carries no TradeLayer candidates, so no candidate
was applied.

## Known limits inside the freeze

- **Admission scope.** Only input 0 is authenticated, and only P2PKH and P2WPKH under
  SIGHASH_ALL; every other template is refused. Parent existence and unspentness rest on the
  observed chain. No authenticated historical bootstrap exists: a node starts from its anchor.
- **Capacity ceiling.** Replay protection keeps every external input id exactly (32 bytes
  each). On the playground's market-maker schedule the state grows 730 bytes and the
  checkpoint 1,460 bytes per block; at `rollback_limit` 64 the 64 MiB checkpoint budget
  is reached after about 45,900 blocks of that activity. The ceiling scales with TradeLayer
  activity, not with Bitcoin blocks; quieter blocks add less.
- **Per-block cost.** Applying a block at the chain level costs 25 ms at block 500 and
  485 ms at block 5,000 on that schedule (release, `docs/PER_BLOCK_COST.md`). A one-block
  rollback replays the window from its floor, about 4.8 s at 5,000 blocks.
- **Commit records.** `tl_check_commits` reads commits from an attached tradelayer.js
  channel registry; `tradelayer-listenerd` attaches none and answers `[]`.

## Outside the freeze

- The Litecoin client (its own bundle, after the Bitcoin launch).
- Admission of other inputs and templates, and an authenticated historical bootstrap.
- Replay-marker archival beyond the capacity ceiling above.
- Code signing and the overnight sleep test (tradelayer-wallet `feat/rust-port`).
- An independent second-model verification pass over admission and the wasm ABI.
