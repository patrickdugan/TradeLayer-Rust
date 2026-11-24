# TAP-VWAP-02B: Pinned Native Regtest & Three-Party Receipt Evidence Audit

Status: **Gate 0 + Gate 1 + Gate 2 + Gate 3 PASS on live disposable regtest** (`taproot-assets` `734ce74dd1e816a833a9727bba741718019fef18`, `lnd` `v0.21.3-beta`, `go1.26.3`). Evidence Tier = **1** (`eligible_for_index: false`). No consensus integration, no dependency changes to `tradelayer-core`, and no live funds/wallets.

---

## 1. Pinned Upstream Revisions & Build Hashes

| Component | Exact Revision / Value | Verification Source |
| :--- | :--- | :--- |
| `lightninglabs/taproot-assets` | `734ce74dd1e816a833a9727bba741718019fef18` (`v0.8.6`) | `git rev-parse HEAD` |
| `lightningnetwork/lnd` | `v0.21.3-beta` | `go.mod` line 34 |
| Go toolchain | `go1.26.3` (`-mod=readonly`) | `go env GOVERSION` |
| `btcd-itest` SHA-256 | `710b29a97e9c5f8688a43480ba7036f1e312d3df1167f135060f47226490b56f` | `github.com/btcsuite/btcd` |
| `tapd-integrated-itest` SHA-256 | `a8df2c5ae99baad2f059c19db314f905e3f1aa61802a3c45ef907f03cc4a3830` | `./cmd/tapd -tags=itest` |

---

## 2. Diagnosis & Resolution of Initial CI Native Smoke Failures

Inspecting the prior failing CI runs on `c833e30` (`TAP VWAP receipt reference` and `TAP VWAP native regtest smoke`) uncovered three concrete defects, each fixed with exact source evidence:

1. **BIP-340 Vector 0 Byte Typo (`receipt.test.cjs`)**:
   - Byte 32 of `s` in official BIP-340 vector 0 had `26` (`526f66a4...`) instead of official `25` (`525f66a4...`). Fixed and expanded to all **15 official 32-byte BIP-340 test vectors (`0..14`)** plus boundary negatives (`pub >= p`, non-residue `x=5`, `r >= p`, `s >= n`, `R = infinity`, odd-`y` `R`, buffer lengths) and frozen golden 526-byte fixtures (`receipt/independent_encoder.py` -> `receipt/golden-fixtures.json`).
2. **Missing `btcd -> btcd-itest` Symlink in Upstream `Makefile` (`itest-cc`)**:
   - Upstream `Makefile:274` runs `go test ./itest/custom_channels` directly without creating `itest/btcd -> itest/btcd-itest` on `PATH` (whereas upstream CI script `scripts/itest_cc_part.sh:31-35` creates that symlink explicitly).
3. **Strict 1-Atom Rounding Shortfall in `734ce74` (`tapchannel/aux_invoice_manager.go:345-366`)**:
   - Commit `734ce74dd1e816a833a9727bba741718019fef18` removed the +1 asset-unit rounding tolerance in `aux_invoice_manager.go`. At mock oracle rate `5,820,600` units/BTC, `forwarding_history_test.go`'s `20_000` units converts to `343,607,188 msat`, whose floor back to asset units is `19,999` units (`343,590,007 msat` — a `17,181 msat` shortfall that stalls in LND MPP partial-payment hold until the 60s test context expires).
   - Using rate-divisible `29,103` units (`5,820,600 / gcd(5,820,600, 10^11) = 29,103` units = `500,000,000 msat` exact) passes `TestCustomChannels/forwarding_history` cleanly on the **unmodified** `tapd-integrated-itest` binary (`purchase`: `29,103` atoms, `500,001,500 msat` out; `sale`: `29,103` atoms, `500,001,500 msat` in, `500,000,000 msat` out).

---

## 3. Upstream Source & Live Regtest Instrumentation Findings

1. **RFQ Quote Signatures (`rfqmsg/accept.go`, `buy_accept.go`, `sell_accept.go`, `rfq/stream.go`)**:
   - **Audit conclusion: ABSENT / ZERO / UNVERIFIED in upstream `tapd v0.8.6`.**
   - `BuyAccept.ToWire()` (`buy_accept.go:114`) and `SellAccept.ToWire()` (`sell_accept.go:119`) leave `Sig` (TLV type 6) populated with 64 zero bytes (`TODO(ffranr)`), and `acceptWireMsgData.Validate()` (`accept.go:263`) never verifies `Sig`.
   - Verified in `TestUpstreamTapdRfqAcceptSignatureAuditAndMutations`: raw wire `Sig == [64]byte{}`; mutating any of the 7 quote fields (`rfq_id`, `direction`, `rate`, `expiry`, `fill_cap`, `asset_id`, `counterparty`) is only caught when wrapped in our separate 2-party BIP-340 signed `QuoteTranscriptV1` (`TradeLayer/TAP-VWAP/QuoteTranscript/v1`, 283 bytes).
