# Accepted contract definitions and source price contexts

`EconomicConfig.contract_binding` enables `SourceContextsConservingV1` inside
the existing `EconomicState`. Accepted contract metadata, the complete ordered
typed input history and all economic state belong to that owner. Collateral
continues to live in its Joint/Spot-owned Ledger.

The deployment supplies the selected creation transaction ID, a commitment to
the exact typed `CreateContractInput`, and an independent commitment to the
pristine Joint bootstrap. `contract_definition_commitment` covers the issuer,
transaction ID and every input parameter using canonical bytes and the
`TradeLayer/AcceptedContractDefinition/v1` domain. The bootstrap commitment uses
`TradeLayer/ContractBootstrap/v1`. Computing a commitment from an untrusted
checkpoint does not establish an independent expectation.

Creation is a real ordered `EconomicInput::CreateContract` transition. It needs
earlier type-16 activation, a normal issuer, an existing open oracle, existing
token collateral and a fresh globally disjoint transaction ID. The catalog
allocates the next numeric ID and derives its ticker, expiry rows and AMM
constructor metadata through the existing typed `ContractRegistry` creation
method. Its accepted rows are reconstructed by replay; `insert_loaded` alone
cannot authorize an imported catalog.

This executable domain supports oracle perpetual definitions with positive
notional and leverage, one generated series, whitelist zero, no native data
pairs, real token collateral, and the source boolean fee metadata represented
as Decimal zero or one. Native prices, base-chain collateral, dated exercise,
nonzero clearlist enforcement and general multicontract execution remain
separate gates. Multiple accepted definitions can be metadata; this owner
executes the independently selected contract. Existing country and address
authentication rules are outside the typed caller boundary.

The selected accepted definition must match the configured contract ID,
collateral, inverse flag and raw notional. Its oracle binds marking and any
funding config; its kind binds the fee route. Bound mode requires marking and
sets the legacy `initial_margin_per_contract` field to zero. Old configurations
retain their existing positive fixed margin and original behavior.

New orders require a positive accepted oracle reference and satisfy the source
strict leverage band at their actual input position:
`abs(order_price - mark) / mark < 1 / leverage / 2 - 0.0065`.
The two divisions that derive the allowed band retain their separate
twenty-place half-up rounding steps; equality at the boundary rejects.
An earlier publication in the same block can change the reference for a later
order. Missing references, a closed oracle and an exhausted band reject.
The source's positive price limit of 9007199254740991 is enforced even when
a larger accepted oracle quote would otherwise give zero deviation.

## Risk amounts and their price contexts

The existing catalog helper preserves the pinned JavaScript operation order:
linear per-contract initial margin is `N * price / leverage`, rounded half-up
to eight places; inverse margin is `N / price / leverage`, with each division
at twenty places and the final result rounded toward positive infinity to
eight places. The source's finite binary64 return bridge remains characterized
by the source vectors. Bound execution rejects invalid economic inputs.

An opening order reserves against its own admission price. Its reserve stores
that per-contract basis; residual custody is recomputed from the remaining
opening quantity and the same basis. Thus a later fill price cannot silently
reprice the money held for the remaining order. Transfers use the difference
between the actual old and new rounded reserve amounts.

For a fill, opening from flat or flipping funds the new exposure at the fill
price. Increasing in the same direction adds fill-price opening margin to the
actual existing margin. Reducing releases only the excess above the remaining
fill-price requirement, with no forced top-up; a full close releases the
remaining margin. For example, inverse positions opened at different prices
retain the sum of actual postings rather than replacing custody with a newly
computed average-price full-margin target.

Maintenance uses the position's arithmetic average entry:
`abs(quantity) * initial_margin(avg_entry) * 0.5`, rounded half-up to eight
places. Liquidation cure funding uses the current mark context. Forced maker
openings use their fill context and original order reserve basis. AMM risk
checks use the pool's average entry; each bid and ask has its own quote-price
margin requirement, affordability and remaining reserve.

These are explicit conserving integrations. Monetary postings and residual
reserves round upward to whole atoms where needed for physical funding. They
do not copy the source's inconsistent residual `initMargin` annotation, reuse
one margin balance as multiple credits, post a flip twice or grant a virtual
`amm` participant collateral. Insufficiency rejects the staged block without
publishing partial metadata, markers or transfers. Exact PnL carry remains
separate from margin rounding.

