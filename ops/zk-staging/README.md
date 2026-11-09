# ZkVerifyV1 staging on Bitcoin testnet4

Rollout step 3 of [docs/ZK_VERIFIER.md](../../docs/ZK_VERIFIER.md): register a test program on
testnet4, submit one valid and two invalid proofs, and check that two independent
`tradelayer-node` instances reach the same verdicts.

The staged program is the cubic test circuit (`z' = z^3 + z + 5`, 3 steps) with the registered key
of `fixtures/zk_nova/cubic_3steps.vk_nogens.hex`. Every frame is sent by one P2WPKH address, the
staging admin: the node bootstrap gives the admin the collateral property (`collateral_seed_atoms`),
which pays the type-36 fee.

| round | transaction | vsize | receipt reason the node records |
|---:|---|---:|---|
| 1 | `activate`: type 36 (`"tl" 0x26`) | 156 | `zk:activated` |
| 2 | `register`: the cubic program (`"tl" 0x25`) | 731 | `zk:registered` |
| 2 | `verify-valid`: proof bound to the admin | 11,701 | `zk:verified` |
| 2 | `verify-corrupt`: the same proof with one bit of `eval_W` flipped | 11,701 | `zk:rejected:Verifier(InvalidProof)` |
| 2 | `verify-other-binding`: the valid proof, `z0[0]` of another address | 11,701 | `zk:rejected:BindingMismatch` |

Round 2 must confirm in a later block than round 1: the registry refuses type 36 in its activation
block (`SameBlockActivation`). The round-2 transactions spend each other's change, so they enter a
block in this order. At 1 sat/vB the five cost 35,994 sats in fees.

## Tools

| tool | what it does |
|---|---|
| `examples/zk_stage_frames.rs` | `binding`: the admin's binding element. `frames`: checks the proof starts from it and verifies it with `ZkNovaAdapter`, checks the corrupted copy is rejected, writes `frames.tsv`. `check-tx`: parses each signed transaction with the node's `inspect_transaction` and requires exactly one carrier holding exactly its frame. |
| `tools/nova-vectors` (`gen_proofs`) | nova-snark 0.76.0 prover; writes the bound proof. |
| `ops/zk-staging/stage.py` | `build`: signs the chain and runs `testmempoolaccept` on it as one package; broadcasts nothing. `send`: broadcasts one round; refuses round 2 until round 1 has a confirmation. `compare`: two nodes' standard outputs must have the same economic head at every shared height. |

`stage.py` passes `bitcoin-cli` parameters on standard input (`-stdin`), since a proof transaction
is 23 KB of hex. Options that start with `-` go as `--cli-arg=-datadir=...`.

## Procedure

Commands run from the repository root. `$CLI` is `bitcoin-cli` with the testnet4 node's options,
for example `bitcoin-cli -datadir=C:/BitcoinTestnet -testnet4 -rpcport=48332`; `$S` is a staging
directory outside the repository.

**1. Admin wallet** (Bitcoin Core 30 or later: older nodes refuse OP_RETURN data above 83 bytes).

```bash
$CLI createwallet tl-zk-staging
$CLI -rpcwallet=tl-zk-staging getnewaddress admin bech32   # ADMIN, fund with 50,000 sats
$CLI -rpcwallet=tl-zk-staging getnewaddress other bech32   # OTHER, never funded
```

**2. Anchor and expiry.** Any block below round 1. The proof commits to it, so the nodes in step 6
must start from the same anchor.

```bash
H=$($CLI getblockcount); A=$($CLI getblockhash $H); E=$((H + 4032))
```

**3. Proof and frames.**

```bash
cargo build --release --example zk_stage_frames --bin tradelayer-node
(cd tools/nova-vectors && cargo build --release --locked)
Z0=$(target/release/examples/zk_stage_frames binding --anchor-hash $A --sender $ADMIN --expiry $E)
NOVA_VEC_OUT=$S NOVA_CUBIC_Z0=$Z0 NOVA_CUBIC_NAME=stage \
  tools/nova-vectors/target/release/gen_proofs 3
target/release/examples/zk_stage_frames frames --anchor-hash $A --admin $ADMIN --other $OTHER \
  --expiry $E --code-hash $(git rev-parse HEAD | tr -d '\n' | sha256sum | cut -c1-64) \
  --proof-meta $S/stage.meta.tsv --proof $S/stage.proof.hex --out $S/frames.tsv
```

