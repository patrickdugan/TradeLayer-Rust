# Original Main lower-transaction corpus

This work-only source corpus expands the original callable Main/TxIndex class
driver to raw1/2/3/4/7/8/9/11–15 and a real two-sided contract match. The complete
original Main and TxIndex class bytes, prototypes and method bodies are used.
Complete pinned Types, Validity, Logic, Activation and reached financial/catalog
modules execute over the same declared source DB adapter. No source financial,
validity, admin, matching or phase result is supplied.

The full corpus has **38 observations**: 25 actual Main method calls, eleven
constructor/input resets, one original raw-document import and one explicit
raw-RPC binding. Of the calls, **23** are available-source comparisons and two
are separately tagged unavailable-capability experiments. The first ten calls
plus reset are preserved as an eleven-row proof, with ten available-source
calls and no unavailable-capability experiment. Generation and `--check` pass
with genuine BigNumber 9.3.1 and source-lock 9.1.2, byte-identically. Exact
fixture, script and original class hashes are recorded in `validation.json`.

## Actual activation and inputs

Every scene uses the unchanged Activation module. Its original registry is
stored in the shared `activationsList` row and loaded by the actual source
methods. There is no declared-answer Activation profile for Main. The chain
initialization method is not invoked; its unavailable client/admin startup is
not fabricated. The corpus includes inactive rows and an active row with an
activation block later than the current height. The latter still passes the
source's placeholder `Validity.isActivated`, which returns true. This is an
observation of the source, not a replacement authentication or activation rule.

Input transaction identifiers are deterministic synthetic 64-hex values, not
assertions of Bitcoin inclusion. Claimed actors, property/oracle state, imported
rows and block heights remain unverified inputs. Full runtime state includes
persisted collections, Tally/Property/Oracle state, ClearList's actual singleton
maps, the separate genuine empty `Tally.instance` receiver, channel/withdrawal
caches, Margin instances, clearing contexts and reward
queues. Complete ordered DB, call, phase and rejection effects are retained.

## Executed behavior

* Fixed and managed issuance, grant/redemption, funded scalar send, ClearList
  creation and attestation, and Oracle creation/publication/admin updates run
  through actual Main transaction loops. Original wrong-collection ClearList
  administration rejection and raw15's `high in number` fatal error remain.
* Processed retries skip mutations. Delta, processed-marker and validity-write
  failures preserve actual financial prefixes and subsequent journal rewriting.
* A live valid scalar send reaches all six actual phases in source order:
  fees, rewards, on-chain queue, channel queue, clearing, withdrawals.
  Its preceding ordinary commitment uses the original pure TxUtils profile,
  bound from the initial reset over explicitly supplied raw transaction data.
  No unavailable commit state is used as the later available sequence's input.
* Original indexed raw3 with absent references is marked processed but retains
  no `valid` flag. Live raw3 with positive matching references reaches the
  original omitted `outputs.length` exception; Main journals the actual fatal
  error with `valid=false` and subsequently runs only withdrawals. This proves
  the real Main catch behavior rather than inferring it from standalone Types.
* A real two-sided raw18 match creates actual positions and a new fee after
  Main's fee flush. Original clearing throws its Property7 supply mismatch of
  -0.00050000 before withdrawals. The corpus asserts actual matched positions,
  the five reached phases and the source exception. It does not repair the
  source's phase/accounting behavior.
  A transparent observer registers every real Orderbook receiver reaching the
  original `loadOrderBook`, including the fresh third constructor used during
  pruning. It forwards the unchanged receiver, arguments, result and promise.
* The explicit bound raw-RPC profile imports a synthetic raw transaction
  document and reuses the original pinned TxUtils pure classifier/extractor
  from the existing Send corpus. Main then executes a successful channel
  commitment and processed marker. No supplied pubkey, validity or actor
  authorization verdict replaces the original helpers.

## Explicit unavailable profile

Default TxUtils/Client/Persistence calls are unavailable capabilities. A fixture
`SourceBoundaryUnavailable` error lets the original Main catch/journal path be
observed, including retained commit debits/credits and post-phase selection.
This error is an adapter boundary, **not** an original source financial throw,
historical acceptance result or Rust-parity success. These two Main calls are
explicitly excluded from available-source comparison counts. Constructor startup
boundaries are also input/audit frames, not successful daemon initialization.
Each experiment starts from its own reset and is followed by another reset;
the verifier rejects any available-source call that depends on its poststate.
The exclusions are `indexed/commit-unavailable-rpc-prefix-isolated` and
`live/commit-logic-error-still-runs-real-financial-phases`. Their retained source
financial/journal prefixes are recorded for audit, never imported as Core state.

The bound raw-document profile has the same explicit helper/data boundary as
the 121-observation Send corpus; it establishes neither network availability
nor authenticity of the transaction document. The current Rust Main guard
must be widened only alongside this source-backed expansion, including raw3.
This corpus itself is not Rust integration evidence until it is actually
integrated and differentially tested. No Rust compilation or complete-block,
source-wide or authenticated-history proof is claimed by this corpus.

## Reproduce

Set `PROOF_REPO` to the real TradeLayer-Rust checkout and `TRADELAYER_JS_ROOT`
to the provisioned genuine JavaScript dependency tree. From the workspace:

```powershell
node work/next-main-lower-corpus/scripts/main_lower_vectors.cjs --check
node work/next-main-lower-corpus/scripts/main_lower_vectors.cjs --lock --check
node work/next-main-lower-corpus/scripts/main_lower_vectors.cjs --proof10 --check
node work/next-main-lower-corpus/scripts/main_lower_vectors.cjs --proof10 --lock --check
python work/next-main-lower-corpus/verify.py
```

Omit `--check` to regenerate the selected fixture. The proof-only mode leaves
the full corpus and inventory unchanged. `prepare.py` reconstructs the driver
from the existing original Main driver without modifying the repository; it
retains exact source class bodies and records its base-driver hash in ORIGIN.json.
