# Main recovery when the control Store is absent

The same actual financial/index owner, source frame, command, permit and sealed
operation now move through a distinct no-control-store type. Only the poisoned
or ambiguous Store is dropped. No dummy Store, financial clone, new permit or
source-result importer is introduced. One independently expected synced Post can
reopen and rebind that retained permit. Source resume or replacement installation
occurs in a separate Synced PC, preventing a second ACK/intent sync.

A healthy Store at independently known Before remains live and can retry only
the retained exact prepared operation. The no-Store loan refuses that case.
Unknown/torn control state refuses unchanged and returns the same whole owner.
Prepared ACK success resolves the same source await once; recovered superseding
intent sync precedes actual replacement-plan installation and fresh append.

Six new private tests pass: original ACK lost-ack, healthy blob-write retry,
torn-tail refusal, superseding-intent lost-ack, two distinct rows in one command
with first supersession then ordinary NativeIntent under the explicit profile,
and fatal-validity recovery without another mark/decode. The complete library
passes 275 tests with eight threads (three child entries ignored), and 25 public
compatibility tests pass. Including the initial six-test run, there are 306 Rust
executions, with overlapping controls. Four duplicated temporary-folder prefixes
in separate test modules were made distinct before the parallel run; production
payloads remain identical to the immutable worker freeze.

Cargo fmt, 152 included-file format checks, all-target Clippy with warnings
denied, supply-chain and inventory pass. This is same-process typed recovery for
native-prefix ACK and superseding-intent sync. Completed-command publication,
Accepted/final command ACK recovery, service/terminal WAL and cold process-loss
source replay remain required. No host power-loss or authentication is claimed.
Old source policies and default GlobalClearingV2 stay unchanged. These checks do
not promote the last full-release baseline of 1,714 Rust / 238 numeric tests.
