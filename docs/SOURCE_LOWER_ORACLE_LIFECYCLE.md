# Literal lower Oracle lifecycle (staged V7)

This work-only stage adds the pinned JavaScript Oracle creation/admin/close
lifecycle and Logic wrappers to the same `SourceContractRuntime` Oracle cache,
watermark map, `oracleList`/`oracleData` collections, Tally and Insurance owner.
It creates no second registry, spendable balance store or supplied validation
result. Source pin: `dca0888b827c5bc28fa9189bc9e1d507f5c5670c`.

The proposed `LowerLifecycleV7` policy is explicit opt-in. Its work-only transform
preserves the default V2 and every V1–V6 method gate, state shape and bootstrap;
V7 inherits the earlier graph through the existing capability helpers. V7 uses
`TLCR\x07` and `TradeLayer/SourceContractBootstrap/v7/`. Parent owns final mounting,
version pins and the coherent repository commit. No repository file is changed by
this stage or its preview script.

## Exact source paths

New actual Oracle methods are synchronous `getInstance`, and asynchronous
`getNextId`, `createOracle`, `isAdmin`, `updateAdmin`, `closeOracle`. The Logic
methods `createOracle`, `publishOracleData`, `closeOracle`, `updateAdmin` preserve
source arity and target precedence. Existing Oracle load/info/price/publication
and Insurance/Tally implementations stay the shared authorities.

The original Types decoder and Validity implementation execute for raw13/14/8;
raw15 is also admitted to its genuine failure. Successful raw calls invoke the
literal Logic arguments and discard the Logic return as the original typeSwitch
does. These details deliberately remain visible:

- Decoder13 produces `whitelists`, while Logic.typeSwitch supplies absent
  `clearlists`; a raw-created Oracle therefore stores `clearlists: []`.
- Raw14 scales price by 1e4; optional high/low/close fields use their original
  unscaled base36 integers. Validation, admin reads and publication circuit
  behavior execute in their original order.
- Raw15 decodes a primitive but invokes the wrong publication validator. It
  rejects with the real `TypeError` at `'high' in <number>` before any close.
- Admin checks read the database, including nested legacy `name.adminAddress`
  and `name.backupAddress`. Undefined sender can equal absent admin fields.
  The actual local Base58 decoder double-pads Bitcoin leading `1`, so ordinary
  raw8 success uses a real Litecoin L address rather than a fabricated RPC answer.
- Logic.updateAdmin selects whitelist, then token, then oracle. Its token branch
  references unbound `PropertyList`, preserving that `ReferenceError`; the new
  implementation never redirects it to PropertyManager.
- Creation initializes the singleton before reading IDs. ID selection uses
  finite Number values from documents, then falls back to stale cached keys only
  when no positive document ID exists. Fractional IDs, parseInt truncation and a
  malformed cached key's `split` TypeError remain literal.
- Create inserts before caching. Admin/close read and write before caching their
  changed document. Storage failures preserve the exact initialized/cache/write
  prefix rather than applying a blanket transaction.
- Close sets `closed` in the database and cache before calling
  `Insurance.liquidate(oracle.adminAddress, true)`: true occupies contractId,
  and isOracle is omitted. The real Insurance implementation may swallow its own
  errors. A closed feed remains publishable because source publication does not
  test `closed`.
- Publication retains its original height watermark before price reads/writes.
  A failed publication followed by the same-height retry can therefore succeed
  as a no-op without inventing a publication row or clearing the watermark.

The source singleton `Map` is projected as ordered pairs in SourceValue, using
the existing Core representation. `getInstance` returns the real cache view;
there is no independently mutable supplied instance. Caller-side JS object
reference mutation, getters/functions/Proxy/Symbol capabilities are outside this
bounded SourceValue transport. Imported rows remain explicit input, never method
outcomes. Oversized domains are local rejections and command rollback, separately
from genuine source exceptions retaining their prior effects.

The whitelist branch of updateAdmin still reaches an uncomposed
`ClearList.updateAdmin`; this produces a local `UnknownDependency`, not a claimed
source throw. Invalid address RPC/Bech32 fallbacks need actual external inputs and
are not replaced by successful verdicts. This stage does not compose raw0–7,
9–12 or a repaired functional raw15 close. Lower Property1/11/12 is a separate
coordinated stage.

