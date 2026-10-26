# Sequential source consensus orchestration

The opt-in `ConservingSequentialV1` runner evaluates the pinned Types dispatcher
and actual source validity rules against the pending `EconomicState`, maps only
representable commands, and applies them to its existing sole Ledger owner.
Rejected candidates remain in the same durable checkpoint as accepted commands.
The runner provides no authenticated L1 admission capability.

The source is `tradelayer.js@dca0888b827c5bc28fa9189bc9e1d507f5c5670c`.
`legacy_consensus_source.txt` is the unchanged 25,057-byte Git blob, SHA256
`af4c2fe7c37680be018dcb0df9f2cf8b348baac8aec1fe2335a220ce27dc0230`.
The existing unchanged `legacy_main_source.txt` has SHA256
`006d59ba3d28d802b0e0c5be2d9d661b58caf1c919a704b3f0d40bc9613c4009`.

## Actual owner lifecycle

`EconomicConfig.source_consensus = Some(SourceConsensusConfig { ... })` binds
the policy and an independently expected pristine Joint bootstrap commitment.
The commitment is SHA256d of
`TradeLayer/SourceConsensusBootstrap/v1/ || canonical_TLJE`.
The constructor accepts the existing bootstrap Ledger; it does not mint native
properties, infer a historical balance snapshot, or authenticate its issuer.

`EconomicChain::apply_source_unverified_block` accepts an
`UnverifiedSourceBlock { network, header, mode, transactions }`. Transactions
retain ascending asserted source positions, the raw prefixed payload, exact
source marker/sender/reference fields and source amount arguments. These are
caller claims. In particular, `reference_amount` is the already scaled argument
passed to Types, not evidence of a signed previous output or an L1 payment.

The private `EconomicBlockStage` begins once and retains the previous finalized
height while commands are pending. Each candidate follows this order:

1. Check the replay set derived from every prior processed candidate, including
   invalid/rejected candidates. Repeated IDs retain an `AlreadyProcessed` audit
   row, without decoding or authorizing a second command.
2. Derive the original nine Types arguments from the raw record and height.
   Run actual Types and validity with `EconomicSourceBackend` borrowed from
   the current pending owner. A prior accepted create-oracle therefore affects
   the next publish-oracle validation in the same block.
3. Require a successful genuine validity result and run the strict mapper.
   Unsupported metadata, missing dependency reads, deferred source rejection,
   fractional atomic amounts and unrepresented operations cannot become
   successful economic commands.
4. Preflight the borrowed cumulative typed inputs. Stage a candidate copy and
   publish it into the pending block only after the actual handler succeeds.
   A failed create, cancellation or multi-leg transfer leaves no partial IDs,
   allocated registry counters, custody, orderbook or fee state.
5. Record the position, ID, exact outcome class, bounded reason and evaluation
   commitment. The commitment includes Types diagnostics, real dependency call
   arguments/results, deferred errors and source global-write diagnostics.

The end phase executes the existing order queue, mark, funding, position/AMM
refresh, maintenance, liquidation and Joint finalization exactly once. A late
end-phase error rejects the entire proposed block and all proposed audit rows.
It does not invent a per-transaction success after a failed settlement.

`DurableEconomicChain::apply_source_unverified_block(expected_head, block)`
uses the original Journal and head comparison. One synced commit publishes
economic state and the source audit together. An uncertain write poisons the
owner and denies cached reads. The existing `rollback_to` restores accepted and
rejected IDs, raw input history, registry allocations and all economic custody
together. There is no separate source metadata journal to reconcile.

The old typed `apply_unverified_block` is denied when this policy is enabled,
preventing a caller from bypassing the rejected-input replay set. When disabled,
the typed path retains its single staging copy and original once-per-block money
logic; it does not incur a candidate-owner clone for every ordinary command.

## Source scheduler behavior and explicit repairs

The pinned `consensus.js` is chiefly database/hash/handshake metadata. The actual
block driver is `main.js`.

