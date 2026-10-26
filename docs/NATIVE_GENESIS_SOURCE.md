# Native genesis: exact source facts and integration constraints

Status: source audit and executable evidence only. No mint, beneficiary choice,
Ledger mutation, deployment policy, chain authentication or catalog import is
implemented by this directory. The source-channel bundle is a separate frozen
task. The actual-owner design is being prepared independently under
the independently configured actual-owner integration.

The JS pin is `dca0888b827c5bc28fa9189bc9e1d507f5c5670c`. All four requested
files were read completely and archived as exact LF Git blobs in `fixtures/native_genesis/source/`.
`source-pins.json` also pins the supporting logic/main/db files. The Rust audit
baseline is commit `6403716f4ee77682430d75bba1c22c08a7c85d1b`; concurrent signed
channel integration does not supply native genesis or vesting functionality.

## Reproduction and evidence boundary

```text
node scripts/native_genesis_vectors.cjs --check
node scripts/native_entitlement_vectors.cjs --check
```

The script executes complete unchanged activation.js, vesting.js, property.js
and tally.js together through a CommonJS VM loader and genuine immutable
BigNumber **9.3.1**, SHA256
`58ba6fb59668342346f17983a61ace455f6f425ac4fbfab84f489df257594d16`.
No source method is replaced. Explicit capabilities provide deterministic
asynchronous DB query/update/insert behavior, network discovery, UUIDs, a native
contract request recorder, insurance launch recorder and clearlist launch
recorder. Those last three record calls; they do not prove contract acceptance,
insurance custody or permission to create lists. This is not a NeDB equivalence
claim, live RPC invocation, or historical replay.

`evidence.json`: **18 scenes / 1,647 dependency events**, SHA256
`7187cdf7a1cfdce9114ba70cc3af5d9cfaff148b7fef10540da8ac10862a5808`.
The final scene explicitly changes only the DB capability to a synchronous
factory to expose downstream vesting behavior after repairing its missing
await. It is labeled conditional and is not the ordinary pinned composition.
`supply-reconciliation.json` is a smaller generated exact-atom/table view.

| Source | LF bytes | SHA256 |
| --- | ---: | --- |
| activation.js | 12,976 | `268f08438eb80765587b8ce24d719f41de573648e108ef64df557faad870ea0e` |
| vesting.js | 19,178 | `4c134b6871f450d3190a0958b629731d960540069d0c8c37a6da33d6e1f70d2b` |
| property.js | 17,638 | `ef0ee7ba5c362227a8bfd44192e351c107871a93a3dc93b04ce6d1a5ebfc70b1` |
| tally.js | 59,507 | `cfde9e5845f810bf62d9f1a07d4db4ea011b11d3d2bb63f041d39bea557cddef` |

## Exact activation and initialization order

1. The wire transaction is type0, but native initialization is triggered only
   when its **selected activation list includes target0**. Logic loops the
   supplied list sequentially (`logic.js:306–320`); each target calls
   `Activation.activate(target, block, codeHash)`. Activating only target2 or5
   must not create native tokens.
2. Activation parses the target and loads its DB registry. Only target0 calls
   `TradeLayerManager.getInstance(adminAddress, chain, test)` and then awaits
   `initializeTokens(block)` (`activation.js:117–140`). The source manager
   factory accepts only admin/chain; its constructor receives no test argument
   and rediscovers chain/test through Client (`vesting.js:119–126`).
3. `initializeTokens` reads `$TLinit`; it fills a missing admin even when the
   flag is already true, but creates nothing when that flag is truthy
   (`vesting.js:129–146`; `tally.js:321–335`). A standalone true flag is enough
   to skip source initialization; the VM proves target0 can become active with
   **no properties or balances at all** under that input DB condition.
4. When uninitialized, create four properties sequentially using actual
   `getNextPropertyId` results, then request the native contract, construct
   insurance `(1,0,0.5,false)`, launch its deposit without awaiting it, execute
   five awaited tally updates, read admin balances, launch three clearlists
   without awaiting completion, and finally await `$TLinit=true`
   (`vesting.js:147–184`). The tally updates omit type/block/txid parameters;
   their delta block is undefined, not the activation height.
5. Only after initializeTokens returns does activation mark target0 active,
   overwrite its activationBlock, optionally set codeHash, save the activation
   DB row and push the consensus vector (`activation.js:131–140`). The JS save
   method swallows DB exceptions (`activation.js:62–77`).
6. Repeating target0 with a true flag does not create more tokens in the VM,
   but it **does replace activationBlock/codeHash**. Registry's existing Rust
   policy instead rejects an already activated target; preserve that named
   policy rather than accidentally changing old rules.

