# Atomic economic execution with one ledger

`EconomicState` combines the existing `JointProtocolState`, contract-order metadata, position metadata, IOU metadata, and fee metadata. Joint retains the sole mutable token `Ledger` inside its existing `SpotExchange`. Order reserves, position margin, collected losses, retained fees, and pending insurance fees are actual balances in derived ledger escrows; metadata is reconciled against those balances before publishing a block. The API exposes immutable views of these components.

The execution policy is explicitly `SettlementPolicy::ConservingV1`. It is a functioning caller-driven settlement policy with identified repairs, rather than a claim that the broken or unfunded paths in the pinned JavaScript have become historical consensus rules. [CONTRACT_ORDERBOOK.md](CONTRACT_ORDERBOOK.md), [MARGIN_MAP.md](MARGIN_MAP.md), [IOU.md](IOU.md), [TALLY.md](TALLY.md), and [JOINT_ECONOMIC_BRIDGE.md](JOINT_ECONOMIC_BRIDGE.md) describe the component source references and boundaries.

## Configuration and admission

The optional [accepted-contract mode](ECONOMIC_CONTRACTS.md) binds the selected
oracle perpetual to a prior type-16 creation and its actual Registry dependencies.
It requires zero in the legacy fixed-margin field and derives initial margin
from that definition at each order, fill, position-average or mark context.
Fees use the source catalog's valued notional at the fill; PnL retains raw
notional. Its complete input history reexecutes from an independently committed
Joint bootstrap, and TLEC version 6 persists the combined state. The descriptions
of configured fixed margin below apply to the original mode with binding absent.
Orders require a prior accepted reference and the source's strict leverage-price
band at their input position. Native/dated contracts, full country/whitelist admission, and
authenticated actors remain unfinished requirements.

`EconomicConfig` explicitly supplies network, one contract, one collateral property, inverse/linear flag, notional value, initial margin per contract, native/oracle fee route, and the repair policy. Notional must be positive; configured initial margin must be positive in legacy mode and zero in accepted-contract mode. The contract ID must match the fee route; Spot is not a contract runtime route. Configured decimals are bounded to 40 coefficient digits and 20 decimal places. Accepted-contract mode checks the actual accepted definition and Registry dependencies against the independent configuration; legacy mode retains its asserted classification and fixed margin. Actors, transaction IDs, input provenance and blocks remain unverified in both modes. Neither supplied configuration nor a locally accepted oracle publication authenticates its issuer or price.

`from_joint_unverified` accepts an unfinalized Joint bootstrap state whose collateral exists and whose components pass their existing checks. It starts with no economic history. The first block is height 1, and subsequent heights must be contiguous. Registry activation uses the existing Joint checks: activating an opcode in a block does not authorize using it in that same block.

The economic inputs include type-18 contract orders and type-6 owner cancellations, with type-16 creation in accepted-contract mode and type-10 LP operations in configured AMM mode. Ordinary implemented Joint inputs still use their original dispatch. Optional configured [mark epochs](ECONOMIC_MARKS.md) settle fresh admitted-oracle observations through the same Ledger. Optional [liquidation epochs](ECONOMIC_LIQUIDATION.md) close deficient positions against actual funded resting makers, with protected system identities rather than fabricated external orders. Options, ADL, channel transfer, insurance withdrawal and unsupported economic opcodes remain separate work. Type-3 UTXO trades and settlement opcodes requiring authentication retain their existing denial gates.

## Block transaction and actual custody

`apply_block_unverified` validates the starting state, then works on a private complete-state clone. It handles ordinary Joint commands and cancellation inputs in the supplied sequence while queuing contract orders. Ready contract orders are subsequently processed in the component's source-derived height/sender/txid order. Each order's admission reads the resulting current position, funds its reserve, and settles all its matches before the next order is admitted. Thus an ordinary send appearing in the input list is handled during the first phase, before the queued matching phase; this is an explicit batch execution boundary.

The optional [sequential source runner](CONSENSUS_SOURCE.md) replaces that typed entry point with `apply_source_unverified_block`. It recomputes [Types dispatch and mapping](TYPES_SOURCE.md) and all called [source validity rules](VALIDITY_SOURCE.md) using [actual pending-owner reads](ECONOMIC_VALIDITY.md). Successful represented candidates publish atomically into the pending block; rejected, unavailable, unsupported and repeated candidates retain their raw input and outcome in the same owner. Orders and the existing economic end phases finalize once per block. The typed entry point is denied while source mode is enabled, so it cannot bypass this audit or its replay set. Missing external dependencies remain explicit failures; source conformance and actual-owner reads do not authenticate caller-supplied inputs.

