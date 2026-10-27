# Literal contract runtime replay

`SourceContractReplay` owns one `SourceContractRuntime`. Its checkpoint records
the commands that constructed that runtime and the receipts produced by actually
executing them. Restore creates a fresh runtime and executes every command again;
it never admits a supplied current database, balance, position, or validity result.
The source pin is `dca0888b827c5bc28fa9189bc9e1d507f5c5670c`.

This wrapper preserves the literal source runtime's current scope and defects. It
does not complete the remaining source financial graph or turn that runtime into
the conserving `EconomicState` owner. The literal runtime has its own sole source
Tally collection/cache; replay introduces no additional spendable balance store.

## Commands and receipts

The typed commands are `Import`, `SetEntry`, `SetHeight`, `SetActivation`,
`FailStorage`, `Invoke`, `EvaluateQueue`, and `ExecuteContract`. Initial persisted
collections, module entry order, activation answers, heights, and injected storage
failures are explicit journal inputs. Their external provenance remains the
caller's responsibility. No command can supply a receipt or dependency verdict.

`apply` executes the actual runtime method, drains its actual effects, and returns
a receipt with the command sequence, previous head, exact outcome, complete effect
list, state commitment, effect commitment, and new head. Outcomes retain the
distinction between `Immediate`, `Pending`, `Throw`, and `PendingThrow`, source
returned/thrown outcomes, and local `SourceRuleError` failures. The runtime is
available through an immutable accessor; mutation goes through the journal.

Source exceptions retain the source's completed writes, cached object mutations,
consumed failures, and other prefixes. These effects are committed and replayed.
A local runtime error restores the pre-command owner, records a `LocalError`
receipt, and advances the journal. A wrapper framing, encoding, or capacity
rejection restores the owner and appends no entry. These explicit local rollback
rules are distinct from the source's exception behavior and the conserving
economic/admission policy.

Repeated equal command values are separate legal occurrences. An encoded entry
cannot be duplicated, reordered, or moved to another sequence without rejection.

## Checkpoint admission

`checkpoint()` produces canonical `TLCR` version 2 bytes for the default
`GlobalClearingV2` runtime. `with_policy(OnchainV1)` retains canonical version 1
bytes. Restore admits only those two versions and constructs the matching policy;
version changes cannot reinterpret a stored journal. The versioned bootstrap and
V2 policy/state fields bind the execution graph. A frozen real TLCR v1 checkpoint
from compiled commit `99210d434528d950890ab459c4a7f2a4cee7caa5` verifies unchanged
bytes, previous unknown-dependency outcomes and future V1 continuation. `from_checkpoint(bytes,
expected_head)` requires an independently retained `SourceContractReplayHead`,
containing both the command count and 32-byte commitment. Reading that expected
head from the same untrusted checkpoint defeats the external history pin.

Restore checks the size, CRC, version, bounded count, supplied head, and actual
fresh-runtime bootstrap commitment. For every entry it checks the exact sequence
and previous head, decodes canonical frames, executes the actual command, and
compares the complete resulting receipt. It then checks end of input, the final
independent head, and exact whole-checkpoint reencoding. CRC-repaired changes to
commands, outcomes, effects, state commitments, or ordering therefore do not gain
admission by supplying internally consistent-looking state.

SHA-256 commitments use separate bootstrap, journal, effects, and state domains.
The bootstrap includes the source pin and fresh runtime state commitment. A journal
head binds its predecessor, sequence, canonical command and outcome, resulting
state, and effects. These hashes and CRC provide integrity checks; they do not
authenticate L1 inputs, collection imports, activation answers, or a head supplied
by an untrusted party.

