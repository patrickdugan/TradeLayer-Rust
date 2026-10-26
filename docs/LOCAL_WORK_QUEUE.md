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
| Native genesis | Local source audit and owner design | Source evidence verified; owner design in progress | Resolve declared supply versus source credits and BTC residual before selecting a conserving actual-owner policy |

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

The current integrated batch passes 1,199 top-level Rust tests plus one
subprocess lock check across 148 executable targets. Formatting, explicit
included-file rustfmt, all-target frozen Clippy with warnings denied, the
zero-third-party policy and 11 mutation tests, and all-target release pass.

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
