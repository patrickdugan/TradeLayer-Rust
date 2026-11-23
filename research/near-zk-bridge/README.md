# zkTL-Bridge: NEAR stablecoins on TradeLayer

ZTB-01, 2026-11-24. Research only; production_ready=false. This subfolder changes no live consensus code. Start with one pinned NEP-141 USDC token contract and one Bitcoin/TL deployment. USDT can reuse the design with a separate asset domain and reserve.

- [PROTOCOL.md](PROTOCOL.md): state machine, proof relations, wire commitment and deployment gates.
- [model.mjs](model.mjs): dependency-free BigInt accounting reference with synthetic events. This is an unauthenticated fixture executor, not a contract, consensus implementation or SNARK.
- [test.mjs](test.mjs): deterministic end-to-end accounting and rejection checks.
- [EXPERIMENTS.md](EXPERIMENTS.md): ordered work packages and measurable acceptance gates.

Run from this directory: `node test.mjs`. The exact model and test function bodies were exercised in an isolated JS runtime before the initial commit; no claim is made about Rust preflight, NEAR sandbox or CI.

The next milestone is a byte-exact statement codec and canonical authenticated state, then a mock-token NEAR vault with receipt/callback tests. Keep any mint/redeem authorization disabled until finality and economic-proof gates are reviewed.