`frames` prints the `program_id`. The proof takes about 15 s and 0.5 GiB.

**4. Node configs** (`docs/NODE.md`, "Bootstrap and config"). `$S/listener.conf`:

```text
format = tradelayer-listenerd-config-v1
network = bitcoin
admin = ADMIN
contract = 1
collateral = 7
permitted_spot_properties = 7
rollback_limit = 16
inverse = false
notional = 1
initial_margin_per_contract = 10000
fee_route = oracle:1
marking = none
```

`target/release/tradelayer-node --print-bootstrap-commitments --config $S/listener.conf
--collateral-seed-atoms 100000000` prints the two commitments for `$S/source.conf`:

```text
format = tradelayer-node-source-v1
policy = conserving-sequential-v2
block_mode = realtime
bootstrap = collateral-seed
collateral_seed_atoms = 100000000
bootstrap_commitment = ...
catalog_bootstrap_commitment = ...
zk_verify = zk-verify-v1
zk_fee_property = 7
zk_fee_atoms = 10000000
zk_fee_sink = OTHER
```

**5. Sign and dry-run.** Nothing leaves the machine.

```bash
python3 ops/zk-staging/stage.py build --cli bitcoin-cli --cli-arg=-datadir=... --cli-arg=-testnet4 \
  --wallet tl-zk-staging --admin $ADMIN --frames $S/frames.tsv --out $S/signed.tsv
target/release/examples/zk_stage_frames check-tx --frames $S/frames.tsv --signed $S/signed.tsv
```

**6. Broadcast and observe.** This step puts transactions on testnet4.

```bash
python3 ops/zk-staging/stage.py send ... --wallet tl-zk-staging --signed $S/signed.tsv --round 1
# after round 1 has a confirmation:
python3 ops/zk-staging/stage.py send ... --wallet tl-zk-staging --signed $S/signed.tsv --round 2
```

Run `tradelayer-node` (this commit or later) on two machines, each against its own Bitcoin Core
testnet4 node (here: this laptop, and the witness box's pruned node), with the same two configs:

```bash
target/release/tradelayer-node --root $S/node --network BTC-TESTNET4 \
  --rpc-address 127.0.0.1:48332 --cookie-file DATADIR/testnet4/.cookie \
  --config $S/listener.conf --source-config $S/source.conf --bind 127.0.0.1:0 \
  --stop-file $S/stop --head-log $S/heads.log \
  --initialize --anchor-height $H --anchor-hash $A --max-blocks N > node-a.jsonl
```

Every path is absolute; `--root` is an existing directory without a store; `N` reaches past round
2's block. Then:

```bash
python3 ops/zk-staging/stage.py compare --signed $S/signed.tsv node-a.jsonl node-b.jsonl
curl -s -X POST http://127.0.0.1:PORT/tl_getZkReceipts -d '{"programId":"PROGRAM_ID"}'
```

Expected: round 1's block `zk_items 1, accepted 1`; round 2's block `zk_items 4, accepted 2,
rejected 2`; identical economic heads at every height; one receipt, sender `ADMIN`. The admin
pays the fee on all three proofs, the two rejected ones included.

## Dry run, 2026-11-10

Steps 3 and 5 ran against a throwaway regtest node (Bitcoin Core 31.1, standardness on) with
anchor testnet4 160,214 and placeholder addresses: the proof bound to the admin verified, the
corrupted copy was rejected as `InvalidProof`, `testmempoolaccept` allowed all five as one package,
and `check-tx` found each frame byte for byte. `send` refused round 2 while round 1 was unknown and
while it was unconfirmed, then sent it after one block; the four round-2 transactions confirmed in
one block in chain order. Step 6 on testnet4 has not run.