2. **Canonical HTLC Identity & SCID Alias Resolution**:
   - `rfq/order.go:1556-1568` (`logForwardingEvent`) records only `IncomingCircuitKey.HtlcID`, which on a sale edge (`Erin`) is the BTC-channel HTLC ID rather than the asset-channel HTLC ID.
   - `native/itest/tap_vwap_02b_test.go` subscribes to live `routerrpc.SubscribeHtlcEvents` across `Charlie`, `Dave`, `Erin`, and `Fabia`, resolves custom-channel SCID aliases (`AliasScids`) to confirmed 36-byte funding outpoints, extracts `IncomingHtlcId` for purchases and `OutgoingHtlcId` for sales, and proves that both observers on each asset channel derive the **identical** 32-byte `AssetHTLC/v1` nullifier.
3. **Channel Commitments, Revocation & BOLT-3 Whole-Satoshi Truncation**:
   - Every settled conversion advances `lnrpc.Channel.NumUpdates` by `>= 2` (`UpdateAddHtlc` + `CommitSig`/`RevokeAndAck`, then `UpdateFulfillHtlc` + `CommitSig`/`RevokeAndAck`) with zero remaining `PendingHtlcs` or `UnsettledBalance`.
   - Custom asset channel balances (`tapchannelmsg.ChannelCustomData.LocalCommit`) conserve exact integer asset atoms across peers (`AssetDeltaEdge == AssetDeltaPeer == fwd.AssetAmt`).
   - On standard BTC channels, `lnrpc.ListChannels.LocalBalance` reflects BOLT-3 whole-satoshi commitment outputs (`1,000 msat` granularity): on reverse purchase `rev_erin_purchase` (`500,001,500 msat` exact HTLC settlement), `LocalBalance * 1000` shifts by `500,002,000 msat` (`500 msat` sub-satoshi rounding into commitment fee weight).
   - `lnrpc.ListChannels` state hashes are local RPC summaries, **not** transferable cryptographic proofs; raw MuSig2 `CommitSig` and `AssetSig` (`tapchannel/aux_leaf_signer.go`) stay inside `channeldb`.
4. **Quote Capacity vs Gross BTC Leg**:
   - `rfq/order.go:402` & `753` enforce `inFlight + settled + htlc.AmountOutMsat <= policyMaxOutMsat`.
   - Verified on live regtest (`quoteCapMsat = 600,000,000` on `sharedQuoteID`): partial fill #1 (`350,000,000 msat`) succeeds; aggregate overfill (`+350,000,000 = 700,000,000 > 600,000,000`) is rejected live by `tapd` (`HTLC out amount is more than the maximum agreed BTC payment`); partial fill #2 (`+200,000,000 = 550,000,000 <= 600,000,000`) succeeds on the same quote.

---

## 4. Explicit Native vs Synthetic Negative Matrix Split

| Suite | Kind | Passing Cases | Coverage Summary |
| :--- | :--- | :---: | :--- |
| `receipt/receipt.test.cjs` | Reference JS | **13** | 15 official BIP-340 vectors, 8 curve/scalar boundary negatives (`pub>=p`, `x=5`, `r>=p`, `s>=n`, `infinity`, odd-`y`, lengths), golden 526B purchase/sale byte/digest/nullifier parity, key-rotation nullifier invariance, unresolved funding/splice rejection |
| `native/adapter` (`go test ./tapvwapadapter`) | Synthetic Go + upstream wire | **21** | Upstream `Accept` zero-sig audit, 7 quote-transcript field mutations + expiry + forged sigs, peer role-scope verification (`AssetPeer`, `BtcPeer`, `Edge`), on-chain resolution exclusion, unrevoked state rejection, wrong channel/asset/amount mismatch, key rotation, exact msat parity with `rfq.AssetPurchasePolicy` |
| `native/itest` (`forwarding_history` + `tap_vwap_02b_matrix`) | **Live 5-node regtest** (`btcd` + `lnd` + `tapd`) | **15** | Smoke forward (`29,103` units) + bidirectional forward (`58,206` units) & reverse (`29,103` units) conversions (6 3-party signed receipts), dual-observer canonical HTLC identity, SCID alias resolution, HODL invoice in-flight hold + cancel failure & balance/quote restoration, constrained quote partial fill #1 + live overfill rejection + partial fill #2, foreign minted asset unbacked-channel rejection, receipt key rotation, and Node `receipt.cjs` cross-verification |
| `prover` (`tap-vwap-nova-prover`) | Native Rust + R1CS `CheckCS` + Nova IVC | **18+** | All 15 official BIP-340 vectors in R1CS (`0..4` sat, `5..14` unsat), tagged SHA-256 midstate parity, golden 526B fixtures, forged sig, rogue unregistered peer key, unregistered channel, same-epoch & cross-epoch nullifier replay, cross-epoch quote overfill, exact `q8` half-up rounding tie boundary, tampered `z_0` SNARK rejection, and real Nova + CompressedSNARK proof over all 6 regtest receipts |

See [TRUST_MODEL.md](../TRUST_MODEL.md) for the complete enumeration of remaining Tier-1 trust assumptions and Tier-2 channel-state proof requirements, and [tap-vwap-02b-native.json](../results/tap-vwap-02b-native.json) for the redacted numeric & SHA-256 artifact manifest.
