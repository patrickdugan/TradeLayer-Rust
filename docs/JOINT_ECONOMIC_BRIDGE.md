# Private Joint economic bridge

`JointProtocolState` still owns the sole `Ledger` through its existing
`SpotExchange`. The crate-private economic bridge lets the combined economic
owner stage ordinary Joint commands, register type-18 contract inputs or
type-6 owner cancellations, and move real collateral through derived
`~econ:` accounts in that same ledger. The outer owner must derive and audit
every account, amount, position, IOU, fee and reserve before publishing its
staged block. This bridge alone does not authorize settlement or establish
authentication of actors, blocks, oracle marks or transaction IDs.

Ordinary commands use the original Joint dispatch, including registry
authorization, activation, same-block activation denial, finalized-height
checks and global replay checks. Economic events form a separate disjoint
input category. Their external IDs cannot collide with registry, spot,
ordinary-ledger or bootstrap-ledger IDs. Internal custody fingerprints cannot
reuse any external ID or an already processed ledger fingerprint. Only types
18 and 6 can register economic events; types 19, 23, 31 and other economic
opcodes gain no authority here.

A transient height records an in-progress combined block. Every ordinary or
economic command must use that height. Custody legs additionally require an
economic event registered in that current block. Finalization clears those
transient markers on a staged copy and validates the complete Joint portion
before publishing. Empty blocks can finalize. Neither public nor private
snapshots accept a partially processed combined block. The outer owner must
apply and validate its entire economic state atomically; the bridge does not
provide atomicity for separately called methods.

The old public Joint verifier and TLJP/TLJC exports continue to reject economic
history. Even an event with zero custody legs permanently records the history
fence, so zero balances cannot discard economic replay metadata. Ordinary
pristine TLJP v1 bytes remain unchanged. Public Spot exports also retain their
history fence and reject economic escrows.

## TLJE v1

The private payload contains `TLJE` plus version byte 1, network tag and
genesis, an economic activity flag, length-prefixed registry and combined Spot
snapshots, sorted global/ordinary-ledger/economic input sets, optional finalized
height, and CRC32. Its total size obeys the existing checkpoint bound; replay
counts use the Joint limit. Each nested codec validates its own bounds. The
reader requires independently supplied network, administrator and permitted
spot properties, validates category disjointness and component heights, and
requires canonical byte-for-byte reencoding.

The activity flag must equal the presence of economic replay IDs. Combined
Joint verification rejects reserved account families other than `~spot:` and
`~econ:`, economic balances without economic event history, and preexisting
channel or legacy settlement liabilities that this owner cannot advance.
Exact `~econ:` ownership and liabilities belong to the enclosing
economic codec and verifier; TLJE is private and is not a complete economic
checkpoint. No live economic owner should accept TLJE on its own. CRC32 and
canonical encoding detect corruption and structural inconsistency, not forged
history or chain consensus.

Private unit tests cover shared real balances, zero-leg history, activation,
block boundaries, replay collisions, reserved account denial, full roundtrip,
truncation, rechecksummed flag/version/network/replay tampering and independent
configuration. Public integration tests pin pre-bridge TLJP bytes and retain
legacy opcode denial with atomic block rejection.

## Enclosing TLEC v1 economic checkpoint

`EconomicState` is the enclosing owner. Its TLEC v1 checkpoint serializes the
exact economic configuration, TLJE payload, full contract book including input
replay IDs, insertion-order runtime positions, TLIO source IOU counters and
claims, complete fee rows and journal, order reserves, margin projections,
cancellation replay IDs, actual loss-pool and pending-insurance custody
amounts, unfunded-loss observations, trade receipts and internal-leg sequence.
The codec checks `EconomicState::verify()` before writing and after reading,
then requires byte-for-byte canonical reencoding. Independently supplied
configuration, administrator and permitted spot properties must match.

The position subset contains contracts, average entry, realized PnL, last
mark, funded margin and IOU entitlement. Available balances are read from the
sole ledger. Unsupported option, expiry and delta metadata cannot disappear:
the writer reconstructs a fresh insertion-order MarginMap and requires full
equality. Finalized book and fee queues must be empty under the current owner
policy. All fee routes and journal entries are reconciled by that owner.

TLEC uses big-endian integers, explicit optional flags, normalized decimal
text, bounded counts, nested lengths and an outer CRC32. Economic decimals
are limited to 40 coefficient digits, 20 fractional places and 128 encoded
text bytes. The total checkpoint bound is 64 MiB; component order, position,
fee, replay, receipt and custody-leg limits also apply. Exhausting retained
receipt or replay capacity fails the staged block; no history is silently
pruned. This is a complete checkpoint for this specific economic owner, not
for unrelated standalone oracle, AMM, vault or insurance experiment owners.

Restart tests exercise a genuine unpaid profit claim followed by a funded
loss collection and payment, exact restoration across all three networks,
funded resting-order cancellation, cross-category replay, independent config
checks, truncation and rechecksummed structural corruption. A nested ledger
with valid CRCs and conserved supply still rejects an unowned economic escrow.
