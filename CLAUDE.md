# TradeLayer agent notes

Read [AGENTS.md](AGENTS.md) before editing or pushing changes. In particular, run `bash scripts/ci-preflight.sh --full` before every Rust-source push so format, Clippy and unit-test errors are resolved locally.

## Current continuation

Read [CLAUDE_HANDOFF.md](CLAUDE_HANDOFF.md) before continuing. Preserve the dirty working tree: it contains tested but not yet linted/committed components and an older unresolved startup proposal. Do not reset it or overlay old whole-file previews. Commit completed checked work on the current branch, stage only task-owned files, and do not push, publish or open a PR without explicit user authorization.
