# ZTB-02B: NEP-141 vault, terminal receipt gate, and accumulator boundary

Status 2026-11-24: Python ABI/frontier oracle executed. Rust vault and mock-token candidate plus sandbox harness authored locally; Rust compiler and network/sandbox runtime unavailable in author environment. Rust source intentionally withheld from GitHub until the repository AGENTS.md required full preflight can be run. No deployment, sandbox result, finality proof, mint or redemption authorization claimed.

## Inbound: receiving is provisional

The token's ft_transfer_call debits sender, credits receiver, invokes receiver ft_on_transfer(sender_id, amount, msg), then executes token resolver to refund unused amount where possible. A vault receiving ft_on_transfer can authenticate predecessor == pinned token, canonicalize sender/recipient and record a unique local candidate sequence. At that point it cannot authenticate the terminal resolver's returned amount. NEAR's candidate sequence is not an execution receipt ID. Do not infer finality from an event or transaction hash.

The NEARPAY01 witness must connect the initiating token action, receiver invocation and terminal token resolver under finalized execution outcome roots. Bind the exact token and vault accounts, action method/arguments, receiver result and candidate sequence, terminal successful status and retained amount, and receipt graph edges, including the dependency used by the resolver. A prior successful receiver receipt is insufficient. If a standard resolver result reports a larger used amount than the intended keep (e.g. incomplete refund), pause/reconcile; never mint based on the requested amount. Review the exact deployed token resolver semantics and supported code version. For real deposits use a stable authenticated receipt/event identity; replay protection covers all ancestors and the local candidate, so selecting another receipt in the same graph cannot duplicate issuance. Freeze receipt identity and event convention before NEARPAY01 is keyed. Full refund consumes candidate and has no leaf. Partial refund adds exactly the proved retained amount.

The fixture-only controller can simulate the terminal proof boundary after observing real standard mock token results. Its methods must be absent in the default WASM. Production-ready, can_mint and can_redeem remain false in both builds. The default vault must not be deployed with real funds because it has no finalization/exit path.

## Frontier

A 16-level append frontier stores one 32-byte left subtree per level plus the current tree root and count. For append index i and leaf x: at each level l, if i is even set frontier[l]=x and x=H_node(x,zero[l]); otherwise x=H_node(frontier[l],x). Shift i right. Store final x and increment count. The public root is H_root(u64le(count),x). Stop at 65,536; no wrapping or reinitializing. New version/rollover must preserve all old nullifiers. This is byte-compatible with ZTB-02A. The frontier has 512 bytes of stored hashes, not 65,536 leaves; leaf data remains available separately for membership witnesses. Root and count are independently authenticated.

The Python oracle reproduces the earlier frozen statement digest, domain, 128-bit limbs and deposit root. It compares incremental and independently rebuilt roots at 271 checkpoints across all 65,536 appends and rejects the next append with the state intact. Measured local result: PASS. This tests the algorithm in Python, not the Rust or NEAR execution.

## Outbound and sandbox gate

The candidate vault uses a reserved liability per exit and records a monotonically increasing attempt before making a one-yoctoNEAR NEP-141 ft_transfer promise to a pinned recipient. The private callback accepts one result only and requires the current exact attempt. Success marks paid and removes reservation; explicit transfer failure returns to pending. Missing/failed callback after successful transfer leaves in-flight, with no timeout retry or operator resend. A production recovery proof must authenticate the exact outgoing terminal receipt and prevent double recovery. Freeze and explicit accounting handle any reserve loss or unexplained balance delta. No arbitrary admin transfer/upgrade privilege in production.

Run the local candidate using near-sdk and near-contract-standards 5.17.2, mock token only and a sandbox-only build. Require:
- Actual ft_transfer_call terminal values for 10m requested/8m retained, 2m retained, 1m fully refunded; before terminal registration, root and D unchanged.
- Spoof token predecessor, unauthorized fixture calls, duplicate candidate, receipt reuse and substituted amount rejected.
- Register 2m exit, reject duplicate and over-reservation. First payout to an unregistered mock-token recipient fails; private callback preserves reservation and returns Pending; register recipient and retry succeeds exactly once. Final token balances: vault 8m, recipient 2m, paid 2m, reserved 0.
- Default WASM has no synthetic finalize/register methods. Callback spoof and paid exit replay fail. Record full receipt graph, gas, WASM SHA-256, dependency lockfiles, and successful vs failed outcome labels.
- Next adversarial extension injects callback OOG after a successful transfer and demonstrates in-flight no resend. It requires a real receipt recovery proof; no authority shortcut.

Run `python3 research/near-zk-bridge/ztb02b_frontier.py` for the committed test. Before pushing any Rust implementation run `bash scripts/ci-preflight.sh --full` per AGENTS.md, compile both WASM variants, and execute actual sandbox harness. Next cryptographic work is a finalized receipt graph witness fixture for NEARHEAD01/NEARPAY01, with exact execution status and receipt-edge authentication.
