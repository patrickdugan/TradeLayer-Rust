# Proof of Exchange: TAP-VWAP (`TAP-VWAP-01A` -> `02B` -> `03`)

Status: **Research Tier 1 (`eligible_for_consensus_index: false`)**. Zero changes to `tradelayer-core`, `Cargo.toml`, `Cargo.lock`, oracle publication, or consensus state transitions. See [SPEC.md](SPEC.md), [CARBON_HANDOFF.md](CARBON_HANDOFF.md), [TRUST_MODEL.md](TRUST_MODEL.md), and [native/EVIDENCE_AUDIT.md](native/EVIDENCE_AUDIT.md).

---

## 1. Repository Layout & Isolation

| Path | Stage | Description |
| :--- | :--- | :--- |
| `benchmark.cjs`, `results/tap-vwap-01a.json` | `01A` | Deterministic synthetic arithmetic & robust control-group median baseline |
| `receipt/receipt.cjs`, `receipt/receipt.test.cjs` | `02B` (Gate 1) | Normative 526-byte `Receipt/v1` wire contract, 77-byte `AssetHTLC/v1` nullifier, BIP-340 reference & boundary test suite |
| `receipt/independent_encoder.py`, `receipt/golden-fixtures.json` | `02B` (Gate 1) | Independent Python encoder + all 15 official BIP-340 32B vectors + frozen golden purchase/sale fixtures |
| `native/adapter/receipt_adapter.go`, `receipt_adapter_test.go` | `02B` (Gates 2–3) | Reviewed `btcec/v2/schnorr` 3-party signing adapter, `ChannelRegistry`, 2-party signed `QuoteTranscriptV1`, upstream `tapd` zero-sig audit & 7-field mutation suite |
| `native/itest/tap_vwap_02b_test.go`, `native/run-native.sh` | `02B` (Gates 0, 2–3) | Disposable 5-node regtest runner (`taproot-assets` `734ce74dd1e816a833a9727bba741718019fef18`, `lnd` `v0.21.3-beta`, `go1.26.3`) & redacted report (`results/tap-vwap-02b-native.json`) |
| `prover/` (`Cargo.toml.isolated`, `Cargo.lock.isolated`, `src/*.rs`, `run-prover.sh`) | `03` (Gate 4) | Isolated native witness verifier + `(Secq256k1Engine, Secp256k1Engine)` R1CS & Nova IVC + Spartan/IPA `CompressedSNARK` prover (`results/tap-vwap-03-benchmarks.json`) |
| `TRUST_MODEL.md` | `02B–03` | Explicit enumeration of every remaining trust assumption and Tier-1 signed assertion vs Tier-2 channel-state proof boundary |

> **Supply-Chain Isolation:** Prover Cargo manifests are stored as `Cargo.toml.isolated` and `Cargo.lock.isolated` and materialized only inside a disposable staging directory by `prover/run-prover.sh`, keeping `python3 scripts/check_rust_supply_chain.py` strictly green.

---

## 2. Reproducible Verification Commands

From repository root:

```sh
# Gate 0 & Gate 1: Reference wire contract, 15 official BIP-340 vectors, boundary negatives & 01A replay
node --test research/tap-vwap/receipt/receipt.test.cjs
node research/tap-vwap/benchmark.cjs > /tmp/tap-vwap-01a.json
diff -u research/tap-vwap/results/tap-vwap-01a.json /tmp/tap-vwap-01a.json
python3 research/tap-vwap/receipt/independent_encoder.py

# Gate 2 & Gate 3: Pinned native tapd/lnd regtest smoke + 3-party adapter + negative matrix (Linux)
bash research/tap-vwap/native/run-native.sh

# Gate 4: Isolated TAP-VWAP-03 native verifier, R1CS BIP-340/Merkle/VWAP tests, and Nova SNARK benchmarks
bash research/tap-vwap/prover/run-prover.sh
TAP_VWAP_RUN_BENCH=1 bash research/tap-vwap/prover/run-prover.sh --out /tmp/tap-vwap-03-benchmarks.json

# Consensus zero-dependency supply-chain hard gate
python3 scripts/check_rust_supply_chain.py
```

---

## 3. Measured `TAP-VWAP-03` R1CS & Nova CPU Benchmark Summary

Curve cycle: **`Secq256k1Engine` (primary $\mathbb{F}_p$, native `secp256k1` base field) $\leftrightarrow$ `Secp256k1Engine` (secondary $\mathbb{F}_q$)** (`nova-snark = 0.76.0`, `secp256k1 = 0.30.0`, `release` `opt-level = 3`).

### Component R1CS Constraints per Conversion Step
- Single BIP-340 Schnorr verification (incl. 2-block `BIP0340/challenge` SHA-256): **`58,932`**
- Three BIP-340 Schnorr verifications (`edge`, `btc_peer`, `asset_peer`): **`176,796`** *(vs `~1,208,100` non-native limb estimate on Pasta `PallasEngine` — **6.8x** reduction)*
- `Receipt/v1` 526-byte tagged SHA-256 (9 blocks from precomputed tag midstate): **`234,492`**
- `AssetHTLC/v1` 77-byte canonical nullifier tagged SHA-256 (2 blocks): **`51,135`**
- `BatchLog/v1` 64-byte hash-chain tagged SHA-256 (2 blocks): **`44,874`**
- Authenticated `ChannelRegistry` 2x Poseidon Merkle inclusions (depth 12): **`8,862`**
- Persistent 256-bit `IndexedNullifierTree` non-membership + insertion (depth 12): **`17,852`**
- Persistent `QuoteConsumptionTree` capacity bound + update (depth 12): **`9,239`**
- Exact wrap-free `q8` half-up Euclidean division & range bounds: **`409`**
- **Total `TapVwapStepCircuit` before Nova augmentation:** **`546,990`**
- **Primary Nova augmented R1CS constraints:** **`560,554`** (`secondary`: **`10,787`**)

### End-to-End CPU Performance & Sizes (`results/tap-vwap-03-benchmarks.json`)
- One-time `PublicParams::setup`: **`26.74 s`** | `CompressedSNARK::setup`: **`4.56 s`**
- Nova IVC folding latency: **`0.72 s` – `1.29 s` per conversion step** (`7.76 s` total for all **6 live regtest conversions**)
- `RecursiveSNARK` verify: **`1.26 s` – `3.19 s`** (`36,894,877` bytes uncompressed)
- Spartan/IPA `CompressedSNARK` size: **`13,088` bytes (`12.78 KiB`)** (**2,819x** compression)
- Spartan/IPA `CompressedSNARK` prove: **`159.9 s`** (6-step regtest epoch on 8 CPU threads)
- Spartan/IPA `CompressedSNARK` verify: **`1.92 s` – `4.31 s`**
- Peak process RAM (`VmHWM`): **`2,490,172 KiB` (`2.37 GiB`)**

All proofs carry `evidence_tier = 1` and `eligible_for_consensus_index = false` (three-party signed assertions, **not** authenticated channel-state proofs; see [TRUST_MODEL.md](TRUST_MODEL.md)).
