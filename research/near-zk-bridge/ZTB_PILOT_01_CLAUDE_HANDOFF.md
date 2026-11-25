# ZTB-PILOT-01 / testnet handoff (2026-11-25)

Status: research only. Fixture 31/31 assertions executed in isolated JS against main's model.mjs; no Node CI, Rust build, sandbox, RPC, proof or chain deployment run in this session. No real money. Branch off main to preserve current Type 36 extension changes. Reconcile outstanding branches without overwriting current main.

## Launch profile

One NEAR testnet token and one TL/Bitcoin testnet domain. First use a self-issued NEP-141 mock on sandbox, then a dedicated mock on NEAR testnet, then optionally Circle test USDC. BTC side regtest first, testnet4 second. Do not fund any mainnet contract or use production keys.

Circle official reference: https://developers.circle.com/stablecoins/usdc-contract-addresses
- NEAR testnet USDC: 3e2210e1184b45b64c8a434c0a7e7b23cc04ea7eb7a6c3c32520d03d4afcb8af
- NEAR mainnet USDC: 17208628f84f5d6ad33f0da3bbbeb27ffcb398eac501a31bd6ad2011e36133a1 (informational only; deny in pilot).
Pin actual testnet genesis, token/vault accounts, code hashes, decimals, TL activation anchor, property, program/VK, and policy in deployment manifest. Verify ft_metadata and storage registration. USDT gets its own reserve and asset domain later. No price-based conversion of collateral or cross-asset netting. Start with BTC/TL-USDC spot and fully collateralized linear BTC/USD test positions; add BTC-margined UTXORef clearing as separate venue/risk bucket with independently authenticated obligations. TL token listing, if wanted, is a separate market and has no reserve claim.

## Run the fixture

    node research/near-zk-bridge/ztb-pilot-01.mjs

Expected 31 checks; USDC opening 10,000, payout 600, final 9,400; BTC opening and final 10,000,000 sats; final USDC balances Alice 3,000 / Bob 3,520 / Carol 2,880; BTC claims Alice 1,000,000 sats / Bob 9,000,000 sats. The source and order tables, positions, marks and payout results are trusted fictional fixtures. No actual UTXORef protocol implementation is called. The model is a reference for dual reserve arithmetic and atomicity, not a proof or native settlement engine. Add fresh malformed/cap/fuzz/rollback controls before CI adoption. USD atom = 10^-6 USDC; BTC atom = sat. Pilot only accepts exact quote/PNL divisions; production needs deterministic rounding and explicit residual accounting.

## P0: Reconcile source work

Read main src/zk_verify.rs, src/registry_zk.rs, src/economic_zk_impl.rs, PROTOCOL.md, EXPERIMENTS.md; compare PR 158 and research/ztb-bridge-sec-02-mythos (its audit, V2 spec and NEARPAY fixtures may be on unmerged refs). Main already has ZkProgramExt.z0_pins and statement_key. Verify tests and exact implementation at the chosen base. Current statement_nullifier hashes program_id, so it is scoped to the verifier program; do not use it alone as the economic nullifier. Persist a global source-event consumption map across program changes, including the vault candidate sequence where needed. For batch effects compare-and-swap consumed root and authorized deposit list against the actual economic owner. Verify each z0 checkpoint against independently authenticated state; no caller-selected root. StateReceipt (actual enum StatementReceipt) alone never changes balances.

## P1: NEAR sandbox vault and authenticated fixture capture

Implement a standalone near-sdk Rust WASM vault and mock token workspace under research/near-zk-bridge, preserving old files and the existing ABI/version ledger. Pin compiler, SDK and near-workspaces. ft_on_transfer checks pinned predecessor, sender, canonical msg, deposit recipient/script, chain domain, caps, storage and amount; reject/refund malformed messages. Start with full retain only. Candidate logs are not final credits. Verify terminal ft_resolve_transfer and full receipt graph; only actual retained amount qualifies for mint. Test partial/full refunds, wrong token, malformed message, duplicate sequence, async callback failure, storage, unexpected token balance, no arbitrary admin transfer, payout ambiguity and stale attempts. Freeze further deposits if reserve reconciliation fails; no timeout-based resend. Test deployment uses only dedicated disposable test keys and no full-access key on a production vault; test code hash and upgrade policy manifest.

