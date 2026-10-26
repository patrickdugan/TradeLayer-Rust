# Prices from executed spot amounts

`EconomicSpotPrices` borrows the original verified `EconomicState` and reads its
actual retained `SpotFill` records. It accepts no imported price, volume row,
account attribution or native-coin valuation, and creates no second Ledger or
mutable price cache. The policy is `ExecutedSpotAmountsV1`.

`last_price(base, quote, through_height)` selects the latest matching height at
or before the cutoff; retained position breaks same-height ties. Height wins
even for older restored histories whose retained array is not chronological.
`vwap(base, quote, from, through)` uses an inclusive range. Both return `None`
when no matching execution exists. Same-asset, unknown-property, reversed-range
and beyond-finalized-height queries are explicit errors.

The exact directed price is `sum(quote_atoms) / sum(base_atoms)`. Both sums use
checked `u128` arithmetic; each original amount is `u64` and the owner bounds
retained fills. Both assets use the Ledger's eight decimal atomic units.
Reversing the pair swaps both sums, including VWAP weights. It does not average
rounded inverse prices. Decimal conversion requires explicit precision and
rounding. Exact amounts remain available when a display value rounds to zero
or exceeds the old `u64` price-at-eight-decimals domain.

The immutable service has two additional Rust routes:

| GET route | Backend target |
| --- | --- |
| `/spotPrice/:base/:quote?height=H` | `spot.last_price(base, quote, H or null)` |
| `/spotVwap/:base/:quote?startBlock=A&endBlock=B` | `spot.vwap(base, quote, A or null, B or null)` |

Omitted height/end defaults to the actual tip; omitted start defaults to zero.
The existing canonical integer parsers bound property IDs and heights. These
are named extensions to the source listener routes. Responses retain requested
direction, policy, network, owner height/tip, `authenticatedAdmission: false`
and a nullable price record. Amounts are decimal strings; `quotePerBase` uses
20 places and HALF_UP rounding. Records retain fill indexes, sample count and
range. `samples` returns actual buy/sell/trigger IDs and directed amounts,
without inventing addresses absent from retained fills.

Tests execute funded spot orders in the real owner on all three configured
networks, check exact direct/reverse VWAP and cutoffs, exceed the old `u64`
price-atom domain, restore full checkpoints, roll back and replace a suffix,
and invoke both routes against the actual backend. A regression covers legacy
nonmonotonic retained heights. Queries preserve the complete checkpoint and
Ledger conservation.

This supplies the executed-amount reporting prerequisite. Native contract
admission still needs accepted property metadata, the relevant native-coin
market evidence and authenticated chain inputs. A property ID is not an accepted
ticker, token-token fills are not automatically LTC valuations, and the source
VolumeIndex fallback `0.001` is not execution evidence.
