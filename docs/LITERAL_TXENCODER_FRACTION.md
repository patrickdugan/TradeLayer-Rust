# Literal Number radix-36 fractions

The literal encoder dispatcher now renders finite binary64 fractions with the pinned observed V8 radix-36 digit/round/carry behavior. Examples from the unchanged source are `0.5 -> 0.i`, `0.1 -> 0.3lllllllllm` and negative zero `-> 0`. This is distinct from BigNumber's 20-fractional-place radix conversion; the existing BigNumber path is unchanged.

The implementation uses the next positive binary64 value to choose its precision interval, scales the fraction and interval together, retains V8's round-to-even/carry test, and handles the smallest subnormal interval without rounding that interval to zero. It does not normalize strings or change framing, array delimiters, signed amounts, boolean representations or original source bugs.

The previous finite/safe-magnitude domain remains: nonfinite computed Numbers and magnitudes above `2^53-1` are explicitly refused. Every finite fractional binary64 value is below that magnitude. Typed wrappers still use their original exact-atom/safe-integer/ASCII policy, and their entire source prefix is byte-identical. String `.toString(36)` continues to ignore the radix; the change applies to Number rendering only. Existing text, JSON, list, custom-coercion and output-capacity guards remain.

Evidence: the complete unchanged pinned `txEncoder.js`, genuine BigNumber 9.3.1/9.1.2 and original base converters executed 4,474 source calls. The 4,470 admitted string returns cover 4,096 deterministic raw binary64 inputs, neighboring values at subnormal/normal and integer boundaries, carry/rounding neighborhoods, both signs, negative zero, and nine complete transaction encoder call sites. Four additional scenes retain explicit unsupported-domain guards and are not counted as parity. The first ten calls are a separate proof fixture. Both numeric profiles reproduce byte-identical fixtures.

Two prior literal tests deliberately change from a fractional refusal to the exact original source return. The older Unicode fixture's raw bytes and source observations stay unchanged; its historical `guard-fractional-number` observation is now an admitted return. No other previous test assertion is relaxed.

Reproduce in an integrated checkout:

```text
node scripts/txencoder_fraction_vectors.cjs --check
node scripts/txencoder_fraction_vectors.cjs --check --lock
node scripts/txencoder_fraction_vectors.cjs --check --proof10
node scripts/txencoder_fraction_vectors.cjs --check --proof10 --lock
cargo test --test legacy_txencoder_fraction --test legacy_txencoder_unicode --test legacy_txencoder_audit
```

The portable source script defaults to its repository and existing pinned dependency fixtures; `--repo` and `--out` allow a self-contained isolated check. Expectations are output fixtures only and are never used by the implementation. Author checks use actual Rust 1.90, a locally compiled candidate encoder/support helper and a pinned immutable Core provider for existing public support types: 5 fractional + 13 previous encoder + 5 Unicode tests pass, as do strict helper/new-test Clippy checks. Whole-Core integration remains Parent-owned.

Algorithm reference: [V8 in Node v24.0.0, conversions.cc, DoubleToRadixStringView](https://raw.githubusercontent.com/nodejs/node/v24.0.0/deps/v8/src/numbers/conversions.cc). This is a primary implementation reference, not a claim that v24.0.0 is the executing host. Source observations were actually produced by Node 24.22.1 / V8 13.6.233.17-node.58. The supported profile assumes ordinary IEEE-754 gradual underflow; the source subnormal controls and native differential checks exercise that behavior.

The Rust adaptation credits Copyright 2011 the V8 project authors. All rights reserved. Redistribution terms and the V8 copyright/disclaimer are retained in [V8-NUMBER-CONVERSIONS-LICENSE](third-party/V8-NUMBER-CONVERSIONS-LICENSE). No V8 runtime dependency or trusted/authenticated transaction capability is added. This bounded path does not imply complete wire parity for unsafe integers, nonfinite/custom values, arbitrary JavaScript objects or lone UTF-16 surrogates.

## Root integration

The real Core passes all five new fractional, thirteen existing encoder and five Unicode integration tests with Rust1.90. All eight complete-source full/proof checks reproduce for both numeric profiles. Strict all-target Clippy, Cargo formatting,464 included Rust files, inventory and supply policy pass. Evidence:`fixtures/reference_fidelity/literal-txencoder-fraction-core-integration.json`. The working queued-startup proposal remains separately failed/uncommitted; its source-ordering diagnosis is unchanged. Full all-target release remains2,204Rust/239numeric, with prior Main734 scoped library evidence at1ac51ad. No new full-suite count or historical/authenticated acceptance claim is made.
