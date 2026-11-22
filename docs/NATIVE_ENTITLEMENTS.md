# Opt-in funded native entitlement owner

`SourceQuoteExactOrRejectV1` is a new, explicitly configured mechanism. It is not a deployed genesis policy or historical acceptance claim. It provides no authenticated actor, parent-UTXO, signature, automatic volume schedule or funded type0 issuance authority.

The actual `Ledger` is the only balance owner. `NativeState` retains an immutable independently expected policy, one ordinary bootstrap witness, non-owned remaining markers and exact event receipts. Adoption checks the whole canonical pre-Ledger commitment, actual issuer/kind/current supply and real available funding, then transfers exactly the configured target amounts from each funding owner into deterministic native escrows. It never credits a declaration, residual or mint difference. Joint/Economic V1 construction requires that exact pristine adoption at the network genesis Tip; positive-height migration remains unsupported.

Every policy field is required: network/bootstrap, pre-Ledger commitment, operator, four distinct role/issuer/kind/current-issued definitions, source catalog classes/declarations/optional ceilings, the two exact coupon→target role pairs, curve scales, initial markers, actual funding owners/amounts, quote policy, driver, coverage policy and payout order. No `Default` selects economics. Ledger Fixed/Managed kind remains separate from source Fixed/Vesting/Native class. `CoverInitialMarkersV1` requires funding to cover the initial marker sum; `PerRequestOnlyV1` makes no aggregate backing promise and still rejects any individual unfunded request.

`SourcePerCallV1` consumes each complete supplied target, preserving cumulative repeats. `MonotonicDeltaV1` stores the absolute observed target and consumes only its nonnegative difference. This choice defines arithmetic, not target authority. The actual local `EconomicInput::NativeTarget` carries its original ID, claimed operator and target at its recorded input position. The input is permanently `ExplicitUnverified`; its original ID enters the real global/Ledger partition exactly once. Physical payout IDs are deterministic and receipt-bound, with collisions rejected against all owner buckets. It is neither a wire transaction nor a system/type18 epoch.

Coupon type2 sends move the original coupon amount and the source whole-token marker drag together. The source first tests integer `ROUND_DOWN(amount)>0`, then calculates `marker * (amount / available)` with20-decimal BigNumber division and floors the drag to whole tokens. The mechanism preserves this whole-token behavior. It rejects lossy source Number bridges, including final coupon/marker storage, rather than silently repairing them by atom-floor arithmetic.

Targets quote each owner as `Number(request * (couponAvailable / curveScale))`, preserving20-decimal source division. Exact eight-decimal quotes, marker availability, receiver Number storage/overflow and the total physical escrow debit are preflighted before any mutation. ASCII account payout order is an explicit deterministic repair of source Map insertion order. A failure rejects the entire transition; no clipping, redistribution or fallback mint is available. Remaining markers are not burned coupon tokens and do not enter spendable totals.

The reachable1.1-send discrepancy is retained in genuine source evidence: starting with A/B coupon/marker10/10, sending1.1 produces coupons8.9/11.1 and markers9/11. A full20-target/20-scale quote exceeds B's marker. The source with a synchronous DB adapter pays A before failing on B; its actual async DB factory instead causes the helper to return no addresses and pay nobody. The funded mechanism atomically rejects the request. Two further unchanged-source scenes cover the string→Number whole-drag loss above2^53 and post-payout target storage loss. These observations are not presented as funded source success.

All configured coupon transfers outside the coupled handler and all reserved-asset grants/burns/channel operations reject. Both native escrow transfer endpoints are reserved. Actual Spot/contract/margin/fee/AMM legs involving target tokens still pass through the same Ledger and append exact target-transfer records. Coupons cannot become Spot assets or contract collateral. Existing channel/pending liabilities for native assets reject adoption; signed-channel/native coexistence is explicitly closed pending a shared independent policy and scoped witness budget. Legacy unsupported embedding owners reject native metadata.

TLL5 embeds an ordinary current Ledger and the complete native metadata. The independent native-aware reader reconstructs adoption and every native event against the pinned native-free witness, compares the actual custody of all four roles, exact `(target,escrow)` pairs and replay IDs, and rejects malformed receipts. It accepts no nested native/signed witness. Public ordinary Ledger readers reject the reserved native custody namespace; a native-aware reader also rejects downgrade to ordinary bytes even when the funding/escrow balance is zero. Ordinary snapshots retain their encoding when no native policy or reserved native custody is present.

TLSP2/TLJP2/TLJE3 carry native-aware ownership only with an independently supplied identical policy. Native Joint chain checkpoints use TLJC3, or TLJC4 for an exhausted retained-history floor; TLJC1/2 bytes and validation stay unchanged for ordinary owners. Economic TLEC8 stores the independently compared complete config, pristine native-aware Joint bootstrap and all ordered typed input frames. It reconstructs the entire actual owner, including orders, matching, fees, margins, IOUs, markers and physical custody. A source-enabled owner additionally retains/reexecutes complete source frames. No stored validity answer or caller disposition is used as execution authority.

Source mode forbids explicit local targets. A genuine source activation and a retained rejected native-send attempt are tested; accepted source sends remain closed because the owner lacks accepted property/clearlist catalogs required by the actual validator. The policy's source class is not promoted into a fabricated accepted catalog. The immutable actual-owner source backend does project real native remaining markers, including marker-only rows, while keeping their spendable amount zero.