Every external input has one shared Joint replay identity. Contract order IDs and cancellation IDs cannot overlap ordinary, registry, spot, or bootstrap ledger inputs. Internal transfers use domain-separated fingerprints with a monotonic sequence. Economic history is permanently fenced from the old standalone Spot/Joint formats, even when an event moves zero atoms.

Escrow account identities bind network, genesis, contract, collateral, role, and role-specific key. They use the reserved `~econ:` family. The roles are per-order reserve, per-owner margin, retained fees, collected-loss pool, and insurance pending. An input owner cannot select another reserved family as an economic destination. The final verifier requires the complete map of economic ledger balances to equal the derived metadata exactly; unexplained reserved balances fail validation.

An opening reserve is `ceil(quantity * configured initial margin * 1e8)` atoms. The component's reducing/flip reserve quantity and matching Number boundaries remain explicit metadata inputs. Partial fills remove the difference between the old rounded reserve and the rounded remaining reserve. Those real atoms move to the owner's margin escrow, then margin is reconciled to the current absolute position. A decrease releases excess atoms to the owner; an increase must debit actual available funds. Ceil rounding cannot release more reserve than was funded.

Matches settle before self-cross residual reserve release. A single placement can partially fill an order and then remove its remaining portion as a self-cross. Releasing that reserve before applying the earlier fill would lose the reserve metadata needed by settlement. The regression in `tests/economic_review.rs` exercises Bob sell 1, Alice sell 1, then Alice buy 2 at the same height: one fill settles, only the buy remainder is released, and Alice's separate resting sell keeps its own funded reserve.

After processing, the owner finishes the book height, refreshes position projections, finalizes the Joint block, verifies every component and custody equation, and verifies that the entire state can be serialized. A late fee debit, arithmetic error, capacity failure, invalid replay marker, or snapshot failure discards the entire clone, including earlier ordinary commands and ledger transfers.

## Position transitions, fees, and realized claims

Each fill changes the signed contract count on both sides by the same quantity in opposite directions. Increasing a position uses a quantity-weighted entry price, with shared exact Decimal arithmetic and 20dp HALF_UP division. A partial reduction retains the old entry price. A full close clears it. A flip realizes PnL only for the closed quantity and sets the remaining opposite position's entry to the fill price. Net signed contracts across all positions must remain zero.

Realized PnL uses the shared source `price_delta`: linear `(exit − entry) * closed quantity * notional`, or inverse `(1/entry − 1/exit) * closed quantity * notional`, with 20dp reciprocal division, the old long/short sign, and final 8dp HALF_UP. The policy's coherent Decimal position transitions should not be substituted for all source Number/coercion observations in the standalone margin module.

The runtime calls the authoritative `tally::calculate_match_fees` at the fill price, with the filled quantity, supplied notional and inverse flag, and the book's maker flags. Channel is false in this slice. The source helper applies 5bps, floors raw fee atoms, increases odd results to an even count, and either splits the debit across two nonmakers or pays a half-taker rebate to the maker. Both actual debits are collected before any rebate is paid. Retained atoms are classified by `FeeBook`; an oracle insurance allocation moves from the fee escrow into the actual pending-insurance escrow. Native contract fees remain in VALUE. No fee metadata operation creates token supply.

Settlement ordering is deliberate: position/reserve and target margin reconciliation, fee collection/rebate/routing, negative realized PnL collection, positive realized claim creation, then claim payment. Positive realized PnL is first a nonspendable claim, even if it is paid later in the same match. A new position or flip cannot rely on a later realized-profit payout to meet its earlier margin/fee debit.

Negative realized PnL is collected from the owner's actual available atoms first, then from that owner's actual margin escrow. Payment stops at the funds present. Collected atoms enter the loss pool. The unpaid portion is recorded in `unfunded_loss_atoms`; it is neither spendable pool balance nor an automatic debit against a future wallet deposit. Consuming margin for a loss can leave an open position below its configured initial target. With optional liquidation disabled, automatic maintenance-margin enforcement and liquidation remain outside this slice; a later position transition still must fund its explicit reconciliation.