## Four property roles and source declarations

On a genuinely empty property catalog the four returned IDs are1,2,3,4. Source
variables initially suggest IDs, but each is overwritten by createToken's
return value (`vesting.js:147–163`). The allocator loads the catalog and returns
max numeric key+1 (`property.js:61–67`). A catalog containing property5 produces
native IDs6,7,8,9 in the VM, while native collateral/redemption helpers remain
hard-coded to1,2,3,4. Empty/reserved IDs must therefore be checked independently;
never silently relocate genesis to the next free ID.

`property.js:88–101` defines type indices Fixed=1, Managed=2, Native=3,
Vesting=4. All four source createToken calls pass whitelistId0 and omit issuer
and backup. Property persistence JSON therefore has **no issuer/backup fields**.
The future Ledger requires real independently configured ownership/authority;
it cannot infer that authority from an omitted source field.

| Role | ID on empty catalog | Ticker suffix | Class | Declared `totalInCirculation` | Initial admin available | Initial admin vesting marker |
| --- | ---: | --- | --- | --- | --- | --- |
| Main native collateral | 1 | base ticker | Fixed(1) | N | N×sale | 0 |
| First coupon | 2 | VEST | Vesting(4) | N×0.5 | N×0.5 | N×0.5 |
| Income coupon | 3 | I | Vesting(4) | 3N | **1,500,000** | **1,500,000** |
| Income payout | 4 | IVEST | Native(3) | 3N+1 | 1 | 0 |

These are declared fields, **not a coherent source definition of issued supply
or an enforced cap**. The awkward ticker/class pairing above is exact. Property
creation only saves metadata (`property.js:70–85,125–153`); it does not mint a
tally balance. Future redemption can create payout balances while declarations
stay unchanged. Treating every declaration as already owned Ledger custody
would require extra allocations the source never supplies.

| Network | N / ticker | Admin main-token sale | Sponsor main-token free | Insurance main-token call | Coupon2 marker | Declaration less current tally and launched insurance | Difference after hypothetical full coupon2 backing |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| BTC | 100,000 / TL | 0 | 10,000 | 10,000 | 50,000 | 80,000 | **30,000** |
| LTC | 500,000 / TLITE | 50,000 | 50,000 | 150,000 | 250,000 | 250,000 | 0 |
| DOGE | 200,000,000 / TD | 0 | 50,000,000 | 50,000,000 | 100,000,000 | 100,000,000 | 0 |

The last two columns are arithmetic diagnostics. Insurance is only a launched
capability in this experiment. Neither column authorizes a mint or proves
missing spendable units. BTC30,000 is specifically the gap in configured
fractions **after hypothesizing** full backing for coupon2. There is no source
beneficiary or disposition for that gap in initializeTokens. No beneficiary is
selected here.

| Network | Declared coupon3 | Actual admin coupon3 available/marker | Declared payout4 | Payout4 admin available | Declared coupon3 minus current coupon3 available |
| --- | ---: | ---: | ---: | ---: | ---: |
| BTC | 300,000 | 1,500,000 / 1,500,000 | 300,001 | 1 | **−1,200,000** |
| LTC | 1,500,000 | 1,500,000 / 1,500,000 | 1,500,001 | 1 | 0 |
| DOGE | 600,000,000 | 1,500,000 / 1,500,000 | 600,000,001 | 1 | **598,500,000** |

The constant1,500,000 is `vesting.js:174`; the declarations use3N at157–163.
An intentional cross-chain rule cannot be deduced uniquely from those two facts.
All atom quantities in the generated JSON are decimal strings. For example
DOGE coupon3's declaration is60,000,000,000,000,000 atoms, beyond exact binary64
integer representation; a Rust spec must use checked integer arithmetic.

## Actual beneficiaries and static/instance loss

Mainnet admin after a successful Activation.init (`activation.js:35–43`):

| Network | Admin | Sponsor (`vesting.js:80–115`) |
| --- | --- | --- |
| BTC | `bc1qktknrnx2jcchjht9anz0uy8ae02xryxq2vxeem` | `bc1qhc2cj60auf67e0pa3dfd46cvg0fehchx56vw0f` |
| LTC | `MTmoypkhRQoJ172ZqxcsVumPZfJ8KCrQCB` | `MWip91xMhaEmDn5oUW5NDNbWSDyG5dSK9Q` |
| DOGE | `DLSfu9qvEggkeXAgCAwBBw5BVLvMCtkewz` | `D8HA73pAhxK7eNXSUVhQrWpUkrszUDGs7Z` |

