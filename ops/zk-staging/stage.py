#!/usr/bin/env python3
"""ZkVerifyV1 staging on Bitcoin testnet4: build, check, send, compare.

Standard library only; Bitcoin Core is reached through `bitcoin-cli`. See README.md beside this
file for the whole procedure.

    stage.py build   --cli BITCOIN_CLI [--cli-arg ARG]... --wallet W --admin ADDR
                     --frames FRAMES.tsv --out SIGNED.tsv [--fee-rate SAT_PER_VB]
    stage.py send    --cli BITCOIN_CLI [--cli-arg ARG]... --wallet W --signed SIGNED.tsv --round N
    stage.py compare --signed SIGNED.tsv NODE_A.jsonl NODE_B.jsonl

`build` signs one chain of transactions, one per frame in file order. Input 0 of each spends the
admin's P2WPKH output (the first from the wallet's largest confirmed admin UTXO, then each
change output), so the node authenticates the admin as the sender of every frame. Outputs are
the frame as one OP_RETURN push and the change to the admin. The whole chain goes to
`testmempoolaccept` as one package; nothing is broadcast. `build` writes `round, name, txid,
expect, hex` rows.

`send` broadcasts one round. Round 2 is refused until every round-1 transaction has a
confirmation, because the registry refuses type 36 in its activation block.

`compare` reads two `tradelayer-node` standard outputs and requires identical economic heads at
every height both admitted, then prints the counts of the blocks that carried staged frames.
"""
import argparse
import json
import math
import subprocess
import sys
from decimal import Decimal

DUST_P2WPKH = 294
# Version, marker and flag, counts, lock time; one P2WPKH input with its witness; one P2WPKH
# change output; the OP_RETURN output without the frame (value, script length, OP_RETURN,
# OP_PUSHDATA2 and its length).
BASE_VBYTES = 11 + 68 + 31 + 8 + 3 + 1 + 3


def cli(args, method, *params, options=()):
    # Parameters go through -stdin: a proof transaction is 23 KB of hex and a package is several,
    # past the Windows command-line limit.
    command = [args.cli, *args.cli_arg, *options, "-stdin", method]
    result = subprocess.run(command, input="\n".join(params) + "\n",
                            capture_output=True, text=True)
    if result.returncode != 0:
        sys.exit(f"stage.py: {method} failed: {result.stderr.strip()}")
    text = result.stdout.strip()
    try:
        return json.loads(text)
    except json.JSONDecodeError:
        return text


def wallet(args, method, *params):
    return cli(args, method, *params, options=(f"-rpcwallet={args.wallet}",))


def read_rows(path, columns):
    rows = []
    with open(path, encoding="ascii") as handle:
        for line in handle:
            line = line.rstrip("\r\n")
            if not line or line.startswith("#"):
                continue
            row = line.split("\t")
            if len(row) != columns:
                sys.exit(f"stage.py: {path}: expected {columns} columns, got {len(row)}")
            rows.append(row)
    return rows


def sats(btc):
    return int((Decimal(str(btc)) * 100_000_000).to_integral_exact())


def btc(amount):
    return f"{Decimal(amount) / Decimal(100_000_000):.8f}"


