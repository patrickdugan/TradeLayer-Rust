# DLC oracle relay verification

`dlc_oracle_bridge` ports the complete pinned `dlcOracleBridge.js` using the
native safe Rust secp256k1 verifier. It exposes the source compatibility result
and a separate **TrustedPayloadBoundV1** check with independently supplied
expectations. Both are evidence checks: neither result admits a transaction,
changes a Ledger, verifies an authenticated balance transition, or establishes
an oracle's authority over the chain.

The module is stateless. It does not create another balance owner, persist
relay acceptance, provide replay protection, sign messages, or implement DLC
Schnorr/adaptor signatures. It checks the compact ECDSA relay signature used
by this particular source file.

## Complete source surface

| Source method/helper | Rust API |
| --- | --- |
| `fromNodeDlcLike` | `source_from_node_dlc_like` |
| `parseRelayBlob` | `source_parse_relay_blob` |
| `validateRelayBundle` | `source_validate_relay_bundle` |
| `canonicalRelayMessage` | `canonical_relay_message` |
| `sha256` of canonical UTF-8 | `relay_message_digest` and the shared native SHA-256 |
| `toPubkeyBuffer` | `source_pubkey_buffer` |
| `toSigBuffer` | `source_signature_buffer` |

Inputs use the existing ordered, finite `JsonValue` representation. Aliases,
falsey defaults, nullish outcome-index selection, first-array-element
selection, and normalized field insertion order match the source. No raw
field is silently upgraded to authenticated metadata.

The canonical signature message has this exact field order:

```json
{"eventId":"event","outcome":"outcome","outcomeIndex":0,"stateHash":"state","timestamp":0,"payloadHash":"optional"}
```

The five leading fields always appear. `payloadHash` appears whenever the
field exists and is not null; empty string, false, and zero are included after
JavaScript string conversion. This differs from the later truthiness test
that decides whether to check the payload hash. Default and numeric coercion
use the source's JavaScript behavior: arrays join recursively, ordinary
objects stringify as `[object Object]`, and derived NaN/infinities serialize
as JSON null. A JSON own noncallable `toString` can cause the same primitive
conversion exception as the source. Finite binary64 rendering reuses the
shared differential-tested JavaScript number formatter.

Normalization accepts JSON objects and arrays, not primitive values.
`parseRelayBlob` first performs the source's JSON parsing step; an already
parsed object does not bypass it. Parsed scalar values remain scalar fallback
results, while parsed objects/arrays are normalized. `b64:` blobs use Node's
permissive Base64 behavior and UTF-8 replacement decoding before JSON parsing.

## Source verdict and exception behavior

`RelayVerdict` carries the actual source `valid` boolean and optional reason.
Malformed nominal public-key/signature lengths produce the source's invalid
encoding verdict. Correct text lengths with truncated or invalid decoded
buffers may instead raise a typed `SourceTypeError`, just as genuine
tiny-secp256k1 throws. A well-formed signature that fails verification returns
an invalid signature verdict. Zero scalar signatures return false; scalars at
or above the group order raise the source signature exception.

Source hex decoding trims JavaScript whitespace, checks UTF-16 text length,
and preserves `Buffer.from(hex,'hex')` stopping at the first invalid pair.
It does not pretend that a nominal text length proves a valid SEC1 point.
The native verifier checks actual points. The source accepts valid hybrid
65-byte SEC1 encodings with prefix 06/07; the bridge checks Y parity before
normalizing that point to native uncompressed encoding. The trusted policy
accepts only ordinary compressed or uncompressed SEC1 encodings.

Signature validation first checks the current canonical message. If it fails,
the source deletes `payloadHash` and retries the legacy message. Default
source verification accepts mathematically valid high-S signatures.

After successful signature verification, a truthy payload hash must stringify
to 64 hexadecimal characters. Its decoded payload is hashed only when
`balancePayloadB64` is truthy. The source permits Base64 whitespace, ignored
characters, URL alphabet, missing padding, and UTF-16 code-unit low bytes.
It also permits Node Buffer-compatible JSON arrays, `{type:"Buffer",data:...}`,
and array-like objects. Numeric byte conversion follows JavaScript truncation
and modulo behavior. Other truthy argument types can throw. Expected state
comparison runs last and is disabled for a falsey expected value.