Missing-admin fallback in vesting appends a comma to BTC mainnet admin
(`vesting.js:136–143`), unlike Activation.init. The VM proves that distinct
opaque account is credited. Do not silently fix/migrate that address or treat
AccountId's ASCII validation as chain-address validation. Rust currently has
mainnet-only profiles; testnet values must not be accepted as those profiles.

The call to `TradeLayerManager.initializeContractSeries(block)` is static.
Its `this` is the class, so instance admin, expiryInterval and hedgeLeverage are
absent (`vesting.js:165,188–209`). Actual recorded arguments are:

```text
issuer = undefined; initialMargin=.2; maintenanceMargin=.1; expiry='perp';
onChainData=[[1,0]]; expiryInterval=undefined; leverage=5; seriesLength=6;
native=true; inverse=true; fee=false; notionalPropertyId=0;
notionalValue=.001; collateralPropertyId=1
```

DOGE's instance hedgeLeverage10 is therefore not passed. The contract registry
reads `expiryPeriod` and `series`, not `expiryInterval`/`seriesLength`
(`legacy_contract_registry_source.txt:145–157,256–267`), and it labels base-chain
notional as LTC even on other networks (127–130). Our VM records this request
at an explicit boundary; it does not assert the complete contract registry
accepted it or created six valid live contracts.

The three static initializeClearlists calls similarly pass undefined admin and
backup (`vesting.js:213–247`). They request IDs1/2/3, names Issuer/Market Maker/
Accredited Clearlist and descriptions archived in the trace. Source completion
does not establish real administrator rights or durable external completion.

## Vesting is coupled entitlement metadata, not a second owned bucket

Owned tally amount is available+reserved+margin+channel, each q8 in
calculateTotal; **vesting is excluded** (`tally.js:313–319`). getTotalForProperty
also excludes it (277–310). The VM credits10 available+10 marker and observes
amount10, not20. A raw tally debit4 leaves available6 and marker10; that is
evidence about the primitive, not the complete Send policy.

Actual scalar sends of property2/3 route through `vestingSend`
(`logic.js:364–443`): obtain source tally, calculate
`floor_whole(vesting × BN20dp(amount/available))`, then four sequential writes
debit/credit coupon available and debit/credit marker. The rounded-down amount
is only a positivity check; actual available writes use the original amount.
Multisend follows a different processSend path. Generic Ledger Transfer of2/3
without coupled marker handling would therefore be an incomplete integration.

`updateVesting` uses property2/3 declarations and returns cumulative targets
(`vesting.js:251–294`): first window1,000→1,000,000,000 LTC-volume units;
second10,000,000,000→1,000,000,000,000 global units. It does not track prior
redemption or decrement declarations. Indexed postprocessing calls it after
fee flush/rewards/queued matching when block.global>0, before withdrawals and
clearing (`main.js:359–386`). The live ending guard compares the block-volume
object itself to0 (`main.js:869–886`); do not assert identical cadence.

Normal pinned async DB composition makes getAddressesWithBalanceForProperty
call `.findOneAsync` on an unawaited Promise; its catch returns[]
(`tally.js:1310–1348`, `db.js:102–107`). The VM proves ordinary applyVesting is
a no-op. With only an explicitly synchronous DB-factory repair, the unchanged
body distributes `target × holder.available / declaration`, reduces only the
coupon vesting marker and credits payout1/4; it leaves coupon available and
all property declarations unchanged (`tally.js:1351–1393`). Repeating a125,000
target consumes LTC's250,000 marker twice; a third call fails. This conditional
experiment is not evidence that historical normal execution redeemed it.

## Failure and replay evidence

- A delta failure during the second LTC balance update leaves four properties
  and only50,000 persisted main-token admin custody; `$TLinit` remains false.
  Retry returns duplicate-ticker Error objects as property IDs, ignores invalid
  tally-ID Error returns, then sets the flag and activates0 with the same
  incomplete balances. See `delta-failure-and-retry`.
- The third tally save can fail and be swallowed (`tally.js:523–535`); subsequent
  loads lose property3's credit, yet initialization completes. Source flags
  cannot certify an atomic genesis.
- Insurance and all three clearlist promises can still be unresolved when
  `$TLinit` and activation0 are true. A fire-and-forget request is not custody.
- Initialization-flag failure leaves prior property/tally effects intact.
  Retrying again creates native contract requests rather than rolling them
  back. Repeat/no-inflation behavior only applies once a trustworthy flag was
  actually written, not arbitrary partial failure.