The actual state commitment includes the shared source database rows, queue
checkpoint including pending failures, Margin instances and established object
aliases, Tally/cache and fee buffer, clearing marks/trades, globals, Insurance,
source oracle cache/watermarks/initialization, and continuation state. V2 also
binds live clearing context arena references, deleverage/liquidation maps, loaded
contract-series and property metadata, channel owner state/action count, lazy
vault initialization, Tally initialization/modification flags, the exact raw Contract modification
flag (with its separately retained effective boolean projection), pending-job
indicators and unhandled source rejection diagnostics. Continuation
state includes RNG, UUID/database counters, height/clock, module flags and entry,
activation, remaining failures, and last decoded parameters. Effects are drained
and committed separately. Restore rebuilds these values through execution rather
than decoding a supplied state projection.

## Encoding and limits

All integers in the container are big endian. The header is five magic/version
bytes, the 32-byte fresh-state commitment, a 32-bit entry count, and the 32-byte
final head. Each entry contains a 64-bit sequence, 32-byte previous head, command
and outcome frames, a 32-bit effect count and individual effect frames, then the
32-byte state, effect, and journal commitments. A frame is a 32-bit byte length
followed by canonical source diagnostic JSON. The container ends with CRC32.

Commands, outcomes, and individual effects are framed independently. Aggregate
runtime state is hashed with a separate canonical binary traversal, avoiding a
single diagnostic value's depth/node limit across the entire journal or database.
Number bits, signed zero, nonfinite values, `Undefined`, `Promise`, BigNumber text,
array order, and object field order retain their source value representations.

| Bound | Maximum |
| --- | ---: |
| Journal commands | 4,096 |
| Effects per command | 4,096 |
| Individual diagnostic frame | 4 MiB |
| Whole checkpoint, including CRC | 32 MiB |
| Aggregate state traversal | 262,144 nodes / 16 MiB / depth 64 |
| State array or object entries | 1,024 |

Diagnostic frames also obey the shared `SourceValue` validation bounds. Declared
entry/effect/frame sizes are checked before their corresponding allocations or
decoding. These are explicit supported-domain limits, not source business rules.

## Verification

`tests/source_contract_replay.rs` has three tests. Four genuine frozen composed
source scenes provide 19 observations, including their initial states: matched
positions, Tally-first fee/Insurance flush, a late volume write failure and retry,
and a partial close producing an IOU. The test executes actual raw/source commands,
compares the frozen outcomes and complete persisted collections, restores after
each action, and compares subsequent fee execution with uninterrupted continuation.
Fixture expectations are assertions only; they never answer runtime dependencies.

Additional cases reject an incorrect independent head, CRC-repaired sequence,
order and duplicate-entry splices, changed commands or recorded state commitments,
oversized declared counts/frames, and every empty-checkpoint truncation and
single-byte corruption. Local error attempts replay with unchanged owner state;
overlong wrapper input leaves both owner and journal unchanged. Three tests pass
against the actual composed Rust runtime. Full source fixture provenance remains
documented in `CONTRACT_EXECUTION_FIDELITY.md`.

## V3 lifecycle and portable command slices

Explicit `SourceLifecycleV3` emits TLCRv3 with its own bootstrap policy domain.
The default remains V2. The original compiled V1 and V2 fixture journals are
immutable compatibility inputs; a repaired checksum cannot promote either to
V3. New definition, channel and declared realm capabilities are gated to V3.

`SOURCE_TRANSITION_WITNESS.md` documents the separate TLSWv1 conditional command
slice. Verification restores the complete prior TLCR journal and re-executes
original admitted commands. It accepts neither supplied poststate nor dependency
answers. An independently retained expected pre-head and explicit execution
boundary are required. It leaves TLSEv1 unchanged and supplies no actor, UTXO,
chain or custody authentication to an external consumer.

V4 is an explicit `OptionLifecycleV4` graph with TLCR version 4 and bootstrap
`TradeLayer/SourceContractBootstrap/v4/`. Populated option indexes, Vault and
synthetic Maps, actual aliases, partial source errors and retries reconstruct
from original commands. The independent compiled V3 fixture at
`fixtures/source_contract_replay_v3.bin` comes from tested commit `dd79498`,
before V4 code existed. Its original 10-command head is an external test pin;
correcting CRC and replacing a bootstrap field cannot promote its history.
V1-V3 retain their earlier capability graphs; default construction remains V2.
