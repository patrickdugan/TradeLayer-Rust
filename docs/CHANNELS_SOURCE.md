# Complete source Channels metadata

`channels_source` executes every active callable member of the pinned
`channels.js` over bounded source values and explicit dependency capabilities.
It owns the source registry and withdrawal queue, preserves partial source
effects, and exposes complete immutable diagnostics. It creates no token Ledger,
signed channel revision, funding proof, authenticated actor or settlement authority.

The unchanged Git blob is `tradelayer.js@dca0888b827c5bc28fa9189bc9e1d507f5c5670c:
src/channels.js`: 50,151 LF bytes, SHA256
`b3fa22beab9a9fdb63246daa12582f86591a4217863240321f88fdff0160ec79`.
The integration base is `6403716f4ee77682430d75bba1c22c08a7c85d1b`.
The actual class has **43 static methods**, an implicit empty constructor and no
instance methods. Both static fields, `channelsRegistry` and
`pendingWithdrawals`, are represented. The existing `legacy_channel_audit`
remains a narrower source-accounting audit; no independently authenticated
capability is derived from it or from this module.

## API and transaction boundary

```text
SourceChannels::new() -> SourceChannels
SourceChannels::from_state_unverified(ChannelsState) -> Result<SourceChannels, SourceRuleError>
SourceChannels::state() -> &ChannelsState
SourceChannels::action_count() -> usize
SourceChannels::execute_unverified(&SourceCall, &mut impl SourceBackend)
    -> Result<ChannelsEvaluation, SourceRuleError>
ChannelsEvaluation::diagnostic_value(&ChannelsState) -> SourceValue
ChannelsEvaluation::require_authenticated_admission() -> Err(UnauthenticatedInputAndState)
```

`ChannelsState` contains an insertion-ordered registry of primitive Map keys and
source values, plus the ordered pending-withdrawal array. Map updates preserve
first insertion order, use SameValueZero for primitive keys and overwrite a
duplicate loaded key with its last value. A supplied state is unverified metadata.
It is not a trusted checkpoint or a replacement for actual Ledger restoration.

`ChannelsEvaluation` retains the exact returned value or source exception,
arguments after mutation, ordered calls/results with awaitedness, deferred
rejections and swallowed load errors. `diagnostic_value` adds both current static
fields in a fixed schema. Numeric values retain binary64 bits, including signed
zero, NaN and infinity. Source argument/property order is preserved, with numeric
object keys in JavaScript's index order.

Source success **and source exceptions** publish the source's partial local
metadata and consume one action. For example, pubkey assignment or a channel
debit can precede a later failed database/tally call. A local unsupported-domain,
capacity or unavailable-backend error discards the staged local state and does
not consume an action. That is a named local safety repair; it cannot undo an
external capability's earlier writes. There is no distributed atomicity or live
settlement implication. Backends are non-reentrant explicit capability adapters.

## Complete method inventory

