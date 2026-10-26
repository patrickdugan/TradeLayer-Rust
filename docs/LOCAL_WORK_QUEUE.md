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

The complete integrated batch passes 1,161 top-level Rust tests plus one
subprocess lock check across 144 executable targets. Formatting, explicit
included-file rustfmt, all-target frozen Clippy with warnings denied, the
zero-third-party policy and 11 mutation tests, and all-target release pass.

The parent integrator retains ownership of shared exports, workflow gates,
Registry and sole-Ledger coordination, complete Cargo checks and local commits.
The current instruction permits local commits only; pushing, publishing and
pull requests require a separate explicit request.
