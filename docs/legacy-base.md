# Pinned legacy Base94 and custom Base256 compatibility

`src/legacy_base.rs` adds dependency-free, bounded **string-domain source compatibility** for `src/base94.js` and `src/base256.js` at `patrickdugan/tradelayer.js@dca0888b827c5bc28fa9189bc9e1d507f5c5670c`. It is a standalone diagnostic helper. No transaction parser, admission rule, signature verifier, chain adapter, or state transition calls it.

The reference sources and the actual installed `bigint-base-converter` 0.1.3 implementation are archived unchanged. The companion script checks each Git blob hash before executing the files in isolated Node VM contexts. It executes the dependency's real implementation, with no replacement arithmetic or mock converter.

| Archived source | Git blob SHA-1 |
| --- | --- |
| `fixtures/legacy_base94_source.txt` | `8d9f3d14d0ca29488c99e460c52020e8dd66b334` |
| `fixtures/legacy_base256_source.txt` | `90305dec38f6be043f039d0e75b296344ac57ba0` |
| `fixtures/legacy_base_converter_source.txt` | `c38f00093bf022f4d2ef136a3e34480d409c60fd` |

The first two blobs were extracted from the exact pinned Git revision. The third is the complete `node_modules/bigint-base-converter/convertbase.js` from the locally installed version 0.1.3; its MIT license is preserved in `fixtures/legacy_base_converter_license.txt`. Dependency source pinning describes the tested artifact, not a claim of npm package provenance beyond that locally installed package.

## API correspondence

All public conversions return `Result<_, LegacyBaseError>`; none accept floating point values. Pass explicit strings to retain large integers exactly. `Option<&str>` models null/undefined only where both have the same source behavior; numeric JavaScript falsiness and arbitrary object coercions are outside this interface.

| JavaScript helper | Rust function |
| --- | --- |
| Base94 `toBase94` | `base94_from_decimal` |
| Base94 `decimalToBase94` | `base94_encode_decimal` |
| Base94 `fromBase94` | `base94_to_decimal` |
| Base94 `validateBase94Input` | `base94_validate_input` |
| Base94 `hexToDecimal` / `decimalToHex` | `base94_hex_to_decimal` / `base94_decimal_to_hex` |
| Base94 `hexToBase94` / `base94ToHex` | `base94_hex_to_base94` / `base94_to_hex` |
| Base256 `toBase256` / `fromBase256` | `base256_from_decimal` / `base256_to_decimal` |
| Base256 `hexToDecimal` / `decimalToHex` | `base256_hex_to_decimal` / `base256_decimal_to_hex` |
| Base256 `hexToBase256` / `base256ToHex` | `base256_hex_to_base256` / `base256_to_hex` |

`custom_base256_alphabet()` returns the precise 256-character source alphabet. Its first digit is `!`; its final digit is U+0143. Its characters occupy more than 256 UTF-8 bytes. It is text encoding, not a one-byte-per-digit binary format, and includes `,`, `;`, `|`, `-`, quotes, and other framing punctuation.

## Preserved behavior and ambiguities

- Both alphabets use `!` for zero. Integer conversion removes leading zeroes. Empty input to the integer converter produces zero; it does not preserve the empty encoding.
- Base94 `fromBase94` returns null for an absent/empty string; Base256 `fromBase256("")` returns `"0"`. Base256's hex wrappers return an empty string for absent/empty hex or encoded input; `hexToDecimal` returns `"0"` for absent/whitespace-only hex. These wrapper differences are preserved.
- Base94 `|` is both digit 91 and the decimal separator. `toBase94("91")` returns `"|"`, while `fromBase94("|")` returns `"15"`: the empty integer component falls back to the literal alphabet digit `0`, whose value is 15. Additional separator components are ignored. No canonical inverse exists for all outputs, and validation accepts `|`.
- Fractional parts are independently converted as integers. `decimalToBase94("1.001")` returns `"\"|\""`, which decodes as `"1.1"`; the original scale is unrecoverable. `1.2.3` only uses `1` and `2`. Integer signs, exponent notation, and decimal points are rejected by the underlying integer converter. A hyphen inside an encoded alphabet value remains a legal digit, not a sign.
- Base256 strips commas and the precise ECMAScript whitespace set from decimal inputs. Its hex helper strips whitespace and allows odd length. Base94's raw hex helper prepends `0x` without that cleaning. Already-prefixed hex is not silently accepted by either helper.
- Base94 hex output has no width padding. Base256 pads to an even character count, including the sign: `decimalToHex("-16")` returns the source bug `"0-10"`. Neither preserves a 32-byte transaction hash's leading zeroes. The source `BigInt(string)` helpers accept signed decimal, unsigned `0x`/`0o`/`0b`, empty/whitespace strings, and reject signed nondecimal prefixes; this string syntax is retained.
- The converter validates digits in reverse order, so the last invalid digit determines its error message. The separate Base94 validator visits characters in forward order. Golden vectors check the source's error messages as well as successful strings and null returns.

The missing scale and ambiguous delimiters cannot be recovered without an external format/version decision. This implementation neither guesses the intended value nor changes these historical expressions into a purported corrected consensus format.

## Deliberate limits

Every public input is limited to 4096 UTF-16 code units **before** cleaning, trimming, or splitting, using `MAX_INPUT_CODE_UNITS`. This is a local resource limit, not present in the JavaScript and not a protocol rule. A little-endian byte-vector integer uses bounded multiply/add and small-radix division, so inputs beyond u128 and JavaScript's safe number range remain exact without new Cargo dependencies. Expanded internal decimal strings are not incorrectly subjected to the external input limit halfway through conversion.

Rust strings cannot contain lone UTF-16 surrogates. For a supplementary Unicode character rejected by the JavaScript converter's `split('')`, the source error contains only its final UTF-16 code unit, an unpaired surrogate. Rust returns the explicit `UnrepresentableUtf16Error` instead of claiming byte-identical error text. The independent JavaScript script asserts this source behavior; a Rust test checks the stated boundary. The Base94 validator uses JavaScript `for...of`, whose complete Unicode character error can be preserved. These restrictions also mean there is no general JavaScript `String(value)`, object, Number-rounding, symbol, or malformed UTF-16 compatibility claim.

## Verification

`node scripts/legacy_base_vectors.cjs` independently reruns the archived actual JavaScript and compares all 1,221 checked-in golden rows byte-for-byte. `--write` explicitly regenerates the fixture. The TSV carries UTF-8 hex input/output, null markers, success/error/null status, and a readable case label; Node is unnecessary for ordinary Rust tests.

`tests/legacy_base.rs` compares all 1,221 rows, covering every Base94 and Base256 alphabet digit, 32 seeded 256-bit samples, zero/empty/leading zeroes, null wrappers, invalid inputs, decimal fractions, separator ambiguity, Unicode whitespace, source BigInt syntax, and padding. Additional tests cover local resource limits, conversion of the maximum 4096 Base256 digits through expanded decimal/hex outputs, and the explicit UTF-16 error boundary.

Validation for this slice: Node source audit passed; six targeted Rust tests passed on the repository's pinned Rust 1.90.0 MSVC toolchain. Broader integration checks are recorded by the coordinating task. These checks establish tested helper behavior only; they do not establish historical transaction acceptance, mainnet parity, authentication, economic validity, or production readiness.
