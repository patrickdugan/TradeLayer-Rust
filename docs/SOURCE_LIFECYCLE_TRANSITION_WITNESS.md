# Conditional lifecycle command-slice witness (TLSW2)

`SourceLifecycleTransitionWitness` is a separate bounded envelope over the actual `SourceContractReplay`. Its verifier restores the complete before journal and executes the original suffix through the sole source runtime. It introduces no supplied financial outcomes, poststate execution input, alternative Ledger, or storage authority.

This is conditional execution evidence. Independent expectations must bind the source/boundary descriptor, runtime policy, scope, before-head and **exact ordered command commitment**. A consumer may also pin the after-head. A hash or a valid witness does not establish actor authorization, UTXO existence, chain acceptance, producer identity, or complete transaction extraction. Before-history imports, activation, module load order, RPC/environment configuration and failure schedules remain assumptions until independently established by the consumer.

The new profile changes no TLSW1 admission, framing or digest, no TLCR1-5 framing, and no runtime policy/default. Existing `SourceTransitionWitness` remains raw18-only. V6 admission is explicit and adds the owned type30 and literal tail32-35 graph. Future owner availability never expands a prior policy automatically.

## Scope and policy

The expectation and envelope both select one of these scopes:

- `RawTransactionSlice` (tag1): original nine-argument `Types.decodePayload` calls and the exact continuation forms below. It admits no other direct Logic calls beyond the constrained reward continuation below.
- `SourceMethodSlice` (tag2): the same commands plus the named direct Logic methods below. Direct method inputs establish a declared source-method experiment; they do not establish that an admitted raw transaction reaches that method with those inputs.

Neither scope claims that a slice contains one whole transaction, every transaction in a block, or the complete source scheduler. A slice can contain several original raw calls and continuations. The consumer must independently pin which ordered slice its claim requires. Empty slices and local-error receipts do not establish successful transaction execution.

| Explicit runtime policy | Admitted Number type argument |
| --- | --- |
| `SourceLifecycleV3` | 16,17,18,19,20,21,22,23,31 |
| `OptionLifecycleV4` | V3 types plus24,25,27 |
| `DistributionLifecycleV5` | V4 types plus26,28,29 |
| `ProofLifecycleV6` | V5 types plus30,32,33,34,35 |

The comparison uses the exact `SourceValue::Number` value; string type labels and other arities are refused. V1/V2 are refused by this new profile. V3 raw17 is deliberately admitted even though the direct derivative method is V4-only: the unchanged raw source reaches its validator failure first. A verified rejected transaction remains a rejection.

Both scopes admit only these continuation forms:

| Replay command | Original shape |
| --- | --- |
| `SetHeight` | One original source value |
| `EvaluateQueue` | `processQueuedOnChainOrdersForBlock` or `processQueuedChannelTradesForBlock`, one argument |
| `Invoke` | `Tally.flushQueuedFeeAccruals` or `Channels.processWithdrawals`, one argument |
| `Invoke` | `Clearing.clearingFunction`, one or two arguments |
| `Invoke`, V6 only | `Logic.settleLiquidityRewards`, one original block argument |

`SourceMethodSlice` additionally admits these `Logic.*` methods, with at most16 original positional arguments. Omitted arguments and original extras inside this bound retain their source meanings:

- V3-V6: `createContractSeries`, `tradeContractChannel`, `tradeTokensChannel`, `withdrawal`, `transfer`, `settleChannelPNL`, `batchSettlement`, `processKeepAlive`, `processClosePosition`, `processNetSettle`, `recordSettlement`.
- V4-V6: `exerciseDerivative`, `deliverContract`, `handleOptionExercise`, `processOptionTrade`, `mintSynthetic`, `redeemSynthetic`.
- V5-V6: `payToTokens`, `tradeBaiUrbun`, `tradeMurabaha`.
- V6: `processStakeFraudProof`, `applyProceduralOracleSettlement`, `queueLiquidityReward`, `tradeTokenForUTXO`, `batchMoveZkRollup`.