These source behaviors leave material trust gaps:

- The relay supplies its own public key. A signature alone does not identify
  an independently authorized oracle.
- Legacy fallback can accept a replacement payload and replacement payload
  hash that were never covered by the accepted signature.
- A signed payload hash can pass without any payload bytes being supplied.
- A self-consistent hash, including a simple balance-query payload hash, is
  not an authenticated transition root or proof that balances are correct.

The genuine source fixture contains a signed legacy relay whose payload and
hash are replaced: source validation still returns valid. This behavior is
characterized, not silently promoted to the trusted path.

## Independently bound policy

`validate_trusted_payload_bound` requires a `TrustedRelayExpectation` supplied
by the caller from independent configuration. It includes the expected
oracle public key, event ID, outcome, outcome index, state hash, timestamp,
and exact raw payload bytes. Expectations must not be copied from the relay.

The policy requires exact string fields, numeric integer index/timestamp in
the safe binary64 range, the independently configured curve point, a present
lowercase payload hash equal to SHA-256 of the expected raw bytes, and a
present payload that decodes to those exact bytes. Public-key hex must decode
fully; truncated-hex acceptance and hybrid key encoding are excluded. Compact
ECDSA verification uses low-S and the current payloadHash-containing message
only. It never retries the legacy message.

The result contains the canonical compressed key, canonical message, message
digest, and payload hash. It is bounded evidence for those expectations. A
separate authenticated admission and replay policy must establish who supplied
the expectations, which state transition they authorize, and whether the
event was already used.

## Bounds and supported domain

The shared JSON limits are 65,536 encoded bytes, 4,096 nodes, and depth 16.
Measurement precedes cloning. Decoded relay/payload bytes are bounded by
32,768 bytes; array-like lengths are checked before allocating. Expected
event/outcome/state strings are nonempty and at most 4,096 bytes. Expected
index and timestamp are nonnegative integers at most `2^53-1`.

The finite JSON domain includes Unicode scalar strings and source Number
coercions whose intermediate nonfinite results stringify as null. Raw
unpaired UTF-16 is explicitly unsupported. Parsing JSON text that would
produce a nonfinite stored number returns `UnsupportedNumber`. Resource
limits return typed errors rather than attempting the source's unbounded
allocation. No Rust string is silently substituted for an unpaired code unit.

## Genuine-source evidence

`fixtures/legacy_dlc_oracle_bridge_source.txt` is the complete LF Git blob from
commit `dca0888b827c5bc28fa9189bc9e1d507f5c5670c`, SHA-256
`954a7fbf30b66e52b587a2d7108e080c41da812acff77ccd457c6e547cb1587f`.

`scripts/dlc_oracle_bridge_vectors.cjs` executes that unchanged source in a
Node VM with genuine Node crypto and genuine tiny-secp256k1 2.2.4. It signs
only with fixed public test keys. A recording wrapper counts real verifier
calls; it never replaces cryptographic verdicts. The script pins the package
version and SHA-256 of its entry, validation code, and WASM binary.

```text
node scripts/dlc_oracle_bridge_vectors.cjs --check --secp <tiny-secp256k1-directory>
```

`--write` regenerates the fixture. Without `--secp`, the script uses
`TL_TINY_SECP_ROOT`, then normal Node resolution of the installed package.
CI must provide the genuine pinned dependency; no mocked verification fallback
exists. The corpus has 294 observations, including 71 actual signature
verdict/exception cases, 161 canonical message/digest cases, aliases, scalar
and object parsing, hex buffers, permissive Base64, hybrid points, high-S,
legacy fallback, payload replacement, and a foreign self-selected oracle.

The Rust integration target compares these complete-source observations with
native results and separately tests trusted expectations, low-S, legacy
rejection, parser boundaries, and allocation bounds. Native arithmetic and
key/scalar tests belong to the separate secp256k1 target. The parent build
runs the combined Rust test/lint/release gates.
