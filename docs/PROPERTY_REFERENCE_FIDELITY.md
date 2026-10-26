# property.js business-rule reference review

Current Rust HEAD `89852594c4a83ddc9460797da0b01eb1d0f00179` is a partial, deliberately stricter accounting implementation. It is **not a complete or reference-exact port of property.js**. Ordinary Fixed/Managed issuance, managed grants and issuer self-redemption have typed Ledger paths. The complete property catalog, automatic IDs, ticker ownership, token admin rotation, and five additional property classes are not implemented as one integrated property owner.

Reference: legacy JavaScript commit `dca0888b827c5bc28fa9189bc9e1d507f5c5670c`, `src/property.js`, Git blob `5184c00333a459135db11c81bd6644c21c23883d`, 17,638 bytes, SHA256 `ef0ee7ba5c362227a8bfd44192e351c107871a93a3dc93b04ce6d1a5ebfc70b1`. All reviewed source/test/fixture bytes were obtained with `git cat-file blob`, without PowerShell text conversion. The current Rust native-genesis fixture contains the same exact property.js bytes. A source fixture establishes provenance, not a working Rust property implementation.

The external six-file property patch attributed to `8deba4a` is unavailable in both local Git object databases. Its implementation, tests and claimed results have not been inspected, verified, recreated or imported.

## Execution evidence and boundaries

The entire captured property.js module executes unchanged in a fresh CommonJS VM for each test. The final portable harness uses the repository's existing `scripts/legacy_numeric_oracle.cjs` to load the complete archived genuine BigNumber **9.3.1** implementation, whose LF-normalized SHA256 is `58ba6fb59668342346f17983a61ace455f6f425ac4fbfab84f489df257594d16`. It is not a replacement numeric model. `node:path` is the actual built-in module.

Source `package-lock.json` pins BigNumber **9.1.2**; `package.json` permits `^9.1.2`. The reference checkout's installed package is 9.3.1. The initial run used that actual installed package, with raw SHA256 `08ff132ba18de1678e18a9f68bcad39cc970c94f7d219850468b2a7510cff0ce`; its LF-normalized source matches the archived 9.3.1 oracle. Both the installed and archived runs are 9.3.1-scoped. This is not execution against the 9.1.2 lockfile dependency or proof that every numeric behavior is identical across versions.

The final portable run passes **38 tests, 0 failures, 0 skipped**. These are bounded source characterizations. Some assert legacy exceptions and stale/partial state; those passing assertions are not working legacy business flows or Rust equivalence successes. The results do not claim exhaustive branch, input-domain, concurrency or differential coverage.

Injected boundaries are explicit: an async `db.getDatabase` factory and isolated in-memory async row operations; a `tally.updateBalance` call recorder with optional injected rejection; and a quiet console recorder. The source DB module is statically inspected to confirm that the factory is async. Actual NeDB persistence, RPC bootstrap, source tally accounting, signatures, authentication, and any Rust implementation are not executed. Successful tally recorder calls prove arguments and ordering only, not real custody changes. No PropertyManager method, propertyIndex static field or missing AMM global is injected.

**No Cargo command or Rust test was run for this review.** Existing Rust tests are code evidence only. Their pass/fail count from this review is **not run**, not zero failures. Syntax/integrity checks and the source-only run are recorded separately in the execution manifest.

## Main conformance findings

1. **IDs and ticker ownership are missing from the integrated Rust owner.** Source allocation reloads the property catalog and computes `Math.max(keys)+1`; numeric strings coerce, mixed keys preserve Map identity, and a synthetic `s-...` key yields NaN. `createToken` checks cached tickers before this reload, so an empty/stale cache accepts a persisted duplicate; a cached duplicate returns an Error object instead of throwing. Rust accepts a caller-supplied nonzero u32 ID and rejects duplicate creation. No token ticker field/index exists in `Ledger::Property`, and the source-economic mapper excludes type 1. Contract and oracle ticker stores do not fill this gap.

2. **Managed accounting is present under different, explicit rules.** `Ledger::check_managed_issuer`, `Mint` and `Burn` require Managed kind, the issuer, positive unsigned atoms, checked arithmetic, and actual issuer funds for burn. They reconcile supply and custody atomically. Raw JS `addProperty`, `grantTokens` and `redeemTokens` lack these helper-level restrictions; higher Logic/Validity layers supply ordinary managed policy and procedural gates. Do not infer unrestricted transaction acceptance from the raw helper tests. The JS helper rounds declared supply to 8 places but passes the original grant amount to tally; memory mutation, tally and DB save are separate failure points. Rust's conserving operations intentionally differ from those effects.

3. **Token admin/backup rotation is unimplemented.** `PropertyManager.updateAdmin` changes issuer or backup in persisted JSON, with numeric-coercing ID lookup, while leaving the singleton cache stale. The helper takes no actor; validity/dispatch must establish authority. Current `AdminTarget::Token` fails with `UnsupportedTokenAdmin`, the source-economic mapper rejects token targets, and Ledger has no backup-address or issuer-rotation API. Existing whitelist/oracle admin support is a different feature. Static inspection also finds that the JS Logic token branch names `PropertyList` while this module imports `PropertyManager`; that surrounding routing defect was not executed by the property-only harness.

