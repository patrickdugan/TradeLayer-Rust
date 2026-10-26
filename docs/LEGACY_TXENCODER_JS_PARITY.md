# File 7/49 — txEncoder.js source-string parity

Historical source: patrickdugan/tradelayer.js@dca0888b827c5bc28fa9189bc9e1d507f5c5670c, complete src/txEncoder.js (684 lines), src/types.js and src/txDecoder.js. The archived encoder's LF-normalized SHA256 is `c3b95909e66ef04320501fd8e919ae954f761af0449a09782c083e71a7ef8c42`; its Git blob is `94201b4eaedcd240b9a9f6d54d11b55206615dff`. Existing archives remain unchanged. Hash comparisons normalize checkout CRLF to the pinned LF bytes.

**Research-only source encodings; NOT a production transaction constructor, authenticated OP_RETURN, protocol admission or signed channel state.** No Rust Ledger, wallet, Registry, chain adapter, orderbook, clearing, AMM or insurance changes. Zero added third-party Rust crates.

## Source interoperability findings

| Source path | Exact behavior | Why it matters |
|---|---|---|
| Type0 activate | Encodes a type list joined by semicolon and a 256-bit hexadecimal code hash converted by BigInt to base36, with complete tl0 prefix | Encoding an activation is not proof of native token genesis or administrator signature |
| Type1 issue | Multiple whitelist IDs joined by literal commas, while decoder splits all commas first and expects one semicolon-delimited whitelist field | Multi-whitelist issue shifts managed/backup/other fields |
| Type2 send | sendAll and multi-array forms return body only; scalar source already returns tl2-framed string | Generic Types.encodePayload always prepends tl2, producing a double type prefix for scalar but not sendAll |
| Fractional amount | Source BigNumber scales by 1e8 but omits tilde suffix on fractional amounts because of nullish-coalescing precedence | An exact 0.5 token encodes as tro8w, which source decoder reads as 50,000,000 whole tokens |
| Type4 commit | Direct encoder is named encodeCommit and produces complete tl4; generic Types calls ABSENT encodeCommitToken | Generic encoding throws. Multi-clearlist array [7,b] is split on comma before decoder bracket detection and yields NaN / shifted fields |
| Type7/8 | Complete prefix, raw text fields, base36 admin target, Boolean flags | Caller fields are not verified addresses or historical administrators |
| Type9 attestation | Directly interpolates revoke: number 1 yields literal 1, Boolean true yields true | Source decoder only treats literal 1 as revoke; Boolean true becomes false |
| Type15 close oracle | Due to operator precedence, encodeCloseOracle ignores its ID and always returns tlf | Generic wrapper produces tlftlf. Types subsequently invokes the wrong publish validator on primitive decoder output |
| Type21 withdrawal | Uses decimal-scaled amount text and literal true/false string flags | Decoder treats amount as base36 and expects 1 for withdrawAll. One token encodes 100000000 decimal, decoded as 2821109907456 atoms; true decodes as false |
| Type22 transfer | Full tlm prefix; base36 atomic amount; destination >42 ASCII chars mapped to claimed ref:N | Source reference text is not UTXO or ownership authentication |
| Type30 fraud/stake/relay | Source 13 comma-separated fields with BigNumber amount scaled by 1e8 and ROUND_DOWN | No oracle authority or fraud proof follows from encoding |
| Types33–35 | Colored coin, bridge, smart-bind encoders return body-only; generic Types adds the selector | Type33 validator is commented out; Types34/35 dispatch to absent decoder exports |

## Rust implementation

src/legacy_txencoder_audit.rs retains the original atom-oriented typed wrappers for 14 type families and adds `source_encode_method_unverified(method, &JsonValue)` for **all 39 own callable exports**. `SOURCE_ENCODER_METHODS` inventories those exact names; tests require every name to occur in actual-source differential cases. This is function coverage within the domain below, not complete JavaScript-value or consensus coverage.

The method dispatcher returns `SourceEncoderReturn::Encoding(UnverifiedSourceEncoding)`, `Helper(String)`, or `Undefined`. The two standalone helpers remain helper results. The source's empty `encodeBatchSettlement` body remains `Undefined`; it does not become an empty signed transaction or a fabricated batch implementation. The existing typed wrappers keep their public API and stricter atom/identifier limits.

