# Local branch consolidation

The user requested integration of the 72 previously retained branch heads into
local main. `reconciliation.json` records every original tip and its resolution.
The original 75 non-ancestor heads (including three separately proven content
duplicates) are also preserved in a verified Git bundle before consolidation.

The local merge gate uses focused financial/native/scanner regressions,
all-target pinned lint and formatting, provenance and independent oracles.
The separate cold full preflight remains pending; this is not an RC or a
publication claim. `validation.json` distinguishes the completed checks.

## Reconciliation

- 18 heads are included through actual feature merges and their shared ancestry.
  These include the formal ledger helpers, read-only wave/funding evidence,
  scanner schedules and reducer variants, Bitcoin RPC benchmarks, executable
  contracts, Litecoin evidence, conditional administrator replay and storage
  research adapters.
- 27 heads match merged PR heads exactly, and their squash merge commits are
  present in main or the integrated canonical formal branch. Their current
  replacements are retained rather than restoring obsolete source snapshots.
- 27 historical CI preview heads add temporary merge scripts, workflow
  definitions or source snapshots. Their unique artifacts are preserved with
  their original Git blob hashes under `retired-previews/`. Shared proof/source
  work is already in the canonical combined tree. The one addition-only Kani
  source patch on a preview is already present byte-for-byte there too.

Archived preview definitions are historical evidence, not active workflows or
instructions to run against this checkout. The canonical combined Rust, Lean,
Kani, Bitcoin and Litecoin checks remain active. Research checks have not been
substituted for production validation.

## Integration decisions

The current native snapshot validation remains inside the staged ledger batch
commit, together with the incoming clone/validate/commit proof helper. The
ordered column backend supports the newer native and signed-channel reads.
Its sorted-vector insertion costs still require workload-specific performance
evaluation; proof models alone do not establish throughput.

The default bounded windowed scanner remains unchanged. Whole-block, weighted,
pooled, barrier, tree/hash reducer and in-place Merkle variants are explicit
read-only alternatives. The benchmark retains phase rotation and the original
pairwise counterbalanced tree/hash comparison. Every measured result is checked
for equality outside its timer; no speed threshold or memory promotion is
inferred from a synthetic smoke test.

Litecoin MWEB parsing is explicit through `inspect_litecoin_transaction` and its
own observation type. Bitcoin's borrowed structural scanner keeps its flag
validation and allocation behavior. MWEB opaque bytes are not a proof of MWEB
consensus. The Litecoin replay gate continues to deny economic admission.

SQLite fixture producers write canonical LF frames on Windows as on Linux;
tests close connections explicitly while retaining transaction semantics.
Canonical frame checks and all rejection assertions remain intact.

## Validation and remaining gates

See `validation.json` for completed checks, failed attempts and remaining gates.
The repository's pending full preflight runs directly on Windows when Git Bash
cannot initialize. Its commands include Cargo formatting, every independently
listed included source file, all-target Clippy with warnings denied, the Main
parseInt guard, the full Cargo tests and the independent native wallet oracle.

Lean, Kani, Linux directory-sync/crash durability, node-backed Litecoin replay,
representative real-block CPU/peak-RSS measurements and deployment validation
remain separate gates unless recorded as completed. Hash consistency does not
authenticate a chain, oracle publisher, script or ledger transition. TL exports
checkable state-oracle data; UTXORef remains a separate consumer repository.

The unrelated queued-startup proposal in the primary checkout is preserved
separately. Its uncommitted tests are not promoted by this integration.
Nothing in this consolidation authorizes a push, a PR or remote deletion.