No direct Orderbook settlement/match input, queue enqueue, arbitrary Tally/Margin/Contract mutation, `Import`, module-entry change, activation change, failure injection, `SourceEnvironment.configure`, `SourceArtifacts.configure` or `SourceGlobal.bind*` configuration is admitted in either suffix. Necessary bootstrap operations belong to the complete before-history and its independent head pin. Actual validation, queue processing and financial writes run through the existing runtime owners.

The pinned `main.js` processBlock path orders fee flush, liquidity rewards, onchain queue drain, channel queue drain, clearing and withdrawals. Its other consensus path orders some phases differently. This witness preserves the supplied original order and does not supply a canonical block schedule. Both actual one-argument Main liquidity-reward call sites are admitted as V6 continuations over the existing owned queue. Queueing/producer calls remain source-method-only; raw3 is outside this profile. The observed source channel queue defect, and onchain draining an active-pair entry before channel draining, remain distinguishable outcomes.

## Interface and framing

The parent replay module includes `source_lifecycle_transition_witness_impl.rs` after its existing TLSW1 include; no runtime edit is needed. Public types are separate from TLSW1:

```rust,ignore
let descriptor = SourceTransitionDescriptor::reference();
let expected = SourceLifecycleTransitionExpectation {
    policy: SourceContractRuntimePolicy::DistributionLifecycleV5,
    scope: SourceLifecycleTransitionScope::RawTransactionSlice,
    descriptor: &descriptor,
    pre_head: independently_retained_before_head,
    commands_commitment: independently_pinned_original_command_digest,
    post_head: independently_expected_after_head_or_none,
};
let witness = SourceLifecycleTransitionWitness::produce(&before, &commands, &expected)?;
let conditional = SourceLifecycleTransitionWitness::verify_bytes(
    &witness.canonical_bytes()?, &expected,
)?;
let receipts = conditional.receipts();
```

`source_lifecycle_transition_commands_commitment(policy,scope,commands)` computes the mandatory pin. Taking that value, the scope or before-head from the same untrusted witness supplies no independent expectation. The conditional result exposes typed scope, exact replay owner, suffix receipts, heads and state/command commitments. Its `claim()` is `conditional-source-raw-command-slice-only` or `conditional-source-method-slice-only`.

All framing integers are big endian:

1. `TLSW\x02`, literal owner1, lifecycle profile2, scope1/2, replay policy3/4/5/6.
2. Forty ASCII source-commit bytes, 16-bit boundary length and exact boundary bytes.
3. Before-head count/commitment, 32-bit complete before-TLCR length and bytes.
4. 32-bit suffix count, original existing canonical diagnostic command frames.
5. Command commitment, after-head count/commitment, after-state commitment.
6. Domain-separated SHA256 and CRC32.

The descriptor remains source commit `dca0888b827c5bc28fa9189bc9e1d507f5c5670c` and boundary `tl-source-reference-db-rpc-clock-rng-numeric-boundary-v1`. Together with the explicit policy it identifies the bounded source values, actual owned source DB/query/clone behavior, environment adapters and numeric domains. The separate TLSW2 profile/scope/policy distinguish admission from TLSW1.

The command digest hashes `TradeLayer/SourceLifecycleTransitionCommands/v2/`, policy byte, scope byte, 32-bit count and each canonical length-prefixed command frame. It binds repeated occurrences, value types, Number bits (including negative zero), source flag positions, original txids/actors and argument omissions. The envelope digest hashes `TradeLayer/SourceLifecycleTransitionWitness/v2/` followed by all preceding envelope bytes. Existing actual replay algorithms derive the after-head and state commitment.

Verifier checks include size/framing, owner/profile/scope/policy, exact descriptor, independent before-head/command/optional after-head pins, bounded canonical command decoding and envelope integrity. It then reconstructs the entire before history using actual replay and executes the suffix. Repaired self hashes cannot replace any independent pin or make a stored poststate an execution input.

## Outcomes, bounds and evidence

Source throws retain actual completed DB/cache/financial prefixes and consumed failures. A retry command runs over that retained state. Local unavailable dependency, unsupported numeric domain or capacity failure retains the runtime's rollback behavior and an explicit `LocalError` receipt. Successful witness verification proves neither JavaScript success for a local-error receipt nor successful business execution for a source throw. Generic unsupported external address validation remains outside the financial execution boundary.

