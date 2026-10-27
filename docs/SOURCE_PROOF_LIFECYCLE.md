# Literal type30 stake, fraud, relay and ordinary settlement

This opt-in implementation follows unchanged source commit
`dca0888b827c5bc28fa9189bc9e1d507f5c5670c`. It composes the actual
`Logic.processStakeFraudProof` and `Logic.applyProceduralOracleSettlement`
handlers over the existing Core Tally, Property cache and collections. Ordinary
redemption, rollover, PnL sweep, BitVM cache/challenge/payout/resolution and
procedural transition paths execute their source writes and source errors.
No financial result, successful validation verdict or replacement custody scheme
is supplied by a caller or fixture.

The opt-in policy is `ProofLifecycleV6`, with `TLCR\x06`, a separate bootstrap
domain and distinct runtime policy name. It inherits earlier methods; default
remains `GlobalClearingV2`. V1–V5 retain previous admission, receipts and diagnostic
shapes. Raw30 uses the real Types decoder and `validateStakeFraudProof`, then
bridges Logic with exactly `(params.senderAddress, params, params.block)`.
The separate `SOURCE_TAIL_LIFECYCLE.md` slice covers literal raw32–35
invalid/failure paths and the direct synchronous rollup handler's first errors;
it adds no functional verifier or repaired handler.

## Actual owners and transport

The existing Core collections and Tally cache remain the sole balance and row
authorities. The four Oracle action methods (`recordStake`, `getStake`,
`applyFraudProof`, `relayTradeLayerState`) operate over Core's actual
`database`, `find` and `update` methods. The standalone Oracle owner owns an
internal DB rather than forwarding shared collections; constructing another
Oracle row store here would duplicate authority. The port instead reuses the
already verified source Number/BigNumber and relay helpers.

V6 shared DB range predicates use the existing source primitive-comparison
bridge for `$lt`, `$lte`, `$gt` and `$gte`: primitive strings compare by UTF-16,
other supported pairs use source numeric conversion, and NaN is unordered.
The earlier policies keep their original numeric predicate behavior. Reward
queries with an original string block bound expose this distinction; version
regressions also cover arrays and BigNumber string primitives.

Property `getInstance`, `load`, `save`, `grantTokens`, `redeemTokens` and the
reached `getPropertyIndex` projection share the existing `property_cache`.
The V6-only initialized flag distinguishes a real singleton constructor from
an empty Map, including after a caught load failure. Grant/redemption fetches
its property, changes supply, reloads the shared cache, installs the changed
property, updates Tally, and saves in source order. No managed-token-only
restriction is added where source omits it.

Procedural transition and BitVM cache/risk calls use the stateless adapters in
`SOURCE_PROCEDURAL_BITVM.md`. Their metadata lives in the same Core
`procedural` collection. Logic owns reached Tally transfers; adapters keep no
alternate balances. `transitionContract` and optional `autoRoll` execute after
the settlement helper.

Both Logic methods and Oracle, Property persistence, Procedural and BitVM write
methods are asynchronous. Property `getInstance` and existing Dlc parse/validation
functions preserve synchronous transport. Explicit environment and artifact
configuration are synchronous adapter operations, not source business methods.

## Source effects and failure prefixes

Stake moves Tally available into margin before Oracle stake metadata is read or
written. Oracle failure retains that movement; retry can move more margin. Fraud
updates the accused stake, then its fraud document, then credits a positive slash
reward. Source does not debit accused Tally margin. Negative direct fraud can
increase stake metadata. These economic defects remain literal source behavior,
separate from the conserving owner.

Relay writes signature-use metadata and the relay document before settlement.
Mismatched normalized signature reuse throws the source Error. Redemption
persists its supply and Tally effects; rollover redeems before granting the next
property. Failure to read that next property retains the first redemption.
Caught Property load and Tally save failures retain their actual continuation.

PnL sweep executes the risk hook, balance check, debit and credit in order.
Cache opening writes risk counters and cache metadata before escrow and bond
transfers. Challenge metadata precedes bond locking. Finalize marks a cache
released before payout checks/transfers; resolve writes its verdict before
refund and bond slash/return. Both upheld and rejected paths retain exact source
destinations and reasons. Window errors, failures and retries receive no blanket
atomic repair. Source throws retain completed earlier writes; replay reconstructs
those prefixes.

## Environment and raw artifact inputs

`SourceEnvironment.configure` replaces the bounded, allowlisted string-only
process-environment input shared by proof, risk and reward paths.
`SourceArtifacts.configure` replaces a bounded map of absolute artifact paths
to raw hexadecimal bytes. V6 diagnostics bind those inputs and the Property
initialized flag in `proofLifecycle`; replay binds their configuration commands
and future continuation. These are unverified boundary inputs, not authenticated
filesystem or chain facts.

