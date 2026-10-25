# Deterministic trade history

`trade_history.rs` supplies a bounded offline history store for the P2
`tradeHistoryManager.js` roadmap item. It accepts typed, caller-supplied token
and contract fills, retains integer amounts and unreduced rational contract
prices, and provides immutable query results. It performs no matching,
settlement, transaction admission, address ownership checks or chain selection.

## Source scope and query behavior

The reference is `tradelayer.js` commit
`dca0888b827c5bc28fa9189bc9e1d507f5c5670c`,
`src/tradeHistoryManager.js`. Its LF-normalized Git-blob SHA-256 is
`670035f8153abda542c732deee0cf995fcd1e1e7c7b9f50464fce6ab1b5c1569`.

| Source function | Typed history behavior |
| --- | --- |
| `loadTradeHistory(contractId)` | `contract_trades` returns contract records; no database document-format migration |
| Instance `getTradeHistoryForAddress` | `contract_trades_for_address(..., AddressMatch::Exact)` compares either participant exactly |
| Static `getTokenTradeHistoryForAddress` | `token_trades_for_address(..., AddressMatch::LegacyOriginalOrLowercase)` accepts either token-pair ordering |
| Static `getContractTradeHistoryForAddress` | `contract_trades_for_address(..., AddressMatch::LegacyOriginalOrLowercase)` compares either participant |
| `getTradesForContractBetweenBlocks` | `contract_trades_between_blocks` includes both endpoints |
| `getLiquidationTradesForContractAtBlock` | `liquidation_trades_at_block` requires the exact block and typed `true` flag |
| `getFirstTradeBlock` | `first_trade_block` returns `Option<u64>`, including a valid block zero |

Legacy static address matching is asymmetric. A query for `Alice` accepts
stored `Alice` or `alice`, while a query for `alice` accepts only stored
`alice`. It does not accept every case variation. The typed `AccountId`
domain is printable non-space ASCII, bounded to 90 bytes. Contract IDs are
nonzero `u32`; property IDs use the existing nonzero `PropertyId` type.
Those constraints deliberately exclude JavaScript number/string coercions,
arbitrary Unicode identities and malformed database records.

Rust query results always sort by `(block_height, event_index)`. The JS
queries, other than `getFirstTradeBlock`, return database row order. The
synthetic differential fixture supplies JS rows in the same chronological
order and inserts Rust rows in reverse order. Matching predicates are
characterized; arbitrary historical database insertion ordering is not a
parity claim. An additional exact token-address mode is a typed API feature.

## Identity, replay and rollback

`TradeId([u8; 32])` identifies a fill, independently of either order's
transaction ID. The caller must derive stable IDs and a unique event index
within each block. Multiple fills may retain the same buyer/seller transaction
IDs. A repeated identical record is idempotent; a reused ID with different
data or a second ID at an occupied event position returns an error. Single
inserts and failed batches leave the store unchanged. Batches apply atomically
using a private clone.

`rollback_after(height)` removes records strictly above the retained height,
including their ID index entries, so replacement-chain rows can reuse the
removed positions and IDs. A retained height of zero preserves block zero;
`u64::MAX` removes nothing. The caller must coordinate history rollback with
the execution ledger and selected network/chain. This API does not choose an
authoritative branch and must not serve as a consensus chain adapter.

## Checkpoint format and limits

The store is limited to 65,536 records. Checkpoints begin with `TLTH\x01`,
then a big-endian `u32` record count and chronological records. Each row holds
the full 32-byte fill ID, `u64` height, `u32` event index, two length-prefixed
ASCII accounts, two full transaction IDs, and one tagged economics record.
Token economics retain both ordered nonzero property IDs and two positive
`u128` atom amounts. Contract economics retain a positive `u32` contract ID,
positive `u64` quantity, positive `u128` rational numerator and denominator,
and a canonical zero/one liquidation byte. No floating-point conversion,
amount arithmetic or fraction reduction occurs.

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

The archived source mode used by CI checks the normalized source hash, evaluates the
unmodified pinned module in a VM with an in-memory database stub, and executes
24 queries against ten synthetic rows. The fixture has 25 queries; the extra
one covers the exact token-address API. No database files, wallet, RPC or
legacy dependency tree are loaded. The source-independent mode regenerates
the explicitly scoped JavaScript predicate projection and checks fixture
drift. Synthetic records are not historical-chain acceptance evidence.

Rust tests exercise the vectors, duplicate/collision handling, batch atomicity,
integer extremes, inclusive bounds, rollback/replay, full record capacity,
canonical round trips and adversarially checksummed invalid encodings.

This is supporting-history coverage, not a complete port of the legacy file.
`saveTrade` uses a random UUID function that is not imported in the pinned
source; Rust instead requires stable fill identities. Legacy positional
accounting, `getTxIdsForContract`, categorization, LIFO, average entry, realized
PnL and `savePNL` are not implemented here. The source mixes flat/nested row
shapes, mutates amounts in position queries, aliases categorization objects,
and calls BigNumber methods after converting an entry price to a number.
Reproducing those behaviors would require a separate explicit compatibility
or corrected-accounting specification. Production storage adapters and
historical migration/acceptance vectors are also still outstanding.