| Member | Executed source behavior and limits |
| --- | --- |
| implicit constructor | Returns an empty instance observation; static maps are shared by the source class, not reset by construction. |
| `addToRegistry` | Stores the source's misspelled `commiterA`/`commiterB` object, then saves all registry rows. |
| `removeFromRegistry` | Deletes only the memory row, then upserts surviving rows; it does not delete the old database document. |
| `setChannel` | Stores arbitrary represented metadata, then saves the registry. |
| `saveChannelsRegistry` | Calls the channels DB factory, snapshots entries and sequentially upserts each `{_id,data}`. |
| `getChannelBalancesForAddress` | Actual trim/lowercase/query, optional Number filter, participant/latest-commit/last-column fallback, enriched positive finite rows and stable descending amount sort. |
| `sumBalance` | Source `reduce` and JS `+`, including string concatenation and falsy-amount fallback. |
| `toChannelPropMap` | Nested property map accumulation with source `+` and numeric-key ordering. |
| `loadChannelsRegistry` | Replaces the map from DB rows; catches only a read error whose message contains `does not exist`. Factory failures occur outside the catch. |
| `savePendingWithdrawalToDB` | Upserts `withdrawal-${blockHeight}-${senderAddress}`, preserving same-key collision behavior. |
| `loadPendingWithdrawalsFromDB` | Uses the actual anchored withdrawal regex and returns ordered `entry.data` values. |
| `removePendingWithdrawalFromDB` | Removes the same height/sender key. |
| `recordChannelDelta` | Destructured defaults, UUID before DB factory, clock after factory, exact delta order and insertion; failure rethrows. |
| `recordTokenTrade` | Builds the offered/desired token key and full trade document. |
| `recordContractTrade` | Builds the contract key and full trade document. |
| `saveTrade` | Source spread can overwrite the constructed document `_id`; update selector still uses the computed key. |
| `getChannel` | Returns a truthy cached row, otherwise reloads; absent/falsy result becomes null. |
| `isValidChannel` | Returns false when absent; an existing channel returns undefined, not true. |
| `getCommitAddresses` | Reloads when needed and returns actual A/B fields or two nulls. |
| `addCommitment` | Throws on the genuinely uninitialized `this.db.updateAsync`; no invented commitment store. |
| `getCommitments` | Throws on the genuinely uninitialized `this.db.findOneAsync`. |
| `compareCharacters` | Strict equality, Number/isNaN grouping and JS string/numeric comparison. |
| `recordParticipantChange` | Exact default/derived memo and non-token participant delta. |
| `debitInitMarginFromChannel` | Resolves actual participant or null column, initializes missing/nonnumber balance, BigNumber check/debit, registry save then delta. Negative debits and source malformed-column effects remain observations. |
| `assignColumnBasedOnAddress` | Initial/open slot, preserved participant and crowded tie/bump paths, including participant deltas and partial effects. |
| `resolveColumn` | Exact existing participant lookup, falsy-address null and source redundant fallback semantics. |
| `assignColumnBasedOnLastCharacter` | Source parity alphabet, default parameter and UTF-16 indexing; invalid index throws through undefined `.toUpperCase`. |
| `tieBreakerByBackChar` | Parity scan of the shortest address and source default winner. A nonfunction fourth argument throws only when the loop actually invokes it; empty scans return the default winner. Scans beyond the local 4,096-unit work bound fail explicitly. Actual injected functions are outside the value interface. |
| `predictColumnForAddress` | Missing/empty/existing/open/crowded paths, retaining the source winner-column prediction. |
| `bumpColumnAssignment` | Force normalization, equal-owner repair, exact/one-sided swaps or overwrite, balances and participant-change deltas; other column metadata does not magically follow. |
| `processChannelCommits` | Calls the real `.size===2` test; its true path throws because `assignColumns` is undefined. |
| `handleChannelPubkey` | First transaction input, address-type and pubkey capabilities; column overwrite, both key orders and exact multisig equality. This is source behavior over supplied capability results, not signature verification. |
| `recordCommitToChannel` | Full channel initialization, prediction/pubkey check, assignment, rounded balance, delta, flags/lists, commit history and registry persistence. |
| `areBothPartiesCommitted` | Reads `participants.size`; the ordinary source object has no size and therefore returns false. |
| `addToWithdrawalQueue` | Source loose boolean column conversion, memory append and DB upsert; no token reservation. |
| `processWithdrawals` | Lazy load, actual seven-block/coercion comparison, current participant recomputation, withdraw-all/partial flow, queue removal and final registry save. Missing channels are removed before a possible later null dereference. |
| `removeEmptyChannels` | Strict numeric-zero scan, memory deletion and source missing argument to DB removal. |
| `isChannelEmpty` | Missing/malformed participants count as empty; string `"0"` is not strict numeric zero. |
| `removeChannelFromDB` | Factory and exact stringified `_id` removal. |
| `adjustChannelBalances` | Source raw `+=` before its too-late missing-channel guard; no invented balance checks. |
| `processWithdrawal` | Actual supplied reserve capability/shortfall, Number debit, external channel/tally updates and registry publication; represented direct-argument versus cached-registry aliases remain distinct. Internal queued withdrawals reject a cached key differing from `data.channel`, which would create an unrepresented shared-reference graph. |
| `processWithdrawAll` | Source ordered `Object.entries` and parseInt property IDs; performs each withdrawal sequentially. |
| `processTransfer` | Source OR clearlist admission and all four transfer branches, including the two broken B branches and same-channel alias behavior. |
| `updateChannelWithColumnAssignments` | Throws because the bare source class never initializes `this.channels`; no invented registry alias. |

The two uninitialized fields above remain absent. Arbitrary monkeypatches adding
`this.db`, `this.channels` or a new `assignColumns` function are not the pinned
module's lifecycle and are outside this finite source-value interface.

## Explicit dependency contracts

The shared `SourceBackend` returns `Immediate`, `Pending`, `Throw` or
`PendingThrow`. Awaited calls settle those outcomes; nonawaited pending calls
remain opaque promise observations and record deferred rejection. `Err` is an
unavailable/local-domain failure and is not fabricated into a source false result.