All positive realized amounts create or increase integer-atom IOU claims. `ConservingV1` pays those claims in the existing claim map's canonical order, capped at the remaining actual loss-pool balance. Ordinary string addresses retain insertion order; array-index-like addresses follow the source object-key numeric ordering. It is therefore not universally chronological FIFO. Every payout reduces both the real pool and the outstanding claim by the same atoms. Repeated matches cannot reuse a former same-block pool.

The policy intentionally replaces the source's proportional payout and reusable same-block loss-budget behavior with sequential, actual-pool-capped payment. It also creates claims for the complete positive realized amount, instead of treating all legacy margin/IOU imbalance helpers as funded transfers. This is an explicit allocation/fairness change: earlier canonical claimants can be fully paid while later ones remain unpaid. The standalone IOU module continues to characterize source pro-rata arithmetic separately.

The principal IOU invariant is:

```text
signed IOU bucket = actual remaining loss-pool tokens − outstanding claim tokens
```

Trade receipts retain fill IDs, quantity/price, each side's realized PnL, retained fee atoms, actual collected losses, paid claims, and unfunded losses. Receipt totals reconcile collected minus paid to the remaining pool and reconcile unpaid losses to the aggregate unfunded counter. The verifier rebuilds the complete expected FeeBook from the ordered retained-fee receipts and configured route, requiring exact metadata/journal equality and the corresponding pending-insurance total. Receipts and local fingerprints are an accounting record, not proof of the actors' or blocks' authenticity.

`Position.margin` and `Position.iou_claim` are refreshed metadata projections. Available funds are read from the sole Ledger, so the copied position `available` field stays zero. This slice also leaves unsupported unrealized PnL, liquidation/bankruptcy prices, old marks, option positions, source delta caches, and block-position flags unset. Those fields cannot silently become a second balance authority.

## Complete checkpoint, rollback, and durable ownership

TLEC v1 stores the explicit economic config, the crate-private complete Joint payload (including its one Ledger), full orderbook metadata/replay state, insertion-ordered positions, IOU state, FeeBook rows/pending entries/journal, order reserves, margin projections, cancellation IDs, loss and insurance counters, receipts, and internal transfer sequence. Typed parsers check bounds, canonical decimal text and ordering, nested structure, full consumption, and CRC. Restore requires independent expected config, administrator, and permitted spot properties, then runs the cross-component verifier and requires byte-for-byte canonical reencoding. No hidden margin index or unsupported option field may disappear during serialization.

`EconomicChain` adds an independently configured genesis anchor, exact parent/tip checks, and bounded complete before-state frames. A rollback restores the entire economic state, including actual custody, positions, orders, replay identities, IOUs, fees, receipts, and counters. It does not retain a ledger-only undo path. TLEH v1 includes the live TLEC state and every retained before-state; each retained frame is structurally decoded against the same independent policy. Internal history linkage does not prove that the supplied blocks were valid.

Source mode selects TLECv7, retaining the independently committed pristine Joint bootstrap, full raw candidate history, headers, execution order and accepted/rejected audit. Restore reruns Types, actual-owner dependency reads, mapping and handlers rather than trusting stored validity booleans or dependency responses. Retained before-states must match the exact current raw-history prefix, preventing rollback to a different history sharing caller-asserted header IDs. With source mode disabled, all prior v1-v6 encodings remain unchanged. The [TLSEv1 producer](ECONOMIC_STATE_ORACLE.md) can export this complete v7 state; an external authenticated transition witness remains a separate gate.

`DurableEconomicChain` owns this complete chain together with its `Journal` in the network's clearing namespace. Initialization rejects existing content. Restoration uses `open_existing`, the one expected state key, independent policy and exact `EconomicHead { sequence, tip, commitment }`, with explicit recovery policy. The head commitment covers the full retained checkpoint and its sequence distinguishes rollback/reapply at the same height. Apply and rollback stage the complete chain, serialize it, commit the OS-synced journal frame, then publish cached state. An uncertain journal write poisons access to the cached chain through the storage health gate. Independent checksum/commitment equality supplies local integrity/freshness, not chain authentication.