| Behavior | Pinned source | This policy |
| --- | --- | --- |
| Indexed replay funding order | Types 4 and 20 first, stable database order | Same stable partition of bounded asserted source positions |
| Live funding order | Literal prefixes `4` and lowercase `m` (22) first | Same, including uppercase `M` remaining in the other partition |
| Decode and state reads | Decode/validate each candidate after preceding logic | Same pending-owner ordering for represented commands |
| Failed logic | Marks invalid/processed after a throw; earlier mutations may survive | Candidate rollback plus retained economic rejection |
| Invalid/repeated transaction IDs | Invalid IDs marked processed; repeats skipped | All processed IDs retained/replayed and undone with the block |
| Live activation | Unbound `activationInstance` throws for nonzero decoded types | Genuine source Types plus actual handler's prior-block activation fence |
| Same-block activation | Paths disagree and source async checks have defects | Existing owner requires activation strictly before this block |
| Funding-only live block | Funding commands execute, but mid handler sees no work and skips clearing | End phase always executes once for an accepted block |
| Empty indexed height 10000 | Forces source post-processing | Same owner end phase as every empty accepted block |
| Empty/live blocks | Usually skip clearing | Due funding/system epochs still run through the existing owner policy |
| Live checkpoint | Header and `maybeCheckpoint` run before transaction execution | Complete post-block owner plus audit commit atomically |
| Reorg | Snapshot selection/directory restoration and separate height markers | Exact retained parent/tip and existing bounded full-owner rollback |

The source catchup post-processing order is fee flush, rewards, on-chain queued
orders, channel queued trades, optional vesting, withdrawals, clearing. Live
mid processing runs clearing before withdrawals and block-end vesting compares
an object to zero. This module does not claim those paths are economically
equivalent. It uses the existing explicitly conserving owner pipeline.
Standalone channel withdrawal, reward/vesting and clearlist/property metadata
owners still require genuine integration before their source paths can be
promoted into this owner. No second Ledger is introduced for those gaps.

Source Type0 maps to actual registry activation only. It does not silently
manufacture source native issuance. Source Type15's mismatched validator is
preserved in diagnostics; the mapper does not repair it into authorized close.
Source Types without real called validation, unsupported order flags and
unrepresented source features retain explicit rejection/unsupported evidence.

## TLECv7 checkpoint and replay

V7 is selected only when `source_consensus` is `Some`. Its envelope is:

```
TLEC 07
expected bootstrap commitment [32]
policy [1]
embedded canonical legacy TLECv1-v6 owner [u32 length + bytes]
embedded complete source state [u32 length + bytes]
CRC32 [4]
```

The source state contains the complete canonical TLJE bootstrap and all ordered
block frames. Each frame retains the full header, mode, every raw transaction
in original order, and every receipt in source execution order. Raw values use
bounded tagged binary encoding preserving undefined and exact finite binary64
bits, including negative zero and ordered object fields. There are no supplied
`valid` fields or persisted backend responses used as replay authority.

Restore checks the independent economic/source config, administrator and
permitted properties, strictly decodes the underlying old owner, and reconstructs
the entire pristine economic owner from the pinned bootstrap. It reexecutes
every raw candidate through Types, actual-owner backend, mapper and handler,
then compares every receipt and the full resulting owner. The source header
tip must also equal the surrounding TLEH chain tip, including every retained
prior-state checkpoint. The transient replay flag is never serialized and
always initializes false.

The embedded legacy owner continues its existing catalog/accepted-input checks.
The outer source replay additionally checks rejected and unsupported candidates.
Disabling the policy preserves the original v1-v6 bytes and fingerprints. Five
new v1-v5 pristine goldens were captured from the actual pre-change compiled
library; existing active v5/v6 golden tests remain required. TLSEv1 export can
carry v7 and still requires the independent exact durable head/config.

Limits are 4,096 candidates per block, 4,096 source frames, 65,536 composed source
records/nodes, 65,536 encoded raw bytes per candidate, 2,048 UTF-8 bytes per
displayed reason, 16 MiB complete source history and the existing 64 MiB total
checkpoint. Length/count/depth budgets precede nested allocation. History is
never silently evicted; reaching a limit refuses the proposed block.

Raw metadata supports finite JSON-safe values plus explicit absence. Runtime
Promise/BigNumber instances, nonfinite input numbers, raw UTF-16 lone surrogates
and non-ASCII payloads are outside this typed input domain. Source outputs can
still characterize those cases in the separate diagnostic evaluator.