| Target | Ordered arguments |
| --- | --- |
| `DB.getDatabase` | category |
| `DB.findAsync` | category, query |
| `DB.updateAsync` | category, selector, update, options |
| `DB.insertAsync` | category, document |
| `DB.removeAsync` | category, selector |
| `UUID.v4` | none; synchronous source call |
| `Clock.now` | none; synchronous source call |
| `TxUtils.getRawTransaction` | commit txid |
| `TxUtils.getAddressTypeUniversal` | sender; synchronous source call |
| `TxUtils.extractPubkeyByType` | actual first input, address type |
| `TxUtils.createMultisig` | keyA, keyB, chain, test, sender; both orders called |
| `Vesting.getInstance` | none |
| `Vesting.getChain`, `Vesting.getTest` | none; source does not await these |
| `Tally.hasSufficientReserve` | channel, property, requested amount |
| `Tally.updateChannelBalance` | channel, property, signed amount, source type label, block |
| `Tally.updateBalance` | sender, property, amount,0,0,0,source type label,block |
| `ClearList.isAddressInClearlistOrDerived` | list ID, actual selected participant |

The regex query's capability-boundary representation is
`{$regexp:"^withdrawal-",$flags:""}` inside the actual `$regex` field. It denotes
the exact source RegExp; it is not a general Rust regex engine. The Node oracle
uses the real RegExp and a clearly bounded in-memory query/upsert adapter. That
adapter is not NeDB, Core RPC, native issuance or an authenticated external store.

## Source defects and separate conserving policy

The pinned class can resurrect removed registry entries after reload, overwrite
historical withdrawal requests sharing height/sender, misreport displaced
committers through fallback columns, and retain pubkeys/flags/lists on their old
columns after owner/balance rotation. Type B transfers can debit the wrong side
or replace a whole destination balance object with a string. Negative commits,
raw adjustment, overdraft and partial external failures are faithfully metadata
observations. The source does not reserve a withdrawal when it is enqueued,
require both clearlist participants, verify signatures on a state allocation,
define canonical revision sequence bytes or revoke old L1 commitments.

These behaviors must never be promoted into live custody merely because the
oracle matches. The actual Ledger still owns channel escrow and conservation;
the separately named `SupersedingCoSignedAllocationV1` lane must enforce its
own funded policy, entry-point fences, canonical signatures, checkpoint and
rollback within that owner. Scaling transfer plans and source `valid` results
remain nonauthoritative. No second spendable ledger is introduced here.

## Bounds and unsupported domains

The owner permits 4,096 completed actions, 1,024 channel rows, 16,384 composed
logical state nodes and 8 MiB measured state/input storage. Inputs have at most
16 arguments; each shared value additionally enforces 24 levels, 8,192 nodes,
1 MiB text and its own array/object entry bound. Dependency traces allow 4,096
calls, 65,536 composed nodes and 16 MiB measured storage, with exception names
up to 256 bytes and messages up to 65,536. Borrowed aggregate checks run before
owner/input cloning; returned capability values are checked before retained
copies. Decimal arithmetic has the existing explicit 4,096-digit/scale limits.
String iteration and `Object.entries` measure the derived code-point/code-unit
count before collecting and permit at most 16,384 elements; a tie scan permits at
most 4,096 units of the shorter input. Larger derived collections or scans return
an explicit capacity error before their allocation/work. These are local resource
limits, not historical protocol rules or compaction.

Sparse array extension is a local domain error before mutation: an indexed write
may replace an existing element or append contiguously, but may not create
holes. `Object.entries` distinguishes holes from owned undefined values. The
oracle retains a genuine adjust-at-index3 then withdraw-all continuation with
only property3 visited, using an explicit `sparse-array` own-index diagnostic.

Primitive Map keys are supported, including signed-zero/NaN SameValueZero.
As in a JavaScript Map, stored or imported numeric-zero keys are canonicalized
to positive zero; input argument bits remain unchanged.
Reference-identity Map keys, cross-argument alias graphs, cycles, actual callable arguments, arbitrary class
monkeypatching, prototype-changing writes and coercion overrides remain explicit
unsupported domains. Arrays with named own properties cannot be represented by
the shared `SourceValue::Array`; attempts to write them return a local domain
error and preserve local state. A UTF-16 indexing operation producing an isolated
surrogate is likewise explicit, even when its original string is valid UTF-8.

Address identity operations require primitive values; object-valued senders or
participants would require a full JavaScript object-identity graph and are
rejected locally at those operations rather than compared as structural copies.

`toChannelPropMap` rejects dynamic inherited Object-prototype names as channel
or property keys, including `constructor`, `__proto__` and the standard Object
methods. The source can mutate inherited functions/prototypes through those
accesses; this owner does not invent ordinary own balances in their place.
An inherited Object-prototype read through any dynamic column/property boundary
also returns a local domain error; it never invents an undefined field and then
reports a false source TypeError. Represented own data fields remain supported.
BigNumber amounts with own `toString`/`valueOf` overrides are also a local domain
error, checked before external calls. Queued withdrawals whose cached registry
key differs from `data.channel` are rejected at the internal alias boundary;
both the successful shared-alias creation and partially failed source debit are
retained as observations rather than modeled as independent copied rows.

