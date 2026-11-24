# Codex work order: ZTB BRIDGE-SEC-02 follow-through

Sequenced tasks from the Mythos audit. Each task lists its inputs, the gate that must pass on CPU
and in CI before the next task starts, and what it must not do. Repository rules apply
throughout: AGENTS.md full preflight before any Rust push, zero-crate gate
(`scripts/check_rust_supply_chain.py`), no live funds, no live contract calls, no new
dependencies. Keep every authority flag false until W10.

| # | Task | Inputs | CPU gate | CI gate | Findings |
|---|---|---|---|---|---|
| W0 | Land research CI | `.github/workflows/ztb-bridge-research.yml` | all 6 Node suites + 2 Python oracles + `lake build` + `leanchecker` locally | workflow green on the research branch | L1 |
| W1 | nearcore light-client vectors | pinned nearcore commit; archived testnet `next_light_client_block` JSON (read-only capture, provenance recorded) | independent decoder (JS + Python) reproduces block hashes and approval messages for >= 3 epoch hand-offs | fixtures checked into `research/near-zk-bridge/nearhead01/` with SHA-256 manifest | C1 |
| W2 | NEARHEAD01 native reference | W1 vectors; NOVA_STATEMENT_V2 section 3 mode H | BRIDGE-SEC-02 T2 arms pass against real headers; record producer-set size and unbonding period at the pin | T2 job in the research workflow | C1, H3 |
| W3 | Type-36 `z0_pins` + statement nullifier | `src/zk_verify.rs`, `src/registry_zk.rs`, NOVA_STATEMENT_V2 section 6 | unit tests: pinned-index mismatch rejects; duplicate public inputs in two txs produce one value effect; snapshot round-trip; existing zk suites unchanged (`tests/zk_node.rs`, release job) | full Rust CI + preflight; consensus format change documented in `docs/ZK_VERIFIER.md` | C2, H1 |
| W4 | Domain-pinned program + constrained upgrade | audit section 5 | Rust port of `propose`/`activate` with CAS, delay, pause-only emergency; BRIDGE-SEC-02 upgrade arms reproduced from Rust | Kani: activation preserves consumed set (bounded) | C3, H6 |
| W5 | Recorded-verdict boundary | `src/zk_verify.rs:527-563`, `src/consensus_source.rs:356-366` | test: a value-moving effect replayed from a foreign receipt bundle re-verifies (or refuses) | Rust CI | H2 |
| W6 | Statement ABI v2 | `codec.mjs`, `ztb02_oracle.py`, NOVA_STATEMENT_V2 section 2 | add `paid`, nullifier roots, LC epoch/bp/head, verifier epoch, prevProgram; cross-language vectors JS/Python byte-equal | research workflow | H6 |
| W7 | TLSTATE01 exit relation (native) | PROTOCOL.md TLSTATE01; M18 window ideas | native complete-replay root over a regtest block range; omitted-burn and reordered-input arms reject; replace `TLSTATE_FIXTURE` in BRIDGE-SEC-02 | research workflow; Rust tests if in-crate | H5 |
| W8 | NEARPAY-OUT recovery | ZTB02B_GATE outbound section | paid-but-callback-lost recovers exactly once from an authenticated outgoing receipt; double recovery rejects | research workflow | M2 |
| W9 | NEAR vault sandbox (local only) | ZTB02B candidate, near-sdk 5.17.2 | sandbox build with zero full-access keys after init; no admin transfer method in WASM exports (export-list check); code hash pinned in domain policy | sandbox job only after preflight; never testnet funds | C4, M7 |
| W10 | Nova circuit for NEARPAY_V2 | W2, W3, W6 | real proof for one benign deposit; z0 substitution, unpinned program, duplicate statement arms reject; cost table vs 4,000 ms budget | release-mode CI job, `--ignored` sweeps off the default path | C1, C2, M3 |
| W11 | Settlement and BitVM3 prerequisites | UTXO-Ref U1; BVA findings 1-10 | oracle nonce PoP with PoC regression; netting-to-zero Kani/Lean for the exact encoder; BitVM3 kept out of custody path until bond + anchor + key deletion + z0 pin + vault-bound statement exist | UTXO-Ref CI; zk-rusto CI | H7, H8 |
| W12 | Containment wiring | Lean Theorem 3; audit section 6 | window cap, independent reconciliation source, quarantine, haircut proposal (never auto-executed); BitAgent propose-only policy as a host check | research workflow | M6, M7 |

## Ordering rationale

W0 first (evidence in CI). W1-W2 replace the one trusted fixture that matters most (C1). W3-W6
close the type-36 and ABI gaps that would make any later proof unsafe to act on. W7-W9 cover the
reverse path and custody. W10 only after the relation, pins and ABI are frozen, because a key is
expensive to regenerate. W11-W12 are independent and can run in parallel with W7-W10.

## Out of scope for Codex

Security decisions on the upgrade quorum, issuer trust policy, exposure caps and any activation
are Patrick's. No task sends anything to a live network beyond the read-only header capture in
W1, and that capture goes through a public RPC with no credentials.
