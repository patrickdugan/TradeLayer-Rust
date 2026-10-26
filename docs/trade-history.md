# Deterministic trade history

`trade_history.rs` supplies one bounded metadata history for legacy integer/token
observations and exact Decimal contract execution events. It provides concrete
position, category, episode, transaction-reference, LIFO and booked-PnL reports.
It performs no matching, settlement, transaction admission, address ownership
checks or chain selection. A report never creates a wallet balance or claim.

The existing integer record API remains available. New `ContractExecution`
events retain real ordinary-order references or explicitly typed forced book/AMM
origins. Both record forms share one fill-ID and event-position namespace and a
single 65,536-record capacity. `records()` returns the original records;
`executions()` returns exact events; `len()` includes both. Position reports
combine both kinds chronologically. Legacy rational prices are projected at
20 decimal places in the caller-selected units, without guessing collateral.

## Source scope and query behavior

The reference is `tradelayer.js` commit
`dca0888b827c5bc28fa9189bc9e1d507f5c5670c`,
`src/tradeHistoryManager.js`. Its LF-normalized Git-blob SHA-256 is
`670035f8153abda542c732deee0cf995fcd1e1e7c7b9f50464fce6ab1b5c1569`.

| Source function | Typed history behavior |
| --- | --- |
| `loadTradeHistory(contractId)` | `contract_entries` returns both supported contract record forms; no database document-format migration |
| Instance `getTradeHistoryForAddress` | `contract_entries_for_address(..., AddressMatch::Exact)` compares either participant exactly |
| Static `getTokenTradeHistoryForAddress` | `token_trades_for_address(..., AddressMatch::LegacyOriginalOrLowercase)` accepts either token-pair ordering |
| Static `getContractTradeHistoryForAddress` | `contract_entries_for_address(..., AddressMatch::LegacyOriginalOrLowercase)` compares either participant |
| `getTradesForContractBetweenBlocks` | `contract_entries_between_blocks` includes both endpoints |
| `getLiquidationTradesForContractAtBlock` | `liquidation_entries_at_block` requires the exact block and typed liquidation origin/flag |
| `getFirstTradeBlock` | `first_trade_block` returns `Option<u64>`, including a valid block zero |
| `getPositionHistoryForContract`, `trackPositionHistory` | `position_history` / `position_history_through`: immutable cumulative Decimal position and trade-average replay; named repair of the broken source compositions |
| `getTxIdsForContract` | `transaction_references` keeps typed event origins and participant order IDs only for ordinary fills; repaired forwarding of contract ID and actual chronological order |
| `getCategorizedTrades` | `categorized_trades` splits each fill into exact closed/opened legs, including flips; independent immutable values replace source aliases and incorrect field names |
| `computeAverageEntryPrices` | `source_weighted_mean`: actual absolute-weighted BigNumber operation order, 20dp HalfUp division and binary64 return, including negative zero |
| `breakUpOpeningTrades` | `source_break_up_opening_indices`: exact source previous-height split within the finite safe-height domain; returned indices preserve reference membership and empty groups |
| `calculateLIFOEntry` | `lifo_entry`: explicitly repaired outstanding-lot stack, prior-close consumption, partial lots and optional as-of event position; returns covered and uncovered quantities |
| `savePNL` | Immutable `ExecutionPnl` facts retained by event and participant; `pnl_history` / `pnl_summary`; same-height fills remain separate rather than overwriting one source key |
| `save`, `saveTrade` | Typed `insert`, `insert_execution`, atomic batch variants and canonical checkpoint bytes replace generic stringified-document writes/random UUIDs; source errors/upserts are characterized separately |
| `displayPositionHistory` | Returns queryable typed snapshots for a caller's renderer; no console Promise display or unawaited background operation |

Legacy static address matching is asymmetric. A query for `Alice` accepts
stored `Alice` or `alice`, while a query for `alice` accepts only stored
`alice`. It does not accept every case variation. The typed `AccountId`
domain is printable non-space ASCII, bounded to 90 bytes. Contract IDs are
nonzero `u32`; property IDs use the existing nonzero `PropertyId` type.
Those constraints deliberately exclude JavaScript number/string coercions,
arbitrary Unicode identities and malformed database records.

`entries()` and the `contract_entries*` methods merge both record forms into
ordered `HistoryRecordRef::{Legacy,Execution}` views. The exact-only query
counterparts are `contract_executions`,
`executions_for_address`, `executions_between_blocks` and
`liquidation_executions_at_block`. Existing predicate APIs returning
`&TradeRecord` retain that original record type; they do not truncate a Decimal
execution into an integer row. `first_trade_block` covers both record forms. Private
economic pool account IDs are valid reporting identities and are not mistaken
for externally authorized actors.

Rust query results always sort by `(block_height, event_index)`. The JS
queries, other than `getFirstTradeBlock`, return database row order. The
synthetic differential fixture supplies JS rows in the same chronological
order and inserts Rust rows in reverse order. Matching predicates are
characterized; arbitrary historical database insertion ordering is not a
parity claim. An additional exact token-address mode is a typed API feature.

## Position and PnL policy

