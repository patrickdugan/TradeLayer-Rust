# Conditional literal-source transition witness

`SourceTransitionWitness` exports a portable, bounded command slice for the
literal `SourceContractReplay` owner. Verification restores the complete before
journal and executes every original suffix command through the actual runtime.
The result is named `ConditionalSourceTransition`; its scope is
`conditional-source-execution-only`.

The claim is: **given this independently pinned prestate, declared execution
policy and exact commands, the runtime produces these outcomes, effects and
poststate**. Original actor claims, imported bootstrap documents, activation
answers, RPC height and storage failures remain assumptions. No chain admission,
signer authorization, UTXO existence, producer identity, custody authority or
historical acceptance is established. An external UTXORef consumer can bind the
data to its own independently checked inputs and commitments. Its proof engine
and any Binohash feature remain outside this producer.

`TLSEv1`, its conserving economic owner, and all existing `TLCR` checkpoint
formats remain unchanged. This envelope uses `TLSW` version 1 and an explicit
literal-owner tag. The literal runtime's source Tally remains its sole balance
authority. Verification introduces one temporary reconstructed owner and no
additional spendable Ledger.

## Public interface

All types are exported from `source_contract_replay`:

```rust,ignore
let descriptor = SourceTransitionDescriptor::reference();
let expected = SourceTransitionExpectation {
    policy: SourceContractRuntimePolicy::GlobalClearingV2,
    descriptor: &descriptor,
    pre_head: independently_retained_before_head,
    commands_commitment: Some(independently_pinned_original_commands_digest),
    post_head: independently_expected_after_head_or_none,
};
let witness = SourceTransitionWitness::produce(&before, &original_commands, &expected)?;
let bytes = witness.canonical_bytes()?;
let conditional = SourceTransitionWitness::verify_bytes(&bytes, &expected)?;
assert_eq!(conditional.scope(), "conditional-source-execution-only");
let actual_receipts = conditional.receipts();
let actual_owner = conditional.replay().runtime();
```

The producer does not mutate `before`. It serializes its actual complete
checkpoint, reconstructs it with the independently expected pre-head, executes
the suffix, and derives the post-head and state digest. An optional independently
expected post-head is checked during production and verification.

The expectation explicitly selects the runtime policy. The envelope and nested
prestate policy tags must match that expectation before historical replay.
`OnchainV1` retains its original execution graph and bytes; a V1 global-clearing
command therefore records the original local unavailable-dependency outcome.
`GlobalClearingV2` selects the full V2 graph. `SourceLifecycleV3` uses the
separate V3 graph and TLCR bootstrap/tag; its broader runtime graph does not
expand this witness profile's raw-type whitelist. Future explicitly mounted
policies use their own replay bootstrap/codec tags; the verifier never
substitutes the runtime's current default.

`SourceTransitionDescriptor::reference()` names source commit
`dca0888b827c5bc28fa9189bc9e1d507f5c5670c` and boundary
`tl-source-reference-db-rpc-clock-rng-numeric-boundary-v1`. The descriptor and
runtime policy together identify the declared adapters: bounded source values,
the policy's source DB/query/storage-clone behavior, explicit RPC height,
deterministic clock/UUID/audit RNG, and implemented numeric domains. The source
commit alone does not describe these external adapters. The BigNumber reference
provenance and separate tested 9.1.2 lock comparison remain documented in
`BIGNUMBER_LOCK_REFERENCE.md`; no whole-version equivalence is implied.

Expectations read from the same untrusted witness supply no independent pin.
The optional command commitment is computed by
`source_transition_commands_commitment`. Without that independent commitment,
verification establishes the relation for the commands actually present in the
witness. Another correctly executed command slice can have the same before-head
and a different valid after-head. A repaired self-hash cannot make it the
independently pinned original slice.

The conditional result exposes actual suffix receipts, before/after state
digests, command commitment and heads. `replay()` is immutable; `into_replay()`
returns the single reconstructed owner for explicit subsequent journal commands.
Neither accessor grants authenticated admission.

## First command-slice profile

Only these suffix forms are admitted:

