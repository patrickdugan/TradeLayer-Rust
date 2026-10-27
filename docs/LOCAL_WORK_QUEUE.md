# Local port queue

Updated 26 October 2026. The source pin is
`dca0888b827c5bc28fa9189bc9e1d507f5c5670c`. The complete module inventory and
remaining gates stay in [PARITY_ROADMAP.csv](PARITY_ROADMAP.csv).

| Work | Owner | State | Next handoff |
| --- | --- | --- | --- |
| Full validity source rules | Local validity agent; parent integration | Integrated source conformance | 43 exports and 564 observations; complete actual-owner dependency coverage and authenticated admission remain separate |
| Full Types dispatch and economic mapper | Local Types agent; parent integration | Integrated source and supported mapping subset | 1228 observations, including two explicit UTF-16 domains; extend genuine represented handlers without fabricating metadata |
| Sequential consensus owner integration | Local consensus agent; parent integration | Integrated sole-owner subset | TLECv7 raw/rejected history replay, candidate atomicity, once-per-block phases and exact rollback prefixes; authorized ingestion and high-anchor history remain open |
| `activation.js` | Local team | Integrated source and canonical owner-history subset | 18 methods plus constructor/static singleton; 670 exact observations plus5 explicit domains; derived accepted origins, prior-height queries, restart/rollback and exact durable head; native genesis and authenticated historical provenance remain open |
| `channels.js` source | Local team | Integrated bounded source conformance | All43 static methods and constructor; explicit prototype/reference domains remain rejected; diagnostic metadata has no custody authority |
| Canonical signed allocations | Local team | Integrated opt-in funded owner | Independently pinned SupersedingCoSignedAllocationV1; both signatures, actual escrow, full recovery and retained transition checks; L1 funding/scripts/revocation remain external |
| Native genesis/entitlements | Local team |24 source scenes and opt-in funded owner integrated | Authenticated type0/genesis, accepted property/clearlist admission, automatic schedule and positive-height migration remain open; no default economics |
| Main-owner insurance backstop | Local team; independent source support | Opt-in actual oracle-perpetual backstop integrated | Full source lifecycle, source+insurance raw replay, ADL and authenticated admission remain open; external source artifacts are unimported |
| Source send/spot/oracle execution | Local team | Partial actual runtime ownership | Scalar/native sends, async real book reads, actual13/14/8 oracle documents and complete TLECv10 replay; V2 actual selected-contract reads now permit genuine raw matches and Paid funding24 under conserving settlement |
| Source contract queue and financial graph | Local team | Literal on-chain path composed | Actual raw18 queue/placement/settlement shares source Tally authority; all135 genuine observations compare complete collections/caches and failed prefixes. Replayed recovery and adapter adversaries are validated separately. GlobalClearingV2 now executes actual funding, three-pass marks, ADL/AMM/book liquidation, insurance/IOUs, option expiry and supply checks; exact V1 replay remains. Channel transaction settlement and authenticated history remain open; conserving EconomicState is a distinct policy |

The user reassigned activation to the local queue because the web worker
stalled. Its earlier external reservation is lifted. The local standalone source
implementation is now present in the checkout; this does not complete actual
Registry/Ledger activation or native issuance.

The six activation source paths are integrated locally:

- `src/activation.rs`
- `tests/activation.rs`
- `scripts/activation_vectors.cjs`
- `fixtures/legacy_activation_source.txt`
- `fixtures/activation_vectors.tsv`
- `docs/ACTIVATION.md`

The source lane covers all 36 registry entries, six literal network admins,
initialization, activation blocks/code hashes, repeated activation, strict
33-of-36 threshold and async persistence/failure behavior. All 18 methods plus
constructor/static singleton execute against the unchanged source. The corpus
has 670 exact observations and five explicit unsupported-domain observations.
Source placeholders and metadata success do not create funded genesis balances.

The [canonical history view](ACTIVATION_HISTORY.md) now derives accepted
activation origins from actual v7 owner replay, with exact Registry, network,
tip and durable-head binding. Funded native property/supply lifecycle and
authenticated historical administration remain open. Importing the standalone registry JSON or calling a source
backend does not grant that authority. The current sequential runner maps Type0
to the existing Registry activation subset and retains its conserving gates.

The earlier supported-source batch passed 1,230 top-level Rust tests plus one
subprocess lock check across 153 executable targets. Later source-catalog and
send/spot/oracle batch evidence is recorded separately under
`fixtures/reference_fidelity/`; the completeness workbook reports the latest
verified committed count. Source conformance, actual runtime coverage and
full-reference parity remain separate measurements.

The parent integrator retains ownership of shared exports, workflow gates,
Registry and sole-Ledger coordination, complete Cargo checks and local commits.
The current instruction permits local commits only; pushing, publishing and
pull requests require a separate explicit request.

## Co-signed channel integration

The new [fixed-pool owner](SIGNED_CHANNEL_OWNER.md) changes actual channel A/B
escrow only under the independently configured two-signature policy, with no
second spendable Ledger. All legacy mutators and pending obligations are fenced
after enrollment. Complete signed journals, pre-enrollment bootstrap witnesses,
exact retained transition heights/parents and a shared bounded witness-validation
session are checked on recovery. The [durable wrapper](DURABLE_SIGNED_CHANNELS.md)
publishes through one synced Journal value and fences uncertain cached reads.

The [complete source channel module](CHANNELS_SOURCE.md) stays diagnostic. The
source defines neither canonical signed allocation bytes nor L1 revocation. Old
revision signatures remain mathematically valid after local supersession; the
actual owner refuses them as a current head. Authenticated funding/outpoints,
participant identity, parent-UTXO/scripts, pruned header ancestry and historical
acceptance remain separate gates. No source module is marked full parity.

The native source audit is preserved in[NATIVE_GENESIS_SOURCE.md](NATIVE_GENESIS_SOURCE.md)
and offline Node fixtures. The next implementation requires an independently
configured pre-funded owner and no default economic policy. The web insurance
worker reports49 executed adversarial observations; the actual patch bytes are
not present locally. Those remain external evidence. The completed local review
checks real clearing/Economic custody and durable replay against the reported
mechanisms: 32 existing focused checks passed, with no new custody defect
reproduced and no increase to the full-suite test count. The review baseline had pending-fee custody only. The new main-owner backstop
now uses actual reserve custody, ordered market-first coverage and full
receipt replay, separately from the sandbox's synthetic full-book closeout.
