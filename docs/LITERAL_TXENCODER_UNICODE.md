# Literal txEncoder Unicode strings

The literal `source_encode_method_unverified` dispatcher now accepts bounded Unicode scalar strings in the same raw JSON positions as the pinned `src/txEncoder.js`. It preserves text without normalization: composed/decomposed strings differ, emoji are retained, and commit/transfer address thresholds still count UTF-16 code units. Newly reachable string-to-number/property-id coercions use ECMAScript whitespace, including FEFF and excluding U+0085/U+200B.

Only the literal dispatcher uses the new text/output validators. The original typed wrappers and generic Types concatenation retain their ASCII contract and bytes. No framing, delimiter, missing method, boolean, amount, fallback or source-bug repair is included. The 4,096-byte UTF-8 text/output bound, ASCII control-byte refusal, JSON/list limits, numeric limits and unknown/custom coercion refusals remain explicit. Unpaired UTF-16 surrogates remain outside `JsonValue`; this is bounded Unicode-scalar support, not a claim to represent every JavaScript String.

The driver loads the complete unchanged pinned encoder, both genuine BigNumber libraries and all original base-converter dependencies. The first ten calls are separately retained. The original Unicode gate fixture has 68 actual calls: 63 initially admitted strings and five then-guarded source scenes. The later literal fractional-Number extension also admits its exact `guard-fractional-number` source return; the other four guards remain explicit. Fixture bytes and original observations are unchanged. Refusals are not counted as source parity.

Reproduce from the integrated repository:

```text
node scripts/txencoder_unicode_vectors.cjs --check
node scripts/txencoder_unicode_vectors.cjs --check --lock
node scripts/txencoder_unicode_vectors.cjs --check --proof10
node scripts/txencoder_unicode_vectors.cjs --check --proof10 --lock
cargo test --test legacy_txencoder_unicode --test legacy_txencoder_audit
```

The source driver has no sibling work-directory dependency. `--repo` and `--out` are explicit alternatives for an isolated package; otherwise both default to the repository containing the script. Existing source fixtures and `scripts/scaling_vectors.cjs` supply pinned genuine dependencies, with normalized-text hashes checked before evaluation. The source business bodies are never rewritten.

Author verification: Rust 1.90 standalone helper with the actual candidate encoder, exact current `state_oracle`/SHA support copies and an immutable previously validated Core provider for other public support types; all five new tests and all 13 unchanged encoder tests pass. Strict standalone Clippy checks of the candidate/helper and new test pass. All four source regeneration checks pass for BigNumber 9.3.1 and 9.1.2. Whole-Core build/integration remains Parent-owned; the support provider does not validate the new Core.

Outputs remain unverified research strings. Existing authentication, list, signed revision and trusted-execution refusal APIs remain unchanged.

## Root integration verification

All five new integration tests and all thirteen unchanged encoder regressions pass against the real Core with Rust1.90. Four complete-source regeneration checks pass (full68/proof10 under genuine9.3.1 and9.1.2). Strict all-target Clippy, Cargo formatting, included formatting, inventory and supply policy pass at the frozen working context. This selective wire commit does not include the separately failing queued-startup proposal. Evidence:`fixtures/reference_fidelity/literal-txencoder-unicode-core-integration.json`. The full all-target release count remains2,204Rust/239numeric; Main734 is prior scoped evidence.
