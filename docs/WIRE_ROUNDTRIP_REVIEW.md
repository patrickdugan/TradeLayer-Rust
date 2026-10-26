# Pinned encoder, Types and decoder composition

The new corpus executes complete, unchanged `txEncoder.js`, `types.js` and `txDecoder.js` from `patrickdugan/tradelayer.js@dca0888b827c5bc28fa9189bc9e1d507f5c5670c`. It adds **169 independent composition cases** to the existing 434 encoder vectors, 64 encoder observations and 1,447 historical decoder observations. It changes none of those older fixtures or implementations.

The generator uses genuine archived BigNumber **9.3.1**, Base94, Base256 and bigint-base-converter source. Each complete source is hash checked before execution. Only Types' external services are adapters: `Validity` records its actual invoked method and all arguments, then returns the same params unchanged; unused `TxUtils` and `TxIndex` are inert objects. The adapter supplies any requested validator name; it does not establish that the original validity module exports that name or would accept the params. These traces identify dispatch, not validation. No source text, arithmetic expression, exported encoder/decoder method or algorithm is replaced.

| Source | LF-normalized SHA256 |
| --- | --- |
| txEncoder.js | `c3b95909e66ef04320501fd8e919ae954f761af0449a09782c083e71a7ef8c42` |
| txDecoder.js | `a573e2ad3e843b2c573cb3f61c8d67b64ff05f1a554d36e3ad6bd5764d86d076` |
| types.js | `a49c1b1f846fe3eab91f4434c9f016586ffddf1bb3fb4492516a065df396a0e1` |
| BigNumber9.3.1 | `58ba6fb59668342346f17983a61ace455f6f425ac4fbfab84f489df257594d16` |
| Base94 | `0b33c7b2beaa3d86f13a3ef5c995121457281b2d744cc87f8ca58d32070234eb` |
| Base256 | `11a0d6be7142f9baa4e41e9c9c295c80583b803b7d79495ed260d28d45db6edc` |
| bigint-base-converter | `a047cb70ef64fce2ad7a70ed4dc844efc05d3c72c5ccf4f53ed7957b95b35b62` |

The corpus includes 122 direct-encoder cases, all 36 numeric Types encoding routes, and 11 raw-body/dispatch observations. It covers all **39 active encoder exports** and all **36 active decoder exports**, including helpers and the empty batch stub. Cases exercise field order, optional fields, Number versus String, safe-integer and radix boundaries, subatom rounding, delimiters, Unicode, empty/truncated/extra fields, returned Error objects and the misordered txIndex call signature. Each Types encoding call is followed, where reached, by its real Types decode route and a separately recorded direct decoder call.

## Concrete source findings

| Cases | Actual source behavior |
| --- | --- |
| `generic-0` through `generic-31` | Types adds `tl` and its selector before invoking an encoder that usually adds the same prefix. Removing only the outer three characters leaves the inner prefix in the decoder body. The corpus preserves this rather than stripping until a plausible body appears. Send-all and multi-send have different source framing. |
| `generic-4` | Types calls absent `Encode.encodeCommitToken`; the real exported encoder is `encodeCommit`. This throws before decoding. |
| `direct-0` | The encoder converts the hexadecimal code hash to base36. The decoder returns that base36 string unchanged under `codeHash`; it does not reconstruct the original hexadecimal. |
| `direct-1`, `direct-4`, `direct-13` | Encoders join list members with commas. Decoders split the entire payload on commas before interpreting their list field. Multiple members shift subsequent fields; the bracketed commit list produces a NaN member. |
| `direct-2`, `helper-amount`, amount boundaries, option cases | Fractional bimodal amounts omit the decoder's `~` flag. For example `0.12500001` is encoded as `7fx29`, then decoded as **12,500,001 token units**. Successful source calls need not round-trip their inputs. |
| `direct-9` | Boolean `true` is emitted literally; the decoder requires the literal string `1` for revoke, so it returns false. Metadata is uppercased. Unicode expansion is observed independently of the Rust encoder's ASCII restriction. |
| `direct-15`, `generic-15` | The direct encoder ignores oracle ID and returns only `tlf`. An explicitly peeled direct body is empty and decodes to zero; the generic outer prefix leaves `tlf`, decoded as a base36 number. Types invokes the publish-oracle validator with the resulting primitive number; the previous block assignment is lost. |
| `direct-21` | Withdrawal emits a decimal scaled amount and Boolean `true`; the decoder reads the amount in base36 and requires `1` for the flag. Both amount and flag can disagree. |
| `oracle-close-only` | Optional high/low/close fields have no placeholders. Supplying only close moves it into the high field. |
| `direct-24`, `direct-25` | Synthetic IDs and amounts are asymmetric. Mint decodes `s-5-37` as `s-5-115`; redeem uses a plain base36 integer prefix and obtains 28. Both interpret the encoded whole amount as atomic units. |
| `fractional-contract-string` | The String `0.125` passes the source encoder unchanged; the decoder's `parseInt(...,36)` returns zero. A fractional Number uses V8's fractional base36 spelling instead and is outside the local Rust encoder domain. |
| `direct-23`, `settlement-negative-base94` | The later duplicate settlement decoder is active; transaction IDs remain raw strings and the king-style optional fields are present. A negative Base94 input throws `Error: Digit is invalid: -`; it is not converted into a valid signed settlement. |
| `generic-28`, `generic-29`, `generic-34`, `generic-35` | The decoder names requested by Types are missing. The standalone `decodeAbstractionBridge` does exist, but substituting it for absent `decodeCrossLayerBridge` would change the source. |
| `generic-32`, type33 cases | These Types routes return decoded params without invoking a validator. Rollup content remains opaque text, including its separators and JSON. |
| `empty-batch-stub` | `encodeBatchSettlement` returns undefined; no body or decoder result is fabricated. |
| `utxo-no-refs`, `utxo-identity-validator` | Type3 returns decoded params without validation when both reference outputs are absent. With explicitly injected matching vouts, its real validator call occurs; the adapter is still only an identity observation. |
| `bad-marker-returned-error`, `txindex-three-args`, `txindex-three-args-tl` | Invalid marker and unknown type return Error objects. They are distinct from thrown exceptions. The three-argument call shifts values into the wrong positions of the nine-argument API. |

