# Atomic offline spot reporting

`reporting::OfflineSpotReports` connects existing `spot::SpotFill` values to
the new trade history and volume indexes. A state holds one explicit
`TradeLayerMainnet`. An observation for another network is rejected before
either index changes. Each observation requires a stable fill ID, an event
position within its block, buyer and seller identities, and a caller-supplied
LTC valuation. `SpotFill` does not contain those identities or valuation, so
the adapter never derives them from transaction IDs or price lookups.

`record_spot_fill` stages both indexes on a private copy and commits only when
both insertions succeed. Duplicate IDs, occupied event positions, capacity
errors, zero amounts, noncanonical pairs and unrepresentable decimal prices
leave both indexes unchanged. History retains exact base and quote amounts.
The volume price is quote/base scaled to eight decimals with explicit
ROUND_HALF_UP; its VWAP consequently uses those rounded observations. The
LTC valuation is supplied metadata for all supported networks; its presence
does not prove an exchange rate. The adapter performs no currency conversion.

`rollback_after(height)` removes the same suffix from both indexes and frees
its IDs for replacement observations. Its height is caller-selected. No
canonical chain selection, trade execution, financial settlement, or parent
UTXO/script verification occurs. Histories are exposed immutably. Raw history
checkpoint bytes alone cannot restore the combined adapter: combined durable
storage with network and chain anchors remains a service integration task.

This is a bounded offline reporting interface. Whole-state cloning favors
reviewable atomicity and does not provide a production indexing throughput
claim. `tests/reporting.rs` verifies successful fill conversion, exact amount
retention and price rounding, second-index failure atomicity, duplicate
valuation rejection, event collisions, network segregation, and rollback
with replacement replay.