Limits are explicit: 4,096 block inputs, 8,192 trade receipts, 4,096 positions, 65,536 internal transfer legs, component replay/order limits, bounded rollback depth, and the shared 64 MiB checkpoint cap. These bounds can reject an otherwise arithmetically valid research workload. They do not imply production throughput or historical protocol limits.

Fee verification now builds the expected journal once through the private batch accrual API, while retaining exact row/journal/insurance equality. Full chain validation still repeatedly decodes and canonically reencodes every retained frame. These checks preserve recovery invariants but require representative workload and peak-memory measurements before deployment. Fewer repeated checks at proven immutable boundaries remain a follow-up candidate; no end-to-end speedup is claimed.

## Remaining integration gates

Optional [funding epochs](ECONOMIC_FUNDING.md) extend this owner through the same Ledger, with complete source observations, explicit conserving allocation and TLJE/TLEC v2 recovery. The disabled default retains the original v1 encoding. Optional [mark epochs](ECONOMIC_MARKS.md) add separate exact settled basis and signed subatom carry, nominal default/reversal offsets and TLEC v3 recovery. Marking disabled retains prior v1/v2 bytes. The earlier average-entry close rules described above apply when marking is disabled; enabled marking uses its persisted basis while retaining the trade average as separate metadata. Funding remains independent of that close basis.

The pending-insurance escrow is a real funded balance, but it is not the insurance module's lifecycle or an authenticated insurance deposit. The actual live insurance handoff, fund accounting, withdrawals, and receipt/reorg linkage must be integrated before that counter can be presented as an insurance balance. This owner does not call a substitute insurance allocator or debit a second ledger.

Authenticated actor/transaction/block provenance, parent-UTXO and script verification, historical activation and contract-registry configuration, authoritative marks/oracles, historical bootstrap supply, representative historical replay, full native/multicontract liquidation and ADL, funded buybacks, channel/options/vesting integration, and source-versus-repair promotion remain separate gates. The source rules and supported sequential execution are integrated subsets; they do not complete the missing dependency owners or historical acceptance. The APIs keep their `unverified` names and do not manufacture authenticated settlement capabilities. The external insurance and txindex/UTXO/script/performance lanes retain their existing ownership.

Optional [funded liquidation](ECONOMIC_LIQUIDATION.md) uses separate system epochs,
actual owner reserve refunds and margin top-ups, guarded firm resting makers,
release-only distressed margin, and explicitly capped actual fees/rebates.
It records initial captured maintenance calls in canonical owner order; new
maker deficits wait for another fresh mark. Version4 retains complete ordinary
book input/placement frames plus forced fills and known margin replay. The
4096-block frame limit and shared record/checkpoint bounds require an explicit
future compaction design for indefinite operation. This is an unverified
conserving closeout policy; full AMM market making, insurance/ADL, complete wallet provenance,
contract authority and historical acceptance remain open.

Optional [funded contract AMM execution](ECONOMIC_AMM.md) adds a private pool
counterparty, actual LP collateral deposits and nontransferable receipt units,
exact quiescent NAV mint/redemption, separately funded bid/ask reserves and
AMM-first guarded closeouts. Pool inventory, basis, funding, IOUs and margin
stay in these same economic structures. The v5 checkpoint includes every
capital input, quote epoch, pool cash leg and private fill; replay derives
cash movements from the typed operations and rejects balanced fictional legs.
External Joint actors and address endpoints retain the reserved-account fence,
including transactions whose net balance change would be zero.

This named policy preserves source Number quote prices while repairing the
source capital/inventory conflation, virtual fee exemptions and reversed inverse
safe-price predicate. Ordinary AMM market making, live-risk LP operations,
transferable LP property issuance, token pairs, options, insurance/ADL and
authenticated historical acceptance remain open. Versions 1–4 retain their
existing disabled-AMM bytes, including an active v4 closeout fingerprint.

Legacy mode retains its asserted fixed per-contract initial margin and fee
notional. Accepted-contract mode already binds actual type-16 definitions and
Registry dependencies, then uses source order-price reserve, execution-price,
average-price maintenance, mark-price forced sizing and collateral-valued fee
contexts; marks/PnL retain raw contract notional. TLECv6 replays the complete
typed lifecycle from its independently committed bootstrap, and optional v7
adds raw source validation and rejected-input replay. Native/dated/multicontract
behavior, full country/whitelist dependencies, authoritative bootstrap and
publication provenance, and authenticated historical replay remain open.
