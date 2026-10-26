# Exact mark settlement basis

`mark_basis.rs` implements the named `ConservingMarkBasisV1` repair. A pure
transition computes incremental PnL, signed whole collateral atoms and the next
position. It owns no Ledger, account registry, oracle choice, scheduler, replay
set, trade average, liquidation engine or authority to settle balances.

The economic owner must keep contract configuration immutable, stage atom
movements and basis updates together, and preserve each owner's carry even when
the position becomes flat. A caller must never pay `pnl` and `atoms` separately:
`pnl` is the exact mathematical increment; `atoms` is its payable projection
after combining the old carry. Funding remains a separate economic obligation.

## Basis and lifecycle

Let signed contracts be `q`, raw contract notional be positive `N`, and define
the price coordinate as:

| Contract | Coordinate `U(p)` |
| --- | --- |
| Linear | `p` |
| Inverse | `-round20HalfUp(1 / p)` |

The reciprocal precision and operation order follow genuine BigNumber 9.3.1
defaults in the pinned source. Each reciprocal is rounded before subtraction
and multiplication. Very large positive prices can have a zero reciprocal at
this precision; a nonflat inverse position with zero basis is consequently
valid. This is not an uninitialized linear basis.

`entry_basis = B` is the **signed aggregate** coordinate exposure. It is not
an average entry price. It is an exact rational number, so reducing one third
of a mixed basis does not round a weighted average or lose the final remainder.

| Action | Exact transition |
| --- | --- |
| Open or add in the same direction by signed `d` | `B += d * U(exec)`; no PnL |
| Mark | `PnL = N * (q * U(mark) - B)`; `B = q * U(mark)` |
| Reduce | Let `c` be closed quantity signed like the old position, and `f = abs(c) / abs(q)`. `PnL = N * (c * U(exec) - B * f)`; retain `B * (1-f)` |
| Close fully | Apply reduce to all contracts; set quantity and basis to zero |
| Flip | Close all old contracts; new opposite tail starts at execution coordinate |

The trade average remains the economic owner's separate historical field.
Marks must not overwrite it, and closes after marks must use this basis rather
than pay the original trade entry again. Identical consecutive marks generate
zero incremental PnL. Event admission and duplicate trade rejection still
belong to the economic owner.

For a fully closed history, exact cumulative PnL equals
`-N * sum(trade_delta * U(execution_price))`, independently of inserted marks.
The tests exercise that identity with fractional additions, partial closes,
flips and both contract types.

## Signed carry and canonical fractions

Every transition forms `total = previous_carry + exact_incremental_PnL`.
`atoms` is `total * 100000000` truncated toward zero, represented by
`SignedAtoms { negative, atoms: u128 }`. Zero output has `negative = false`.
The next carry is exactly `total - atoms / 100000000`, with magnitude strictly
less than one atom. Carry can have the opposite sign from the latest PnL and
survives a flat position and a later reopening. Independent q8 rounding at
each hop is deliberately replaced; the change is not claimed as source parity.

`ExactRatio` stores canonical integer Decimals: a signed numerator and positive
denominator with greatest common divisor one. Zero is only `0/1`.
`from_parts` rejects noncanonical fractions rather than silently normalizing
checkpoint input. Public exact arithmetic normalizes its own results.
`to_decimal` is an explicit rounded projection, not used for basis transitions.
`SignedAtoms::to_decimal` converts an atom receipt exactly into token units.

Input quantity, delta, price and notional have at most 40 coefficient digits and
20 decimal places. Prices and notional are strictly positive; zero trade delta
is rejected. Stored fraction components have at most 512 digits. Oversized
reduced results and atom magnitudes above `u128::MAX` fail without mutating the
input position. These are local resource limits, not historical protocol rules.
`validate` checks shape, signs, input bounds and the subatom bound. The caller's
canonical action replay must additionally bind a restored basis to its actual
execution and mark history; shape validation alone cannot establish provenance.

## Actual source evidence and deliberate differences

The reference JavaScript commit is
`dca0888b827c5bc28fa9189bc9e1d507f5c5670c`. The new script executes complete,
unchanged existing archives, with no rewritten economic formulas:

| Archive | LF SHA256 |
| --- | --- |
| `fixtures/legacy_clearing_source.txt` | `612d1ec49c1d5cc412d0072a194da4975c5232ba5294b81cd3097df11bff1528` |
| `fixtures/legacy_margin_map_source.txt` | `ad5af4a639a50623319236cce42e8728027109ac3b8ae4616387798110cfb636` |

The full source classes run with pinned genuine BigNumber 9.3.1. Explicit DB,
registry and UUID adapters record dependency calls; unused dependencies reject
unexpected use. No adapter supplies PnL math. `node scripts/mark_basis_vectors.cjs
--check` verifies all 130 generated source vectors and 9 characterized traces.

The vectors execute active `calculateClearingPNL` (raw),
`calculateMarkToMarkPNL` (q8), `calculateNewContractPNL` (raw) and
`MarginMap.calcPriceDelta` (q8). Positive bounded-price and notional cases match
`source_clearing_pnl`, with q8 applied only when comparing q8 source helpers.
This comparison includes fractional quantities beyond Number precision and
reciprocal underflow at valid finite prices.

The separate quirks file records actual method behavior:

- The raw clearing helper retains a tenth of an atom where the q8 mark helper
  returns zero.
- `clear` updates `oldMark`/`lastMark` and q8 unrealized PnL but leaves trade
  `avgPrice` unchanged. Calling `realizePnl` afterward at that same marked price
  calculates the original entry PnL again and removes its absolute magnitude
  from unrealized PnL. These method traces show why a custody integration needs
  an explicit paid basis; they do not by themselves prove a historical double
  transfer.
- Calling `clear` twice accumulates the supplied increment twice; it provides
  no epoch replay fence.
- Linear `settlePNL` uses binary64 `(price-lastMark)*contracts` and omits
  notional. It also rewrites unrealized PnL through its rounded subtraction.
- `normalizePositionMarks` overwrites the in-memory mark without computing
  PnL or making a DB write in that method.
- Source truthiness defaults numeric zero notional to one, while string zero
  remains zero. Zero-entry guards disagree between the raw and other helpers.
- `recordTrade` takes `Math.abs` through Number and loses integer precision at
  `9007199254740993`; the repair keeps bounded Decimal quantities exact.

The source `updateMarginMaps` orchestration takes a canonical previous/current
mark and calls the raw helper with current position contracts. Source
`recordTrade`/pool tracking, separate new-contract helpers, liquidation, ADL,
option adjustments, DB failure ordering and IOU payment branches are not
reimplemented here. This module is a basis math increment, not complete
clearing parity or validation of historical transactions.

## Verification

Ten direct Rust tests cover all 130 source vectors, repeat-mark/close behavior,
adds/reductions/flips, repeating rational partial closes, mark-independent closed
cashflows, signed carry across flat/reopen, opposite-owner conservation,
canonical restore/resource bounds, overflow atomicity and exact quantities.
Direct actual-source Rust compilation, Clippy with warnings denied, formatting
and the unchanged-source oracle check run without Cargo in the delegated lane.
The parent owns combined runtime integration, full Cargo checks and commit.
