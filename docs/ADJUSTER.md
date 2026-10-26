# Tick and minimum-size adjustment metadata

`src/adjuster.rs` ports the complete finite calculator behavior of the pinned
`src/adjuster.js`. `TickSizeAdjuster` owns bounded state, an action journal and
ordered events. Its `AtomicMetadataV1` policy commits those three together.
It owns no Ledger, performs no database I/O and does not change orderbook tick
acceptance or contract size rules. A future combined owner must choose which
contract uses an instance and commit any resulting economic policy explicitly.

The complete unchanged source, including its top-level example, is archived as
`fixtures/legacy_adjuster_source.txt`, at commit
`dca0888b827c5bc28fa9189bc9e1d507f5c5670c`. Its LF SHA256 is
`b6eea50581a9cc19f2296bae180b528c0116900a73662e5ce9a8ce762c36f77c`.
The oracle also executes complete unchanged `fixtures/legacy_db_source.txt`,
LF SHA256 `b92a05fee6652494b72bf6893d9633850c697be773e88b24d62798a8bc8f5e38`.
Datastore and client adapters prevent disk/RPC access; they do not replace the
database's initialization, category selection or async method behavior.

## Complete method map

| Active source member | Typed implementation and concrete behavior |
| --- | --- |
| constructor | `new`/`default`: tick `0.1`, minimum `1`, threshold `2`, multiplier `10`, interval `3`, counter `0` |
| `adjustTickSize(notionalValue)` | `adjust_tick_size(notional, time_millis)`: binary64 `notional >= threshold * minimum`; increment the qualifying counter, double/reset on every third qualifying call, then multiply tick by ten; below threshold resets only the counter |
| `increaseNotionalSize()` | `increase_notional_size(time_millis)`: multiply minimum by ten and emit its event; preserve tick and counter |
| `saveAdjustmentEvent(type,value)` | `save_adjustment_event(type,value,time_millis)`: append a bounded named finite observation without changing calculator state; no assertion that the observation changes a real orderbook |

On the third qualifying call, `DoubleTickSize` records the doubled intermediate
tick **before** `IncreaseTickSize` records the subsequent tenfold tick. Starting
from the constructor, qualifying calls produce ticks `1`, `10`, `200`, and events
`Increase(1)`, `Increase(10)`, `Double(20)`, `Increase(200)` in that order.
Increasing the minimum does not clear the qualifying counter. The next notional
is compared with the newly increased threshold, and a nonqualifying call then
clears it. Finite negative notionals and negative zero are supported reset inputs.

All calculator operations use native IEEE-754 binary64 in source order. Decimal
arithmetic or an algebraically collapsed multiplier would change this domain.
Finite minimum size can approach `1e308`; `2 * minimum` then becomes positive
Infinity. Any finite notional fails that comparison, so the calculator resets
the counter and returns its unchanged finite tick. This harmless intermediate
overflow is supported. An operation that makes tick, minimum or a newly emitted
numeric event nonfinite returns `ArithmeticOverflow` and publishes nothing.

Every method takes explicit deterministic event time. Zero and decreasing times
are valid; event `sequence` defines order. Supported milliseconds range from zero
through `8_640_000_000_000_000`, the nonnegative finite JavaScript Date range.
The module never reads a wall clock or treats a timestamp as authorization.

Typed returns are useful local values: adjustment returns the tick, increasing
minimum returns the new minimum, and direct save returns the event. Source
`increaseNotionalSize` returns `undefined`, and source direct save returns a
Promise resolving to the database's return value or rejecting on failure.

## Actual source persistence failures and example

Source calls `db.getDatabase("contractAdjustments").updateAsync(...)` without
awaiting `getDatabase`. Actual `db.js` defines an **async** getter, so the receiver
is a Promise and `updateAsync` is not a function. The generated fixture records
the actual resulting error. Independently, initialization creates 32 categories
and omits `contractAdjustments`; even an awaited lookup returns `undefined`.
Fixing only the missing await therefore would not supply a datastore.

`adjustTickSize` and `increaseNotionalSize` ignore the Promise returned by save.
They publish their in-memory updates even when each write rejects. The unchanged
top-level example ends with tick `4_000_000`, minimum `10`, counter `1`, prints
nine messages, and attempts ten event saves. With actual `db.js`, all ten Promises
reject and no Datastore update is reached. The file does not export its class.
The oracle retrieves its VM lexical binding after executing the whole file.

The oracle uses a Promise observation adapter to collect rejected fire-and-forget
calls without triggering the host Node process's unhandled-rejection shutdown.
That harness observation is not source error handling. A separate explicitly
synchronous DB-shaped recording adapter lets the unchanged source successfully
record its intended update arguments and deterministic Date values. Successful
recording-adapter observations do not claim the actual DB path works.

`AtomicMetadataV1` deliberately repairs this behavior: a single method preflights
input and history limits, stages all resulting state/events, then commits them
together. A third call requiring two events fails completely if only one event
slot remains. Overflow, invalid time, nonfinite input and exhausted action/event
capacity leave state, history and events identical to their prior values.
No successful write to the broken source database is inferred.

## Replay and checkpoint

Queries expose immutable `state`, `actions`, `events` and the policy.
`checkpoint_bytes` writes canonical `TLAD` version 1 with all six state fields,
the complete ordered action journal and every derived event. Numeric values use
their exact finite binary64 bit patterns, including negative-zero direct event
values. `from_checkpoint_bytes` enforces limits while parsing, checks CRC32,
replays from the exact constructor, compares state/event bit patterns and order,
and requires byte-for-byte canonical reencoding. The checksum detects damage;
it does not authenticate a contract, actor or block.

Resource limits are 8,192 actions, 4,096 events, 64 UTF-8 bytes per event name and
2 MiB per checkpoint. Empty, Unicode and control-containing names within that
byte bound are valid finite string observations, as in the source. Direct save
can use the same name as an automatic event; its distinct retained action makes
clear that it did not execute a calculator transition. Below-threshold calls
consume action capacity even though they emit no event. These are implementation
bounds, not inferred historical consensus rules.

The typed boundary rejects NaN/Infinity numeric inputs, values coerced from JS
strings/booleans/arrays/objects/BigInts, arbitrary event objects, negative dates,
caller overwrites of constructor fields and prototype manipulation. The source
accepts or behaves differently on several of those inputs: NaN resets, positive
Infinity qualifies, numeric strings and `[2]` qualify, and Symbol comparison
throws. Source permits arbitrary direct event types/values, including a numeric
type and Infinity. Those observations are recorded separately from finite Rust
parity. Source tick overflow on qualifying call 282 and minimum overflow on
increase 309 are observed nonfinite mutations, which the functional policy
rejects atomically.

## Verification

```text
node scripts/adjuster_vectors.cjs --check
node scripts/adjuster_vectors.cjs --source PATH/adjuster.js --check
cargo test --test adjuster --frozen
```

`--write` regenerates the fixture; default and `--check` verify it. The complete
source oracle records **641 finite constructor/state/action rows across ten
traces**, plus **19 separate source-only observations**. Every finite row is
consumed by Rust tests, including event order/value bits and times. Tests also
cover reset/retry, exact event/action limits, mutation overflow versus harmless
threshold Infinity, deterministic nonmonotonic time, direct-event independence,
checkpoint restart, every truncated prefix and valid-CRC forgeries of state,
action, event, time, version and signed zero. Combined core checks and repository
integration are owned by the parent batch; finite synthetic source vectors do
not establish historical replay or live consensus acceptance.
