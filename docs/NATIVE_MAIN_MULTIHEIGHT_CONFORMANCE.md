# Main catchup across multiple heights

Four unchanged original Main/NeDB cases are checked in with their complete raw
inputs, bootstrap calls, effects, metadata operations and source observations.
Both genuine numeric profiles regenerate the same fixture. Four Main catchup
calls and six real raw bootstrap calls yield eight top-level records per profile.
Repeated checks do not increase those distinct source-case counts.

Four new Rust tests execute the same raw bootstrap commands, never supplying
processed flags, decoded verdicts, financial snapshots or settlement results.
They check skipped9/funding10/trade11 ordering, the double native validity error
that escapes funding before either trade phase, clearing-factory rejection after
earlier settlement effects, and already-processed invalid funding skipping all
settlement while final height11 is saved. Actual financial prefix, native flags,
marks, decode counts, phase order and absent later effects are checked.

The initial four-test run and complete thirteen-test catchup regression run pass
(seventeen Rust executions, thirteen distinct tests). Both original-JS checks,
Cargo fmt, 149 included-file checks, all-target Clippy, supply-chain and inventory
pass. No production business logic or prior policies changed. These comparisons
establish the listed boundaries, not complete equality of every serialized
source-state/effect field, broad service parity, authentication or cold recovery.
Full-release promotion remains pending at the 1,714 Rust / 238 numeric baseline.
