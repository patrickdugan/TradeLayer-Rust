# ZTB experiment register (2026-11-24)

All acceptance criteria are prospective. No live token custody; no inherited proof status grants authorization.

| ID | Work | Acceptance gate |
|---|---|---|
| ZTB-01 | Accounting reference and spec | Deterministic conservation, hybrid outpoint movement, PnL graph, duplicate deposit/mint/exit, insufficient collateral, failed and stale callback, overflow. |
| ZTB-02 | Canonical statement codec and NEAR vault | Independent cross-language byte vectors; mock NEP-141 sandbox; exact retained deposit, token/predecessor spoof, partial refund, callback failure/retry and in-flight reserve tests. |
| ZTB-03 | Native TL hybrid property and custody effects | Opt-in independent activation, deterministic full native replay, no generic balance writes, old behavior unchanged; Rust preflight full. |
| ZTB-04 | NEARHEAD01 / NEARPAY01 | Independent light-client vectors from authentic NEAR history; approval/epoch/receipt substitution and fork rejection; actual Nova proofs; type-36 proof frame and verification budget. |
| ZTB-05 | TL authenticated roots | Native differential roots after each transition; M18-inspired map with authenticated initialization; reorg and restart fixtures. |
| ZTB-06 | Spot, reservation and token transitions | Circuit and native equivalence, bounded integer arithmetic; forged actor, unrecognized outpoint spend, omit/reorder and wrong-root attacks. |
| ZTB-07 | UTXORef PnL proof | Three-trader netting, matched orders and signed inputs, authentic index, rounding, liquidations, insurance; forged but conserving obligation rejects. |
| ZTB-08 | Bitcoin anchor | Adapt M16 payment statement; prove exact OP_RETURN, chainwork/finality and range/tip; reject competing fork, replay, omitted block/witness and circular anchor. |
| ZTB-09 | NEAR transparent decider | Actual proof verification, immutable VK, exact transcript and predecessor, all chunks, no replay; wrong proof/circuit/statement/chunk and concurrent finalization controls. |
| ZTB-10 | Wrapper benchmark | Compare same exact relation with direct transparent decider and succinct transparent wrapper; report prover RAM, time, proof bytes, cumulative TGas, transaction count, liveness. Stretch target < 300 TGas in one receipt; no weakening of relation to hit target. |
| ZTB-11 | End-to-end testnet | 10k mock USDC; deposit, mint, UTXO swap/import, 3-party PnL, burn, retry, successful payout; exact reserve/liability reconciliation, cross-chain adversarial replay and outage/recovery. |

Use CPU and CI first. Keep proofs and verdicts labelled independently: native_statement_verified, near_finality_verified, btc_anchor_verified, nova_verified, near_decider_verified, economic_authorized, production_ready. For each run pin code commit, circuit/program/VK, fixture hash, chain checkpoint, seed, exact command and logs. A proof of transcript arithmetic or a proof hash is not a proof of authorization.

ZTB-01 reference exercised 42 assertions in the isolated JS runtime, final reserve and outstanding 8000000000 atoms; pending 0. This is a synthetic arithmetic fixture, not a security audit or proof.
