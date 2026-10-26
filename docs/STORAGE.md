# Local disk journal and funded IOU restart

`src/storage.rs` supplies a standard-library-only, exclusively locked disk
journal. `Journal::commit` writes an ordered key/value batch and calls
`File::sync_all` before publishing the new in-memory state. Opening the journal
replays only complete, checked batches. This is a local persistence adapter,
not NeDB query/migration parity or authenticated chain recovery.

The complete pinned `src/db.js` source is archived in
`fixtures/legacy_db_source.txt`. `scripts/storage_layout_vectors.cjs` executes
that actual source against explicit filesystem/client/NeDB adapters to check
the 32 category names and lowercase `<chain>-main` / `<chain>-test` layout.
Rust bounds chain identifiers to 32 ASCII alphanumeric, hyphen or underscore
bytes, and rejects unknown categories. It writes `category.tldb`; existing
`category.db` NeDB files are never opened by this adapter. Rust's explicit root
must already exist. Environment-variable resolution, RPC bootstrap, singleton
ownership and NeDB document predicates are caller/migration responsibilities.

`Journal::open_existing` restores an existing file without creating missing
roots, namespace directories or journals. Durable runtime owners use this API
to keep a failed restore from silently becoming a fresh empty state.

```rust,ignore
let ns = Namespace::for_mainnet(TradeLayerMainnet::Bitcoin, "iou")?;
let (mut journal, recovery) = Journal::open(root, ns, Recovery::RejectIncompleteTail)?;
journal.save_funded_iou("tip", &funded_state)?;
// Drop/restart, then reopen with independently chosen network and atom policy.
let restored = journal.load_funded_iou("tip", TradeLayerMainnet::Bitcoin,
    PayoutAtomPolicy::Exact)?;
```

Funded IOU storage is restricted to the matching BTC/LTC/DOGE **mainnet `iou`
namespace**. The complete TLFI checkpoint includes the sole ledger, IOU buckets,
claim ordering, escrow ownership, replay sets, height, genesis and atom policy.
Restore verifies its existing typed snapshot invariants and configured network
and policy. A successful write does not authenticate imported losses or matches.
It also does not atomically commit caller-owned state outside this checkpoint.

## Format and failure semantics

The header is `TLDB\x01`, chain/category lengths and bytes, a test flag and CRC32.
Each transaction has `TXN1`, payload length, monotonically increasing u64
sequence, preceding frame/header CRC, a separate header CRC, an ordered mutation
payload, frame CRC and `DONE` footer. Payloads contain a u32 count and strictly
ordered unique UTF-8 keys; put and delete are distinct tags. There is one
committed sequence per batch. Empty values and deletion of absent keys are valid.
Duplicate mutation keys and empty batches reject before disk changes.

All lengths, item counts, individual values, aggregate encoded batch size and
resulting live-store size are checked before copying values or starting writes.
Limits are 256 key bytes, 64 MiB per value, 65 MiB encoded batch, 1,024 operations
per batch, 4,096 live keys, 128 MiB live key/value bytes and 512 MiB log bytes.
These bound accepted data and transient copies; they do not promise a measured
peak RSS. Replay holds the current map and one bounded batch at a time. There
is no compaction: a full log returns `LimitExceeded` without changing state.

`Recovery::RejectIncompleteTail` leaves a short final header or payload/footer
untouched and returns its offset. The caller can explicitly reopen with
`DiscardIncompleteTail`; that mode truncates to the last complete transaction,
syncs the truncation and reports discarded bytes. A complete invalid header,
CRC, commit footer, sequence, chain, namespace or payload rejects even in that
mode. The separately checked header prevents a damaged complete frame length
from being mistaken for a normal torn payload. A short store header is never
auto-repaired. Operators must retain evidence and choose recovery deliberately.

After any uncertain append/seek/sync result, the object is poisoned: reads,
enumeration, sequence queries and further commits reject. Drop and reopen to
discover whether that batch reached disk. In particular, an error is **not** a
promise that the batch was absent. Callers must reconcile transaction identity
and replay IDs rather than blindly issuing a new economic operation.

One OS exclusive lock is held on the opened file until drop. This prevents
cooperating writers; it does not defend against a privileged process replacing
the path or writing without respecting advisory locks. Use a trusted local root
with stable filesystem identity. No multi-file transaction, online truncation,
network filesystem lock guarantee, encryption or malicious-tampering defense is
provided. CRC chaining cannot detect deletion of the final complete transactions
or replacement with an older valid journal; an independently trusted chain tip
and state commitment remain necessary.

File syncs are attempted on every commit, successful reopen and explicit truncation. On Unix the
adapter also syncs the root after creating the namespace directory and the
namespace directory after opening/creating its journal. The caller owns root
and ancestor creation/durability. Windows uses file sync; explicit directory-entry
power-loss durability remains unverified. Storage-device/controller guarantees,
power-loss tests, deployment filesystem behavior and full economic recovery
must be validated on the intended host. The locking API is available since
Rust 1.89; this repository uses pinned 1.90. See the
[standard library file documentation](https://doc.rust-lang.org/stable/std/fs/struct.File.html#method.try_lock).

## Verification

Tests cover real disk reopen, ordered atomic batches, all byte truncations of a
second transaction, every single-byte corruption of a complete transaction,
explicit recovery followed by another commit/reopen, namespace mismatches,
same-process and child-process lock exclusion, invalid-batch nonmutation,
malformed canonical payloads, CRC/sequence chain errors, capacity bounds, a real
failed write poisoning all access, and funded IOU escrow/replay/policy retention.
They are local tests, not machine power-loss or malicious-storage certification.
