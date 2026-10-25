# Finite decimal arithmetic

`decimal::Decimal` represents a finite signed base-ten coefficient and scale.
It supports parsing decimal/scientific notation, total ordering, canonical
hashing, exact addition/subtraction/multiplication, and division/quantization
with explicit decimal places and rounding. It introduces no external Rust
dependency and converts no decimal amount to binary floating point.

`Rounding::Down` truncates toward zero; `Rounding::HalfUp` rounds to nearest,
with exact ties away from zero. These correspond to the BigNumber modes used
by the pinned source. Division places are fractional decimal places, rather
than significant digits. For example, `1 / 3` at 20 places is
`0.33333333333333333333`. Multiplication retains its full finite result; a
caller must explicitly choose any persistence or atom rounding.

`Display` prints the exact canonical value in ordinary decimal notation.
It is different from BigNumber's `toString(10)`, which rounds to its configured
decimal places. IOU field persistence therefore explicitly calls
`quantize(20, HalfUp)`. Plain display never silently applies that policy.

Zero has no negative sign or fractional scale. Numerically equivalent inputs
such as `1`, `1.0` and `100e-2` compare, hash, and display identically. Decimal
and scientific input may include a sign and ECMAScript whitespace. Hexadecimal,
nonfinite values, separators, malformed exponent strings and dynamic JavaScript
coercions are outside this finite decimal parser.

## Resource bounds

A canonical value has at most 4,096 coefficient digits and 4,096 fractional
places. External input is capped at 8,192 UTF-8 bytes before trimming. Working
integer buffers are bounded separately, allowing normalization to remove
trailing zeroes before the final representation limit is checked. Exceeding
those local limits returns `DecimalError::LimitExceeded`. These bounds are
not historical consensus rules. Division by zero returns a distinct error.

Schoolbook coefficient multiplication and long division use decimal integer
digits. Addition aligns scales; comparisons account for virtual trailing
zeroes without building expanded coefficients. Every public arithmetic method
returns a replacement value and leaves both operands unchanged. The type
provides arithmetic, not balance ownership or transaction authentication.

## Independent verification

`scripts/decimal_vectors.cjs` evaluates the actual complete BigNumber 9.3.1
implementation preserved in `fixtures/legacy_bignumber_source.txt`. Its
LF-normalized SHA-256 is
`58ba6fb59668342346f17983a61ace455f6f425ac4fbfab84f489df257594d16`.
The package's MIT licence is preserved alongside it. This is a pinned test
reference, not a Cargo dependency or a Rust runtime arithmetic backend.

The shared loader `scripts/legacy_numeric_oracle.cjs` verifies that hash before
evaluating the original code in a Node VM. The other economic source oracles
also evaluate archived, hash-checked JavaScript modules. They require no npm
installation, live RPC, wallet or external database. Optional source-path flags
verify another checkout against the same source hashes.

The decimal oracle compares 1,942 deterministic vectors with Rust, covering
positive/negative numbers, exponents, fractional and very large coefficients,
exact operations, comparison, 0/8/20/40-place division, both rounding modes,
and seeded varied-sign/scale cases. Separate adversarial Rust tests cover
representation boundaries, signed minimum/maximum conversions, carry and
overflow, normalization of larger intermediates, extreme scales, hashing,
invalid syntax and the minimum representable rounding quantum.

```text
node scripts/decimal_vectors.cjs
cargo test --frozen --test decimal --test decimal_adversarial
```
