# Complete source catalog checkpoints

TLECv10 is the economic checkpoint wrapper for deployments that opt into the
actual source property and clearlist owners, with an optional actual oracle
owner. Deployment configuration supplies
two independent pins: the pristine Joint bootstrap commitment and the complete
source catalog bootstrap commitment. Restoring a checkpoint never adopts pins
from its own bytes as deployment authority.

The byte format is:

| Field | Encoding |
| --- | --- |
| Magic/version | `TLEC` followed by byte 10 |
| Expected Joint bootstrap commitment | 32 bytes |
| Source policy | byte 1 for `ConservingSequentialV1` |
| Expected source catalog bootstrap commitment | 32 bytes |
| Source-free complete economic owner | u32 length followed by canonical owner checkpoint |
| Complete original source audit | u32 length followed by the existing source audit encoding |
| Complete initial source catalogs | u32 length followed by the combined owner checkpoint |
| Complete current source catalogs | u32 length followed by the combined owner checkpoint |
| Checksum | CRC32, four bytes in big-endian order |

All lengths are big-endian. The outer writer and reader retain the existing
64 MiB economic checkpoint limit. Source audit encoding retains its own
16 MiB, frame, transaction, metadata-node and reason limits. Each catalog's
checkpoint parser enforces its own limits and canonical representation.

The source audit stores the original Joint bootstrap and every original source
frame, including rejected and duplicate candidates, original raw metadata,
source execution order, receipts, evaluation commitments and reasons. The
catalog wrapper uses the existing audit reader/writer rather than deriving a
reduced audit from applied typed economic inputs.

The catalog sections use the actual owners' complete checkpoints. Property
documents, cache values, unknown metadata and replay journals are retained.
Clearlist collections and singleton maps are retained using the full owner
checkpoint rather than a database-only restart projection. The initial owners
are distinct from the current owners: replay starts from the independently
pinned initial state and compares the complete final result.

Restore checks both configured pins before accepting state. It decodes and
canonically re-encodes the catalog checkpoints, checks the actual initial
catalog bytes against their expected commitment, and checks the source audit's
actual Joint bootstrap against the expected Joint commitment. It decodes the
embedded complete economic owner with source configuration disabled, then
installs the original source audit and both catalog states. Full economic
verification replays every original candidate from both pristine bootstraps
and compares the entire resulting owner. Finally, the complete TLECv10 bytes
must match canonical re-encoding exactly. A checksum repaired after stripping
history, replacing a catalog or altering a rejected candidate does not satisfy
those replay and equality checks.

When the initial catalogs contain the optional oracle owner, restore instead
builds the entire economic owner directly from both independently expected
bootstraps and every original raw frame. Supplied inner owner bytes and current
catalogs are comparison targets; they do not seed that reconstruction. This
preserves oracle documents, cache/watermarks, retained source exceptions and
funded price observations that the source-free typed history cannot replay.
See [SOURCE_ORACLE_CHECKPOINTS.md](SOURCE_ORACLE_CHECKPOINTS.md).

Native entitlement deployments compose through a canonical source-free TLECv8
owner embedded in TLECv10. Its native policy, original typed native/contract
frames, exact Ledger custody, replay markers and economic books remain whole.
The wrapper does not turn a native deployment into a bare Ledger checkpoint.
Metadata-only source inputs are allowed only during the existing private
history replay paths; public typed source-free application remains denied.
Insurance plus source catalog execution remains outside this composition.

When neither native nor contract typed histories exist, the embedded generic
owner still retains metadata-command replay markers. The wrapper supplies their
identities from the full source audit through a private decode context, then
clears that temporary context before installing and verifying the outer owner.
The public generic decoder always uses an empty context and rejects a stripped
inner checkpoint containing otherwise unexplained metadata markers. Native and
contract owners derive those identities from their actual typed frames. These
temporary identities neither add a new serialized section nor replace the full
original-source replay check.

The implementation is `src/economic_source_catalog_snapshot_impl.rs` and the
snapshot adversary suite is `tests/economic_source_catalog_snapshot.rs`. The
tests cover complete owner round trips with and without native history,
independently expected pins and deployment policy, retained rejected source
metadata, canonical catalog splices, property history stripping, clearlist
stripping, raw audit/receipt tampering, frame stripping, truncation and trailing
bytes. Adversarial replacements use freshly recomputed outer CRCs. These are
structural and replay-integrity checks for the explicitly unverified owner;
they do not authenticate L1 transactions or authorize a deployment bootstrap.