The source supplies two different notional values. Marks and PnL use raw
`notionalPerContract`. The active matching fee caller uses price-valued
`notionalValue`: oracle-linear `N * price`, inverse `N / price`, with the
catalog's source rounding. Funding uses index-valued notional. Bound mode
passes each value to its corresponding caller; legacy fee behavior remains
unchanged. The contract's boolean fee metadata is not a numerical fee rate.

## Complete checkpoints, replay and recovery

TLEC version 6 is canonical whenever contract binding is enabled, including
before the selected creation exists. It stores independent binding config,
optional funding/liquidation/AMM feature flags, full Joint bytes, all existing
economic state, complete accepted catalog rows, the pristine bootstrap and
every ordered typed external input frame. It also stores order reserve price
bases, liquidation pre-event average entries and AMM quote-epoch average
entries. Those context fields are absent from versions 1 through 5.

The frame codec covers every `EconomicInput`, `JointOperation` and
`RegistryAction` variant. Encoding a typed variant does not admit it: replay
still runs the real dispatcher and rejects unsupported or unauthorized typed
operations. The private replay recursion flag is never serialized and is
always false in decoded states.

Restore binds the expected full economic config, network, administrator and
permitted spot properties. It reexecutes complete ordered history from the
independently committed pristine bootstrap through the actual owner, including
catalog creation, oracle/property dependencies, orders, ordinary Joint commands,
LP inputs, marks, funding, liquidation and AMM actions. The reconstructed full
state must equal the stored state. This binds dependency timing and historical
publication identity in the bound mode, including same-height order, while
preserving one live economic owner. A temporary verification replay is not a
separate deployed wallet.

CRC detects accidental damage; canonical encoding, commitments and semantic
replay do not authenticate claimed L1 blocks or actors. An equivalent alternative
caller-supplied history can be structurally valid. Verified chain admission,
signatures, bootstrap provenance and a historical acceptance corpus remain
required before production consensus claims.

The combined limits are 4096 accepted contracts, 4096 complete block frames,
4096 inputs per frame and 65536 shared catalog records. Shared accounting also
charges nested activation types, data pairs and generated expiry rows in both
stored definitions and inputs. Counts are checked before their element loops;
strings, decimals and total checkpoint bytes retain explicit bounds. There is
no history-pruning shortcut that invents earlier definitions or loses dependency
history. Capacity failure rejects a mutation before publication.
The direct state, chain and durable entry points preflight borrowed inputs
against retained history before cloning nested caller data or the owner.

Versions 1 through 5 retain their exact bytes and checks. An active v5 fixture
with LP shares, quotes, a funded AMM forced fill and complete histories was
captured from committed `69a72c7` before this extension: 8840 bytes with
FNV-1a64 fingerprint `12997489052212977811`. That regression fingerprint is
not authentication. Existing older golden fixtures remain in the test suite.
No old active state is silently relabeled as catalog-bound history.

`EconomicChain` retains complete before-state checkpoints, including catalog,
input frames and risk contexts. Rollback before creation removes allocated
IDs and replay markers with the economic effects; replay allocates them again
from the restored catalog. Its existing outer TLEH format can wrap TLECv6.
`DurableEconomicChain` publishes the entire owner through the existing synced
one-key Journal commit, requires an independent expected head and tip on
reopen, and denies cached reads after uncertain writes. There is no separate
catalog namespace or cross-namespace atomicity claim.

## Source qualifications and checks

The pinned `contractRegistry.js` has inconsistent static/instance caches,
asynchronous loader return behavior and mismatched saved/read fields. Source
seller margin calls and flip/reuse paths also contain concrete defects. Those
failures are characterized separately from the functional Rust owner. The
average-price full-margin rebalance in `clearing.js` is commented out; it is
not an active source policy. The archived unmodified source and composed VM
vectors remain the numerical reference, with conserving repairs named above.

Snapshot tests cover genuine active v5 compatibility, three-network v6 restore,
independent expectations, repaired-CRC catalog/dependency/input/risk forgeries,
and durable rollback before creation followed by exact replay. Codec tests
exercise all typed frame variants and nested bounds before element reads.
