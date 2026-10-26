# Vesting and initialization

`src/vesting.rs` ports the concrete numerical and initialization behavior in
`vesting.js` at JS commit `dca0888b827c5bc28fa9189bc9e1d507f5c5670c`.
The complete unmodified LF source is archived in
`fixtures/legacy_vesting_source.txt`, SHA256
`4c134b6871f450d3190a0958b629731d960540069d0c8c37a6da33d6e1f70d2b`.
This is a caller-driven local module. It does not authorize genesis, mint into
the sole ledger, settle trades, validate chain addresses, or authenticate volume.

## Initialization behavior

`VestingManager::initialize_tokens` executes an explicit
`InitializationBackend`. Every call is typed and ordered; returned property
identifiers flow into subsequent insurance and balance calls. The method first
checks the initialization flag and fills the source's fallback administrator
even when the flag was already set. An existing flag prevents all subsequent
creation and distribution calls. For a fresh state it requests four token
creations, one contract series, insurance construction/deposit initiation, five
balance updates, an administrator balance query, three clearlist initiations,
and finally the flag write. Its return is the backend's administrator balances.

The backend interface makes those effects inspectable. It is not an in-memory
funding ledger: no balance, property supply, or insurance reserve is created by
this module itself. A backend must implement real registry and accounting
semantics independently. A backend error stops the call sequence. Prior external
calls remain visible, matching the source's absence of an enclosing transaction.
The test backend records a partial initialization failure after creating four
properties and two balance updates, with no initialization flag write. Retrying
that partial state is not claimed idempotent. Durable transactional recovery is
still required before production use.

The source calls insurance deposit and clearlist creation without awaiting their
promises. Corresponding `Launch*` requests record initiation only, not durable
completion. A Rust backend may reject a launch immediately; completion and
asynchronous failure propagation belong to that backend. A final flag therefore
does not establish that those source effects finished.

The source's chain constants and allocation calls are preserved:

| Chain | Base issuance | Vest issuance | Income-vest issuance | Native-income issuance | Insurance call | Admin base available | Sponsor base available |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| BTC | 100,000 | 50,000 | 300,000 | 300,001 | 10,000 | 0 | 10,000 |
| DOGE | 200,000,000 | 100,000,000 | 600,000,000 | 600,000,001 | 50,000,000 | 0 | 50,000,000 |
| LTC/default parameters | 500,000 | 250,000 | 1,500,000 | 1,500,001 | 150,000 | 50,000 | 50,000 |

The second ticker is `<ticker>VEST` with available and vesting both equal to
vest issuance. The third ticker is `<ticker>I`, type Vesting, and receives
**1,500,000 available plus 1,500,000 vesting on every chain**. The fourth ticker
is `<ticker>IVEST`, type Native, and receives one available unit. These names,
types, and values are source behavior, including their apparent inversion and
chain-specific supply discrepancy. They are not silently reconciled or treated
as funded ledger operations. The insurance constructor is always called with
contract ID 1, independently of the returned native contract ID.

Two source initialization defects remain explicit:

* Static `initializeContractSeries` reads `this.adminAddress`,
  `this.expiryInterval`, and `this.hedgeLeverage` from the class, although
  initialization assigned them to the instance. Ordinary calls therefore pass
  undefined administrator and expiry interval, with leverage fallback **5 even
  on DOGE**. The request uses `seriesLength` and `expiryInterval`, while the
  registry reads different `series` and `expiryPeriod` fields. The Rust request
  preserves those actual field names and absent values.
* Static `initializeClearlists` similarly passes undefined administrator and
  sponsor/backup. The Rust requests preserve both as `None`. BTC mainnet's
  fallback administrator includes the source's trailing comma. Unknown chains
  receive LTC parameters but no fallback administrator.

## Chain ownership and retry behavior

`VestingManager::new`, `get_chain`, and `get_test` preserve explicit instance
inputs. `ensure_chain_initialized` skips RPC attempts when chain is nonempty
and test is present, otherwise invokes a caller-owned `ChainProbe` at most ten
times. Each failed attempt requests a 2,000 ms delay, including attempt ten.
After ten failures it sets chain BTC and leaves test unchanged. Delay execution
is delegated, so deterministic local tests do not sleep.

`set_chain_params` intentionally ignores its argument and reads the stored chain,
as the source does. Exact uppercase BTC and DOGE have specific parameters;
everything else has LTC parameters. `ManagerSlot` replaces process-global
singleton ownership. Its `get_instance` uses first-call admin/chain only and
passes no test argument, matching source `getInstance` despite callers sometimes
supplying an ignored third argument. Concurrent asynchronous singleton races and
the empty object produced by manually constructing a second JS instance are
not modeled as useful Rust lifecycle behavior.

## Calculations and failure paths

`update_vesting` returns **cumulative target amounts**, not an incremental
vesting debit. It returns `None` if either property supply is absent. Its first
linear range is 1,000 to 1,000,000,000 cumulative LTC volume; its second is
10,000,000,000 to 1,000,000,000,000 global volume. The function does not persist
volume, compare with already vested amounts, or apply `tally.js::applyVesting`.
Repeated calls with unchanged inputs produce the same target.

