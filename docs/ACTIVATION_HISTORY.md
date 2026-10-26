# Derived activation history from the sole economic owner

`ActivationHistoryView` derives bounded activation origins from the actual v7 source-consensus owner. It borrows `EconomicChain`, allocates only small origin records and an index, and adds no Ledger, mutable registry, persistence store, checkpoint version, or wire format. Existing checkpoint bytes are unchanged. The existing owner verifier itself performs full economic replay; its internal replay allocations/cost are not a new balance authority provided by this view.

This is an unverified metadata integration. A claimed sender, accepted repaired source candidate, local header, stored codehash, source `valid` result, and matching origin record do not authenticate an L1 transaction. View and record `require_authenticated_admission()` always return `UnauthenticatedInputAndState`. In particular the administrator is the owner's configured administrator, not proof that a signing key was verified.

## Construction and provenance

```rust,ignore
let view = ActivationHistoryView::from_chain_unverified(&chain, expected_tip)?;
let origin = view.origin(18)?;
let historical = view.activation_at(18, historical_height)?;
let usable_next_block = view.is_effective_at(18, expected_tip.height + 1)?;
```

`from_durable(&owner, expected_head)` first checks the actual health-checked durable head, including its changing journal sequence, then borrows the healthy owner. A rollback returning to the same tip does not revive an old expected head. `from_chain_unverified` requires the exact expected tip. The borrow prevents mutation of the owner while the view exists.

Construction requires `source_consensus: Some(...)`, a nonempty bootstrap, contiguous source frames beginning at height one and linked from the configured network genesis, complete transaction/receipt counts, the current source tip, and the existing bounded history budgets. It calls `EconomicState::verify()`. The v7 verifier recomputes Types, source predicates, actual immutable owner reads, candidate rollback, accepted economic effects and finalization from the independently committed pristine bootstrap. Stored receipt dispositions/digests are not accepted as self-authenticating evidence.

After that verification, the view selects only actual `Applied` type-0 receipts in execution order. Each receipt must identify its exact original candidate by both source position and txid. The candidate's actual body is decoded with the existing pinned-source historical `decodeActivateTradeLayer` implementation. Accepted target order and the explicit canonical 32-byte codehash are preserved. Claimed actor must match the actual Registry administrator. Each target is unique across origins and must exactly match the actual current Registry height/hash; every current Registry activation must have an origin.

`AcceptedActivationOrigin` contains the actual accepted `txid`, `claimed_actor`, full local `header`, original source `position`, actual receipt `execution_index`, source block `mode`, original accepted `types`, and `code_hash`. It never fills in a missing origin from the Registry's processed-ID set or assumes that set order is execution order. At most 36 records can be retained; current owner rules actually exclude type 34 and already-active targets, making the attainable count smaller. Accepted lists are bounded to 36 targets. Raw histories continue to use the owner's existing 4096-frame/65,536-record limits.

`records()` and `origin(type)` expose immutable derived records. `verify_claimed_history(records)` compares an exported/asserted history to this exact borrowed owner's complete canonical view, rejecting altered actor, txid, block, position, execution index, target order, codehash, omissions, reorderings, and extra records. It adds no restore/admission API or trusted capability.

## Historical queries and unchanged gates

`activation_at(type, height)` returns the activation recorded at the end of an actual historical block: no activation before its originating height, the exact record at that height, and no future-height guesses. Genesis height zero is queryable. `is_effective_at(type, height)` implements the existing repaired Registry/Joint strict prior-height rule, `activation.height < height`, and allows the immediate next candidate height as a query. Height beyond tip plus one is outside this view's history.

The existing Registry/Joint source files and error mappings are unchanged in this slice. The view does not replace execution gates or change `EconomicSourceBackend`: source-shaped `Activation.isTxTypeActive` remains a presence read, including a record created in the same block; actual owner execution separately rejects same-block use. The pinned source's `Validity.isActivated` is unconditional true, and several threshold callers omit await. Neither source quirk is silently promoted into a gate here. Centralizing shared execution predicates can be considered separately with their original error behavior preserved.

Legacy/no-source-history owners return `MissingOriginHistory`, including legacy owners that have structural activation records. A complete pristine v7 genesis owner legitimately returns an empty complete view. Missing bootstrap or incomplete/shifted frames are rejected explicitly, and invalid owners fail verification. Old TLRG/TLEC structural summaries, evicted economic rollback headers, an uncommitted bootstrap, or a caller-supplied source transcript are not substitutes for complete v7 origin evidence. No migration from a preactivated legacy owner is invented. Root imports that attempt to forge or remove raw source evidence with repaired CRCs remain rejected by the actual owner's existing full replay decoder.

## Genesis and source differences

The current Registry action is metadata only. A type-0 transaction selecting ordinary targets is distinct from selecting target 0 itself. `AcceptedActivationOrigin::requires_native_genesis_policy()` reports whether the accepted list contains 0; success does not initialize vesting or mint native balances. The tests confirm that actual available balances remain unchanged by such metadata activation. A future funded genesis adapter must resolve the existing property/insurance/clearlist and supply-policy dependencies under the sole Ledger, with one explicit conserved issuance budget and independently authenticated administration.

This view follows actual accepted repaired owner behavior: no repeat targets, no duplicate IDs, type 34 excluded, strictly prior-block effective gates, configured-admin equality and explicit canonical hash. The frozen `activation` source API separately preserves source overwrite/backward-block behavior, arbitrary truthy codehashes, 36 names, repeated initializer calls and partial persistence effects. This view does not feed source class state into Registry or claim literal historical chain acceptance.

## Evidence and integration

All seven real Cargo integration tests against the actual v7 core pass in the complete frozen repository run. Earlier isolated direct Rust checks also passed. They cover all three networks; real source activation and same-block rejected oracle creation; next-block creation; explicit source presence versus strict owner gates; unauthorized/repeated/type-34 candidates; target-0 metadata without issuance; immutable bytes before/after queries; absent legacy origins; forged/exported/prebootstrap assertions; repaired-CRC raw-candidate/frame corruption; exact checkpoint restart; rollback-history eviction to an empty floor and a new branch; real durable journal restart/rollback; stale head and ABA rejection. The tests use the actual `EconomicChain`, `DurableEconomicChain`, Registry, Types/validity backend, codecs and disk journal, not a parallel Ledger or fabricated history fixture. Temporary cleanup checks resolved absolute paths stay under the actual temporary root and match the exact task prefix before recursive deletion.

The module is exported and integrated in the repository. The full frozen Cargo run, formatting, all-target Clippy with warnings denied, dependency-policy checks and all-target release build pass. The view changes no Ledger or checkpoint bytes. A separate restore-boundary correction makes ordinary Joint snapshots repeat their constructor channel-liability fence; five real regression tests confirm valid channel custody/withdrawal/net/King snapshots remain supported by their standalone Ledger/Spot owners while ordinary Joint import rejects them. See ROADMAP_PROGRESS.md for the exact combined test totals. No publication or push is part of this local integration.