- Genesis delta totals are NaN because recordTallyMapDelta does not await
  getTally (`tally.js:1178–1186`); fields type/block/txid are absent/empty.
  These source records are not a valid replay witness for the intended mint.

## Safe next owner work, without choosing supply policy

The real gap is between source metadata and the sole owner, not a missing call
from Registry into the metadata backend:

- RegistryAction::Activate records targets only (`src/registry.rs:72–78,337`).
  Joint applies it without any Ledger operation (`src/joint.rs`,
  apply_inner_unverified). Source mapper type0 builds that same action
  (`src/types_economic.rs:224–239`).
- Ledger has only Fixed/Managed roles and requires supply equal to actual owned
  atoms. CreateProperty issues a positive amount to a concrete issuer; it has
  no declaration/cap/remaining-entitlement representation (`src/ledger.rs`,
  PropertyKind/Property/execute/verify_conservation). Do not map native payout
  creation to generic Managed grants or double-count the marker as custody.
- Spot import forbids permitted IDs≤4 and requires every permitted property to
  exist (`src/spot.rs:191–224`). EconomicState construction additionally
  requires collateral already exist (`src/economic.rs:222–233`). An empty
  native Ledger cannot be booted by inventing a dummy collateral balance.
- Current accepted Economic contract binding rejects native definitions
  (`src/economic_contract_impl.rs:225`); oracle marking/funding configuration is
  not a substitute for actual native spot/base-chain price evidence.
- Existing TLECv7 recomputes source Types/validity against the staged owner,
  retains rejected raw candidates and verifies full source prefix on rollback.
  A future native event must be inside that candidate/block replay, not a
  post-load balance patch or a separately mutable activation module.
- DurableEconomicChain already publishes the full checkpoint only after its
  Journal commit, takes an independent expected EconomicHead, and denies reads
  after uncertain writes (`src/economic_chain.rs:363–473`). Native state must be
  covered by the same owner/checkpoint/rollback transaction.

Reviewable mechanics can be specified now: independently expected network/
genesis/admin plus exact genesis input identity; explicit roles/IDs and source
declarations **separate from selected current-issued quantities, policy caps
and remaining redemption entitlements**; a bounded Pending/Initialized owner
state; canonical deployment/spec commitment; immutable read projections; full
rejected-input history; reserved-ID and duplicate-trigger fences; staging,
restart, rollback and lost-ack tests using zero balances before a selected
genesis event. Native mutations remain disabled until a complete expected
specification is supplied. Legacy snapshots without native metadata retain
their old bytes; no old format may silently strip a new native liability.

Required decisions remain explicit alternatives, not defaults:

1. **Literal source declarations/distribution:** preserve all raw source fields
   and failures as observations. This is not itself a conserving funded policy,
   especially BTC coupon3 where available exceeds its declaration.
2. **Current-issued versus cap/entitlement interpretation:** choose an explicit
   spec that says which declaration is a cap/reference denominator, what is
   actually issued now, and which authority funds/mints later redemption. This
   cannot be inferred just by subtracting tally from declarations.
3. **Pre-funded fixed payout reserve versus bounded future issuance:** either
   requires independently approved initial quantities, escrow/control and exact
   entitlement conservation. Neither permits inventing a beneficiary for
   BTC's30,000 diagnostic or silently changing coupon3 to3N.
4. **Cross-chain correction:** replacing1,500,000 with3N, changing declarations,
   or rejecting inconsistent networks are materially different policies.
   The audit selects none of them.

The external property worker must supply a concrete versioned metadata/role
contract and reserved ID/issuer rules before integration, without another
spendable Ledger. Clearlist and insurance authorities remain their reserved
lanes; a genesis initializer may not treat these mocked calls as implemented
custody. Native contract issuer/expiry/pair/price semantics also require an
explicit accepted-definition contract. Authenticated parent UTXOs, script
verification, actor/order admission and historical activation evidence remain
external gates. Self-hashes and expected local heads provide integrity and
freshness, not those authorities.

## Reachable fractional-transfer evidence

`native_entitlement_vectors.cjs` executes the unchanged logic/tally/vesting
modules. Four extra source scenes start with10 coupon units and10 remaining
entitlement markers for each of Alice and Bob. Sending1.1 moves only one whole
marker: available becomes8.9/11.1 and markers9/11. Under the explicitly repaired
synchronous DB capability, a full20 target pays Alice8.9 and then fails when
Bob's11.1 quote exceeds his11 markers, leaving partial source effects. The actual
asynchronous factory instead produces the known empty-address no-op. Two other
scenes prove cumulative curve repetition and the below-one-coupon rejection.
These are source observations, not a selected distribution or funded policy.
