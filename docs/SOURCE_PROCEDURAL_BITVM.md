# Shared source Procedural and BitVM lifecycle adapters

This V6-only include composes the complete exported methods of pinned
`procedural.js`, `bitvmCache.js` and `bitvmRisk.js` over the existing literal
contract Core. The source commit is
`dca0888b827c5bc28fa9189bc9e1d507f5c5670c`. Their archived SHA256 pins are,
respectively, `0a89e5447a07d592a25032d55ced478b01797cda96da055c888a0da6664f062e`,
`053d2c2518485a0dcc0f42a76b100a945e0c568f54bcfb04a5b8fa97b80ba0ca` and
`14e6f4edcb6590e043a46188a3ef122adcae88cbf18bc2853d64e59b8791ef66`.
The unchanged full modules, rather than selected function bodies, are executed
by the differential reference.

All persistent rows use the same Core `procedural` collection, storage clone
semantics, failure schedule and clock. There is no private metadata database,
cache or second balance owner. The existing standalone `ProceduralRegistry`,
`BitvmCacheRegistry` and `BitvmRiskBook` have narrower metadata domains and
private/atomic stores. Their stores and atomic commits are not used here. The
existing source Number, JSON, String and SHA256 helpers are reused; source
binary64 addition and eight-decimal Tally operations remain in their original
owners.

## Admitted interfaces

The original positional arguments are passed through `self.call`:

| Target | Methods | Transport |
| --- | --- | --- |
| `Procedural` | `_base`, `upsertTemplate`, `upsertContract`, `getContract`, `getTemplate`, `ensureIssuanceContext`, `ensureRedemptionContext`, `transitionContract` | async |
| `Procedural` | `_normalizeHash` | sync |
| `BitvmCache` | `_base`, `get`, `open`, `challenge`, `finalize`, `resolve` | async |
| `BitvmCache` | `_challengeBlocks`, `buildCacheId` | sync |
| `BitvmRisk` | `onCacheOpen`, `onEscrowRelease`, `onSweep` | async |

The two `_base` methods return the existing structural database capability.
It is not an imported plain object with execution authority. Nested exported
class calls retain their original order and arguments. Lexical Risk helpers
perform the actual Core factory/read/update calls without an invented exported
method. `Date.now()` reads the existing Core clock; it adds no alternative clock.

`SourceEnvironment.configure` is an explicit synchronous configuration command,
not a JavaScript source method. It replaces one bounded object of string values.
The allowed keys cover the Risk/cache and settlement/bundle variables below,
plus the original liquidity-reward configuration consumed by the producer:

```text
TL_BITVM_CHALLENGE_BLOCKS
TL_BITVM_SCHED_WINDOW_BLOCKS
TL_BITVM_MAX_PENDING_ESCROW
TL_BITVM_MAX_PENDING_ESCROW_PER_DLC
TL_BITVM_MAX_DEPOSIT_PER_WINDOW
TL_BITVM_MAX_WITHDRAW_PER_WINDOW
TL_BITVM_MAX_SWEEP_PER_WINDOW
TL_BITVM_REQUIRE_BUNDLE
TL_BITVM_BUNDLE_PATH
TL_ORACLE_REQUIRE_STATE_ROOT
TL_ORACLE_STATE_COMMIT_SCHEME
LIQUIDITY_REWARD_ADDRESS_CLEARLIST_ID
LIQUIDITY_REWARD_ISSUER_CLEARLIST_ID
LIQUIDITY_REWARD_PROPERTY_ALLOWLIST
LIQUIDITY_REWARD_MAX_SHARE
LIQUIDITY_REWARD_MIN_SHARE
LIQUIDITY_REWARD_PIVOT_LTC
LIQUIDITY_REWARD_SLOPE
```

The same allowlist also includes the seven original liquidity-reward environment names documented in `SOURCE_LIQUIDITY_REWARDS.md`.