4. **Metadata, getters, history and restore are not catalog-equivalent.** JS stores ticker, totalInCirculation, class, whitelistId, issuer, backupAddress and optional Number-converted proceduralType. It permits truthy-fallback overwrites, repeated-ID supply increments and class changes. Getter ID semantics differ between strict `isManagedAndAdmin` and coercing `getPropertyData`/`updateAdmin`; static `getPropertyIndex` can serve stale cache. Rust stores issuer, Fixed/Managed kind and total_supply_atoms, with no property catalog projection. `EconomicSourceBackend` returns `UnknownDependency` for property source reads, rather than inventing metadata; an existing test asserts this for `Property.getPropertyData`.

5. **Source defects must remain distinguished from intended behavior.** The unchanged-source harness confirms `getAMM` reaches undefined `PropertyRegistry`, `isSyntheticToken` reads absent class-level propertyIndex after instance load, and matching-ID `updateTotalInCirculation` calls `updateAsync` on an unawaited Promise from the actual async DB-factory contract. It also confirms malformed-load stale cache, ticker error/throw differences, prototype-inherited class-map acceptance, synthetic-key allocator NaN, and non-atomic failure splits. These behaviors do not justify silently claiming a repaired implementation is source-exact. Rust's documented rejection/conservation policies should be labeled as repairs or bounded omissions.

## All seven property kinds

| JS named kind / code | Current Rust evidence | Conformance scope |
| --- | --- | --- |
| Fixed / 1 | `PropertyKind::Fixed`, Ledger create/transfer and fixed-supply mint/burn rejection | Partial accounting intent; lacks ticker/metadata/allocator. Source raw upsert can alter class/supply; Rust forbids it. |
| Managed / 2 | `PropertyKind::Managed`, authorized Mint and issuer-own Burn | Partial ordinary intent; atomic exact atoms intentionally replace rounded non-atomic helper effects; token admin/backup still missing. |
| Native / 3 | `SourceCatalogClass::Native` on `NativeEntitlementPolicy` | Separate catalog role, not a Ledger kind. Adopts independently supplied prefunding; does not mint a declared figure or establish source history/schedule. |
| Vesting / 4 | Native coupon/marker bindings and source-quote-exact-or-reject policy | Separate explicit entitlement owner over the sole Fixed/Managed Ledger; no general property catalog implementation. |
| Synthetic / 5 | `VaultRegistry`, canonical `s-property-contract` identifiers and vault/link calculations | No synthetic Ledger class or integrated property/margin/vault economic issue/redeem; source mapper excludes 24/25. |
| Non-Fungible / 6 | No NFT property kind/owner | Missing. JS property.js itself stores only a type code/supply; per-unit NFT semantics are not inferred. |
| Procedural / 7 | Source validity evaluator understands some procedural checks; typed economic bridge rejects procedural/DLC context | Missing property/economic implementation. Source high-level procedural redemption can be holder-based with contract-state gates, unlike issuer-only Managed Burn. |

`vesting.rs` emits initialization calls through an external backend, including four token creation requests. Those request types and successful mocked/fixture receipts are not a complete catalog owner. The native entitlement implementation is real state on the sole Ledger, with prefunded custody, event history and expected-policy restoration. It must not be collapsed into a claim that Native/Vesting are absent everywhere, nor generalized into a seven-kind property port.

The native-genesis supply-reconciliation fixture explicitly distinguishes declared catalog figures from tally ownership and launched insurance calls. For BTC, its property-3 declaration is 300,000 tokens while the source tally observation is 1,500,000; property 4 declares 300,001 with 1 token owned. Such declarations are not an enforced issuance cap or mint authority. Rust keeps declared figures, optional ceilings and actual expected issued atoms separate in the explicit native policy.

## Method and transaction inventory

`property-findings.json` reviews every **18 class methods and 2 helpers**, with source locations, observed reference behavior, classification, actual current Rust evidence and named source checks. It separately maps all seven named kinds and nine transaction paths: 0, 1, 2, 8, 10, 11, 12, 24 and 25. This inventory covers property issuance/admin/grant/redeem and directly coupled native, send, AMM and synthetic paths; unrelated protocol opcodes are not claimed complete.

There is no reference-exact property Rust owner in the reviewed checkout. Reference-exact evidence here means the archived property.js bytes and directly observed source returns/exceptions/capability calls under the stated boundaries. The main classifications are partial accounting intent, intentional stricter accounting/restore policies, missing integration, reference defects, and unavailable external claims.

JS property.js has no independent creation/admin history or transaction-ID journal: it overwrites one JSON catalog. Its grant/redeem calls pass reason/block into the separately loaded tally module. Rust Ledger processed IDs, canonical snapshots, joint persistence/rollback, and native event replay provide explicit history/restore mechanisms for the typed owner; they do not reconstruct unrecorded JS token metadata/history. Ordinary and native snapshots require their respective policies; checksums alone do not authenticate state. Relevant existing test code includes `ledger_invariants.rs`, `registry_transitions.rs`, `native_ledger.rs`, `economic_validity.rs`, `types_economic.rs` and `persistence.rs`; none was executed here.

## Reproduction

From the Rust repository root, using Node with its built-in test runner:

```text
node --check fixtures/reference_fidelity/property.js
node --check scripts/audit_property_reference.cjs
node --test --test-reporter=tap scripts/audit_property_reference.cjs
```

The harness checks the exact property SHA256 and the existing loader checks the genuine archived BigNumber pin before source tests execute. Optionally set `PROPERTY_REVIEW_MANIFEST` to a writable file to capture runtime and dependency boundaries. Committed TAP/manifest evidence records the reviewed run; it is not a claim that an unavailable patch or Rust target was executed. Source provenance hashes can be reproduced with `git cat-file blob <revision>:<path>` at the two stated pins.