def build(args):
    frames = read_rows(args.frames, 4)
    info = wallet(args, "getaddressinfo", args.admin)
    if not info.get("ismine") or not info["scriptPubKey"].startswith("0014"):
        sys.exit("stage.py: --admin must be a P2WPKH address of --wallet")
    script = info["scriptPubKey"]
    utxos = wallet(args, "listunspent", "1", "9999999", json.dumps([args.admin]))
    if not utxos:
        sys.exit(f"stage.py: {args.admin} has no confirmed output in {args.wallet}")
    funding = max(utxos, key=lambda u: sats(u["amount"]))
    outpoint = (funding["txid"], funding["vout"], sats(funding["amount"]))
    rows = []
    for round_, name, expect, frame in frames:
        vbytes = BASE_VBYTES + len(frame) // 2
        for _ in range(2):
            fee = math.ceil(vbytes * args.fee_rate)
            change = outpoint[2] - fee
            if change < DUST_P2WPKH:
                sys.exit(f"stage.py: the admin output cannot pay for {name}")
            raw = cli(
                args,
                "createrawtransaction",
                json.dumps([{"txid": outpoint[0], "vout": outpoint[1], "sequence": 0xFFFFFFFD}]),
                json.dumps([{"data": frame}, {args.admin: btc(change)}]),
            )
            parent = [{
                "txid": outpoint[0],
                "vout": outpoint[1],
                "scriptPubKey": script,
                "amount": btc(outpoint[2]),
            }]
            signed = wallet(args, "signrawtransactionwithwallet", raw, json.dumps(parent))
            if not signed.get("complete"):
                sys.exit(f"stage.py: {name} did not sign: {signed.get('errors')}")
            decoded = cli(args, "decoderawtransaction", signed["hex"])
            if decoded["vsize"] <= vbytes:
                break
            vbytes = decoded["vsize"]
        rows.append((round_, name, decoded["txid"], expect, signed["hex"]))
        outpoint = (decoded["txid"], 1, change)
        print(f"{round_}\t{name}\t{decoded['txid']}\tvsize {decoded['vsize']}\tfee {fee}")
    verdicts = cli(args, "testmempoolaccept", json.dumps([r[4] for r in rows]))
    refused = [v for v in verdicts if not v.get("allowed")]
    for verdict in refused:
        print(f"refused\t{verdict['txid']}\t{verdict.get('reject-reason')}"
              f"\t{verdict.get('package-error', '')}")
    if refused:
        sys.exit("stage.py: testmempoolaccept refused the package; nothing written")
    with open(args.out, "w", encoding="ascii", newline="\n") as handle:
        handle.write("# round\tname\ttxid\texpect\thex\n")
        for row in rows:
            handle.write("\t".join(str(c) for c in row) + "\n")
    print(f"testmempoolaccept: all {len(rows)} allowed; wrote {args.out}; nothing broadcast")


def send(args):
    rows = read_rows(args.signed, 5)
    if args.round > 1:
        for round_, name, txid, _, _ in rows:
            if int(round_) < args.round:
                found = wallet(args, "gettransaction", txid)
                if found.get("confirmations", 0) < 1:
                    sys.exit(f"stage.py: {name} ({txid}) is not confirmed; round {args.round} waits")
    chosen = [r for r in rows if int(r[0]) == args.round]
    if not chosen:
        sys.exit(f"stage.py: no transactions in round {args.round}")
    for _, name, txid, _, raw in chosen:
        sent = cli(args, "sendrawtransaction", raw)
        if sent != txid:
            sys.exit(f"stage.py: {name}: node returned {sent}, expected {txid}")
        print(f"sent\t{name}\t{txid}")


def blocks(path):
    events = {}
    with open(path, encoding="utf-8") as handle:
        for line in handle:
            line = line.strip()
            if not line.startswith("{"):
                continue
            event = json.loads(line)
            if event.get("event") == "block":
                events[event["observed"]] = event
    return events


def compare(args):
    staged = len(read_rows(args.signed, 5))
    a, b = blocks(args.node_a), blocks(args.node_b)
    shared = sorted(set(a) & set(b), key=str)
    if not shared:
        sys.exit("stage.py: the two outputs share no admitted block")
    for height in shared:
        if a[height]["economic_head"] != b[height]["economic_head"]:
            sys.exit(f"stage.py: economic heads differ at {height}")
    carried = [h for h in shared if a[h].get("zk_items", 0)]
    for height in carried:
        e = a[height]
        print(f"{height}\tzk_items {e['zk_items']}\taccepted {e['accepted']}"
              f"\trejected {e['rejected']}\trefusals {e['refusals']}")
    items = sum(a[h]["zk_items"] for h in carried)
    print(f"{len(shared)} shared blocks, identical economic heads; {items} of {staged} staged "
          f"frames seen")
    if items != staged:
        sys.exit("stage.py: not every staged frame reached both nodes")


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = parser.add_subparsers(dest="command", required=True)
    for name in ("build", "send"):
        p = sub.add_parser(name)
        p.add_argument("--cli", required=True)
        p.add_argument("--cli-arg", action="append", default=[])
        p.add_argument("--wallet", required=True)
    build_p, send_p = sub.choices["build"], sub.choices["send"]
    build_p.add_argument("--admin", required=True)
    build_p.add_argument("--frames", required=True)
    build_p.add_argument("--out", required=True)
    build_p.add_argument("--fee-rate", type=float, default=1.0)
    send_p.add_argument("--signed", required=True)
    send_p.add_argument("--round", type=int, required=True)
    compare_p = sub.add_parser("compare")
    compare_p.add_argument("--signed", required=True)
    compare_p.add_argument("node_a")
    compare_p.add_argument("node_b")
    args = parser.parse_args()
    {"build": build, "send": send, "compare": compare}[args.command](args)


if __name__ == "__main__":
    main()