The ordinary state-root gate computes payload SHA256, parses bytes, compares
roots, and checks transition hashes/inclusion in source order. Required-bundle
execution parses declared raw bytes and uses the existing bundle hashing helper.
It checks artifact self-hash and supplied expected hash; no successful verifier
result is injected. Read effects bind path, encoding and raw bytes. Rust performs
no host filesystem reads.

Unchanged Node business modules use an explicit per-environment raw-byte
`fs.readFileSync` boundary for admitted artifact cases. Each VM has its own
input map; tests create or delete no shared host artifact files and can run
concurrently. Source performs its own JSON parsing and hashing. The separate
missing-artifact probe records an actual read-only host ENOENT, but is **not**
a Rust parity claim: absent artifact input causes local `UnknownDependency`
and whole-command local rollback. Malformed raw artifact JSON whose Node-specific
diagnostics are unrepresented is also an explicit local domain refusal.

The optional Binohash verifier remains unavailable. Its preceding source
missing-root and root-mismatch errors execute normally and retain relay prefixes;
only the actual experimental verifier boundary is local-unsupported. No proof
authentication or new verifier is introduced. Finite JSON, bounded source values,
representable UTF-16 slices and supported signature metadata are required.
Non-ASCII signature metadata, opaque representations, unrepresented path
resolution and capacity cases remain explicit local refusals. They are not
relabeled as source exceptions or counted as matching source prefixes.

## Evidence and compatibility

The genuine whole-module corpus expanded before implementation to **234
observations**: 29 resets, seven environment/artifact configurations, 197 matched
source calls and one missing-artifact probe with a separately checked local
refusal. Thus 233 observations are compared with Rust. The former two ordinary
continuation refusals now execute. Coverage includes all four import orders,
raw validation errors, stake/fraud/relay, redemption/rollover/sweep, both challenge
verdicts and bond directions, payout windows, procedural rollover, fifteen
DB-failure/retry scenes, risk caps, state-root errors and positive/negative
artifact hashes. Regressions check payout property-error priority and the
pre-verifier Binohash guards.

Archived genuine BigNumber9.3.1 and independently verified genuine9.1.2 produce
byte-identical full outcomes, caches, collections and ordered effects, SHA256
`a3c05d730a6d7e6ba786a410e8d96df12b70b0038eb1c891b26e0ca665469893`.
This establishes equality for this corpus, not whole-version equivalence.
Observers preserve original functions, receivers, arguments and Promises.
The embedded driver reads repository source archives, not scratch captures or
supplied business outcomes.

Two composed tests pass against the private combined V6 library: complete source
differential and prefix/checkpoint/continuation across failure, cache, environment
and artifact scenes. Parent-owned version tests also pass against that library.
Permanent policy-parameterized regressions additionally run the original genuine
synthetic78, registry166, on-chain135 and option117 corpora under V6: all seven
tests pass, covering 496 source observations and their original partial-write,
alias, ordered-DB and replay checks. The source drivers and expected business
observations were unchanged. These checks found no inherited financial/cache
divergence caused by the V6 Property constructor/persistence interception; they
are evidence beyond retaining old-policy checkpoint bytes.
They preserve the independently compiled Rust1.90 MSVC V5 fixture from commit
`fcb8d0f5708788794b412148bfa2c9f0dd26faf5`: 18,336 bytes, nineteen commands,
SHA256 `f7858f2994a79378de58c210faf9de5a843daf3be1bccc47e2b81e0991d72e7a`,
trusted external head
`3ca16cb2adb98a43b79500982923415e50137cf86b6ec7d9168c4998b9c607ce`.
Old receipts, populated Vault/option/expiry state and future refusals remain
unchanged; corrected CRC/bootstrap substitutions cannot promote or downgrade
history. Replay integrity is not L1 authentication.

The versioned source runtime and witness targets are checked by the pinned Rust1.90 CI gates. The validation receipt in `fixtures/reference_fidelity/source-proof-candidate.json` records exact frozen inputs and executed checks. Original V1-V5 snapshots and default V2 remain separate; optional external-verifier capabilities are not inferred from a source error or a hash.

Pinned integration validation: 1,449 Rust test executions across208 targets, 84 source-lock tests across31 targets, formatting, Clippy, doctests, release and Python policy/bridge checks pass. Exact scope, corpus counts and input hashes are recorded in `fixtures/reference_fidelity/source-proof-candidate.json`; these checks do not establish complete reference parity or authenticated block execution.
