# Literal derivative exercise prefixes

The `OptionLifecycleV4` batch adds the final active member methods
`Logic.exerciseDerivative`, `Logic.deliverContract`, and
`Logic.handleOptionExercise` from source commit
`dca0888b827c5bc28fa9189bc9e1d507f5c5670c`. They use the existing sole Core
collections, Tally authority, Margin object arena and real database adapter.
There is no second registry, financial ledger, supplied exercise verdict or
successful-delivery substitute.

The complete unchanged Logic module has LF SHA256
`f09d476e581d38cf7278f1424aadba7266f25e0fbc23763ceba4afa79c5b3e1d`.
MarginMap is `ad5af4a639a50623319236cce42e8728027109ac3b8ae4616387798110cfb636`;
ContractRegistry is
`d14c8db7d2a40471d51c5d51931eccb5a0d4f4380c9a0589d5eb1c0990636121`.
The common composition driver verifies these and the Types/Validity/decoder,
Tally, Oracle and other complete module pins before executing the reference.

## Active definitions and actual first failures

The later synchronous `exerciseDerivative(sender, txParams, block)` overwrites
the earlier asynchronous four-argument method. Its predecessor's required-field
guard is no longer active. Zero or missing `amount` still starts the Margin
load; old positional calls instead fail while reading the absent
`contractId.match`. Null/undefined params fail during destructuring. Non-string
contract IDs fail at the actual `.match` property read or method call before any
Margin dependency begins.

For a string ID, the source counts hyphens and tests uppercase `P` or `C`.
At least two hyphens plus either letter selects the option branch. It starts
`MarginMap.getInstance(contractId)` without awaiting it. It then calls delivery
or option exercise with an extra leading sender argument. The implementation
retains that exact argument order, including an absent amount.

The genuine ContractRegistry export has no `getContractDetails` method.
Delivery therefore throws synchronous TypeError
`ContractRegistry.getContractDetails is not a function`. Logic itself has no
`getContractDetails` helper; its option branch throws
`this.getContractDetails is not a function`. Direct calls to either helper
produce those same first failures.

These failures precede strike arithmetic, balance queries, token transfers and
contract removal. Delivery's unbound `propertyId` and the other assumed helper
methods are later placeholders. They are not made functional. No successful
financial exercise is reachable through the ordinary pinned member graph.

Raw type 17 keeps the real decoder and validator. The decoder supplies
`derivativeContractId`; validity references undeclared `derivativeRegistry`
before it can call the later undeclared `marginMap`. The actual ReferenceError
survives even for inactive or malformed raw transactions. The raw handler is
not redirected around validity or supplied a success result.

## Real asynchronous Margin suffix

The database factory starts before the synchronous delivery/option failure.
The Margin database read and instance construction happen in the Promise
suffix afterward. A local job captures the actual factory handle or rejection.
After the real synchronous branch fails, the existing MarginMapSourceOwner
evaluates its actual `getInstance` implementation. A one-shot backend supplies
only that already-observed factory result for its exact first `db.getDatabase`
request; all subsequent calls use Core. It does not return a fixture-selected
position or load result.

Factory rejection is outside Margin's source catch and becomes an independent
unhandled rejection. The exercise outcome remains its original synchronous
TypeError. A failed database read is caught and resolves the ignored Margin
Promise to undefined without an unhandled rejection. Malformed JSON is also
caught, after the real constructor has allocated its empty map. The allocation
and its order survive in the diagnostic owner, and a later healthy retry creates
the next actual instance. A parameter-evaluation failure occurs before the
factory, leaving an armed factory failure for the next real Margin call.

There is no persistent deferred queue: the observed boundary drains this one
real suffix before returning the receipt, as the unchanged Node driver drains
the event loop. Its actual state, consumed failure schedule and rejection list
are replay bound. Local unsupported-value and capacity failures retain the
existing explicit admission rollback policy; source exceptions keep their
observable prefixes.

## Genuine comparison and scope

The corpus has 128 observations: 11 resets, three explicit database seeds,
112 ordinary direct/raw calls and two Node-only bare-call probes. Four complete
CommonJS entry orders cover overwritten signatures, malformed params and IDs,
case-sensitive option detection, zero/missing amount, real persisted positions,
fresh instance allocation, direct placeholder methods, factory/read/JSON errors,
failure consumption, raw validation and genuine retry.

It compares exact outcomes, synchronous/Promise transport, source member-call
arguments, ordered database/capability effects, all collections, the full Margin
instances and real positions, sole Tally/Channels/Oracle caches and flags, and
independent unhandled rejections. A second test restores entire journal-backed
checkpoints between factory/read/JSON/argument-error prefixes and retries.

The diagnostic oracle leaves source function bodies unchanged. Transparent
member wrappers observe original argument vectors and invoke the original
methods with their actual receiver. A constructor observer records the genuine
`seriesId` assignment by immediately defining the same own enumerable, writable,
configurable data property; it captures real allocations before a later parse
error, including allocations no longer returned to the protocol. It supplies no
business result and performs no value-equality alias inference.

Complete genuine BigNumber 9.3.1 and independently SHA-verified 9.1.2 executions
are compared. The latter source SHA256 is
`20db2d8d32fd388a927588beb9c1cdbb2c91ddf0273a180161334b28cb00902e`.
Equality is evidence for this corpus, not a whole-version equivalence claim.

Bare calls use the non-strict source function's global receiver and fail with
`this.deliverContract is not a function` or
`this.handleOptionExercise is not a function`, while their unawaited Margin
suffix still executes. These are Node-only probes. Runtime admission retains
the actual Logic member receiver; caller-supplied functions, accessors, custom
receivers and VM globals are outside SourceValue. No public bare-call adapter
was added.

## Runtime integration

Core includes `source_derivative_exercise_impl.rs`. V4 dispatch routes the three
named `Logic.*` targets to `source_derivative_logic(method, arguments)` before
V3's generic Logic dispatch, classifying them as synchronous Immediate/Throw
dependencies. Existing raw17 Types/Validity and its original five-argument
switch plumbing remain. There are no new mutable fields or snapshot shapes.
V1/V2/V3 continue to reject the new member routes and keep their raw17 behavior.

The runtime and version tests exercise the integrated V4 graph alongside the
option and synthetic owners. Integrated Rust 1.90 validation is recorded in
`fixtures/reference_fidelity/source-option-lifecycle.json`; no successful
financial exercise or authenticated historical input is claimed.
