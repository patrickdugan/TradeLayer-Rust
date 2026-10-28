# V9 Main and spot source composition

`SpotLifecycleV9` owns raw5/6 and 34 Main transaction kinds. Native activation0
and AMM10 remain separately staged policies. The default `GlobalClearingV2`
is unchanged. Literal source errors, invalidity, partial writes and ignored
Promises remain source outcomes.

Fresh Root differential checks pass for the standalone and Main spot corpora,
all 109 available upper Main calls, two isolated capability refusals, 68 transfer
continuation calls, 20 threshold/fallback calls and 12 withdrawal calls. V8's
independently compiled bytes remain exact. Full release verification is recorded
separately from these focused results.

The additional continuations are described in `SOURCE_MAIN_UPPER_INTEGRATION.md`,
`SOURCE_MAIN_TRANSFER_CONTINUATIONS.md`, `SOURCE_MAIN_TOKEN_THRESHOLD_FALLBACKS.md`
and `SOURCE_MAIN_WITHDRAWAL_SCHEDULE.md`. Existing TLSW1/TLSW2 remain unchanged;
separate TLSW3 covers selected V7/V8 source slices and refuses V9.

This is a new revision over the frozen 64-frame standalone spot snapshot and
27-frame genuine Main+spot corpus. Neither frozen source-evidence directory is
modified. Main starts from the separately owned 14-kind lower-route include
SHA-256 `56aa06843c4f2113f9873292e98c85c29dee6a76cbc374cd5f794b4a1655818b`.
The revision adds only V9 admission for raw5/6 and a nonempty fee buffer, leaving
the older whitelist and nonempty reward-queue boundary unchanged.

## Source scheduling over one owner

V9 Main moves the existing Runtime queue arena into Core for the duration of a
Main call. Nested dependency calls borrow that same arena; queue evaluation
moves it back through the existing queue evaluator and returns it afterward.
These moves introduce no second ledger, collection cache or metadata authority.
All spendable balances remain in the existing Core Tally collection/cache.

The original raw6 validator calls the async address lookup without awaiting it.
For V9 Main only, a typed continuation records its original receiver, address,
book key, factory outcome and trace slots. It starts the genuine queue DB
factory and resumes the actual read after the next awaited factory. Explicit
queue fragment APIs split factory/read and factory/save using the original
load parser and update machinery. Original queue evaluators keep their old
factory/body graph; eager standalone spot calls preserve the frozen evidence.

The observed Main sequence is initial load factory/find, address-lookup factory,
Logic's load/save factory, address-lookup find, then Logic's read/update. A read
that rejects is caught by the original getter semantics and returns an empty
indexed array. Source cache aliases and other failure prefixes remain visible.
The continuation reads the same arena and updates the same receiver cache; it
does not accept a supplied lookup or validity result.

V9 diagnostics bind the existing module entry, Main activity, pending read job,
schedule version and actual fee-Map initialization. The old policies omit this
diagnostic, and all new callbacks are V9 gated. V9 uses its existing distinct
`TLCR09`/bootstrap domain; journal replay recomputes these diagnostic states.

## Real fee failure

The real historical enriched input scene creates a fee buffer for block 10.
A processed-only retry leaves it untouched. New work in that same block starts
the genuine flush and preserves `ContractRegistry.isNativeContract is not a
function` from the captured CommonJS export. Existing fee code already models
that module-entry failure. No accrual success, fee rows or post-state results
are substituted. The diagnostic fee-initialization flag records the source Map
property's existence without storing any balances separately.

## Evidence and checks

The copied standalone fixture remains 64 frames, SHA-256
`05ca901528b286368d2b1c40db40ed0107eb92015907d7f338d32190979fae3b`;
its original 11-frame proof is unchanged. The copied Main fixture remains 27
frames/21 actual calls, SHA-256
`9ecf7c8876f33f8c8607799f4be0d03a1cb5917658d15942f6523afea88b801b`.
Both genuine BigNumber versions execute the unchanged source. The single
unavailable Main RPC experiment is isolated and locally refused in Rust parity.

Staged tests retain the three standalone tests and add full Main source outcome,
complete ordered DB/RPC/phase effects, all receiver/cache states, fee buffer,
ClearList/Tally-instance snapshots and complete checkpoint reconstruction.
A separate test checks that V8 Main still refuses raw5/6 and has no V9 job
diagnostic. The independent V7 binary and V8 ClearList files remain untouched.

The work-stage syntax checks were followed by fresh Rust 1.90 library builds
and differential tests. Receiver observation and asynchronous continuations
were corrected from actual failing evidence. Original source bodies and
financial results stayed unchanged. CI checks both numeric constructors,
first-ten and complete corpora, full receiver observation, old-policy
compatibility and independent witness command pins.

General JS object identity, arbitrary non-index array properties, nontransitive
V8 sorting and unrestricted event-loop equivalence remain outside the admitted
bounded graph. Bitcoin authentication, production startup/RPC and compact-index
adapter equivalence remain separate gates. No push or publication is performed.
