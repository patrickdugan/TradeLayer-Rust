Root integration note: the focused checks recorded in [SOURCE_AMM_LIFECYCLE.md](SOURCE_AMM_LIFECYCLE.md) have passed. The staged handoff below is preserved for provenance; its work-only/uncompiled statements describe the earlier handoff, not the current Root integration. Full release promotion remains pending.

# Remaining original AMMPool class methods, unreleased V11

This WORK-only extension uses the original pinned full `amm.js`, SHA-256 `bfb4f5d94c1d4754ae9a17b646c42c4ffe62e077590d0a184ac8ddc0b38255b1`. Its inventory contains 17 static methods and eight instance entries including the constructor. The accompanying method map distinguishes earlier capital/quote evidence from the 12 newly implemented static methods. Configured inverse/linear `generateOrders` dispatch is also covered here.

## Source distinctions retained

Constructor fields belong to AMMPool instances. They do not initialize the exported class's static fields. Default class methods therefore retain genuine undefined/NaN prefixes and TypeErrors. Static insert/redeem mutations occur before the original unbound uppercase `TallyMap` ReferenceError; the lowercase existing `tallyMap` realm binding is a separate identifier. Class position, LP shares, strategy and order arrays are original class bookkeeping, not an independent spendable balance store.

Two explicit replay inputs describe ordinary caller state: `SourceAMM.configureStatic` assigns only the eight known own data fields, and `SourceAMM.bindStaticTally` binds the existing actual Tally owner as the source realm's uppercase identifier. The genuine Node VM executes these same data assignments and actual module binding; no method/prototype replacement, validator result, price verdict, balance verdict, fake Tally response or second balance owner is supplied. Unknown field names are refused atomically before assignment. These data facts are conditional initial/caller inputs and confer no authentication or financial authority.

Static LP calls use the same original Tally update path and preserve fulfilled Errors ignored by the caller. Native plus, relational comparisons, string concatenation, NaN, negative positions, zero-share deletion, strategy JSON catch/TypeError boundaries and nested static calls are retained. V1–V10 never enter this helper and their policies/codecs remain unchanged.

Default `generateTokenOrders` still has an unbound free `txid` identifier. It creates the actual orderbook receiver and evaluates numeric expressions before each attempted Promise.all catches that ReferenceError; neither addTokenOrder body runs. The ordered caught-message list is now replay-bound diagnostic state and strictly compared with the source logger observations. Zero, negative, fractional, string, undefined and coercion-error cases are executed. Bound free-txid generation remains an explicit unavailable capability; no new binding or synthetic success is introduced. Infinite/unbounded loops are outside the finite admitted domain; Rust retains its explicit capacity guard. Console observations unrelated to these specific caught-token messages remain raw source characterization metadata, not claimed Rust comparisons.

## Evidence and gates

The first ten genuine default-class calls were executed before extension. The final corpus contains 67 frames: 49 actual original static calls, six resets, and 12 explicit own-class-data/Tally-binding inputs. All four genuine checks pass: full and ten-call proof under BigNumber 9.3.1 and 9.1.2. Snapshot views use nonmutating reads and assert storage inventory is unchanged. Source state/effects retain all module tables, actual Tally caches/singleton, property/contract registries, AMM/orderbook receivers and source outcomes; unsupported callback/prototype domains are not guessed.

Three strict Rust tests are staged and UNCOMPILED: the 67-frame differential with exact class state, Tally, receiver, effect and replay checks; all new APIs/methods refusing V1–V10 atomically; and class inputs refusing method/prototype replacement. Actual Rust 1.90 rustfmt parse/check passes. No Core/library build, runtime parity execution, V11 release validation, Root edit, push or commit was performed by this worker. Root owns the first actual combined V11 build, Clippy, old version gates and commit.

## Apply after the fresh current V11 layer

Use `prepare.py::prepare_runtime` on the current V11 runtime, `prepare_amm` on its AMM lifecycle include, and `prepare_quote` on its quote include. Copy the one new helper, three-test file, generator, three fixtures and two documents. The `preview/` source snapshots demonstrate composition over manifest `d0eef4f70f09b81b8436235d34aaee6c266cb02764530562205fe833ba1ac66e`; apply the narrow functions to any newer Root text rather than overwrite that text wholesale. Do not rebuild the earlier frozen overlays.