| Command | Required shape |
| --- | --- |
| `SetHeight` | One original source value |
| `ExecuteContract` | Exactly nine original Types arguments; type argument is Number 18 |
| `EvaluateQueue` | `processQueuedOnChainOrdersForBlock` with one original argument |
| `Invoke` | `Tally.flushQueuedFeeAccruals` with one original argument |
| `Invoke` | `Clearing.clearingFunction` with height, optionally original unused realtime argument |

`Import`, module-entry changes, activation changes, failure injection, arbitrary
financial `Invoke`, and other queue operations are rejected from the suffix.
Bootstrap history can contain those existing replay commands, including the
remaining failure schedule, and is bound by the independent before-head. No
diagnostic escape profile is provided.

This is a command-slice profile. It preserves the supplied order, repeated
occurrences, call arity and original source values. It does not assert that the
slice contains every L1 transaction or the complete source main-loop scheduler.
A complete block claim still needs an independently bound extraction, complete
transaction ordering and phase schedule. Source validation and financial rules
are recomputed by the runtime; no supplied `valid` result, backend response,
match, balance, current state or output verdict is admitted.

Source exceptions retain actual completed storage/cache/financial prefixes and
consumed failures. Subsequent suffix commands can retry and reproduce source
re-execution behavior. Local unsupported-domain/admission/capacity failures
retain the runtime's explicit rollback policy and produce a journal
`LocalError` receipt. Successful witness verification does not assert that every
command succeeded as a business operation.

## Envelope and replay checks

All container integers are big endian. Fields appear in this exact order:

1. `TLSW\x01`, literal owner tag 1, command-slice profile tag 1, replay policy tag.
2. Forty ASCII source-commit bytes, 16-bit boundary length and exact boundary bytes.
3. Before-head command count and commitment, then 32-bit complete before-TLCR length and bytes.
4. 32-bit suffix count and canonical existing diagnostic command frames.
5. Command commitment, after-head count/commitment and after-state commitment.
6. Domain-separated witness SHA256 and CRC32.

The command digest hashes
`TradeLayer/SourceTransitionCommands/v1/`, the 32-bit count and each existing
length-prefixed canonical command frame. The witness digest hashes
`TradeLayer/SourceTransitionWitness/v1/` followed by every preceding envelope
byte. The after-head and state digest reuse the exact existing replay algorithms;
they are not alternate state projections.

Verification checks framing, tags, expected policy/descriptor/pre-head, declared
length/count budgets, canonical commands, optional independent pins and envelope
integrity before retaining a checkpoint copy. Existing `from_checkpoint` then
reconstructs the before-owner by executing its complete history. Every suffix
command executes through `apply`. The verifier compares the actual after-head
and complete state digest, then returns actual receipts and continuation state.
Exact canonical whole-envelope encoding is required. Supplied poststate bytes
do not exist in this format.

| Bound | Maximum |
| --- | ---: |
| Complete envelope, including checksum | 32 MiB |
| Before history plus suffix commands | 4,096 commands |
| Individual diagnostic command frame | 4 MiB, additionally shared SourceValue bounds |
| Boundary descriptor | 256 bytes; only the exact reference descriptor is admitted |

The existing runtime/checkpoint state, effect, node, depth and collection limits
also apply. Witness framing/capacity refusal appends no command and leaves the
producer unchanged. The whole-envelope limit can refuse a before-checkpoint at
the journal's exact size limit because the envelope requires additional bytes.

## Verification cases

`tests/source_transition_witness.rs` covers three focused tests. Four genuine
composed-source scenes supply 19 frozen observations and 14 exported operation
slices, covering quantity-two matched positions, fee flush/Insurance, partial
close/IOU and a late volume-write exception followed by retry. Failure injection
is established in the independently pinned before-history, never the suffix.
Every slice compares the genuine outcome and complete persisted collections;
restored full continuation owners and receipts also equal uninterrupted Rust
execution.

A fresh unchanged-module global-clearing reference compares the actual global
call and persisted collections. A shared clearing-context/Margin position
survives restoration and a later assignment mutation remains visible through
both aliases. Adversaries cover owner/policy/source/pre-head substitution,
another valid before-journal, reordered commands, independently pinned command
digests, changed after-state, repaired witness hashes/CRC, noncanonical JSON,
forged frame/count/length limits, excessive nesting, forbidden commands, V1
local-error rollback, and explicit V3 selection/optional realtime arity. No
external chain evidence is fabricated by these tests.