| Source exports | Concrete functionality |
|---|---|
| encodeAmount, encodePropertyId | Whole/fractional amount path with missing tilde; synthetic `s-` identifiers use actual parseInt-style prefix parsing and ignore later components |
| encodeActivateTradeLayer | Scalar/array selector coercion, semicolon join, arbitrary hexadecimal hash up to the local bound converted exactly through BigInt-style magnitude |
| encodeTokenIssue, encodeSend | Raw/nullish fields, truthy flags, whitelist comma join; sendAll, unequal multi arrays, scalar/array first-amount paths and mixed framing |
| encodeTradeTokenForUTXO, encodeCommit, encodeOnChainTokenForToken, encodeCancelOrder | Decimal scaling with actual Number bridges, decimal-BigNumber base36, long address reference fallback, optional integer-only clearlist ID, truthy branch versus loose 0/1 price dispatch, source spot price multiplier **8** |
| encodeCreateWhitelist, encodeUpdateAdmin, encodeIssueOrRevokeAttestation | Raw joins and Boolean/Number/String distinctions |
| encodeAMMPool | Strict true/1/"1" flags; alpha base36 strings preserved lowercase before numeric coercion; finite Number truncation and nonfinite fallback |
| encodeGrantManagedToken, encodeRedeemManagedToken | propertyid/propertyId nullish aliases, amount fallbacks, reserved empty grant field, truthy DLC metadata |
| encodeCreateOracle, encodePublishOracleData, encodeCloseOracle | Comma whitelist list, price-only 1e4 scaling, optional unscaled high/low/close, precedence-induced ignored close ID |
| encodeCreateFutureContractSeries, encodeExerciseDerivative | Source **extra nested** onChainData `[i][0][0/1]` reads, optional expiry/fee and raw leverage |
| encodeTradeContractOnchain, encodeTradeContractChannel, encodeTradeTokensChannel | Half-up price atoms, raw amount radix36, exact per-source field order and flags |
| encodeWithdrawal, encodeTransfer | Decimal versus base36 amount distinction, raw withdrawal flags, long reference and payRef fallback |
| encodeSettleChannelPNL, encodeKingSettle | Existing actual-source Base94 conversion including ambiguous `|` digit and fractional leading-zero loss; legacy aliases; optional extension triggered by presence, including null |
| encodeMintSynthetic, encodeRedeemSynthetic, encodePayToTokens | Distinct source field names, raw radix36 quantities |
| encodeOptionTrade | encodeAmount reuse, optional combo only when ticker and amount are both truthy, no invented price scaling repair |
| encodeTradeBaiUrbun, encodeTradeMurabaha | All active source fields in original order, including separate expiries and installment interval |
| encodeStakeFraudProof | Number coercion of action/IDs with truthy-zero defaults, explicit ROUND_DOWN amount, all thirteen fields |
| encodeBatchSettlement, encodeBatchMoveZkRollup | Undefined empty stub; separate real body-only payment list, colon/comma/semicolon/pipe joins and JSON.stringify metadata |
| encodeColoredCoin, encodeAbstractionBridge, encodeBindSmartContract | Original body-only strings and raw coercion; no fabricated missing decoder or validator |

The result UnverifiedSourceEncoding has private fields and exposes an exact SOURCE STRING for comparison. Its source_generic_types_concat_unverified method calls the existing File5/49 read-only Types generic concatenation diagnostic, which explicitly refuses Type4 because the missing source export is real. Caveats expose source vulnerabilities, not authenticated actions. Historical transaction, list/activation, co-signed revision and economic execution permission methods ALWAYS return errors.

The complete method dispatcher uses the existing finite, bounded `issuance_intent::JsonValue`. Missing object fields differ from explicit null. Boolean truthiness, empty-array/object truthiness, raw String/Array/Object conversions, Array.join nullish empty slots, optional fields, JSON property enumeration, Number parsing and original framing are preserved. No input or external state is mutated.

Local domain restrictions, **not historical consensus rules**:

- Serialized input at most 32,768 bytes; existing JSON limits also apply (16 depth, 4096 nodes). Every string/key is printable ASCII and at most 4096 bytes. Every array has at most 64 entries. Final source result is printable ASCII and at most 4096 bytes.
- JSON cannot represent undefined values, functions, symbols, BigInts, unpaired surrogates or custom prototypes. Missing fields model source undefined. An own `toString` property used for primitive coercion is rejected instead of invoking caller code.
- A source **Number.toString(36)** is supported only for finite signed integer Numbers of magnitude at most 2^53−1. Fractional Number radix36 and unsafe integer Numbers fail explicitly. This includes raw fractional contract quantities supplied as Number; numeric **Strings retain their original spelling** where the source calls String.toString(36). This adapter does not silently substitute a decimal algorithm for V8 fractional base36.
- BigNumber operands are finite decimal Number/String inputs, at most 512 input characters, within existing Decimal work bounds. Decimal strings support subatoms and large magnitudes. Their base36 conversion uses 20 **base36 fractional places** and half-up rounding, including carry into the integer. Actual source `.integerValue()` uses half-up; stake/fraud uses ROUND_DOWN. Only actual `.toNumber()` sites cross binary64 before the safe-integer radix36 check. NaN/Infinity source observations remain separate; invalid BigNumber input is rejected.
- Numeric Number(string) supports the ASCII grammar; nonnumeric strings become NaN where the source explicitly consumes that result (for example settleType) or applies finite fallback (AMM). Prefixed binary/octal/hex integer coercions are capped at u64. Synthetic-ID parseInt numeric results must fit the safe-integer radix36 domain. Reference substitutions require a String address; no array/object address coercion is added.