Core owns `source_proof_environment`, initially `{}`. The coordinated V6 mount
binds it under `proofLifecycle.environment`; replay reconstructs it from the
original configuration command. It is never read from ambient Rust process
environment. V1–V5 expose neither this configuration route nor the new methods
or diagnostic field. No old checkpoint is reinterpreted as V6.

## Literal effects and failures

Template/contract upserts retain source object spread, arbitrary state strings,
metadata overrides and field ordering. A transition performs factory, read,
merge, uppercase-state conversion and write in the original order. It does not
invent state-machine restrictions or verify a DLC. Issuance/redemption context
helpers check stored metadata only; successful context results do not prove
settlement or authorize minting.

Cache ID hashing uses the exact source JSON property order and coercions. Cache
open, challenge, resolve and finalize retain source defaults, falsey fallbacks,
status checks, challenge-window arithmetic, bond metadata and replacement
behavior. A falsey recipient override in finalize refers to the same fetched
recipient field; it is not compared as a newly created object. Reopening a
released cache preserves the source `$set` behavior, including fields not
removed by the new document. A rejected challenge shortens its deadline with
source `Math.min`; it does not clear challenge or bond history. NaN deadlines
and source falsey/default defects remain observable.

Risk functions first read all needed global/DLC/window documents, check limits,
then write them sequentially. A failed second write retains the first write.
Retrying can therefore increment counters unevenly. The existing atomic Risk
book is deliberately not used. Caps, schedule windows, negative/nonfinite
amounts and `Number` coercions follow the actual source. These counters describe
source metadata, not conserved custody or proof verification.

This include performs no Tally transfer. Cache principal/bond movement, reward,
refund and slash ordering belong to unchanged Logic orchestration through the
existing sole Tally authority. It supplies no financial result to that caller.
Ordinary source exceptions retain earlier writes; bounded/local errors restore
the composing runtime according to its established admission policy. Local
capacity/representation refusals are never labeled source errors or successful
partial settlements.

The supported domain is bounded SourceValue metadata with the existing Core
storage adapter. Functions, accessors, arbitrary prototypes, host objects,
custom database replacement and unrepresentable UTF-16 string properties are
outside that domain. Exported database capabilities are structural handles.
The tests make no claim of arbitrary NeDB equivalence, filesystem durability,
authenticated chain/sender authority, or BitVM/DLC proof acceptance.

## Evidence and mounting

The genuine reference contains 127 observations: thirteen resets, one explicit
environment reconfiguration and 113 original source calls. It loads all three
unchanged modules with original receiver and return/Promise behavior. Both
archived BigNumber9.3.1 and verified genuine9.1.2 produce identical captures.
No validation, context, financial or proof-verifier success is supplied.

Two focused tests compare original return/error transport, all shared database
rows, environment values and ordered factory/read/write/class-call effects.
They exercise template/context branches, source spreads/overrides, next-contract
funding shape, cache bond/challenge/uphold/reject/release/reopen, signature-free
metadata hashing, falsey and NaN behavior, cap failures, ten injected DB failure
prefixes/retries, exact replay after those prefixes and old-policy refusal.
The composing Logic corpus separately exercises actual type30 autoRoll and
financial bond/cache flows. These tests alone do not claim that composition.

Both tests pass against the coordinated full V6 runtime library. Isolated test
Clippy with `-D warnings` and pinned Rust1.90 rustfmt checks pass. No separate
whole-library build was launched by this adapter lane. Both genuine reference
captures have SHA256
`f407bb8e111b3ca185760bc8fc779d543fc7b2f16d51e49a7f65a99443b475c1`.

The V6 runtime binds these adapters to its existing Core fields, collection and failure schedule. Pinned full-library validation is recorded in `fixtures/reference_fidelity/source-proof-candidate.json`; no independent metadata store is mounted.

Pinned integration validation: 1,449 Rust test executions across208 targets, 84 source-lock tests across31 targets, formatting, Clippy, doctests, release and Python policy/bridge checks pass. Exact scope, corpus counts and input hashes are recorded in `fixtures/reference_fidelity/source-proof-candidate.json`; these checks do not establish complete reference parity or authenticated block execution.
