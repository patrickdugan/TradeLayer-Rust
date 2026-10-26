# Guarded book liquidation checkpoints

`EconomicConfig.liquidation` selects the explicit
`ConservingLiquidationV1` policy and requires settled-basis marking. Its price
guard and per-owner fill limit are independently expected configuration: price
deviation is at most 9999 basis points and the fill limit is 1 through 64.
The policy uses real firm-limit makers in the combined owner's contract book.
Its private reduce-only requests are system metadata, never wire orders or
external transaction authority.

The same Joint-owned Ledger holds actual reserves, margin, fees and collected
losses. Liquidation must preserve existing basis/carry and IOU accounting;
metadata about a claim or a loss does not create collateral. No separate
AMM, insurance or ADL balance owner is composed by enabling this policy.
Contract configuration, oracle inputs and chain actors retain the existing
unverified boundary. This conserving policy does not claim historical
JavaScript liquidation acceptance parity.

The forced fee is senior to closing PnL: the actual charge is capped by the
distressed owner's available and margin atoms. Half of that charge, rounded
down, rebates the maker; its remainder enters the configured fee route.
Nominal fees above actual custody never become fabricated collateral. A best
maker that cannot fund its required leg stops the attempt rather than granting
permission to execute a worse maker. Guarded fills remain strictly reduce-only
for the distressed position.

A fresh final-accepted configured oracle publication supplies the mark. Ordinary
matching and that mark settle first, then funding, then closeouts. The policy
captures the resulting below-maintenance owner set in canonical account order.
It cancels each target's own orders, returns their actual reserve atoms and
funds a margin top-up from available custody toward the initial target. If that
cures maintenance, it stops; otherwise each fill takes the selected maker's
maximum exact quantity, bounded by the remaining distressed exposure. It stops
when closed or cured, no guarded maker exists, the best maker is unfunded, or
the configured fill count is exhausted.

This is one bounded pass over that captured owner set. A maker who acquires a
new deficit during the pass remains a maintenance call until another fresh
mark; the policy does not claim cascading liquidation. Oracle closure in the
same block suppresses the final mark and closeout. The source's coverage,
liquidation sizing, AMM-first flow, insurance and ADL orchestration remain
separate parity and integration gates.

# Versioned recovery

TLEC version 4 is canonical exactly when liquidation is configured. Its
configuration extends version 3 with the expected price guard, per-owner fill
limit and policy. Its state appends the bounded liquidation journal and every
block's complete book input/placement frame after the complete mark state.
Forced mark events identify a liquidation epoch, fill
index, real maker transaction and distressed owner. These are distinct from
ordinary admitted trade events. Earlier TLEC versions reject forced tags;
version 1, 2 and 3 bytes remain unchanged for their existing configurations.
Enabling liquidation requires replay from bootstrap with the complete frame
journal; loading an earlier checkpoint does not invent its missing book
history or silently change its independently expected policy.

Checkpoint loading validates independent expected network, admin, permitted
properties and complete economic policy, enforces record bounds before loops,
then verifies complete economic invariants and canonical re-encoding. CRCs
detect byte corruption; they do not authenticate an asserted history. Existing
historical publication provenance limits described in `ECONOMIC_MARKS.md`
remain applicable.
Maker priority and cancellation checks reconstruct the book from its retained
input/placement journal and compare captured snapshots. These caller-derived
records do not authenticate accepted external-order history or historical
custody observations.

The liquidation journal is bounded to 4096 events, at most 64 fills per event,
4096 book frames, and 65536 aggregate records across all captured queued,
drained, resting and seen book rows, cancellations, fills, optional blocked
makers, and frame inputs/placements. Each frame bounds inputs and placements
to 4096 each and persists all cancellation criteria. The decoder
charges the shared budget before each corresponding record loop. Hitting a
bound rejects the staged transition; history is not silently pruned. Because
frames cover every applied height starting at one, this version has an
explicit 4096-block lifetime bound until a separately verified history
compaction format is introduced.

`EconomicChain` includes this state in complete rollback checkpoints.
`DurableEconomicChain` stages the whole owner, commits one synced journal frame,
then publishes memory. Reopening requires an independently expected head and
tip. Separate storage namespaces do not provide cross-owner atomicity.

Regression coverage includes genuine pre-liquidation version-3 byte
fingerprints, explicit version gates on forced tags, nested-book allocation
bounds, complete forced-close restart and durable rollback/replay on Bitcoin,
Litecoin and Dogecoin profiles, and repaired-CRC mutations of identity, guard,
book, frame inputs and placement positions, fill, fee and outcome metadata.