The original atom wrappers retain their separate nonnegative safe-atom domain. Their exact-atom abstraction corresponds to whole token Number inputs or nonwhole exact decimal String inputs; it must not be read as a promise that converting arbitrary atoms to a binary64 token Number preserves those atoms. The actual-source corpus demonstrates the distinction: Number `90071992.54740991` has already rounded to `90071992.5474099`, so encodeAmount returns one fewer atom than the exact decimal String `"90071992.54740991"`. The new method dispatcher preserves the input kind and this difference. Paths that consume a BigNumber result require it finite; a whole branch can ignore an unused source NaN intermediate (for example Boolean/null encodeAmount inputs).

A successful encoding remains a source string, with permanent default-deny permission methods. It does not prove that the source decoder agrees, that an order or amount is valid, or that signatures and referenced outputs authorize it.

## Exact source tests

`fixtures/legacy_txencoder_source_vectors.tsv` retains its 22 frozen inner/generic pairs. Its existing audit runner now uses genuine archived BigNumber9.3.1 instead of the old NumberStub, retains every original assertion, and normalizes CRLF for source and fixture reading.

`scripts/txencoder_vectors.cjs` executes **the complete unchanged encoder** with genuine archived BigNumber9.3.1, complete unchanged Base94/Base256 helpers and their actual archived bigint-base-converter0.1.3. It checks pinned hashes before execution. Logging is silenced; imported wallet/chain/DB services are absent. The separate Types checks run complete unchanged types.js with explicit inert unused imports. No encoder expression, rounding path or number conversion is replaced by a stub.

`fixtures/txencoder_vectors.tsv` contains **434** successful differential cases across all **39** exported methods, including 96 deterministic seeded amount pairs, decimal half-up/down and radix rounding boundaries, Number/String distinctions, nullable and missing fields, original malformed framing, Base94 and batch JSON output. The columns are `V`, name, method, JSON-input UTF8 hex, `string|undefined`, output UTF8 hex (`-` is empty/undefined).

`fixtures/txencoder_quirks.tsv` separately records **64** source/dispatch observations: missing fields, thrown source errors, nonfinite/coercion boundaries, fractional/unsafe Number radix36, shallow onChainData behavior and positive-length object `.map` failure, negative-price half-up, Base94 delimiter collisions, and actual generic Types dispatch results. `Q|D`, name, method, JSON-input UTF8 hex, `string|undefined|throw`, output/error UTF8 hex, note. These observations are not automatically admitted into the Rust supported domain. Both fixtures are deterministically regenerated by `--write` and checked byte-for-byte by default or `--check`.

Rust tests compare the full supported corpus byte-for-byte, require exact export inventory coverage, retain all earlier checks, and explicitly test source coercions, half-up versus truncation, Base94/JSON ambiguity, bounded failure and immutable input. The isolated actual-source Rust harness passes all 13 encoder tests; parent integration runs the full Cargo checks.

    node scripts/audit_legacy_txencoder_source.cjs
    node scripts/txencoder_vectors.cjs --check
    node scripts/txencoder_vectors.cjs --source /path/to/pinned/src/txEncoder.js --check
    python3 scripts/check_rust_supply_chain.py
    cargo fmt --all -- --check
    cargo clippy --all-targets --frozen -- -D warnings
    cargo test --frozen --test legacy_txencoder_audit
    cargo test --frozen
    cargo build --all-targets --release --frozen

## Remaining boundaries

All active exported encoder functions now have useful bounded functional coverage, including the deliberately empty batch stub. Arbitrary V8 fractional Number radix36, arbitrary JavaScript objects/prototypes and nonfinite BigNumber results remain outside the supported domain. Source defects such as missing Type4 generic export, prefix mixtures and Type15/21 incompatibilities are preserved. This module does not supply a corrected canonical wire format, authenticated reference/UTXO validation, wallet signing, historical activation proof or accepted economic dispatch. Those require their own explicit version and authority boundaries.
