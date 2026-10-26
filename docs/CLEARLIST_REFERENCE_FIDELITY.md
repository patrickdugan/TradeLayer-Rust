# Independent clearlist source-conformance review

Archived structured findings and source hashes are in `fixtures/reference_fidelity/clearlist-findings.json` and `clearlist-source-manifest.json`. Commands, runtime results and dependency boundaries are consolidated under their original evidence keys in `fixtures/reference_fidelity/clearlist-execution-manifest.json`.

The current Registry is a deliberately corrected transition prototype, not a literal clearlist.js port. The existing unverified clearlist audit preserves several important source defects, but it had four accidental conformance gaps. Those gaps are now fixed in the audit only. The separately reported six-file clearlist/BIP32 port remains **unavailable and unverified**.

## Source identity

Legacy reference: `tradelayer.js@dca0888b827c5bc28fa9189bc9e1d507f5c5670c`.
Rust review base: `TradeLayer-Rust@89852594c4a83ddc9460797da0b01eb1d0f00179`.

The review extracted immutable blobs with `git cat-file blob COMMIT:PATH`; it did not treat a current JS working file as the pin. `source-manifest.json` records blob IDs, exact byte counts, SHA256 and saved paths. The reference-capture working-tree comparisons precede the audit fixes; clean Rust files were equal to their blobs after CRLF normalization. `rust-execution-evidence.json` records the final hashes of the two changed Rust files.

| Source | SHA256 |
| --- | --- |
| `src/clearlist.js` | `fbdb702e5e89bac113226223f42bf762af13fff40dee4e0c4ab6cd4a50d451e1` |
| `src/validity.js` | `ed9b7fed5b733740c26033030379abe0f4a63d488ff431dfc7b1f10eb4f93b00` |
| `src/logic.js` | `f09d476e581d38cf7278f1424aadba7266f25e0fbc23763ceba4afa79c5b3e1d` |

`findings.json` contains all 25 clearlist methods, source locations, typed Rust projections, transaction findings and remaining boundaries.

## Accidental audit gaps fixed

1. **Canonical revocation omitted root rows.** JS first finds every active row matching address and list, including `merkle:<list>:<root>` rows, then revokes all of them (`clearlist.js:415-449`). The audit previously fetched only the address key. A root-only row could not be revoked, a direct/root pair left the root active, and a canonical root match could incorrectly fall through to another list's direct row. The audit now processes all canonical matches before its deliberately preserved unsafe legacy fallback.
2. **History did not sort.** JS sorts current matching records by descending timestamp (`:548-558`). Direct and root rows can coexist, so BTreeMap order was observably wrong. The audit now sorts the selected rows; it still does not claim append-only history or source ordering for equal timestamps.
3. **Country metadata was only a string.** JS accepts `countryCode`, `country`, `code` and `value` object aliases, and uses `blockHeight || timestamp || null` (`:452-501`). The audit now has an explicitly unverified, bounded string-valued object projection, with first-truthy-alias selection before trimming and the source height precedence. Generic imported legacy rows, non-string aliases, arbitrary ID coercion and Unicode remain outside this typed projection.
4. **Empty explicit xpub and empty inline suffix were conflated.** `addAttestationWithXpub` uses the explicit value or null; it never falls back to an inline metadata prefix (`:175-193`). `addAttestation` retains an empty `xpub:` suffix as an empty string (`:147-173`), then membership skips that falsy candidate. The audit now distinguishes these cases.

Only `src/legacy_clearlist_audit.rs` and `tests/legacy_clearlist_audit.rs` were changed in the repository by this review. Six focused regressions were added to the twelve preexisting audit tests. No Registry runtime behavior was changed.

## Declared Registry corrections retained

| Rule | Exact JS reference | Current Registry |
| --- | --- | --- |
| List IDs and create | Falsy ID allocates by `parseInt` maximum; explicit ID upserts; URL is discarded (`:10-31,135-145`). | Sequential u32 IDs; bounded printable ASCII; URL retained. |
| Admin lookup/update | Type8 uses `getList` from **attestations**; create stores `admin`/`backup`; updates write string IDs beside numeric originals. Reload can restore the old numeric admin (`:86-144`; `validity.js:949-958`). | Direct list lookup, primary/backup authorization and durable primary/backup state rotation. |
| Attestation scope | `_id=address` replaces an address's previous list and active metadata (`:147-193`). | Key is `(list_id, account)`; active duplicate is rejected. |
| Revocation | All canonical active rows, otherwise legacy address fallback can affect another list or an already revoked row; reason and update timestamp stored (`:415-449`). | Requires the exact list/subject row; rejects missing/inactive; no reason field. |
| List 0 | Self-cert country and protocol-admin BANLIST command are accepted by the validator and Logic (`validity.js:1044-1093`; `logic.js:889-932`). | Explicit `UnsupportedSelfCertification`; no global banlist action. |
| Merkle/xpub | Root records and volatile registrations support derived membership. | No root/xpub action or derived-address membership implementation. |

These differences are already declared as intent/corrected behavior in `registry.rs`. Reintroducing source storage defects into that runtime would change its chosen rules. Its tests cover duplicate rejection, list0 rejection, primary-only attestation and backup-admin rotation; this review did not rerun that separate test target.

## Source-validity and country/banlist integration

