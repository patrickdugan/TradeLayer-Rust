# V9-only upper Main guard proposal

Retain the existing V8 lower-kind predicate and the separately validated spot5/6 predicate. Add exactly this additional allowed arm at the original Main kind guard:

```rust
&& !(self.core.policy == SourceContractRuntimePolicy::SpotLifecycleV9
    && matches!(ty, Some(16 | 17 | 19 | 20 | 21 | 22 | 23 | 24 | 25 | 26
        | 27 | 28 | 29 | 30 | 31 | 33 | 34 | 35)))
```

The guard is an error predicate: this extra conjunction permits these kinds only for the exact explicit V9 policy. It does not add these kinds to the shared V8 whitelist. It deliberately uses an exact policy comparison, rather than `spot_lifecycle_enabled()`, because that helper may be inherited by a later policy. V1–V8 and future policies must not acquire this new Main admission implicitly.

`prepare.py:prepare_main` is a pure, narrow transform. It locates the original `if !matches!(ty, Some(...))` expression and inserts only the arm above. It composes with the sibling spot transform in either textual order; it does not touch postphase scheduling, runtime owners, standalone Types/Logic admission, replay codecs or default policy. It must be applied only after the parent has mounted the explicit V9 enum/profile.

The corpus establishes original-source results and available/unavailable boundaries; it is not evidence that this Rust guard is ready for promotion. Existing owned standalone handlers do not automatically demonstrate Main caller equivalence. Parent differential checks must retain the source's exact Main argument bridge, actual persisted activation, source throws and completed prefixes, live `didWork` behavior, fee/reward/queue/clearing/withdrawal order, un-awaited height jobs and public-boundary drain behavior.

The funded raw19 live case is particularly useful: valid processed parameters are committed before the first fee-phase missing-export TypeError. Keep that source result and prefix. The fraud sequence separately demonstrates a real supply discrepancy caught at clearing. Neither should be repaired or omitted merely to obtain a successful live block result. The two unavailable-capability scenes must remain excluded and causally isolated until the corresponding exact source dependency domains are genuinely implemented.

Future work may extend Main witness exports to these kinds after actual V9 Rust parity succeeds. Existing TLSW1/TLSW2/TLSW3 policy ranges remain frozen; this proposal does not widen their witness admission or establish authentication, accepted Bitcoin history or a complete block proof.
