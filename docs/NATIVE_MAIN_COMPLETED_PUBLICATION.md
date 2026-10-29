# Native Main completed-command publication

The moved row/catchup controller retains the original financial owner, source
frame, native index and command permit while final control writes are pending.
The original session head/history remain private until the actual completed
command has an independently verified final ACK. No financial snapshot is
accepted as a recovery input.

## Final barriers

Before source work, the constructor captures the original native physical
binding, native head/history length, source replay head and existing effects,
and reserves the history slot. At actual source completion it prepares the
existing logical receipt/head bytes from the real grouping, validity, effects,
financial commitment and complete native metadata observation. The Accepted
frame bytes and publication context are captured once before final operations
exist; recovery does not recompute them or create a circular control commitment.

`prepare_completed_acceptance` and `prepare_completed_acknowledgement` retain
sealed Store operations with exact expected Before/Post pins before IO. Typed
Prepared/Attempted/Synced PCs distinguish the two awaits.

| Observed control outcome | Permitted continuation |
| --- | --- |
| Healthy, exact independently retained Before | Retry the same sealed operation on the same Store; detaching it is refused. |
| Ambiguous sync result | Retain the unresolved command, permit and all financial/source owners; drop only the control Store through the typed no-Store loan. |
| Exact independently expected synced Post | Open without tail repair and rebind only the retained same-command permit. AcceptedPost resumes only final ACK; commandAckPost resumes only guard/install. |
| Foreign, stale, unknown, torn or missing referenced bytes | Refuse without repair or publication and return the same retained owner. |

Final recovery never reruns decode, logic, markProcessed, the row frame or its
native append. Both ordinary and explicit superseding-intent Store profiles
retain their existing wire shapes. In particular, an initial CommandIntent
with no native write keeps its absent `nativePrefix` key; later Accepted/ACK
records keep their original null representation.

## One publication boundary

After verified Idle ACK, the guard checks the actual command, native prefix,
accepted blob, previous ACK and saved publication context; original head/history
and source replay head; current complete metadata/financial/effect state; frame
result/grouping/validity; and the unchanged logical receipt/head algorithm.
Every fallible check precedes the single effect drain and reserved history push.
Completed receipt/head access and owner release remain unavailable before that
install. Published effects remain available through the committed receipt;
repeated completion does not drain or append again. Guard failure after ACK
retains the owner and effects and refuses release.

## Evidence and remaining gates

Parent's actual compiled Core log `work/root-native-final-controller-first.log`
records 32 intended passing controls: 11 publication, 9 sealed Store, 3 empty
command and 9 final-controller recovery tests. These include real before/synced
post failures, both final lost ACKs, no-prefix commands, original/superseding
rows, torn/foreign/missing-blob refusal, stale heads, capacity/local stops,
guard refusal after ACK, next-command receipt parity and drain-once behavior.
The broad test selection also included a separately known failing active-service
test; that command exited 101. Parent then reran the exact session module
prefix: `work/root-native-final-controller-scoped.log` records all20 publication
and final-controller tests passing with zero failures. These are scoped gates,
not a full-suite pass.

Cold reconstruction of financial/source owners after process death remains a
separate replay problem. The control helpers recover only a retained actual
permit and owner. The active-service running component and its bounded source-effect comparisons
are now integrated; its durable WAL linking source inputs, pending source PCs,
publication and next-command admission still requires integration/crash gates. Final
control recovery alone does not authorize source replay or manufacture a new
owner. File-sync/injected crash tests do not establish Windows power-loss
durability, and integrity hashes do not authenticate an actor or chain state.

The subsequent combined gate passes 318 library tests and 25 public compatibility
checks, with all-target Clippy and formatting clean. These checks leave the last
full-release baseline at 1,714 Rust / 238 locked numeric-reference executions.