## Consensus metadata and source defects

`source_state_consensus_hash` reproduces the exact 16-field JSON object order,
latest-row-as-last-entry semantics and database array order. The tally and
activation latest entries are followed by channels, clearlists, contractList,
feeCache, insurance, marginMaps, oracleData, oracleList, orderBooks, propertyList,
syntheticTokens, vaults, volumeIndex and withdrawQueue. `source_tx_index_hash`
filters `_id.startsWith("tx")` without sorting. Both are SHA256 of source JSON.
They are separate from TLEC/TLEH/TLSE commitments and supply no state authority.

The unchanged-source oracle also records these concrete defects:

- Constructor calls a static loader on the instance after publishing the
  singleton; the first construction throws and later returns the partial object.
- Processed rows are written at top level, but parameter/address/block queries
  read a nested `value` schema. Highest-height queries use another block key.
- `compareBlockHeights` omits a required callback; `generateHashes` overwrites
  its own `txIndexHash` method, making repeated handshakes fail.
- Consensus-vector save uses an unbound variable. Activation vectors skip block
  zero in one path, convert zero to -1 in another, retain the first tie and append
  duplicates. Missing loader/static-instance discrepancies break investigation.
- `reconstructConsensus` computes the current database hash after a code-hash
  lookup; it does not reconstruct historical state.
- Flagging uses an out-of-scope variable and save does not await its database.
  These paths do not become a fabricated reliable peer-authorization service.
- Code hashing concatenates 25 listed files, JSON-encodes the resulting string
  and hashes it; absent files are skipped. It is not a compiled Rust identity.

## Verification and remaining boundaries

`node scripts/consensus_vectors.cjs --check` executes the complete unchanged
consensus source with genuine Node SHA256 and explicit recording DB/filesystem/
clock boundaries. All 30 active methods plus constructor execute in 56 cases.
Missing-loader branch injections are explicitly labeled boundary observations.
Full unchanged main.js execution records the scheduler cases above; tx-execution
and endless polling are explicit isolation boundaries in those scheduler cases.

`tests/consensus_source.rs` covers real funded bootstrap preservation, sequential
create/publish owner reads, failed-candidate allocation rollback, rejected-ID
replay, source order/case behavior, empty due funding, canonical 3-network restore
and rollback, durable restart/CAS, TLSE export, repaired-CRC tampering, input bounds
and typed-bypass denial. A private unit test exercises a real synced uncertain
commit and the poisoned cached-read/reopen fence. All 12 consensus integration
tests and the private poisoned-owner regression pass in the full frozen Cargo
run. Combined formatting, Clippy, dependency-policy and release checks pass;
the exact repository totals are recorded in ROADMAP_PROGRESS.md.

This is not a complete historical/mainnet consensus claim. The current economic
chain starts at physical genesis height zero. It cannot silently jump to source
protocol start heights 880000 BTC, 2819000 LTC or 5520000 DOGE. Independently
authenticated historical bootstrap/activation, real property metadata, complete
source native issuance, all missing runtime owners, signatures/parent UTXOs,
wire-to-actor binding and an authorized ingestion adapter remain required.
The raw RPC observer's journal remains separate and cannot authorize this runner.

## Actual source catalog option

Set the independently expected `catalog_bootstrap_commitment` and construct the
state with `from_joint_with_source_catalogs_unverified`, or use
`EconomicChain::new_with_source_catalogs_unverified`. Actual property and
clearlist owners supply pending dependency reads; genuine query/cache effects
are retained even when validation rejects. The owned path allocates type1 IDs,
applies ordinary types11/12 through the sole Ledger, and performs types7/9 and
representable type8 clearlist mutations without substitute Registry rows. The
source token-admin ReferenceError remains visible. A missing owner is still an
explicit unavailable dependency. See `SOURCE_CATALOG_EXECUTION.md`,
`PROPERTY_CATALOG.md` and `CLEARLIST_SOURCE_OWNER.md`.

TLECv10 binds both complete catalog bootstraps, complete current owners and all
original accepted/rejected source candidates to full economic replay. Native
composition retains canonical v8 typed histories. Public typed metadata bypass
remains denied. The existing TLSE producer has not been extended to codecs8,9
or10, and no exported transition witness is claimed.