## Genuine evidence and current checks

`execution.cjs` loads the full unchanged pinned CommonJS business modules using
the existing genuine composition harness. The harness supplies deterministic
storage/RPC clock/activation inputs; it never supplies validator, financial,
Oracle, Insurance or matching outcomes. Source wrappers record calls and return
exact original values/Promises. The observer snapshots returned projections at call completion so later singleton
mutations cannot retroactively rewrite earlier receipts. The tests compare all collection rows, actual
Oracle/Tally/Property/Insurance caches and flags, raw parameters and complete
ordered DB effects plus the observed Logic/Oracle/Tally/Insurance calls. Internal
singleton/load/price wrappers are excluded from the call projection because the
existing composed owner represents these as direct private calls; their complete
DB effects and resulting cache state are still compared.

First ten executed observations (reset plus nine calls) passed both genuine
archived BigNumber 9.3.1 and lock-pinned 9.1.2 before implementation. The expanded
corpus has **212 observations: 32 resets, 12 explicit row imports, 88 raw calls,
80 direct source calls**. One composite actor call is an explicit local identity
refusal in Rust (the unchanged source returns false); the other 211 observations
pass strict comparisons. All 212 genuine source observations are byte-identical between
those two dependencies; this establishes tested corpus equality, not whole
version equivalence. Final 212-observation JSONL SHA256 is
`b7c73f74da0d1b2c4f5b19a6d0dfbfadc3319f23b18ba2842fd6f734a3b69bce`.
Each full unchanged-module run uses the pinned module body
hashes from the original harness. The lock dependency source is independently
verified at SHA256
`20db2d8d32fd388a927588beb9c1cdbb2c91ddf0273a180161334b28cb00902e`.

Coverage includes four import orders, duplicate/empty creation, optional prices,
inactive/outsider/backup validation, nested legacy records, malformed/fractional
IDs, stale cache fallback, all create/admin/close storage boundaries, poisoned
publication retry, and close's real Insurance/Tally graph with imported snapshots. The boolean contractId returns the genuine invalid-property Error value from Tally; it does not create a fictitious payout balance. A scheduled Tally save failure in that branch remains unconsumed because the source never reaches the save.
Composite/BigNumber/Promise actors in isAdmin are explicitly refused locally: a
by-value SourceValue cannot supply JavaScript reference identity, including
imported rows. Newly inserted BigNumber admin fields already pass through the
existing genuine structuredClone storage boundary; no global DB change is made.

The three staged Rust tests cover whole-source comparison, 24 complete raw/source
failure sequences with per-command replay/continuation, and explicit earlier
policy refusal plus local admission rollback. All three tests passed on the actual combined Core using pinned Rust 1.90 MSVC,
including both genuine BigNumber versions inside the differential test and all
24 per-command replay/continuation sequences. The parent-coordinated focused
batch passed 21 tests across 7 targets, including inherited reward behavior and
separate independent V6 preservation/version-promotion regressions. No worker
whole-library build or repository edits were performed for this stage.

The parent supplied an independently compiled actual Rust 1.90 V6 release fixture
from commit `6ed0bff1969a8d8952d26653765ce7ad02bd5033`: 27 commands, 34,379 bytes,
SHA256 `a98e40b3ca747b852b7979fa8b15bec9188a00169861b16ef066782f646a07ad`,
external head `e8b616851f86a69ba178b363ef2d2d3d81b98b96303769ecc88a2a890a928b0c`.
Its separate parent-owned tests preserve byte-exact V6 replay/continuation and
reject repaired-checksum V6/V7 magic/bootstrap substitutions.

`prepare.py` only writes a fresh work-directory preview with hashed inputs and
payload manifest. It has no apply option and starts no compiler. The pure
`prepare_policy`, `prepare_runtime` (Oracle-only), and `prepare_replay` hooks let
the parent compose the independently owned lower Property fragment (`--with-property`) without
rewriting shared state. V6 files and the repository remain frozen during staging.
