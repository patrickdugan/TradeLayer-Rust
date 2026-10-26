# Literal source contract fees and reached Insurance writes

`source_contract_fee_impl.rs` is included by `SourceContractRuntime`. It ports
the reached Tally fee queue/flush/accrual and Insurance instance/load/deposit/
event/snapshot methods from source commit
`dca0888b827c5bc28fa9189bc9e1d507f5c5670c`. The actual archived sources are
`fixtures/native_genesis/source/tally.js`, LF SHA256
`cfde9e5845f810bf62d9f1a07d4db4ea011b11d3d2bb63f041d39bea557cddef`, and
`fixtures/insurance_owner_review/insurance.js`, LF SHA256
`5abbe7934ca4c2f4180ed910989ae8f90d752bf9e17cb8944b8b49e33d03c39c`.

This runtime is an alternative to the conserving EconomicState. Its actual
source Tally collection/cache is the balance owner. The fee helper introduces
no second Ledger and does not repair the source into conserving fee policy.

## Queue identity, flush and module order

`source_queue_fee` implements queueFeeAccrual. Block resolution returns an
explicit non-null argument unchanged; otherwise it reads TrackHeight and
MaxProcessedHeight in order and applies the source truthy fallback. Source
database exceptions in that lookup produce null. Null/NaN-resolved blocks
fall back to immediate accrueFee. Buffered entries retain string block keys,
the first original propertyId, the effective string contractId and actual
eight-place BigNumber accumulation.

`source_update_fee_cache` separately preserves updateFeeCache's sixth-argument
block override. Direct queueFeeAccrual still uses its fourth argument, even
when extra arguments exist. `source_flush_fees` uses resolved block identity,
processes entries in insertion order and deletes the block only after every
accrual succeeds. `source_flush_all_fees` retains numeric block sorting and
Number conversion before flush.

The real captured CommonJS exports remain observable. The main-subgraph,
Types-first and VolumeIndex-first entries throw
`ContractRegistry.isNativeContract is not a function` on reached contract fee
flush. That synchronous failure occurs before the source `.catch` can handle
an asynchronous rejection. Tally-first loading reaches the genuine method.
An asynchronous native-query rejection instead falls back to false, as in
the source. No missing export receives a substitute successful answer.

## Fee rows, dust and Insurance

Accrual acquires the feeCache database before inspecting the amount. It truncates
raw satoshis toward zero and retains source spot/native/oracle routing:

| Source path | Literal destination |
| --- | --- |
| Direct spot, property other than 1 | Entire fee to value cache |
| Direct spot, property 1 | Integer-satoshi half to Insurance; exact remainder to stash |
| Native contract | Entire fee to value cache |
| Oracle contract | Integer-satoshi half to Insurance; exact remainder to stash |

A queued null contract becomes string `"1"`; a subsequent flush therefore
enters the contract path. It does not regain direct-accrual spot identity.
Fee-row reads and eight-place Number writes retain their actual document
arguments and order. Conversion dust uses the declared feeCache `get`/`put`
storage capability, with the source caught-read fallback, whole-satoshi callback
before fractional storage, and distinct spot/contract dust routes. These are
explicit storage adapter capabilities, not a claim about every production DB
implementation supporting that interface.

The runtime's Insurance state holds actual instances keyed by contract and
oracle suffix. A new instance loads before being published in the instance map.
Every deposit then reloads again, including zero deposits. Balances retain
property string identity and decimal strings truncated to eight places.
Deposits mutate the cached balance before inserting a real deposit event,
then obtain the actual configured RPC-clock boundary and save the source
snapshot using `{key}`. The clock read occurs even when a snapshot was supplied.

The original catch scopes remain: an Insurance event/snapshot failure can leave
cached mutations and earlier documents while contract accrual continues to its
stash write. A later fee-cache failure propagates after successful Insurance
writes. A failed flush retains its buffered entries; retry can deposit again.
Dust callbacks and their following put can likewise leave a real prior prefix.
No blanket rollback, fabricated Insurance result or conserving retry fence is
inserted into these source methods.

BigNumber arithmetic here admits finite Decimal representations, preserving
source rounding, division and Number/string boundaries. Unsupported numeric,
executable/prototype or resource domains return explicit local errors rather
than invented fee/Insurance amounts. This slice does not establish complete
JavaScript numeric-domain equivalence.

## Independent source checks and remaining boundary

`tests/source_contract_fees.rs` reuses only the environment builder of the
frozen genuine composed source harness. Its complete unchanged hash-pinned
modules, real CommonJS cache and genuine archived BigNumber 9.3.1 execute the
fee methods. Only the declared storage, clock and other imported boundaries
remain the same as that harness; no business outcome is supplied.

The 55 differential actions cover all four entry orders, queue identities and
retries, spot/native/oracle routing, odd/zero/negative satoshis, conversion dust,
source block resolution, sixth-argument plumbing, live Insurance instances,
events/snapshots, actual DB/RPC ordering and injected late failures. Every
comparison checks complete source documents and cache values, not just totals.
Fixtures remain attributed to archived BigNumber 9.3.1; the separate verified
9.1.2 corpus comparison is described in CONTRACT_EXECUTION_FIDELITY.md.

This reached fee/Insurance deposit graph does not implement buyback order
matching, withdrawals, Insurance payout/liquidation, global clearing, every
numeric domain, disk durability or authenticated L1 ingestion. Source-thrown
financial prefixes in this runtime remain distinct from atomic funded/admission
rollback in the conserving economic runner.
