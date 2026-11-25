# TAP-VWAP Trust Model & Evidence Tier Boundary (`TAP-VWAP-02B` / `TAP-VWAP-03`)

> **Consensus Safety Gate:** All artifacts in `research/tap-vwap/` operate at **Evidence Tier 0** (untrusted local RPC smoke) or **Evidence Tier 1** (`eligible_for_consensus_index = false`, three-party BIP-340 signed settlement assertions). Zero changes are made to `tradelayer-core`, oracle publication, or consensus state transitions.

---

## 1. Evidence Tier Comparison: Signed Settlement Assertions vs Authenticated Channel-State Proofs

| Property / Predicate | Tier 0: Local RPC Observation (`TAP-VWAP-02A`) | Tier 1: 3-Party Signed Assertion + Nova IVC (`TAP-VWAP-02B` & `03`) | Tier 2: Authenticated Channel-State Proof (Future Gate) |
| :--- | :--- | :--- | :--- |
| **Primary witness** | Single-node `ForwardingHistory` / `ListChannels` | 526B `Receipt/v1` + 3x BIP-340 Schnorr signatures (`edge`, `btc_peer`, `asset_peer`) | Bitcoin SPV funding proof + Taproot Asset MS-SMT proof + dual-signed pre/post commitments + `RevokeAndAck` |
| **Peer key authorization** | Unauthenticated local config | Authenticated Poseidon Merkle inclusion in `registry_root` (immutable BOLT-2 33B pubkey order, rotatable receipt keys) | Derived directly from confirmed on-chain 2-of-2 MuSig2 Taproot funding output + Taproot Asset script key |
| **HTLC replay nullifier** | Local DB primary key (`IncomingCircuitKey` — buggy on sales) | Global cross-epoch 256-bit `IndexedNullifierTree` over `AssetHTLC/v1` (`genesis \|\| asset_outpoint \|\| asset_offerer \|\| asset_htlc`) | Same canonical `AssetHTLC/v1` identity extracted directly from signed commitment HTLC descriptors |
| **Quote authenticity & cap** | Upstream `tapd` V1 `Accept` TLV type 6 `Sig` is **64 zero bytes** (`unverified`) | 2-party BIP-340 signed `QuoteTranscriptV1`; persistent `quote_root` enforces `consumed + delta <= max_out_msat` in R1CS | In-circuit verification of `QuoteTranscriptV1` Schnorr signatures + rate/fee consistency |
| **Channel balance movement** | Reported by local LND/tapd RPC | Asserted by signing peers (`asset_pre != asset_post`, `btc_pre != btc_post`); **not** proven against BOLT-3/MS-SMT scripts | Verified via MuSig2 `CommitSig` + `AssetSig` over `AuxLeaves` and counterparty revocation secret (`RevokeAndAck`) |
| **Cross-leg binding** | Edge local memory only | `Edge` verifies both legs; `AssetPeer` & `BtcPeer` verify only their own leg and **acknowledge** the opposite leg | Cryptographic cross-leg linkage + fee/principal decomposition |
| **Consensus index eligible** | `false` | **`false`** (`evidence_tier = 1`) | Candidate only after independent security audit (`TAP-VWAP-06`) |

---

## 2. Exact Upstream `tapd` / `lnd` Source & Live Regtest Findings (`734ce74dd1e816a833a9727bba741718019fef18`)

1. **Upstream RFQ Quotes Are Unsigned on Wire (`rfqmsg/accept.go`, `buy_accept.go`, `sell_accept.go`, `rfq/stream.go`)**:
   - `acceptWireMsgData` defines `Sig tlv.RecordT[tlv.TlvType6, [64]byte]`, but `BuyAccept.ToWire()` (`buy_accept.go:114`) and `SellAccept.ToWire()` (`sell_accept.go:119`) leave `TODO(ffranr)` and serialize 64 zero bytes.
   - `acceptWireMsgData.Validate()` (`accept.go:263`) and `NewIncomingAcceptFromWire()` (`accept.go:382`) never verify `Sig`. Mutating `rfq_id`, `direction`, `rate`, `expiry`, `fill_cap`, `asset_id`, or `counterparty` on raw `Accept` wire data is not caught by any signature check in upstream `tapd v0.8.6`.
   - **Mitigation implemented (`TAP-VWAP-02B`)**: Separate 283-byte domain-separated `QuoteTranscriptV1` (`TradeLayer/TAP-VWAP/QuoteTranscript/v1`) co-signed with BIP-340 Schnorr by both quote requester and responder (`btcec/v2/schnorr`), rejecting all 7 field mutations, expiry violations, and forged signatures.

2. **Asymmetric HTLC Identity in `ForwardingHistory` (`rfq/order.go:1556-1568`)**:
   - `logForwardingEvent` stores only `htlc.IncomingCircuitKey.HtlcID`. On a purchase edge (`Dave`), the incoming channel is the asset channel (`IncomingHtlcId == assetHtlc`). On a sale edge (`Erin`), the incoming channel is the **BTC** channel, so `ForwardingHistory` omits the outgoing asset-channel HTLC ID (`OutgoingHtlcId`).
   - **Mitigation implemented**: Live `routerrpc.SubscribeHtlcEvents` instrumentation captures full `(IncomingChannelId, IncomingHtlcId, OutgoingChannelId, OutgoingHtlcId)` across all four nodes (`Charlie`, `Dave`, `Erin`, `Fabia`), resolves custom-channel SCID aliases (`AliasScids`) to confirmed 36-byte funding outpoints, and verifies that both peers on the asset channel derive the exact same `AssetHTLC/v1` nullifier.

