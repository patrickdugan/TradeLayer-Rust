# Actual Main row continuation

The existing Main invocation now has a factored begin/finish token. Legacy
wrappers preserve their source return conversion, rejection notifications, queue
leave, height drain, capacity check, alias reset and local rollback behavior.
The new opt-in row component holds actual row/id/kind/params/height/live/skips and
processedAny outside Clone/Eq Core. Primary and fatal validity PCs retain the SAME
borrowed financial queue across a native await. Resuming never reenters Main,
decodes again or marks the row again.

Only actual source evaluation constructs the row frame. The native prepared
selection/write supplies its real plan; an external caller cannot supply a
validity verdict or current financial snapshot. The moved controller consumes
the existing native owner and intent store, keeps its frame private and exposes
no mutable financial/index escape. Raw command intent precedes source begin;
native intent precedes append; exact-post verification and prefix acknowledgment
precede resolving the stored await. Accepted scalar fields and Ack precede public
result release. An ambiguous append attempt cannot be repeated blindly.

Complete control observations stream scalar fields, including retained rows,
parameters, results, validity receipts and effects. No aggregate remaining-row
snapshot or early effects drain bypasses the existing limits. The original
65536-effect guard is checked at both prefix and actual finish boundaries.

Root checks pass 13 row/controller tests plus 55 compatibility executions across
13 targets. They cover one decode/mark, same-await exact-post resolution, primary
find rejection, fatal mark and second-validity escape, mixed skipped/new rows,
live processedAny, retained financial writes and moved-owner intent/ack order.
Existing mixed history spanning23 transaction types, old policy/Invoke bytes,
native step/history and newer rich/TxIndex tests pass. Counts reuse controls and
are excluded from the last full-release total.

The checked-in observer executes unchanged pinned Main/Logic/Activation/Vesting/
Tally bodies with genuine NeDB1.8 and both genuine numeric profiles. Four distinct
calls/eight input-output observations per profile establish that an activation
save failure is swallowed and remains valid. A tally initialization read failure,
or the sixth tally update after five successful balance writes, genuinely rejects
Logic: Main still returns true while recording valid=false and the exact leading-
space Logic-error reason. Rust preserves these differences. Failure facts and raw
inputs are execution inputs; recorded params/financial state are observations only.

Cargo formatting, all136 included implementation formatting checks, all-target
Clippy with warnings denied, supply-chain and per-type inventory checks pass.
Source/input/log digests are in
`fixtures/reference_fidelity/native-row-first-core-integration.json`.

This is a one-shot row-list controller. It does not expose an old native head
after source execution, but next-command/head/history composition is unfinished.
Enclosing catchup/settlement PCs, typed no-index reopen, superseding native plans,
terminal suffixes and process-loss source reexecution remain follow-on work.
It is not full Main crash recovery, host power-loss proof, full-reference parity
or external authentication. Default GlobalClearingV2 and old wire versions remain
unchanged. TL exports checkable state-oracle data; UTXORef remains a separate consumer.
