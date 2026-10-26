# Local port queue

Updated 26 October 2026. The source pin is
`dca0888b827c5bc28fa9189bc9e1d507f5c5670c`. The complete module inventory and
remaining gates stay in [PARITY_ROADMAP.csv](PARITY_ROADMAP.csv).

| Work | Owner | State | Next handoff |
| --- | --- | --- | --- |
| Full validity source rules | Local validity agent | In progress | Freeze complete source oracle, Rust port, tests and documentation for parent integration |
| Full Types dispatch and economic mapper | Local Types agent | In progress | Freeze orchestration, source observations, supported mappings and tests |
| Sequential consensus owner integration | Local consensus agent | In progress | Private block stage, complete raw/rejected input replay and versioned checkpoints |
| `activation.js` | Local validity agent, after validity freeze | Queued by user | Six isolated files below, followed by parent-owned Registry/Ledger integration |

The user reassigned activation to the local queue because the web worker
stalled. Its earlier external reservation is lifted. This queue entry records
an assignment, not an integrated implementation or passing validation.

The activation worker owns only these six new paths, staged first under
`work/full-activation-port/`:

- `src/activation.rs`
- `tests/activation.rs`
- `scripts/activation_vectors.cjs`
- `fixtures/legacy_activation_source.txt`
- `fixtures/activation_vectors.tsv`
- `docs/ACTIVATION.md`

Cover the full pinned module: all 36 registry entries, administrators and
network selection, activation history/code hashes and repeated activation,
block-dependent checks, strict greater-than-90-percent threshold, initialization
and async persistence/failure behavior. Execute the unchanged source in the
Node oracle. Distinguish actual source placeholders and defects from explicit
repairs; do not invent funded genesis balances.

The parent integrator retains ownership of shared exports, workflow gates,
Registry and sole-Ledger coordination, complete Cargo checks and local commits.
The current instruction permits local commits only; pushing, publishing and
pull requests require a separate explicit request.