3. **BOLT-3 Whole-Satoshi Commitment Truncation vs Millisatoshi HTLC Settlement**:
   - Live regtest reverse conversion (`rev_erin_purchase`: `29,103` asset units = `500,000,000 msat` + `1,500 msat` routing fee = `500,001,500 msat` exact HTLC settlement) demonstrated that `lnrpc.ListChannels.LocalBalance` shifts by `500,002,000 msat` (`500,002 sat`) because BOLT-3 Bitcoin commitment transaction outputs are denominated in **whole satoshis (`1,000 msat`)**, rounding sub-satoshi msat balances into commitment transaction fee weight while off-chain HTLC state tracks exact `1 msat` precision.
   - Any future Tier-2 commitment-transaction circuit must explicitly model `< 1,000 msat` dust/fee rounding between off-chain HTLC msat amounts and whole-satoshi commitment outputs.

4. **Quote Capacity vs Gross BTC Leg Volume (`rfq/order.go:402`, `753`)**:
   - Both `AssetPurchasePolicy` and `AssetSalePolicy` track `inFlightMsat + settledMsat + htlc.AmountOutMsat <= policyMaxOutMsat` at interception (before `GenerateInterceptorResponse` rewrites custom-channel outgoing msat to `354,000 msat` anchor wrapper dust).
   - For purchases (`direction = 0`), `gross_btc_msat == quote_delta_msat == AmtOutMsat`. For sales (`direction = 1`), `gross_btc_msat == AmtInMsat >= quote_delta_msat == AmtOutMsat`, with `AmtInMsat - AmtOutMsat` equal to the edge's BTC routing fee (`2,000 msat` forward, `1,500 msat` reverse on regtest). Both the Go adapter and the Nova R1CS circuit enforce this exact directional relation.

---

## 3. Explicit Remaining Trust Assumptions in `TAP-VWAP-03` (Tier 1)

Even when `CompressedSNARK::verify` succeeds over `(Secq256k1Engine, Secp256k1Engine)`:

1. **Three-Party Collusion / Sybil Wash Signing**:
   - A Nova Tier-1 proof establishes that the registered `edge`, `btc_peer`, and `asset_peer` keys signed a consistent 526-byte `Receipt/v1` with an unspent `AssetHTLC/v1` nullifier and available quote headroom.
   - If all three roles are controlled by one entity (or collude out-of-band), they can co-sign arbitrary synthetic conversions without moving real channel balances, or execute circular wash trades (`Charlie -> Dave -> Erin -> Charlie`) bounded only by channel/quote caps and fees.

2. **Two-Party Edge + Counterparty Misstatement of Opposite Leg**:
   - `AssetPeer` has zero visibility into `Edge <-> BtcPeer` channel state; `BtcPeer` has zero visibility into `Edge <-> AssetPeer` channel state.
   - When `AssetPeer` signs `Receipt/v1`, it cryptographically verifies `asset_outpoint`, `asset_htlc`, `asset_atoms`, `asset_pre/post`, and `payment_hash`, while **merely acknowledging** `btc_outpoint` and `gross_btc_msat` asserted by `Edge`. If `Edge` and `BtcPeer` collude on a real asset transfer with an honest `AssetPeer`, `Edge` + `BtcPeer` could inflate or deflate `gross_btc_msat` before presenting the receipt to `AssetPeer` unless `AssetPeer` independently cross-checks implied rate bounds (e.g., against its signed `QuoteTranscriptV1`).

3. **Selective Omission / Pre-Admission Censorship**:
   - `batch_log_commitment` proves integrity of the ordered sequence of receipts *admitted* into the batch. It does **not** prevent an edge or batch sequencer from omitting valid settled conversions before they enter the batch log.

4. **External Authentication of `registry_root` and `prev_quote_root` Admission**:
   - The circuit verifies Poseidon Merkle membership against public `registry_root` and `prev_quote_root`. If the external registry authority admits Sybil channels, unconfirmed/reorged funding outpoints, or forged quote leaves `(rfq_id, quote_transcript, max_out_msat)`, the circuit will faithfully fold proofs relative to that corrupted root.
   - Notably, the two-party Schnorr signatures on `QuoteTranscriptV1` are verified natively by the adapter/registry gate prior to populating `quote_root`; verifying those two additional Schnorr signatures *inside* the R1CS circuit is left for Tier-2 / quote-admission circuits.

5. **Gross Leg Price vs Principal-Only Spot Price**:
   - `vwap_price_q8` aggregates `gross_btc_msat` (`S_B`) and `asset_atoms` (`S_U`), which includes RFQ maker spread and Lightning hop fees on the BTC leg rather than a pure fee-stripped principal spot price.

6. **Timing, Finality, and Asset Peg**:
   - Epoch assignment reflects batch inclusion, not a consensus-authenticated execution timestamp.
   - On-chain force-closed HTLC resolutions (`FinalHtlcEvent`) are explicitly excluded from off-chain spot receipts and require separate disposition rules.
   - Any stablecoin Taproot Asset (`USDT`) prices BTC in asset atoms, not USD under issuer freeze or depeg conditions.