Capture real NEAR testnet raw next_light_client_block and receipt execution proofs (light_client_proof or EXPERIMENTAL_light_client_proof, pin RPC schema). Start from independently vetted trusted checkpoint and full producer sets. Store immutable raw JSON, canonical Borsh bytes, hashes, protocol version, genesis and external cross-check. A successful RPC response is not authentication. Save full NEP-141 receipt/data graph and terminal outcomes, not only transaction/log. Run against positive and adversarial real-data mutations. Sources:
- https://nomicon.io/ChainSpec/LightClient.html (explicitly warns it is stale)
- https://github.com/near/nearcore (pin version and use exact current primitives/consensus implementation)
- https://github.com/near/near-light-client (archived 2026-10-30; reference only)
- https://github.com/near/NEPs/blob/master/neps/nep-0141.md

## P2: Native verification then Nova

Build exact native NEARHEAD01, verify approval messages, all relevant stake denominators and protocol-specific validator/epoch rules, next_bps Borsh hash, checkpoint freshness, roots and ancestry. Reject truncated vectors, wrong approval index/key/epoch/height, stale checkpoints, malformed encodings, forged roots and insufficient quorum. Never assume the older nomicon pseudocode is current. Keep source data untrusted until native verification. Compose NEARPAY01 with authenticated outcome, block and receipt/data commitments; bind token, vault, source locator, recipient, domain and retained amount. Use nearcore serialization differential tests and actual testnet data. Only after native agreement implement the Pallas/Vesta Nova relation (no Groth16), constrain digest limbs, checked counters, input pins, full graph, list accumulator and persistent event nullifiers. Measure native and proving cost (constraints, elapsed time, RAM, proof bytes) by actual signature/receipt sizes. Type 36 node frame cap 30,000 bytes and block declared verifier budget 4,000 ms; prove transport (regtest witness or supported explicit data lane) without pretending a large proof fits a conventional OP_RETURN. First submit genuine proof as receipt-only. No production custody effect.

## P3: UTXORef and native economic owner

Find real UTXO-Ref checkout and pin its commit (audit cites main 0dff29e; verify). Replace fixture BTC records with independently checked reserve vault outpoints, spend/finality status, encumbrance policy and canonical owner. Never count the same outpoint in multiple domains. TL-USDC credits only after independently verified proof and atomic nullifier consumption. Spot: matched authenticated orders, sufficient free balances, exact atom conversions, atomic debit/credit. UTXORef settlement: authenticated positions, prices, contract terms, funding, liquidation, insurance, signed inputs and one settlement ID. Keep each currency's liabilities reconciled to its own reserve, even if a dealer provides both. Implement no generic admin credit and no balance-only migration. Run native differential tests and compare fixture trajectory. Explicitly test rollback/reorg and source replay after program upgrades. Keep BitVM3 BTC vault enforcement separate; current security audit has unresolved assumptions.

## P4: reverse path and deployment

Prove ordered complete TLSTATE01, with independently authenticated BTC headers, full relevant block data, prevouts and witness authority, canonical chainwork policy, state predecessor CAS, free-balance burn and exact exit destination. Implement TLEXIT01 inclusion and transparent Nova verification in NEAR contract; do not accept a proof hash or recorded receipt as authorization. Benchmark direct verifier and sound transparent wrapper; gas: nearcore gas.md reports 1 Pgas max_total_prepaid_gas since protocol 83; query actual testnet config. Original <300 TGas remains stretch target. Reject wrong VK/root/chain/amount, missing/duplicate chunks and concurrent predecessors; reconciliation for transfer success followed by callback failure. Use a mock-only route with separate namespace if testing a trusted adapter while real proof gate is incomplete. Never silently change an authenticated route into a mock route.

Deploy first NEAR sandbox + BTC regtest, then dedicated NEAR testnet mock + Bitcoin testnet4. If actual proof path does not finish, deploy only receipt-only/non-custodial harness, or isolated mock route explicitly labelled. No mainnet commands, no production key disclosure. Persist chain IDs, account names, contract hash, txids, height, RPC evidence, proof hashes/bytes, program/VK, reserve and liability diffs, exact commands and failures in a redacted receipt. Report native_near_finality_verified, deposit_verified, nova_verified, btc_anchor_verified, tl_state_verified, near_decider_verified, economic_authorized and production_ready separately. Actual production readiness remains false.

## Deliverables

Commit code and fixtures on a continuation branch; open PR with test logs and testnet deployment manifest. Run existing ZTB-01, BRIDGE-SEC-01/02 when present, ZTB-PILOT-01, full Rust preflight, near-workspaces and native verifier mutation tests. If dependency missing, report and keep gate false. Do not stop after drafting plans: execute gates sequentially while environment permits. End with a short ledger of gates passed/blocked and exact next commands.
