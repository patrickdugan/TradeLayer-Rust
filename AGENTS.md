# Agent development instructions

## Default workflow: checked local commits

Work on the current branch. Complete the implementation, run the relevant checks, and commit the completed work locally before reporting it done. Stage only task-owned files and preserve unrelated changes.

Create a branch or pull request only when the user explicitly requests one for that specific work. A general request to build, fix, continue, parallelize, or use subagents authorizes local work and commits. It does not authorize branches, draft PRs, merge-preview branches, or a separate PR for each stage, agent, experiment, or check.

Keep a coherent change together in local commits. Use local staging directories for parallel workers and experiments. Record unfinished work and remaining checks in the handoff; a draft PR is not the default way to package incomplete work.

Push, publish, or merge only when explicitly authorized. Never delete branches with unique work based solely on their name or age. Verify containment or preserve a recoverable backup before authorized cleanup.

## Checks before an authorized push

Before **each GitHub push that changes Rust source**, run `bash scripts/ci-preflight.sh --full` in the repository root. This checks the pinned Rust formatting, the included implementation files that Cargo fmt does not cover, Clippy with warnings denied, and Rust tests. Fix the local errors and amend or update the commit before publishing the branch. For local iterations, `--fast` omits the Rust test run; the full command remains required before pushing.

When altering Lean or Kani work, run its relevant local verifier as well. The preflight script covers only the first Rust CI gates. Leave GitHub's broader Bitcoin, Litecoin, Kani, Lean, and replay audits active, and keep proof claims scoped to checks that completed successfully.

Prefer one tested branch update over a sequence of knowingly failing intermediate pushes. Preserve failing evidence and diagnose regressions instead of weakening CI checks.
