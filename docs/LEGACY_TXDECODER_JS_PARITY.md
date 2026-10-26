# txDecoder.js: complete bounded historical observations

Pinned source: `patrickdugan/tradelayer.js@dca0888b827c5bc28fa9189bc9e1d507f5c5670c`, `src/txDecoder.js`. The complete archive is `fixtures/legacy_types_source_txDecoder.txt`, pinned LF SHA256 `a573e2ad3e843b2c573cb3f61c8d67b64ff05f1a554d36e3ad6bd5764d86d076`, Git blob `4728f1bdce1190428f91ae5b02f380584073c431`. A Windows CRLF checkout has raw-byte SHA256 `8a395367be182141129be6f5a96ec2cf334bc4376edfccd0b7dcacafd8f5a885`; normalization reproduces the pinned Git blob byte-for-byte before verification and execution.

`source_decode_historical_method_unverified` implements all **36 active exported methods** for caller-supplied string arguments. `source_decode_historical_type_unverified` follows the existing source Types route inventory: 32 exported body decoders, four absent route names, and no invented aliases. These are observations of a historical decoder, not accepted transactions or new wire rules. They supply no chain, UTXO, sender, activation, clearlist, oracle, custody, channel, proof, or economic authorization. The existing strict parser and accepted runtime are separate.

The original ten-layout `source_decode_body_unverified` API remains compatible with its existing callers and 17 shared fixtures. That older API deliberately rejects noncanonical numbers, malformed bracket lists, framed text, Unicode/control characters, and unsafe atomic integers. It is a restricted research projection. The new historical API represents the source results instead of imposing those restrictions silently.

## Complete active inventory

| Type / helper | Active method | Source behavior retained |
|---|---|---|
| Helper | `decodeAmount` | Trailing `~` selects `parseInt(base36)` → Number → BigNumber /1e8; otherwise the integer Number is returned. It differs from exact base36 BigNumber amount parsing. |
| 0 | `decodeActivateTradeLayer` | Decimal integer prefixes; semicolon branch filters invalid elements, single invalid value remains NaN; missing codeHash remains an own undefined field. |
| 1 | `decodeTokenIssue` | Integer initial amount, semicolon whitelist values, literal `1` flags, nullable proceduralType. |
| 2 | `decodeSend` | Send-all, four/five-field scalar branch shadowing comma multi-send, multi-send array with missing amounts undefined, dropped address, unmatched shape returning undefined. |
| Helper | `decodePropertyId` | Regular integer prefixes and both `s1-a` / `s-1-a` forms; malformed synthetic forms return literal `s-NaN-NaN`; parsed Number text is preserved. |
| 3 | `decodeTradeTokenForUTXO` | Exact base36 BigNumber amount /1e8, eight-place truncation toward zero, mixed decimal/base36 fields, nullable withdrawal tag. |
| 4 | `decodeCommitToken` | Source comma splitting before bracket-list parsing, missing channel TypeError, raw text after `ref:`, nullable final list ID. |
| 5 | `decodeOnChainTokenForToken` | Two synthetic-capable IDs and two exact base36 amount fields. |
| 6 | `decodeCancelOrder` | Loose Number comparison to one, UTF16 txid-length branch, raw contract price, undefined desired ID for contract orders, source spot-price bug reading cancelAll and dividing by8. |
| 7 | `decodeCreateWhitelist` | Four string fallbacks, including missing fields. |
| 8 | `decodeUpdateAdmin` | Literal string flags and base36 target ID. |
| 9 | `decodeIssueOrRevokeAttestation` | Literal revoke flag, Unicode uppercase metadata, missing fourth-field TypeError. |
| 10 | `decodeAMMPool` | Integer quantities without atom scaling, ignored index6, decimal auto-radix option selectors at indexes7/8, opaque strategy. |
| 11 | `decodeGrantManagedToken` | Exact amount, trailing DLC hash overrides the legacy hash slot. |
| 12 | `decodeRedeemManagedToken` | Exact amount and DLC metadata fields. |
| 13 | `decodeCreateOracle` | Semicolon whitelist and base36 lag. |
| 14 | `decodePublishOracleData` | Integer Number → BigNumber /1e4 price; optional high/low/close remain unscaled integers. |
| 15 | `decodeCloseOracle` | Primitive integer result, not an object. |
| 16 | `decodeCreateFutureContractSeries` | Source parses on-chain pairs only when native is false; nullable pair entries; integer notional; parseFloat-prefix leverage; nullable expiry and series. |
| 17 | `decodeExerciseDerivative` | Two integer quantities without atom scaling. |
| 18 | `decodeTradeContractOnchain` | Integer Number price with `||0`, BigNumber /1e8 without the token-amount truncation; all source flags. |
| 19 | `decodeTradeContractChannel` | Same price pipeline, channel expiry and maker/seller flags. |
| 20 | `decodeTradeTokensChannel` | Exact amounts, synthetic IDs, expiry, offerer/maker and two colored-output flags. |
| 21 | `decodeWithdrawal` | Optional reference uses decimal prefix parsing and finite-or-false, preserving signed zero and negative references. |
| 22 | `decodeTransfer` | Missing destination TypeError and raw text reference, distinct from withdrawal. |
| 23 | `decodeSettleChannelPNL` | Later duplicate wins; raw txid strings, genuine Base94 numeric conversion and optional own undefined king fields. |
| 24 | `decodeMintSynthetic` | Synthetic-aware property helper plus exact amount. |
| 25 | `decodeRedeemSynthetic` | Plain base36 property parsing: `s-1-a` becomes integer28, unlike mint. |
| 26 | `decodePayToTokens` | Two plain integer IDs and exact amount. |
| 27 | `decodeOptionTrade` | Ticker alias, flagged amount helper, any seventh field creates combo keys even if empty. |
| 30 | `decodeStakeFraudProof` | All 13 stake/fraud/relay/DLC fields, exact amount, literal autoRoll. |
| 31 | `decodeKingSettle` | Genuine Base94 conversion, integer versus float projections, channel root. |
| 32 | `decodeBatchMoveZkRollup` | Entire input retained as opaque text, including Unicode, control bytes and apparent framing. |
| 33 | `decodeColoredCoin` | Decimal action and plain base36 ID/ratio without missing-field fallback. |
| Helper | `decodePublishNewTx` | Entire opaque string; no implicit proof/JSON validation. |
| Helper | `decodeAbstractionBridge` | Two plain base36 fields; no destination field and no Type34 route alias. |