`POSITION_HISTORY_POLICY` is `CanonicalPositionHistoryV1`. For an owner's signed
quantity `q` and execution delta `d`, replay computes `after = q + d`. Opposite
signs close `min(abs(q), abs(d))`; any remaining delta opens a new lot. A flip
closes the old episode and opens the tail at the same event, using distinct
category values. A flat position has no average; additions use a weighted
20-place HalfUp trade average; reductions preserve it; flips reset it to the
fill price. This reporting average is separate from settled mark basis.

`position_episodes` groups actual flat/flip lifecycle boundaries rather than
guessing them from a close in the previous block. `lifo_entry` consumes earlier
closes from newest outstanding lots, then quotes the requested absolute quantity
against the remaining stack. `(height, event_index)` bounds prevent lookahead.
Uncovered quantity is returned explicitly. Zero requests consume nothing.
LIFO cost is price-times-quantity in reporting units; it is not an alternative
calculation of protocol PnL, fees, funding or collateral payments.

`ExecutionPnl` separates booked signed value, actual collected loss atoms,
cancelled positive-claim atoms and cancelled prior-loss atoms. Negative booked
entries permit claim offsets/collections; positive entries permit loss offsets.
Their remaining negative atom totals must equal event unfunded losses, and
participant collections must sum to the event collection. Values are metadata
assertions unless projected from an independently checked economic owner.

`paid_claim_atoms` is retained at event level. It can pay older unrelated
claimants and is never attributed to the current buyer or seller. Participant
summaries deliberately omit invented per-owner payout/fee allocations. Legacy
rows with no PnL evidence produce no PnL rows rather than fabricated zero facts.

`source_weighted_mean` and `source_break_up_opening_indices` are the two exact
functional source helpers. Position/category/episode/LIFO/PnL storage behavior
uses the named repair above; it does not claim literal parity with source paths
that throw or return NaN.

## Exact event API and runtime projection

`ContractExecution` retains fill ID, height, event index, contract ID, buyer and
seller, positive Decimal quantity/price, participant PnL facts, retained fees,
collected losses, paid claims and unfunded losses. `ContractEventOrigin` is:

```text
Orders     { buy_txid, sell_txid }
ForcedBook { epoch, fill_index, maker_txid }
ForcedAmm  { epoch, fill_index, pool_id }
```

Forced epochs and pool IDs are not represented as fabricated external buyer or
seller transaction IDs. `transaction_references` exposes ordinary participant
order IDs and leaves that field absent for forced events while retaining their
complete typed origin. The history itself holds no Ledger.

The immutable `economic_reporting::EconomicHistory` adapter verifies and
borrows its source economic owner, then inserts a complete bounded event batch.
Mark trade entries supply owners, deltas and execution ordering; ordinary and
forced receipts supply their actual origin and fee facts. Nontrade mark/funding
events remain borrowed side queries. Unmarked states with receipts and no owner
evidence return `MissingOwnerEvidence`; owners are never inferred from hashes.
Regenerating reports from a reopened/rolled-back owner avoids a separately
mutable economic balance or independently committed history suffix.

## Identity, replay and rollback

`TradeId([u8; 32])` identifies a fill, independently of either order's
transaction ID. The caller must derive stable IDs and a unique event index
within each block. Multiple fills may retain the same buyer/seller transaction
IDs. A repeated identical record is idempotent; a reused ID with different
data or a second ID at an occupied event position returns an error. Single
inserts and failed batches leave the store unchanged. Batches apply atomically
using a private clone. Exact batches prevalidate all values, clone once, apply
all rows and verify replay/index consistency once; no per-event checkpoint or
full-history clone occurs inside the batch loop. Single-event conflicts and
capacity failures are preflighted before cloning.

`rollback_after(height)` removes records strictly above the retained height,
including their ID index entries, so replacement-chain rows can reuse the
removed positions and IDs. A retained height of zero preserves block zero;
`u64::MAX` removes nothing. The caller must coordinate history rollback with
the execution ledger and selected network/chain. This API does not choose an
authoritative branch and must not serve as a consensus chain adapter.

## Checkpoint format and limits

The store is limited to 65,536 total records. Existing-only checkpoints keep
**identical `TLTH\x01` bytes**. Their payload begins with that magic, followed
by a big-endian `u32` record count and chronological records. Each row holds
the full 32-byte fill ID, `u64` height, `u32` event index, two length-prefixed
ASCII accounts, two full transaction IDs, and one tagged economics record.
Token economics retain both ordered nonzero property IDs and two positive
`u128` atom amounts. Contract economics retain a positive `u32` contract ID,
positive `u64` quantity, positive `u128` rational numerator and denominator,
and a canonical zero/one liquidation byte. No floating-point conversion,
amount arithmetic or fraction reduction occurs.

When at least one exact event exists, canonical `TLTH\x02` contains a complete
length-prefixed v1 legacy section, chronological exact events, and a sorted
nonzero `(contract, owner, signed quantity)` capture. Decode replays quantities
from all records and requires that capture to match. It rejects duplicate IDs
or event positions across record forms, invalid origin tags, zero pool IDs,
invalid PnL allocations/totals, noncanonical decimal spellings, ordering,
counts, prefixes and trailing bytes. A v2 image with zero exact events rejects;
rolling back the last exact event selects the original v1 writer again.

