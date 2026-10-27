# Literal insurance lifecycle in the shared source runtime

`source_insurance_lifecycle_impl.rs` implements the complete lifecycle methods
of the unchanged `fixtures/insurance_owner_review/insurance.js`, pinned to
`tradelayer.js@dca0888b827c5bc28fa9189bc9e1d507f5c5670c`, SHA-256
`5abbe7934ca4c2f4180ed910989ae8f90d752bf9e17cb8944b8b49e33d03c39c`.
It includes `getInstance`, `loadFromSnapshot`, `deposit`, `withdraw`,
`saveSnapshot`, `getSnapshot`, `recordEvent`, `liquidate`, `getPayouts`,
`calcPayout`, `getTotalBalanceForProperty`, `updateInsuranceSummary`, and
`getInsuranceFundBalance`. The existing fee include supplies the shared
instance/load/deposit logic. No insurance result is supplied by a fixture or
external answer recorder.

All insurance singleton state lives in `ContractCore.insurance`; all persisted
snapshots, payout records, standard events, and summary rows use the same
`insurance` collection in the runtime. Liquidation calls the same actual Tally
owner used by source settlement. The literal source runtime is an alternative
to the conserving economic owner, with one spendable balance authority.

The runtime dependency namespace is `Insurance`. Static methods take their
original source arguments. `Insurance.getInstance(contractId, oracle?)` returns
the adapter handle `{sourceInsuranceInstance: key}`; instance methods receive
that handle first, followed by the exact original arguments. The handle
identifies the live cached instance and supplies no balances or verdicts.
Original ID and oracle values remain on the instance; keys collide as the
source template string and oracle truthiness dictate. `calcPayout` requires a
`SourceValue::BigNumber` loss, preserving the source's missing `.abs()` method
error for plain numbers and strings.

The following source behavior is deliberate:

- Deposit reloads the latest snapshot even for ignored invalid or zero input.
  Valid positive deposits use tolerant string property matching and truncate
  to eight places. Existing malformed/nonfinite balances propagate source
  BigNumber NaN or infinity strings.
- Withdrawal neither reloads nor normalizes property IDs. It uses strict ID
  equality, JavaScript relational comparison, and Number subtraction. Two
  strings can compare lexically; a negative withdrawal can increase a balance.
- A failed event or snapshot write leaves prior cache changes. Retrying a
  withdrawal or payout can debit the cache again. A new deposit reloads the DB
  snapshot instead.
- `getSnapshot` ignores the oracle suffix, even for an oracle instance.
  `saveSnapshot` always calls the block-count boundary, including when supplied
  an explicit snapshot. The explicit snapshot does not refresh the cache.
- Payout sums and consumes balances across all properties in array order. It
  preserves the loss sign when not capped; a capped loss uses the total fund
  balance directly. Each debit rounds independently to eight places. A reached
  deficit calls the source's missing `handleDeficit` method and throws after
  preceding cache changes.
- Payout writes its dedicated record, then records a second event without a
  block argument, producing the source `event-undefined` key. It then snapshots
  and returns a BigNumber rounded to eight places.
- Liquidation reads the unsuffixed snapshot and reduces amounts using ordinary
  JavaScript `+`. String amounts can concatenate before division by two. It
  credits Tally using the contract ID as the property ID, then reaches the
  missing static `InsuranceFund.updateFeeCache` method. Its source catch hides
  that failure, retaining prior Tally effects and leaving the fund undebited.
  Retrying can credit again.
- Total and summary reads scan all DB documents with a truthy balances field.
  They can count historical or duplicate documents. Summary uses a plain
  JavaScript object, including its inherited-property `.plus` failure. The
  legacy fund balance getter instead queries `_id` and reads `amount`.

The BigNumber constructor comes from the independently verified source helper,
with finite decimal, signed zero, NaN, and infinities retained. Decimal resource
limits, arbitrary custom coercion functions or object identity graphs, and
unsupported regular-expression metacharacters remain explicit local domain
errors. `getPayouts` emits the exact source regex/range query; the DB adapter
supports its anchored literal prefix domain instead of inventing wildcard
matches. A local domain or capacity error rolls back the coupled runtime;
source exceptions retain their actual partial effects.

The focused test executes 144 observations against the complete unchanged insurance module and actual
Tally module in the pinned CommonJS realm. The storage/clock/UUID adapters are
declared boundaries. It compares every outcome, full DB rows, complete
insurance and Tally caches, and ordered DB/RPC/UUID effects after each action,
including failure/retry paths. Reference results remain observations and are
never inputs to the Rust implementation. The same corpus is checked with the
archived genuine BigNumber 9.3.1 and the verified source-lock 9.1.2 preload;
this demonstrates corpus equality, not blanket version equivalence.

Two genuine withdrawal/payout failure-and-retry scenes also restore from the
complete original-command journal after each of ten observations. The test
retains the expected replay head independently, compares the whole actual owner
state and canonical checkpoint bytes, and continues the retry from the restored
GlobalClearingV2 runtime.
