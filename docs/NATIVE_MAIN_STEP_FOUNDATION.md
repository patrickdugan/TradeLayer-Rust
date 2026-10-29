# Native Main step foundation

The explicit Native13 step controller now owns the existing financial replay,
native metadata index and source scheduler together. It executes original pause,
timer, shared-height, block and file-retry continuations without cloning a second
financial owner. The immediate and file-retry profiles have distinct admission
and archive contexts. Legacy APIs and default GlobalClearingV2 are unchanged.

The command-only decoder checks the complete archive before source execution,
then reconstructs one fresh owner by executing raw bootstrap and commands. Kernel,
result and financial observations are comparisons, not state constructors. Large
command inputs use complete scalar frames. Original shutdown remains terminal
after a local budget stop and never drains pending timers or saves source state.

Segmented history publishes a bounded whole-record prefix and releases only its
receipts and exact input-byte count. A lost acknowledgment preserves that same
prefix for retry without executing commands again. The reviewed correction fixes
the 35-command byte-window stall. Typed admission distinguishes a retained
publication from a single receipt exceeding the declared 64 MiB segment boundary.
No unlimited history/resource or synced filesystem-publication claim follows.

Prepared native appends expose independently retained before/intermediate/post
candidates before file IO. The intent store syncs raw command intent, native
intent, accepted prefix and acknowledgment as separate steps. Namespace and
persistent identity are checked before opening the store. Prepared validity reads
all matching metadata rows, selects the original first suffix and holds distinct
read-generation and current-write pins. Exact post-reopen resolution avoids a
duplicate append. These are metadata components; actual financial Main row/catchup
program counters and durable binding are still unfinished.

Root validation passes 88 focused Rust executions. They include nine append
checks, seven prepared-validity checks, five source scheduler/file checks and 67
public/compatibility checks. Two ignored entries are child-test runners; five
append and eight intent-store process exits are checked by their parent tests.
Counts reuse existing controls and are not added to the release total. Tests
include 1,100-row reads, 4,353 commands across 18 segments, unchanged independent
byte-pressure controls, complete original-source file continuations, native13
replay and the newer rich/TxIndex regressions.

All 131 included formatting checks, Cargo formatting, all-target Clippy with
warnings denied, supply-chain and per-type inventory checks pass. The source
handoff also preserves four genuine default/locked full/proof regeneration
checks, original failing evidence and the fbd667/cc3 corrections. Input/log digests
are recorded in `fixtures/reference_fidelity/native-foundation-core-integration.json`.

The last full release remains 963aae7 with 1,714 Rust executions and 238 numeric
reference checks. Safe complete-KV-before-tail-repair integration, actual row and
catchup crash resume, superseding intent recovery and host power-loss validation
remain separate gates. Per-type full parity, complete-block proofs and external
authentication remain open. TL produces checkable state-oracle data; UTXORef is
a separate consumer repository. A matching digest does not authenticate inputs.
