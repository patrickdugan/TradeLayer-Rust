# BitVM cache metadata lifecycle

`BitvmCacheRegistry` implements the eight active methods of pinned `bitvmCache.js`: `_base`, `_challengeBlocks`, `buildCacheId`, `get`, `open`, `challenge`, `finalize`, `resolve`. The complete unchanged LF archive has SHA256 `053d2c2518485a0dcc0f42a76b100a945e0c568f54bcfb04a5b8fa97b80ba0ca`. Source revision: `dca0888b827c5bc28fa9189bc9e1d507f5c5670c`.

The source database category is the real existing `procedural` category. The Rust registry owns only cache/bond/challenge metadata, with records and a complete input journal. It does not own a Ledger, lock balances, verify a challenge, authorize a resolver, or prove that a bond was deposited.

Cache IDs preserve the explicit truthy override or SHA256 of the source's eight-field ordered JSON object. Property/amount fields use Number coercion; nonfinite values stringify as null in this ID-only helper. Open itself requires positive finite property/amount. Fractional property IDs and arbitrary positive Number amounts remain source metadata; these are not accepted Ledger properties or exact atomic quantities. Address and reference values retain their bounded JSON representations.

The challenge-block helper preserves missing versus null/empty input, floor for valid nonnegative explicit input, and an un-floored environment fallback. Environment is a bounded explicit snapshot, not a process-global read. Timestamps are supplied safe integer milliseconds. Nonfinite stored fields reject before committing; source NaN/infinite storage behavior is outside this finite checkpoint domain. The source oracle records a NaN-created-block case separately and Rust explicitly rejects it atomically.

Source lifecycle quirks are deliberate observations:

- Reopening RELEASED replaces supplied fields through `$set`; stale omitted resolution/release fields survive in storage. The returned open document contains only that write's fields.
- Any non-RELEASED cache may be challenged, including after its deadline and after RESOLVED_UPHELD.
- Challenges append history; only the first positive challenge bond replaces the zero bond metadata.
- Finalization blocks CHALLENGED, RESOLVED_UPHELD and RELEASED, accepts the exact deadline, and checks recipient/amount/property against the record.
- Resolution accepts any non-RELEASED status. Rejecting a challenge may shorten the deadline; zero deadline has the source truthy-fallback behavior. Verdict lowercasing does not trim whitespace.
- The source registry never settles bonds. In upstream logic.js, repeated challenge bond locks can exceed the first stored bond, and normal payout does not return the cache bond. This metadata port does not manufacture those missing transfers.

Mutations use staged atomic metadata updates. This is a named storage repair: failed validation/capacity leaves records and journal unchanged. It does not reproduce partial external database writes or provide cross-process transactions. Calls remain repeatable according to source lifecycle rules; there is no invented transaction idempotency policy.

Each JSON document is limited to 64KiB, 4096 nodes and depth16. Total records are capped at 1024; journal entries at 4096; combined metadata at 8MiB and 65536 value nodes. Cache IDs are limited to 1024 bytes. Full checkpoints store the expected environment, canonical ordered complete journal and sorted complete records under a versioned header and CRC. Restore checks independently supplied environment, strict canonical bytes, full replay, reconstructed record equality and byte re-encoding. Recomputing CRC over fabricated record state does not bypass replay. CRC is not authentication; an entirely fabricated coherent history remains metadata.

The full-source VM oracle uses genuine Node crypto, detached `$set` database adapters, explicit environment and clock. Its 52 observations cover all methods, source error order, challenge/bond/reopen quirks and an injected single-write failure trace. Rust tests replay source values/rows, reject rechecksummed state fiction and wrong configuration, and check atomic/nonfinite/resource failures. `node scripts/bitvm_cache_vectors.cjs --check` reproduces the evidence; `--source-root` checks the pinned checkout. Adapter traces characterize source behavior and do not certify NeDB durability or concurrency.

Restore and mutation preflight aggregate JSON nodes before journal copies; metadata budgets reserve room for encoded keys, configuration and framing. Input ordering, binary64 values and source quirks are retained inside these explicit resource limits.