Economic/Joint rollback owners check exact retained journal/frame prefixes and require each newly introduced suffix to belong to that transition's actual height; old processed IDs cannot become new native originals or payout legs. The existing outer owners retain actual parent/header checks. Native receipts contain only the available network/height, never an invented parent hash. Durable Economic uses the existing exact head/CAS, OS lock, synced Journal and poisoned-I/O health boundary.

Limits are explicit:4096 native events,4096 aggregate markers,16384 aggregate receipt rows,512KiB policy,8MiB total TLL5 body plus CRC,4096 economic frames and the existing65536 nested typed-record/64MiB checkpoint bounds. Live native mutations preflight canonical encodability before publication. Bootstrap witness/current-body decoding cannot reset a recursive native/signed validation budget because those combinations reject structurally.

The owner has8 Ledger tests,2 actual Joint tests and7 actual Economic tests.
They cover three networks, real funded matching/fees, both drivers, strict
last-leg rollback/retry, Number boundaries, valid-CRC foreign-asset and retained
history splices, source composition/closed capabilities, complete restart,
history eviction/floor/rebranch, durable CAS/ABA freshness and OS locking.
The additional `native_insurance_composition` target uses the native target
property as actual derivative and insurance collateral: it preserves coupon
sends, target payouts, real book closeout and a funded backstop through v9
restart, independently wrong policy rejection, rollback/rebranch and synced
durable recovery. Repository gate results are recorded in
[roadmap progress](ROADMAP_PROGRESS.md).

## Bitcoin TL + TLVEST token structure (opt-in policy v2)

The Bitcoin native policy is now **TL** (property ID 1) plus **TLVEST**
(property ID 2). Only TL is the liquid native token. TLVEST is the
coupon/vesting entitlement backed by a committed pool of already issued TL.
Both balances are independently pinned at genesis. The constructor
`NativeEntitlementPolicy::bitcoin_tl_with_vesting` requires the exact
supply of each, the issuer, a pre-Ledger commitment, the vesting funding
owner, the prefunded TL amount and the initial TLVEST marker owners. It
creates one source-compatible TLVEST→TL binding with full coverage,
without minting tokens.

TLI and TLIVEST are absent from this v2 native policy. In the preserved
historical source, property 3 is an income coupon and property 4 is its
payout asset; those original four-property fixtures and the v1 policy
decoder remain unchanged for archival replay. The v2 policy has a distinct
`TradeLayer/NativeEntitlementPolicy/v2/` commitment and cannot be read
as v1. Existing four-property deployments require a separately approved
consensus migration; the v2 path supports fresh, independently funded
configurations only.

### Liquidity reward and BTC volume

`src/tl_liquidity_reward.rs` implements a **read-only quote**, derived
from `tradelayer.js/src/volumeIndex.js`. Its inputs are one executed trade
in BTC atoms, the amount of TL traded, net fee paid in BTC atoms, cumulative
BTC volume, an explicit BTC-volume threshold at which TLVEST becomes fully
vested, and the observed BTC-per-TL VWAP. The caller must authenticate
the trade, fee, volume and price, and ensure a matching funded reserve
before any settlement.

When a BTC/TL VWAP is missing, the source's numeric low fallback of
`0.001` base coin per TL becomes `0.001 BTC/TL` in this Bitcoin
profile. An explicit zero price is invalid. Effective trade notional is
capped by TL quantity priced at VWAP so an inflated execution price
cannot inflate the reward. Before full vesting, the quoted reward asset
is TLVEST (property 2). From the full-vesting volume threshold onward,
it is TL (property 1). Both outputs are nominal TL units converted using
the TL price, and both require existing funded custody.

The source fee share is `clamp(0.35 / (1 + log10(1 + V/1000)), 0.02, 0.35)`,
where V is cumulative BTC volume in whole BTC. The quote multiplies the
**actually paid net fee** by this share, with a further VWAP-based
notional cap. At the full-vesting volume, the share is frozen at its
then-current level, which is below 100% of net fees. Thereafter the
share halves at each completed tenfold increase relative to the
full-vesting threshold: a decade counter of 0, 1, 2, … applies factors
1, 1/2, 1/4, … . There is no 2% floor after full vesting, allowing
the post-vest reward rate to continue to decay.

The JavaScript baseline currently bases its attenuation on cumulative
**LTC** volume and credits property **3**. Those source details are
recorded as historical evidence; the new policy uses BTC and removes
TLI from reward issuance. The JavaScript `vesting.js` also retains an
older TLI conversion-value parameter, which must be retired from any
subsequent source-consensus upgrade.

The floating-point logarithm in this module mirrors the JavaScript
preview and is not a consensus-safe computation. It authorizes no
balance movement, fee diversion, reward issuance or automatic release
of TLVEST. A production transition needs an integer-specified
logarithmic share, a finalized BTC/TL VWAP source, a vested-volume
commitment, token/fee escrow debits, replay, rollback and funding tests.

New tests: `cargo test --locked --test native_tl_only` and
`cargo test --locked --test tl_liquidity_reward`.