The exact Type8 validator with the actual clearlist helper rejects an ordinary Type7-created numeric list because `getList` reads the attestations collection. The source evaluator preserves `ClearList.getList` and the validator's expected fields; a mock object containing `adminAddress` and `backupAddress` can bypass this actual storage mismatch in a fixture. Mock return-value parity therefore does not verify a working end-to-end admin transition.

Type9 uses `getClearlistById`, permits only the primary admin for nonzero lists, allows list0 self-cert/banlist commands, permits repeat issuance, and checks active direct membership before a revoke. The full-source checks reproduce those decisions with the real helper and explicit activation/address stubs.

`getBanlist` calls `findAsync`, receives an array and returns its nonexistent `.data` property. It yields undefined even after `setBanlist`. The validity module's nullish fallback contains `US,KP,RU,IR,CU`, whereas the manager's cached default also includes `SY`. The Rust source evaluator preserves the five-country fallback. A corrected getter changes historical business behavior and must be identified as a repair.

The send validator also passes a recipient address to `getAttestations(clearlistId)` and then reads `data.clearlistId` although native writes use `data.listId` (`validity.js:324-343`). This existing source defect is separate from ordinary direct membership.

Consumers in transaction types 2, 4, 5, 10, 18, 19, 20 and 22 use clearlist/country dependencies; 4, 19 and 20 can request derived membership. Fixture backends do not establish a verified wiring of actual persisted clearlist state and BIP32 into accepted economic transactions.

## Persistence and derivation boundaries

The JS module persists lists, direct attestations, root rows and banlist through its DB adapter. `xpubCache` and `merkleXpubMap` are static memory; `loadClearlists` does not rehydrate root registrations. The Rust audit explicitly remains an in-memory observational store with no checkpoint or storage adapter.

The Registry snapshot preserves its own corrected list/attestation state and height/replay metadata (`registry_snapshot_impl.rs:159-183,248-277`), with canonical reserialization and validation. It has no country-object/banlist/root/xpub state because those actions are absent. That format is not a JS NeDB recovery port.

Real derivation remains unavailable in this review. JS tries `bitcore-lib-ltc`, then `bitcore-lib`, scans chains 0 and 1 for indices 0 through 19, ignores its `network` argument and caches by xpub only (`clearlist.js:200-234`). The source harness deliberately lacks those libraries. Its non-direct xpub result demonstrates the source's missing-library handling, not cryptographic membership parity. The Rust audit reports `DerivationUnavailable` for nonempty active derivation candidates. SHA256 Merkle proof checks use genuine Node crypto but prove neither BIP32 derivation nor administrative authority.

The reported external commit `1ad166aab4f612d1021a3a673482a8730362d69c` is not a local Git object, and all six reserved files are absent. Its reported patch hashes and worker check counts are not locally verified execution evidence. No reserved file was recreated or edited.

## Completed checks and reproduction

Keep the following evidence categories distinct:

| Check | Result | Boundary |
| --- | --- | --- |
| New full-source JS harness | **18 checks, 118 modeled DB calls passed**, Node v24.19.0 | Entire unchanged clearlist/validity modules, typed-ID cloned-document DB model; explicit activation/address stubs. |
| Existing clearlist expression audit | Four diagnostic PASS groups | Manually modeled source expressions; not a full-source module execution. |
| Existing validity corpus generation | **43 exports, 564 observations**; 562 ordinary V and 2 quarantined Q | Genuine archived source/BigNumber/options, explicit dependency mocks including ClearList. 15/8/14 V cases for create/admin/attestation validators. |
| Focused Rust audit tests | **18 passed**: 12 preexisting plus 6 new | Existing unverified audit only; no live DB/chain/BIP32. |
| Focused Clippy | Passed with `-D warnings` | Same audit test target plus library. Compiler cache hard-link fallback messages were environmental. |
| Targeted Rustfmt / diff whitespace | Passed | Two changed Rust files / repository diff. |

The source-check count evolved from 14/86 to 17/99 to the final 18/118 as root and alias regressions were added. Those earlier counts are superseded, not additional tests.

`command-evidence.json`, `execution-evidence.json`, `ready-execution-evidence.json` and `rust-execution-evidence.json` preserve commands, results and dependencies. No full Cargo suite was run.

Run the scratch harness without the external JS checkout:

```powershell
node work/clearlist-reference-review/source-checks.cjs
```

The minimum repo-ready package is in `ready/`: `scripts/audit_legacy_clearlist_reference.cjs`, the new nonreserved `fixtures/legacy_clearlist_audit_reference.txt`, and `IMPORT-MANIFEST.json`. It reuses the already committed `fixtures/legacy_validity_source.txt`; its scratch copy is included only for standalone verification. After integration it runs as:

```powershell
node scripts/audit_legacy_clearlist_reference.cjs --evidence PATH_TO_SCRATCH_JSON
```

Focused Rust commands from the repository root:

```powershell
& ../../work/cargo-check.ps1 test --offline --test legacy_clearlist_audit
& ../../work/cargo-check.ps1 -CargoArguments @('clippy','--offline','--test','legacy_clearlist_audit','--','-D','warnings')
```

The parent owns repository integration, final review and the required local commit. This delegated review did not commit, push, publish or open a PR.

## Subsequent owned-state implementation

The review above describes its immutable baseline. The later actual local
owner and its bounded source conformance are documented in `CLEARLIST_SOURCE_OWNER.md`;
`SOURCE_CATALOG_EXECUTION.md` describes its opt-in sole-Ledger integration and
remaining policy differences. The external six-file handoff remains unimported
and is not used as evidence for this independently implemented local owner.
