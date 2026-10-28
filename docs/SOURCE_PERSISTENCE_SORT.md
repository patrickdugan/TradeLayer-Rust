# Source Persistence comparator schedule

Two existing uses of Rust's stable sort diverge from the pinned JavaScript
Persistence module when subtraction returns NaN. A stable sort alone does not
determine the ordering of a nontransitive comparator. With heights
`[1,0,NaN,0]`, original `getLastKnownBlock` returns row2/NaN while the prior
candidate returns row3/0. With backup mtimes `[-2,-1,NaN,0]`, original restore
keeps backup3 while the prior candidate keeps backup1. The admitted IO facts
explicitly support Number NaN, so this affects an admitted input.

The small natural-run/binary-insertion helper already present in the channel
queue owner explicitly refuses more than63 rows. This correction instead
translates the complete V8 natural-run, binary insertion, min-run, pending-run
collapse, gallop, MergeLow and MergeHigh schedule. Both Persistence call sites
use it across their existing full bounded row domain (up to1024 rows); no new
input restriction, external authority, balance owner or format is introduced.
The existing same-runtime Number-hint helper preserves native object/array\ncoercion TypeErrors instead of silently converting them to NaN. Work-array
changes are published only after successful completion, as in the original
V8 Array.prototype.sort implementation.

The exact primary implementation is Node v24.22.1's
`deps/v8/third_party/v8/builtins/array-sort.tq`, Git blob
`703b74789b4d7e8f223e2c3da44f9fa0eb8983cc`. The executed runtime is
Node v24.22.1 / V8 `13.6.233.17-node.58`. This is a deliberately pinned
implementation schedule for this inconsistent comparator, not an assertion
that ECMAScript mandates the same result across engines or future versions.

Sources:

- https://github.com/nodejs/node/blob/v24.22.1/deps/v8/third_party/v8/builtins/array-sort.tq
- https://github.com/nodejs/node/blob/v24.22.1/deps/v8/third_party/v8/builtins/LICENSE
- https://github.com/nodejs/node/blob/v24.22.1/deps/v8/LICENSE

The Rust source is an adaptation of the official implementation and retains
the Python Software Foundation and V8 notices plus both complete licenses in
`licenses/`. Changes are a safe Rust translation of the dense work-array
algorithm, bounded by the existing callers, without V8 heap/accessor machinery.
Zero production crates and no unsafe code remain true; this adapted-code
provenance must remain visible in redistributed source/binary documentation.

Independent validation executes44,890 actual-V8 cases and compares the complete
row order AND every comparator-call pair. Cases include exhaustive short
NaN/finite arrays in both directions, infinities, equal keys, natural runs,
min-run/merge/gallop patterns and sizes63/64/65 through1024. Separate injected
comparator-error tests cover publication and multiple stages. Standalone
Rust1.90 debug/release and Clippy pass.

The unchanged full Persistence module executes32 new observations/16 calls
under both genuine BigNumber profiles. Larger getLastKnownBlock cases include
63,64,65,127,128,129,257,513,1024 rows; backup pruning includes65/129 entries.
The original66-observation/10-call-proof fixture bytes remain unchanged.
The actual prior IO Core provider `bf84ce1d…` independently reproduces both
end-to-end failures; its tests and logs are retained in this work package.
The correction's complete Core integration tests require Root's fresh build;
standalone primitive evidence does not substitute for that build.

Apply `prepare_file_io(current source_main_file_io_impl.rs)` and
`prepare_snapshot(current source_main_snapshot_impl.rs)`; copy the new helper,
tests, source generator/observer, fixture and licenses. These are narrow fresh
transforms over the current IO stage; no frozen predecessor is modified.

Root actual corrected-Core validation now passes 36 focused tests across 15 targets, including both primitive tests and the complete unchanged-source sorting extension. All-target Clippy with warnings denied and 104-file included-source formatting pass. This replaces the earlier uncompiled candidate status; full-suite promotion remains separate.
