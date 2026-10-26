# Transactional funded IOU disk owner

`DurableFundedIou` owns a `FundedIouLedger` and the matching mainnet `iou`
`Journal`. Its API moves loss collection, match-profit claim accrual and funded
payouts through one staged operation batch. It exposes the staged result only
after the complete TLFI checkpoint and all replay IDs have been synced to disk.
No caller can mutate its live ledger independently of persistence.

This extends the existing [funded IOU accounting](FUNDED_IOU.md) and
[disk journal](STORAGE.md). Inputs still require independent authentication.
The explicit atom policy is a local accounting choice, not a historical rule.
This owner and `PersistentJointChain` are separate experimental state owners;
they must not be combined as two live ledgers for one TradeLayer. Integrating
all economic modules under one ledger and one checkpoint remains required.

## Commit and restore

`initialize_unverified` imports a complete existing funded state into an empty
namespace and persists it before returning. Existing records or an earlier
commit reject; initialization never overwrites an existing owner. An empty
header left by a failed pre-commit import can be retried deliberately.

`open_existing` never creates missing roots, directories or journals. It requires
the fixed single `funded-iou/state/v1` key, configured mainnet and atom policy,
and verifies all existing TLFI invariants. Extra records reject rather than
silently adopting a shared namespace. Restore can additionally require an
independently retained `DurableIouHead` containing sequence and fingerprint.
Without that comparison it accepts any internally valid local journal prefix;
that is not evidence of the latest chain state.

Every `apply_batch_unverified` requires the current head. A stale sequence or
fingerprint rejects before operation execution. The owner clones its complete
state, executes all commands in order, serializes the result, writes one journal
transaction, and then replaces the live state. Any validation, replay, resource,
funding or later-command failure leaves both the live state and file unchanged.
All command results retain submission order and return only after successful
sync. Receipts include actual credited atoms and source rounding differences.

The comparison fingerprint is SHA256d of an explicit domain prefix and canonical
TLFI bytes. It is a local equality token, not an authenticated state commitment.
The journal sequence advances once per batch, including initial import; replay
markers advance per IOU operation. Different batches can thus produce identical
economic balances with different replay sets and fingerprints.

## Uncertain writes

An append/sync error can occur after a complete batch reached disk. The owner
therefore denies all later state, head, replay-query and mutation access when
its journal is poisoned. Drop it and reopen. A complete committed batch restores
its replay markers; a repeated input rejects. A torn tail requires the caller
to explicitly select `DiscardIncompleteTail`, which truncates and syncs before
returning the previous complete state. `is_processed_input` exposes the outer
funded IOU input marker to reconcile a known submitted operation.

Explicit tail recovery occurs at the journal layer before later owner/head
checks. Therefore a restore that ultimately rejects an unexpected head may
already have discarded an incomplete tail if the caller selected that option.
Use rejection mode to preserve evidence until recovery is deliberately chosen.
A trusted prior head detects replacement by an older complete valid prefix;
retaining that head in the same untrusted storage does not add authentication.

## Resource policy and tests

A batch accepts 1 through 256 operations. Across payout results it retains at
most 16,384 allocations and 4 MiB of logical receipt payload: account UTF-8,
source amount text, signed rounding-delta text and 16 bytes for each credited
atom value. These are explicit local limits, not measured peak RSS or protocol
limits. The current individual payout is already bounded by the standalone
claim cap; the aggregate check prevents repeated zero-atom dust payouts from
retaining millions of allocation records. Exceeding a bound discards the entire
staged batch before a write. Journal and TLFI bounds also remain in force.

Tests run the complete claim/loss/payment cycle as one durable commit, reopen
and compare all state, reject replay and stale heads, exercise mid-batch rollback
and namespace/policy mismatch, prevent overwrite/missing-file bootstrap, and
detect complete-prefix rollback with an independently supplied head. Fault
tests perform real partial writes or lose acknowledgement after a real sync;
they verify cached-read denial and distinguish committed replay markers from
an explicitly discarded torn transaction. The capacity regression exercises
repeated dust allocations and whole-batch nonmutation.

Deployment power-loss behavior, trusted branch selection, matching/clearing
admission, production storage migration and a unified economic owner remain
open. This API does not promote an unverified checkpoint into chain authority.
