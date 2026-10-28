# Original Main orchestration profile V12

Full release `9ff4aab` passes **1,678 Rust executions and 238 locked numeric-reference checks**. All 1,337 frozen inputs match. Startup/snapshot/control and IO/sort phases are covered; native storage/13 and rich14 are excluded. Full source parity, authenticated history and complete-block proofs remain open. Evidence: `fixtures/reference_fidelity/main-io-sort-release-validation.json`.

`MainOrchestrationV12` composes original finite indexing/catchup phases and selected Main/Persistence service paths over the existing owners. One literal `TxIndexSourceOwner` holds class/client metadata; its adapter reads and writes the same Core collections used by Main, Consensus, Tally, Queue and registries. It introduces no second balance or financial snapshot authority. The default remains `GlobalClearingV2`.

These independent original-JavaScript corpora are separate evidence sets, not a combined historical block run:

| Corpus | Frames | Actual calls | Scope |
| --- | ---: | ---: | --- |
| Main production phases | 68 | 39 | indexing, ordered catchup, block end, errors and partial writes |
| Finite service paths | 72 | 41 | Persistence/network initialization, empty block phases, retries, recovery, finite live ingress and native reorg failures |
| RPC Error labels | 28 | 14 | native defaults, message coercion, constructor errors and ordered effects |
| Complete Main/init/db startup | 65 | 55 | five original module imports and shared-owner cold startup |
| Scheduled snapshots | 57 | 33 | artifact copying/pruning/restoration and native partial failures |
| Persistence file IO | 66 | 41 | direct lifecycle, retries, mtimes and retained source writes |
| Literal V8 sorting | 32 | 16 | NaN, merge boundaries and native coercion errors |
| Typed terminal shutdown | 3 | 3 | actual source child exit0 and exact console prefix |

Both archived BigNumber numeric profiles regenerate the corpora. Strict Rust comparisons execute real commands and compare outcomes, every ordered effect, complete tables and row order, receiver/client identities, caches, pending jobs and checkpoint reconstruction/re-encoding. Failure descriptors map original storage operation names to existing Core `*Async` names, so failure scenes execute instead of being rejected as malformed test setup. The first ten Main calls remain separately reproducible. The expanded service corpus retains all earlier 51 frames and its first-ten fixture byte-for-byte.

The profile preserves source-specific behavior: numbered private RPC references resolve to unchanged client metadata; index parsing/static state remains separate from Main height state; funding transactions retain their group/order; awaited failures and returned-Promise rejections differ; native throws retain preceding writes; empty/zero/NaN buckets return successfully. Progress/resource guards apply before active loops. A local unavailable-domain/resource error restores the prior runtime and is never presented as a successful JavaScript completion.

The height parser uses ECMAScript whitespace and rounds hexadecimal integers once after exact conversion. Repeated binary64 multiply/add double-rounded some long prefixes. `fixtures/main_production_parse_int.tsv` records 716 native Node observations: the independent review's 343 cases plus deterministic prefixes, representable boundaries, ties, signs, negative zero and overflow. A private Rust test checks exact binary64 bits; `scripts/check_main_production_parse_int.cjs` checks the fixture against Node. These are parser observations, not 716 complete Main executions.

Recorded network metadata, heights, raw empty blocks, filesystem inputs and IO failures remain unverified inputs. The harness supplies no financial, validity, signature or authentication verdict. Its POSIX filesystem map and virtual clock do not certify host durability. LF normalization applies only to reused test-harness text/hash; pinned JavaScript bodies and case fixture bytes remain unchanged.

V12 has distinct TLCR12 magic and bootstrap domain. V1-V11 outcomes, checkpoint bytes and admission refusals remain unchanged. Independent old-provider evidence covers eleven initial Debug/state/checkpoint forms, populated V11 and older histories, and the 39-record nonempty durable archive compatibility corpus. The new private owner is omitted from old Debug views only when actually absent; unexpected state remains visible.

TLSW4 and TLSW5 still admit only their original V9-V11 policies. Tests reject V12 in both command/Main scopes. Existing `SourceDurableReplay` persists V12 commands over the same owners, but this does not extend exported witness coverage or authenticate an archive. Hashes/publication expectations are consistency pins.

Native Main storage/replay binding, pause/resume and startup exit1 scheduling, richer thrown values, host service durability, broader historical/value domains, authenticated admission and complete-block proofs remain open. Source restore does not reload live DB handles/Tally caches; it is not a repaired financial rollback. An endless pending daemon has no successful completed outcome to fabricate.

Root integration evidence is recorded separately in `fixtures/reference_fidelity/main-v12-integration.json`. Focused checks do not replace a complete release validation or establish full reference parity.