Exact quantities, prices, booked PnL and source weighted-helper inputs have at
most 40 coefficient digits and 20 decimal places. Quantities/prices are positive;
participants must differ. Derived positions allow 64 coefficient digits. Amount
fields remain checked `u128` atoms. Decimal checkpoint fields are canonical and
at most 128 bytes. Record/account/decimal bounds keep valid encoded states under
the shared 64 MiB checkpoint cap. Bounded errors return without mutation.

The common codec adds a big-endian CRC32 and caps checkpoint input at 64 MiB.
The record and account limits keep an encoded store below that cap. Decoding
rejects unknown versions/kinds, noncanonical flags/order, duplicate IDs or
positions, excessive counts, invalid identities/economics, truncation, trailing
bytes and checksum damage. CRC32 detects accidental corruption; an attacker
can replace it. There is no state-root, network tag, chain anchor, signature
or authentication in the raw history checkpoint. Persistence bytes must be
bound to the caller's corresponding execution and network context.

`encode_checkpoint`/`decode_checkpoint` provide concrete persistence bytes.
Filesystem writes, atomic file replacement, crash durability, database
migrations, archival pagination and concurrent writers remain the service's
responsibility. Retention is explicit: rollback removes a suffix; reaching the
capacity limit returns an error rather than silently pruning records.

## Verification and remaining work

Run the Rust integration suite and the independent query fixture check:

```text
cargo test --frozen --test trade_history
node scripts/trade_history_vectors.cjs --check
node scripts/trade_history_vectors.cjs --check --source fixtures/legacy_trade_history_source.txt
node scripts/trade_history_vectors.cjs --check --source /path/to/tradelayer.js/src/tradeHistoryManager.js
```

The runner now always executes the complete archived unchanged source, checks
its normalized SHA and loads genuine archived BigNumber9.3.1. `--source` checks
the same pin against another checkout. No numeric stub or source rewrite is
used. The explicitly injected metadata DB boundary supports the source's actual
find/update/cursor calls; no database files, wallet or RPC are accessed.

The unchanged query fixture retains ten rows and 25 predicates; 24 run against
source (the extra exact token-address query is a typed extension).
`trade_history_method_vectors.tsv` adds 43 method scenes covering all 18 active
exports, 102 weighted means compared at binary64 bit level, and 103 opening
split cases compared by original row indices. `trade_history_quirks.tsv` retains
24 explicit source defects or dependency-boundary observations. Observation
JSON tags preserve undefined/nonfinite/negative-zero values; alias identity is
recorded independently. `--check` (also default) compares deterministic bytes;
`--write` explicitly regenerates all three fixture files.

The extra last-resort array case explicitly makes the second DB getter return
an array to exercise that source branch; a real DB handle is not an array and
returns an empty fallback. This is not historical database migration evidence.

Rust tests exercise the vectors, duplicate/collision handling, batch atomicity,
integer extremes, inclusive bounds, rollback/replay, full record capacity,
canonical round trips and adversarially checksummed invalid encodings.
They also cover fractional additions/reductions/flips/closes, independent
category legs, episodes, prior-close/as-of LIFO, private pool origins, PnL
offsets versus event-wide payouts, exact batch atomicity, v1 preservation,
v2-to-v1 rollback selection and forged derived positions with repaired CRC.

## Concrete source defects preserved as observations

- `loadTradeHistory` normally unwraps rows to flat trades, while downstream
  position/category methods dereference `trade.trade`. Normal composed calls
  can throw; specially wrapped input is characterized separately.
- `getTxIdsForContract` fails to forward contract ID and queries
  `contract-undefined`; `action` is unused.
- Categorization reads `buyerClosed`/`sellerClosed` although the active
  orderbook stores `buyerClose`/`sellerClose`. One object is reused across
  open/close arrays, corrupting flip quantities. Seller flips reuse the closed
  amount rather than `flipShort`. Self trades reuse that object for both sides.
- `trackPositionHistory` converts its average to Number then calls `.minus`,
  mixes `block`/`blockHeight`, never updates its previous-height variable and
  resets position after each nonzero-block snapshot. Source LIFO expects absent
  `tradedAmount`/`blockHeight` fields and returns NaN/undefined for ordinary input.
- Position history changes the loaded seller row's amount in place. Oracle
  details show local adapter-object mutation, not a source database write.
- `saveTrade` references an unimported `uuidv4` and rejects before updating.
  `savePNL` overwrites same-owner/contract/height values; generic save stores
  JSON text, catches/logs/rethrows DB errors. Display tables an unawaited Promise.
- Contract prefix regex interpolation is unescaped. Token query errors return
  a literal error string while equivalent contract query errors propagate.

Every active source method has a functional mapping or explicit broken-source
disposition. Arbitrary historical document migration, custom JS objects,
authenticated fill provenance, durable service wiring and historical-chain
acceptance remain separate work. A coherent reporting checkpoint is metadata
consistency, not proof that a trade or settlement was authorized.
