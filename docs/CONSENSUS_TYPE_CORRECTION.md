# Separate V10 Consensus type correction

## Root integration status

Fresh Rust1.90 Root builds passed46 focused executions across11 targets,
including both numeric references, actual Main0, all16 mixed native subsets,
the inherited Main routes, and independent V8/V9/V10 byte/refusal gates.
All-target Clippy, formatting and14 genuine source-regeneration checks pass.
Main0 audit snapshots are nonmutating, as documented in
`SOURCE_MAIN_V10_OBSERVER_STORAGE.md`; actual JS bodies and financial outcomes
are unchanged. A populated pre-V11 snapshot has explicit focused-debug
provenance and makes no release claim. The complete release suite is a separate
gate; the last verified full baseline remains V9 b02a045. Source inputs and
state hashes do not establish chain or actor authority.

## Historical work-stage handoff

Upstream is the immutable mixed-native V10 root-ready manifest
8ccbd4bbd29a4baa6623485ea12fa882a14c20ac9f70b6d3f6083fbbcde73fcc.
No original stage, root checkout, AMM snapshot or whole-library build is changed.

The reviewed source_activation_lifecycle_impl.rs lines77-84 rejects non-object
registries and converts every maximum to f64; line98 constructs a numeric
blockNumber. Original pinned legacy_consensus_source.txt lines286-289 uses
for..in and the actual JavaScript relational operator, then assigns the raw
activationBlock to maxBlock. Line309 stores that raw value unchanged.

Executed unchanged-original probes show that the heights "9" then "10" select
the string "9". A single "10" remains a string; true remains a boolean. Arrays
of activation rows work. Null or primitive registries return fulfilled undefined.
Null rows, invalid JSON and failed primitive conversion are caught, also yielding
fulfilled undefined. Object coercion failures must not become local unsupported
errors, nor append a fabricated vector entry.

prepare_activation applies only the narrow selection/iteration correction and
adds a stateless JSON-value relational helper. It preserves raw selected values,
UTF-16 string ordering, native Number conversion and original catch transport.
No state owner, balance, schedule, source hash pin, previous policy method graph
or byte format is added or changed. The helper is used only in V10's new
Consensus push method.

The new original-source generator executes 24 cases with full unchanged
Consensus and real source file hashing. The cases cover number/string/bool
maxima; mixed and array coercion; array/null/string/number/bool registries;
integer key enumeration; invalid JSON; source catch behavior; zero/non-numeric
heights and a raw overflowing JSON numeric height. Both genuine BigNumber9.3.1
and9.1.2 regenerate the identical fixture and pass --check:

bf1efed1d0a593f1c88153e9b20e9a06d09bd7cc062ac686d0875aa97f722e7c.

Each command records its explicit pre-call collections before the original
method runs. Tests import those inputs. Expected poststate and hash answers are
never supplied as inputs. The original method computes its own code/state hash
from the archived source and actual DB rows. The retained effects include all
original DB and archived-file observations in order.

One Rust differential/replay test is staged and UNCOMPILED. It compares both
genuine source versions, fulfilled return/error transport, the exact vector,
all tables and ordered DB/file effects, then checkpoints/restores every case.
There are ten actual typed selections and fourteen caught/no-selection cases.
Rustfmt1.90 syntax passes; actual Root Rust1.90 parity and old-policy regression
checks remain required before promotion.

The separate original review probes remain at work/review-v10-clearlist. Their
Consensus default JSON SHA-256 is
1d616dc06fe35e033ac6dac31936ef9a8edfc55bdffb81aee3cad22ad630eb72.
The review also identified a distinct shared Insurance predicate issue at
source_contract_fee_impl.rs line516: null balance rows are silently treated as
undefined, unlike source insurance.js line57 which throws before credit. That
financial-prefix finding is reported separately and is not silently folded into
this Consensus correction.