The fixture records 164 successful string encodings, four source throws and one undefined return. Direct decoding yields 150 returns, four throws, ten absent-method observations and five unreached calls. Complete Types decoding yields 127 returns, 18 throws, three returned Error objects and 21 unreached observations. A successful identity-adapter return has no `valid` or settlement authority added by the harness.

## Fixture and Rust consumers

`fixtures/wire_roundtrip_vectors.tsv` has 18 fields per `W` row. UTF8 strings and JSON are hex encoded; `-` denotes zero bytes. Thus delimiters, control characters and Unicode cannot alter TSV framing.

| Index | Field |
| --- | --- |
| 0–5 | `W`, name, `direct|generic|raw`, numeric type or `-`, encoder export or `-`, input JSON hex |
| 6–8 | Rust encoder disposition, `ok|throw|undefined`, encoded String or `[errorName,errorMessage]` as JSON hex; undefined uses JSON null |
| 9–11 | Exact body policy, decoder export, exact body UTF8 hex |
| 12–13 | Direct decode `ok|throw|missing|unreached` and tagged result/error/missing method as JSON hex |
| 14–15 | Complete Types decode `ok|throw|error|unreached` and tagged result/error as JSON hex |
| 16–17 | Ordered identity-validator call trace as JSON hex; descriptive note |

Tagged values use the same recursive representation as `HistoricalValue::diagnostic_json`: `undefined`, null, Boolean, String, array and insertion-ordered object tags; Number values are exact 16-digit binary64 bit patterns, or `NaN`, `Infinity`, `-Infinity`. Negative zero is therefore distinct from positive zero. Error objects are observed explicitly before object enumeration would erase their message. Input `-0` is retained as the JSON token `-0`.

Body policies are deliberate experiments. `strip-one` removes one generic outer prefix. `strip-one-if-present` removes exactly the known direct prefix when present. `identity` leaves body-only exports unchanged. `truncate` and `append` first peel one matching direct prefix, then retain two code units or append the recorded extra fields. `raw` supplies the literal body. These operations are harness framing choices, not a claim that the chain parser performs them.

The integrated `tests/wire_roundtrip.rs` target has four passing tests using
the existing APIs:

- `legacy_txencoder_audit::source_encode_method_unverified` compares every supported direct return, including undefined.
- `legacy_types_audit::source_generic_concat_unverified` assembles the actual Rust inner return and checks each generic output, retaining the missing-Type4 error.
- `legacy_txdecoder_audit::source_decode_historical_method_unverified` compares every reached direct return's complete tagged JSON and exact source throw. Missing exports remain `MissingMethod`.
- `source_decode_historical_type_unverified`, `source_route` and `source_inspect_decode_invocation` check Types' actual missing names, validator inventory and marker/signature outcomes. Complete Types metadata injection and its identity-validator results remain explicitly source-only observations; the static Rust inspection API does not execute historical validation.

Every row has an asserted encoder disposition: 149 supported/exact cases (including the generic missing export and undefined stub), two ASCII rejections, two V8 Number radix36 rejections, two nonfinite BigNumber rejections, three source-failure rejections, and eleven explicit raw-body cases. The last groups still exercise their reached Rust decoder calls. Nothing is silently skipped. Source failures assert typed Rust rejection; the Rust encoder API does not expose the source exception's exact name/message.

## Checks and limits

Run offline from the repository root:

```text
node scripts/wire_roundtrip_vectors.cjs --write
node scripts/wire_roundtrip_vectors.cjs --check
cargo test --test wire_roundtrip --frozen
```

Default mode checks the committed fixture. The generator reads no precomputed expected encoder/decoder vectors. It derives all outcomes directly from the independently supplied arguments and complete source modules. Repeated regeneration/checks produce identical bytes under Node **v24.19.0**. The real `tests/wire_roundtrip.rs` target now passes all four tests in the integrated Rust **1.90.0** Cargo suite; the Node check also passes all 169 composition cases. Coverage includes all 39 encoder exports, 36 decoder exports and 36 numeric Types encoding routes. The source Types identity adapter observes dispatch and arguments; it does not execute the original validity rules or establish transaction acceptance.

The existing encoder's bounded JSON/printable-ASCII/safe-Number restrictions remain intentional local restrictions. Decoder observations include nonfinite and negative-zero results and do not turn those values into accepted transactions. This is compatibility evidence, not a corrected wire format, authentication, historical-chain acceptance or authorization to mutate balances.
