# Reports derived from the complete economic owner

`EconomicHistory::from_state` verifies and immutably borrows `EconomicState`,
then rebuilds bounded exact execution metadata for `TradeHistory`. Ordinary
fills retain their real buy/sell transaction IDs. Forced book fills retain epoch,
fill index and maker ID; forced AMM fills retain epoch, fill index and pool ID.
Private pool counterparties remain reportable without becoming public actors.

Buy/sell orientation comes from the actual opposing signed basis deltas. Fill
prices, quantities, booked PnL, collected atoms, claim offsets, loss offsets,
fees and event-wide paid claims come from verified owner events and receipts.
An event-wide payout is never assigned to a participant without evidence.
Deterministic report IDs bind network/genesis, contract, original event position
and typed execution provenance. They are metadata IDs, not accepted transaction
IDs or signatures. Projection inserts one validated atomic batch instead of
rebuilding the history after every fill.

Pure mark events and funding epochs remain borrowed from the same owner.
Booked trade PnL excludes pure mark PnL and cash funding; callers can inspect
those separate facts without presenting one as the other. Position/episode,
LIFO, category, reference and PnL reports use the documented exact history policy
in [trade-history.md](trade-history.md). They do not recalculate settlement or
change positions, custody, replay IDs, claims or checkpoints.

Old unmarked economic receipts lack retained owner evidence once orders have
filled. A nonempty such history returns `MissingOwnerEvidence`; transaction IDs
are never used to guess owners. Empty old state projects an empty report.
Restoration or rollback of the sole owner naturally changes the next projection;
there is no second mutable economic history or independent rollback authority.

Integration tests compare projected cumulative positions against actual margin
positions and booked trade plus pure mark PnL against actual realized PnL across
fractional linear/inverse adds, closes, flips, defaults and reorgs. Actual forced
AMM/book cases verify provenance, private pool references and canonical TLTHv2
round trips. Source characterization and these consistency checks establish a
reporting repair policy, not historical/authenticated protocol acceptance.