Rebate curves, vest fractions, projections, months-to-target, CMGR, annualized
growth, discounting and buyback sizing preserve source binary64 operation order,
including NaN/Infinity branches and greedy input order. `calculate_buyback_amount`
does not sort orders or repair source acceptance of zero/negative values. These
are source numerical observations; an execution layer must separately validate
accepted inputs. Rust libm and V8 transcendental results can differ in low bits;
tests use a stated tolerance, not a cross-platform bit-exact consensus claim.
Initialization allocation products use shared `Decimal`; all pinned chain
constants/results are small exactly represented numbers at source `.toNumber()`
boundaries, and the complete journal is compared exactly.

The source `fetchOrderbookForToken` is empty. Rust returns `None`.
`performBuyback([])` is a no-op; a nonempty cache reaches an undefined orderbook
and throws. Rust returns `UnimplementedOrderbook` for that path and executes no
buyback.

The source `computeTokensForTrade` computes a local `projectedYearVolBTC` but
returns the different shorthand `projectedYearVolumeBTC`. Every ordinary typed
call throws `ReferenceError: projectedYearVolumeBTC is not defined`.
`compute_tokens_for_trade` preserves that failure as
`UndefinedProjectedYearVolume`; no successful historical token award is claimed.
An explicitly separate `quote_tokens_for_trade_repaired` repairs that return-name
defect and returns an inspectable fee/PV projection. It remains a numerical
quote with no issuance effect. Its golden cases execute the unchanged source
with an explicitly injected ambient binding for the missing return name; those
12 cases are labeled `REPAIRED_QUOTE` and are not ordinary source success cases.

## Coverage and evidence

| Source member | Rust entry point / treatment |
| --- | --- |
| constructor | `VestingManager::new`; explicit ownership |
| getChain / getTest | `get_chain` / `get_test` |
| ensureChainInitialized | `ensure_chain_initialized`, delegated probe and delays |
| setChainParams | `set_chain_params` / `ChainParameters::for_chain` |
| getInstance | `ManagerSlot::get_instance` |
| initializeTokens | `initialize_tokens`, complete backend call sequence |
| initializeContractSeries | `initialize_contract_series`, actual absent static fields |
| initializeClearlists | `initialize_clearlists`, explicit unawaited launches |
| updateVesting | `update_vesting`, caller-supplied property supplies |
| calculateTradeRebates | `calculate_trade_rebates` |
| performBuyback | `perform_buyback`, ordinary source failure preserved |
| fetchOrderbookForToken | `fetch_orderbook_for_token`, empty stub |
| calculateBuybackAmount | `calculate_buyback_amount` |
| EPS | `EPS` |
| vestFraction | `vest_fraction` |
| projectNextYearVolumeBTC | `project_next_year_volume_btc` |
| monthsToReach | `months_to_reach` |
| annualizeFromCMGR | `annualize_from_cmgr` |
| discountFactorMonths | `discount_factor_months` |
| cmgrFromSeries | `cmgr_from_series` |
| computeTokensForTrade | failure-preserving entry plus separately named repair |

`node scripts/vesting_vectors.cjs` runs without npm dependencies or network.
`--write` deliberately regenerates golden TSV, and `--source PATH` checks an
external exact source copy instead of the archive. Both paths verify SHA256.
The actual archived BigNumber 9.3.1 reference is loaded by
`legacy_numeric_oracle.cjs`. Dependency mocks record calls from the unmodified
vesting source; they do not replace its implementation. They do not execute the
actual property, tally, contract, insurance, clearlist or client modules.

There are **151 recorded cases**: five chain configurations, 45 rebate values,
eight vesting targets, 49 other numeric helpers/EPS, ten buyback sizes, five
missing/stub/error observations, 12 explicitly repaired-return quotes, nine
full initialization journals, one partial failure journal, six chain retry
cases and one singleton case. Repeated-initialization flag checks and absent
registry keys are additionally asserted in the oracle. Rust tests check those
vectors and local backend reply/block-bound errors. Numeric tolerance is
256 machine epsilons times `max(abs(expected), 1e-6)` for finite binary64 outputs;
NaN and signed Infinity branches are checked explicitly.

This covers the file's concrete methods and characterizes its broken paths.
The typed domain uses optional boolean test mode, string chain names,
`AccountId`/nonzero `PropertyId`, and a bounded safe-integer block height.
Arbitrary JS objects/coercions, null-as-test distinct from absent test, invalid
property-creation error objects used as IDs, monkey-patched class globals, and
non-array CMGR inputs are excluded. Attempting fresh initialization without
configured parameters returns `MissingParameters` instead of propagating
source undefined fields and NaN into dependency calls. These are explicit local
domain boundaries, not new historical transaction acceptance rules.

Production genesis authorization, coherent native/vesting supply policy, source
defect decisions, sole-ledger and `tally.js` integration, transactional durable
initialization/recovery, asynchronous effect completion, authenticated volume,
network-derived addresses, and historical replay acceptance remain open.
