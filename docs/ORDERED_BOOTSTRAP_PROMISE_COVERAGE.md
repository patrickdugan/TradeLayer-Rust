# Ordered bootstrap Promise conformance follow-up

This is additional executed source evidence, not a Core implementation change.
Frozen 23894768 and a1f3344f remain unchanged. No network/account operation or
Root Cargo build was performed. The source environment uses the unchanged full
TxUtils and TxIndex modules, exact whole Main class, genuine pinned dependencies
and explicitly recorded client/RPC capabilities. Process rejection listeners
observe actual Promise events; default unhandled-rejection process termination
is not claimed by that observer profile. The full Main/reOrg/IIFE boundary is
still recorded as unavailable.

## Existing fourteen scenes reviewed

| Existing scene ID | Verified source boundary |
| --- | --- |
| immediate-success-import-before-static | Import getInstance[] precedes constructor/static getInstance[true]. |
| deferred-import-resolves-before-main | Import await completes in an earlier turn. |
| deferred-import-resolves-after-main | Main caller can complete while import client remains unassigned. |
| deferred-import-rejects-before-main | Actual import IIFE rejection notifies before Main construction. |
| deferred-import-rejects-after-main | Import rejection notifies after the independent Main caller. |
| immediate-import-rejects-before-main | Full-turn drain before construction preserves that notification. |
| immediate-import-rejects-same-turn-main | Same-turn Main caller precedes the later unhandled notification. |
| deferred-static-rejects-after-main | Actual Main.txIndex promise rejects; instance client remains null. |
| immediate-static-rejects-same-turn-main | Static-init rejection is independent of Main height success. |
| both-deferred-reject-import-first | Two separately drained turns preserve import-first notification order. |
| both-deferred-reject-index-first | Two separately drained turns preserve static-first notification order. |
| rejected-import-raw-call-preserves-original-init-reference-error | Missing-client guard creates a new ReferenceError before its try/catch. |
| main-height-rpc-rejection-is-separate-from-bootstrap | Actual direct RPC error remains distinct from both bootstrap reasons. |
| static-rejection-explicit-late-handler | Actual late catch follows unhandled notification and produces rejectionHandled. |

The enhanced observer's --baseline --check passes against both exact existing
99-observation fixtures, SHA256 088be935ca15e2c1f7db14069d3bd3ebb49ace5b7d1c682357cddbd4008e5200.
It therefore does not replace their observations or silently fix source bodies.

## New source controls and results

Twelve independent scenes execute twelve actual original Main height calls.
There are 143 observations per numeric profile, including one honest full-Main
import refusal. Both genuine numeric writes/checks retain identical fixtures,
SHA256 0d6a8046edae6c7ea69e3ffa5c477c7ea15ce1c060ae89ab690e51a85ac51a24.

| New scene ID | Executed finding |
| --- | --- |
| static-immediate-reject-handler-before-first-turn | Catch attached immediately after construction runs once; no unhandled or rejectionHandled notification. |
| static-deferred-handler-before-reject | Catch attached before the client rejection suppresses its unhandled notification. |
| static-deferred-reject-then-handler-same-turn | Rejecting the client and attaching catch before any drain also suppresses notification. |
| static-notified-late-handler-repeat-turns | One late catch and one rejectionHandled; additional turns do not repeat either notification. |
| both-immediate-reject-one-turn | Import and static rejections both survive; import notifies first. |
| both-deferred-reject-one-turn-import-first | Same-turn settlements notify import then static. |
| both-deferred-reject-one-turn-static-first | Reversing same-turn settlement order reverses notification order. |
| same-turn-import-resolve-static-reject | Fulfilled import and rejected static remain independent. |
| same-turn-static-resolve-import-reject | Fulfilled static and rejected import remain independent. |
| both-deferred-resolve-static-first-checkpoints | Client fields are still unassigned at the first actual host-await checkpoint, and both are assigned at the second on pinned Node24.22.1. |
| same-error-object-rejects-two-source-promises | Two distinct rejected source promises retain the SAME supplied Error object identity; two notifications do not imply two error objects. |
| two-late-handlers-one-rejection-handled-notification | Both actual catch callbacks run, but rejectionHandled occurs only once. |

Each source control is a raw caller action or client capability settlement.
No expected validity, financial result, source poststate, cache or balance is
supplied to the original modules. Complete source tables/receivers/traces remain
observations. Observer counters count actual callbacks and are never execution
answers. The original frozen loader has its own unique filename, preserving the
newer shared service loader in Root.

## Concrete remaining port tests and boundaries

The first ten scenes strengthen controls expressible at the existing profile's
full-drain/turn boundaries: early catch, reject-then-catch, same-turn settlement
order and repeated-turn deduplication need additive actual-Core differential
tests. This source-only package does not claim those tests passed or alter the
twenty-one queued Core tests.

The six intermediate host-await observations are genuine cutoffs, not durable
JS checkpoints or proof that a port may adopt client fields as a completed
Promise. The current profile has full DrainMicrotasks/EndTurn controls, not a
single-native-microtask scheduler. Keep unresolved and notification-pending
checkpoints/adoption refused. A future suspended startup continuation must retain
the actual Promise/job positions and reexecute raw control history, rather than
importing these observed fields or turning an unresolved await into success.

The final two scenes expose explicit additional domains: shared rejection-object
aliasing cannot be represented by two copied data-only rejection facts, and the
current fixed single-handler control does not represent multiple catch callbacks.
They require a distinct explicit capability/profile extension (or the properly
bound rich arena) before broader fidelity can be claimed. Equal error text must
neither merge distinct objects nor force one shared object into two identities.
Preserve old namespace/receipt meanings rather than silently widening them.

General thenables, synchronous module-import throws, arbitrary callback bodies,
mutated thrown graphs, full original client.js connection setup, startup crash
recovery and the complete global Main initialization remain outside this bounded
evidence. This package adds no financial owner, authorization or economic choice.
