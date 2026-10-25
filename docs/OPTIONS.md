# Options helper port

`src/options.rs` implements all nine methods of `src/options.js` at JavaScript
commit `dca0888b827c5bc28fa9189bc9e1d507f5c5670c`. The LF-normalized source SHA256
is `02277caaece5339a9ea0dac15437d14ce3fac8dc5ee92a727a0f544efcdbb703`.
This is a working pure calculator module. The source helper itself contains no
contract issuance, order matching, collateral mutation, option exercise or
assignment; those integrations remain separate roadmap work.

| JavaScript method | Rust function | Preserved behavior |
| --- | --- | --- |
| `isOptionTicker` | `is_option_ticker` | Nonempty series, decimal integer expiry, uppercase C/P and optional nonnegative decimal strike; a hyphenated series can match even when the parser rejects it. |
| `parseTicker` | `parse_ticker` | Splits on every hyphen, reads only the first four segments, uses radix-10 integer prefixes and floating-point prefixes, and retains invalid strike NaN. |
| `computeReduceFlip` | `compute_reduce_flip` | Binary64 addition, side changes, partial/full reductions and flips; numeric NaN inputs become zero and infinities remain observable. |
| `rpnlForClose` | `rpnl_for_close` | Exact decimal quantity times price difference; every nonempty side other than LONG takes the short branch. |
| `intrinsic` | `intrinsic` | Exact nonnegative call intrinsic; every type other than Call takes the put branch. |
| `priceEUApprox` | `price_eu_approx` | Source intrinsic-plus-noise approximation, including its binary64 time and square-root bridge. |
| `nakedMaintenance` | `naked_maintenance` | Spot/10 for Call and strike/10 otherwise, using the source default decimal division rounding. |
| `mtmExposure` | `mtm_exposure` | Signed net premium and intrinsic, default 144 blocks/day, clamp negative blocks/day to one, and preserve caller position order. |
| `portfolioMaintenance` | `portfolio_maintenance` | Group by type and numeric expiry, charge naked shorts, then consume protective longs with the source greedy order and width/10 offsets. |

## Numeric and parsing boundaries

Amounts, strikes and spot values use the shared finite arbitrary-coefficient
`Decimal`. Division uses BigNumber's default `DECIMAL_PLACES=20` and
`ROUND_HALF_UP`; multiplication and addition keep the resulting precision.
For example, source naked maintenance can round a tiny nonzero input to zero,
while source option pricing can retain over 50 fractional decimal digits. The
port does not add an eight-place final rounding step.

The source's quantity-reduction arithmetic, expiry fields, volatility, days and
block counts use JavaScript `Number`. The Rust API accepts already numeric
`f64` values for those fields and performs the source numeric truthiness/default
rules. Dynamic JavaScript objects, strings-as-numeric-arguments, and BigNumber
instance identity are not Rust argument types. At a serialization boundary,
callers must convert those inputs explicitly before invoking the calculator.
An empty position slice represents a missing/null source positions array.
An absent side is `None`; an empty side string is also false in the source.

The ticker parser deliberately accepts forms the regex rejects: `12tail`
expiry, `.5` strike, `1e2`, a trailing strike suffix, or an invalid strike stored
as NaN. It uses the ECMAScript whitespace set, including BOM and excluding NEL.
`x-1-C-1e-2` parses a strike of 1 because splitting on hyphens occurs first.
An overflowing expiry can become positive infinity, which the source accepts;
these parser observations must be validated before any contract creation.

`price_eu_approx` computes time and its square root in binary64, converts finite
volatility/root values through shortest decimal strings, then executes the
original decimal multiplication chain. V8 and Rust/platform square roots and
shortest number formatting are cross-runtime compatibility risks. The committed
vectors compare the actual pinned JavaScript VM outputs exactly on the checked
runtime; they do not prove universal bit equivalence across future runtimes.
This approximation must not be promoted to a consensus price rule without
specifying and checking that bridge across supported platforms. A nonfinite
pricing bridge returns `OptionsError::NonFinitePricingInput`; BigNumber NaN and
Infinity are not silently converted to spendable decimal balances. Decimal
resource-bound failures return `OptionsError::Arithmetic`.

## Portfolio behavior retained from source

The algorithm first sums naked maintenance for all shorts in a group. It then
walks shorts and longs in their original order and subtracts
`min(abs(long strike - short strike) / 10, naked maintenance) * covered qty`.
Both remainders decrease, so a long cannot cover several shorts beyond its
quantity. Only a higher-strike Call long or lower-strike Put long is protective.
Different expiries or types cannot offset each other. Numeric NaN/zero expiry
inputs group at source default zero; infinities remain distinct expiry groups.
The final total floors at zero.

This source rule is unusual and order sensitive. With spot 100, one short Call
at strike 90 and longs at 100 then 200 produce maintenance 9. Reversing those
longs produces maintenance 0. The port preserves those results instead of
sorting legs or substituting a maximum-loss spread optimizer. Empty types are
skipped; unknown nonempty types retain source put-like naked maintenance but
never become protective.

## Verification

Run the independent oracle against the archived pinned sources, or optionally
verify a separate JavaScript checkout:

```powershell
node scripts/options_vectors.cjs
node scripts/options_vectors.cjs --source-root C:\path\to\tradelayer.js
cargo test --frozen --test options
```

The oracle checks the source hash and executes the actual module in a VM with
the archived real `bignumber.js` 9.3.1 source and its checked code hash and default
configuration. It compares all committed fixture bytes, covering 36 ticker
cases, 60 calculation cases and 22 portfolio cases. The Rust integration tests
consume every case and also check nonfinite handling, expiry grouping, source
order sensitivity and consumed hedge quantities. No network, wallet, database,
mock decimal implementation or caller-supplied expected arithmetic runs inside
the oracle. No npm installation is required; see [decimal arithmetic](DECIMAL.md).

This helper port does not authenticate market data, authorize a position or
connect pricing to ledger settlement. Source-helper parity and complete
option-contract execution are separate completion claims.