Limits reuse replay: at most32MiB envelope,32MiB complete before checkpoint,4096 total before-plus-suffix commands,4MiB per frame, and the existing SourceValue/runtime state/effect budgets. Oversized count/length values are refused before replay. This is a bounded full-history replay proof; it is not a constant-size Merkle opening, succinct proof or adversarial execution-time benchmark.

Eight Rust tests cover the policy/scope matrix, forbidden financial/bootstrap suffixes, independent pins with repaired-hash substitution attacks, framing/domain bounds, frozen compatibility and genuinely composed source slices. The batch validation receipt records the pinned full-library checks; the independent command digest fixtures remain unchanged.

The composed test reuses the existing complete pinned-source environments from the registry, derivative, channel financial, channel Logic, option, synthetic and distribution tests. It executes138 selected original source command slices across raw16,17,19-29,31 and direct/queue continuations. Source outcomes, persisted collections, sole Tally cache, live channel/withdrawal/queue caches, available Margin object identities/indexes, rejected asynchronous continuations and ordered DB/RPC/source effects are compared after witness verification. Output state is comparison evidence, never a witness execution input. Source-derived failures, shifted/wrong arguments, stale cache behavior and late prefixes remain visible. Type18 financial composition remains independently covered by TLSW1's existing source tests; the TLSW2 matrix checks its continued admission.

A further100 populated V6 exports (76 proof and24 rewards) compare unchanged-source outcomes, complete state and ordered effects after witness verification. They exercise raw30, redemption/cache/bond/transition failures and retries, declared environment/raw-artifact inputs in the independently pinned before history, the real trade producer and reward settlement over populated queues. Explicit missing-artifact and Infinity local refusals are excluded from source-parity counts. V6 tail admission is separately checked against its genuine raw/result/error corpus and the policy matrix.

The new raw21/22 probes call the unchanged source encoders/Types/decoder/validation/Logic. Raw22 uses Litecoin input `LKDxGDJq5fF4FohAB8zJH24mDDNHDNtqsE` to avoid the pinned source's leading-zero Base58 checksum defect and unavailable external `TxUtils.validateAddressWrapper` fallback. This declares the original input, not an address-validation or financial verdict. Original raw RPC documents, when needed, remain prestate-bound external inputs to the actual pure TxUtils implementation.

The selected corpus passes with genuine archived BigNumber9.3.1 and the independently verified lock9.1.2 constructor. Only numeric constructor loading differs; source methods are unchanged. This establishes equality for these observations, not whole-version equivalence. Existing source environment/observer/storage declarations and limits remain applicable.

The two small envelope fixtures pin exact framing and the `SetHeight(-0), SetHeight(10)` slice. They were captured with the explicitly recorded V5 runtime library and isolated replay/helper compilation; they are not historical chain evidence or an independently authenticated prestate. Literal command digest pins were independently assembled with Node Buffer framing/crypto. The original independently compiled TLCR1-4 fixture bytes and heads are separately restored and re-encoded without changes. See `fixtures/source_lifecycle_transition_witness_v2.txt` for capture details.

Run the target:

```text
cargo test --locked --test source_lifecycle_transition_witness
TL_SOURCE_BN_VERSION=9.1.2 cargo test --locked --test source_lifecycle_transition_witness
```

`TRADELAYER_JS_ROOT` supplies the same genuine provisioned external packages required by the existing source environments. The lock constructor is loaded from the existing hash-verified archived9.1.2 source, with no npm installation by this test. CI checks the include formatting and both numeric references. The profile executes the actual shared owners; TLSW1 and all previous policy admissions and frozen bytes remain unchanged.

Pinned integration validation: 1,449 Rust test executions across208 targets, 84 source-lock tests across31 targets, formatting, Clippy, doctests, release and Python policy/bridge checks pass. Exact scope, corpus counts and input hashes are recorded in `fixtures/reference_fidelity/source-proof-candidate.json`; these checks do not establish complete reference parity or authenticated block execution.