There are 37 method declarations but 36 active exports because `decodeSettleChannelPNL` occurs twice. The earlier Base256 txid decoder is unreachable through the exported object. The reference runner loads the genuine Base256 module and instruments calls; every observed active path makes zero Base256 calls. Base256 is independently functional in `legacy_base`; the decoder does not revive this shadowed branch. Types28/29/34/35 route to absent exports, including Type34's `decodeCrossLayerBridge` rather than the existing standalone abstraction helper.

## Values, numbers, and source failures

`HistoricalValue` retains ordered own object fields, arrays, strings, booleans, null, undefined, NaN, infinities, and signed zero. Its deterministic tagged diagnostic JSON encodes finite numbers as their exact binary64 bits. Consequently the fixtures can distinguish omitted fields from own undefined fields, NaN from null, and negative zero from positive zero. It is not a transaction serializer. `HistoricalDecodedBody` keeps the method and value privately; authorization methods always fail.

Numbers follow the actual pinned source call boundary. `parseInt` accepts signed prefixes and the ECMAScript whitespace set; decimal/auto-hex parsing and parseFloat-prefix leverage are distinct. The reference V8 radix36 path skips leading zeros and accumulates five-digit chunks; the Rust diagnostic policy freezes that observed behavior even beyond safe integers. ECMAScript permits implementation-dependent approximation for some radix36 values, so this is a characterized host numeric policy, not a claim of universal cross-engine consensus portability. Missing unguarded fields are coerced to the string `undefined` by parseInt; in base36 this can be a finite number.

BigNumber9.3.1 explicit-base36 inputs use the real constructor's case/normalization rules, 20-place half-up conversion/division and source eight-place truncation where called. Number inputs first use the shared exact JS Number formatter. Positive/negative zero and nonfinite values remain observations. Base94 uses the existing exact source port and genuine archived converter reference: the literal default `0` represents Base94 digit15, `|` collides with the fractional separator, and fractional leading zeros are lost. These defects are preserved rather than repaired.

Missing indexed `.startsWith`, `.includes` and `.toUpperCase` reads produce typed `SourceThrow` observations. Base alphabet failures retain their source Error message. Strings are accepted as UTF8 within the bounded domain; arbitrary non-string JS argument coercion and lone UTF16 surrogates are not part of this Rust API. Source error messages are compared after the Node reference runner’s UTF8 projection; a lone surrogate in a non-BMP invalid-digit error becomes U+FFFD. Source logging and the sloppy global assignment inside `decodeSend` are not reproduced as process effects.

## Bounds and evidence

Body inputs are limited to 4096 UTF8 bytes and method names to 128 bytes before splitting or cloning. Public diagnostic rendering preflights 32 levels, 32768 value nodes and 512KiB raw text, then caps the escaped output at 512KiB. BigNumber normalization retries are capped at 32, including repeated matching radix prefixes. Over-budget work returns Capacity. These explicit resource boundaries do not strengthen historical acceptance rules; the source itself has no comparable body limit. Unicode uppercasing follows the pinned Rust Unicode tables, with the checked source corpus covering case expansion and non-ASCII metadata; exhaustive cross-version Unicode parity is not asserted.

`scripts/audit_legacy_txdecoder_source.cjs --check` executes the full unchanged source with genuine, SHA-pinned BigNumber9.3.1, Base94, Base256 and bigint-base-converter sources. It preserves the original 17 fixture assertions and regenerates/checks `fixtures/legacy_txdecoder_historical_vectors.tsv`: **1447 observations across all 36 exports**, including 512 seeded long radix36 cases, missing and empty fields, signed/partial numeric values, exact amount fractions, mixed-case failures, synthetic IDs, branch shadows, UTF16 length checks, duplicate settlement behavior and dispatch-name absence. The Rust corpus test compares the complete tagged output or exact source error name/message for every row; additional tests check caller bounds, authorization denial, unchanged strict API behavior and concrete malformed-branch outcomes.

Offline source check:

```text
node scripts/audit_legacy_txdecoder_source.cjs --check
```

The parent validation batch runs pinned formatting, Clippy, the targeted Rust tests and full suite. Full historical transaction acceptance still requires independently authenticated carrier/chain/UTXO/sender evidence, versioned source runtime rules, state and activation, proof verification and funded economic integration. Complete function observation coverage does not supply those capabilities.
