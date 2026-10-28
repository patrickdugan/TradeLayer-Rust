# Closing oracle-abuse and UTXORef exercise

Status: planned at the user's request. Run after the source logic and integration
are nearly complete. No exercise result or production-readiness claim is implied
by this plan.

TradeLayer produces checkable state-oracle data. UTXORef is the separate consumer
repository. Its verification and acceptance tests belong there; optional Binohash
work also remains there.

## Pin the experiment

Record both repository commits, the selected source/replay/witness profiles,
numeric and module-entry boundaries, network, block/transaction ordering, and an
independently expected starting state and head. The witness must support that
exact producer profile. An older profile must not silently accept a newer one.

Use controlled test identities and a small oracle-linked position. Bind the
oracle's registered administrator and the publication's actor/input evidence.
Identify which chain/script facts are actually verified and which are declared
mock inputs. A matching hash does not establish authentication.

## Valid control

1. Start from the agreed registry, price history, balances and open position.
2. Apply a valid oracle publication through the real TradeLayer transaction path.
3. Run the reached settlement steps over the existing owners and sole balance
   authority. Generate the witness from the actual execution and journal.
4. Give UTXORef the pinned context, transition claim and witness. Its independent
   verifier must accept the correct claim and produce the expected verdict.

## Provable abuse

Create a deliberate invalid transition claim: for example, alter the claimed
accepted price/history or resulting settlement balances while retaining the
original ordered publication input. Where the pinned source rules reject an
unauthorized publisher, also claim that such a publication was accepted.

UTXORef must reject the forged claim by checking the actual transition against
the independent starting anchor and bound inputs. Repeat with repaired envelope
checksums, substituted state/head/command commitments, a different oracle or
network, stale height/context and a replayed claim. Keep the original valid
control accepted after each isolated mutation. Specify replay handling explicitly
and ensure verification cannot apply economic changes twice.

A publication from an authorized actor can still contain a misleading market
price. Proving an externally false price requires an agreed dispute rule and
admissible evidence. Transition replay alone establishes the pinned protocol's
behavior, not external market truth.

## Acceptance evidence

Keep the real producer and consumer test commands and logs, both commit IDs,
fixture/bundle hashes, accepted and rejected claims, and the exact reasons for
each verdict. Include cross-repository byte/commitment checks and a minimized
reproducer for every unexpected result. Preserve the original source behaviors;
any repair requires a separately identified policy.

Success means the controlled end-to-end handoff accepts the correct transition
and rejects the deliberate invalid claims under the stated evidence boundary.
Historical completeness, production ingestion and authentication remain separate
claims unless their own gates have actually passed.
