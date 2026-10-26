# Balance payloads from the existing Ledger

`state_oracle` ports the six functions in pinned `stateOracle.js` at commit
`dca0888b827c5bc28fa9189bc9e1d507f5c5670c`. The complete unchanged LF archive has
SHA256 `4a8a06227544ffa12c43c5fe5f987dc02b1ae2c1c0f27aa70de3645fdcc57988`.

The Ledger builder queries actual available atoms through an immutable borrow.
Channel, order and margin custody are not folded into available balance. It
neither creates a balance store nor mutates supply, accounts or replay IDs.
A callback builder supplies the source's optional tally scalar for adapters
and source comparisons. Calls preserve requested address order and duplicates
before the final stable sort. Missing tallies become zero.

## Source functions and numerical behavior

| Source function | Rust surface |
| --- | --- |
| `sha256Hex` | SHA256 result used by payload and Base64 hash helpers |
| `quantize8` | `quantize8` |
| `bucketize` | `bucketize` |
| `buildAddressBalancePayload` | Ledger and callback payload builders |
| `encodeBalancePayload` | Typed payload encoding and generic JSON-safe wrapper |
| `payloadHashFromB64` | Bounded source-compatible permissive Base64 hash |

Quantization and bucketing retain binary64 evaluation order. Quantization uses
`floor(n * 1e8) / 1e8`, rather than Decimal rounding. Even a genuine available
balance such as 0.29 can lose one displayed atom at this source reporting
boundary. It never changes the underlying Ledger. Zero and negative requested
bucket sizes retain their quantized metadata while the calculation uses a
minimum positive step of 1e-8. Negative balances in callback characterization
retain floor behavior. Nonfinite inputs and overflowed intermediate values
reject; the source's resulting null JSON values are separately characterized.

JSON Number rendering uses shortest round-tripping decimal candidates and
ECMAScript fixed/scientific exponent rules. The same finite-number formatter
is shared with issuance-intent JSON metadata. UTF-8, field order, escaping,
Base64 and SHA256 bytes are checked against complete unchanged JavaScript.

## Address ordering

Account identifiers use the existing printable ASCII domain. Sorting preserves
the observed en-US locale primary order, punctuation significance and deferred
case tie-breaks, with lowercase before uppercase. It is not byte ordering.
Equivalent duplicate requests remain stable. Source `localeCompare` depends on
the host locale and ICU configuration; the Rust policy and oracle's en-US
configuration are explicit. Other locale and Unicode collation domains are
not silently claimed compatible.

## Encoding and interpretation

Typed payloads retain schema, property ID, quantized bucket size and sorted rows.
The strict decoder checks the fixed schema, field order, canonical bytes and
bounds. It does not prove a claimed balance or recompute its bucket: repeated
source quantization can drift, and the stored bucket size does not preserve the
original raw request. Re-hashing a fabricated canonical payload is possible.
Independent state provenance and real signatures remain consumer requirements.

The generic encoder retains `payload || {}` for bounded JSON-safe values.
It shares the issuance-intent JSON limits of 64 KiB, 4096 nodes and depth 16.
Typed payloads allow 4096 rows and 2 MiB of JSON. Base64 input has its own 3 MiB
bound. Counts and lengths reject before callbacks or decoded allocation where
applicable. There is no background network or signing operation.

The source hash helper deliberately retains Node Buffer's permissive Base64
semantics: URL alphabet, skipped invalid characters, missing padding and `=`
termination, including its UTF-16 low-byte behavior. The strict typed decoder
is separate and rejects alternative encodings. A source-compatible hash does
not imply a valid typed balance payload.

The checked-in oracle covers 2319 rows: 2136 numeric renderings, 91 payloads,
17 generic encodings and 75 Base64/hash cases. Eight payload inputs explicitly
characterize the unsupported nonfinite source domain. Rust tests compare final
JSON/Base64/hash bytes and exercise real Ledger custody, callback ordering,
locale cases, canonical forgeries and resource limits.