BigNumber negative-zero objects have different `valueOf` and string-hint
coercions. The oracle preserves their sign with the diagnostic text `-0`, and
the Rust boundary rejects that object state before cloning or dependency calls.
Ordinary binary64 negative zero and positive-zero BigNumber objects remain
supported; the source distinctions are not flattened into one text conversion.

The corpus retains 46 genuine source cases as `Q` observations: seven
astral/array-property/reference-key/coercion/object-identity cases, 24
inherited channel/property-key cases, two BigNumber own-coercion cases, one
astral `Object.entries` case, two internal withdrawal-alias cases and six
inherited dynamic-column adjustment/withdrawal cases and two negative-zero
BigNumber coercion-hint cases, one sparse-extension/withdrawal continuation
and one malformed commit whose array balance container receives a sparse write.
BMP string
entries are supported exactly. The oracle records unsupported
named array fields with its explicit `array-properties` diagnostic tag. They are
not counted as Rust/source matches. The test parser drops those sidecars only
while obtaining dependency prefixes for a **required local rejection**; it never
accepts or compares that value as supported state.

## Differential evidence and reproduction

The full unchanged VM corpus has **480 scenes, 445 exact action observations
and 46 explicit-domain observations**. It covers every static method and
implicit constructor, missing arguments, successful lifecycle chains, stale DB
rows, scalar and malformed containers, negative/zero/NaN/infinite BigNumber
results, fractional rounding/signed zero, prefixed numeric strings, 64 seeded
finite binary64 patterns, withdrawal delay/expiry/coercion and alias behavior,
all transfer directions, clearlist OR semantics, and synchronous/asynchronous
failure injection before and after mutations. Full original LF source and
genuine immutable BigNumber9.3.1 are executed; no source method is replaced.
Review regressions include a distinguishing tie at unit 1,025, zero-iteration
invalid callbacks, negative-zero Map keys, numeric array writeback, mixed-case
and fractional BigNumber prefixes, BMP entries, string read-only writes and
plain-array destination columns through JavaScript abstract equality, and
distinct BigNumber/promise arguments whose strict identity is never inferred
from structural equality.

BigNumber reference SHA256:
`58ba6fb59668342346f17983a61ace455f6f425ac4fbfab84f489df257594d16`.
The only mocks are explicit external module capabilities, deterministic UUID and
clock, and bounded in-memory DB behavior. Signatures, chain inclusion, NeDB
parity and funded source effects are not inferred from them.

`S` rows contain a fresh unverified source state. `A` rows contain method,
arguments, explicit overrides and the complete source observation. `Q` rows
retain genuine observations requiring a declared typed rejection. `M` is the
43-method inventory and `C` contains aggregate scene/exact counts. The shared
tagged value schema preserves order and exact binary64 bits. The test-only
tag parser handles the diagnostic wrappers directly instead of broadening the
production JSON parser's depth limits.

```text
node work/full-channels-port/scripts/channels_vectors.cjs --repo outputs/TradeLayer-Rust --check
```

After parent integration, the same script defaults to its repository root:

```text
node scripts/channels_vectors.cjs --check
cargo test --frozen --test channels_source
```

Scratch-only checks use the actual already-built core plus this owned module,
without Cargo or a full-core rebuild. Parent owns final integration, complete
format/Clippy/test/release validation and commits. Full historical acceptance,
actual external capability owners and authenticated admission remain open.

The current standalone check passes all six owned tests, including the complete
445+46 corpus, borrowed composed and derived-collection/work limits, hostile
backend-value rejection, action budget and permanent authority denial. Direct Rust1.90 Clippy with warnings
denied passes for both owned production and test modules. The Node `--check`
reexecutes the complete unchanged source and genuine BigNumber reference. These
checks do not constitute the parent's pending full-repository validation.

## Integrated repository validation

The parent integrated this code and ran the actual pinned Rust1.90 repository:
1,199 top-level tests plus one subprocess check pass across148 executable
targets. Formatting (including included implementation files), all-target frozen
Clippy with warnings denied, all11 supply-chain mutation tests and the all-target
frozen release build pass. The final run held553 implementation/check inputs
unchanged. The source corpus has445 exact observations and46 explicit domain
cases; new-policy references cover81 transcript vectors and12 real-bootstrap
signed envelopes. See [ROADMAP_PROGRESS.md](ROADMAP_PROGRESS.md) for scope and
remaining gates. These checks grant no L1 admission or revocation authority.
