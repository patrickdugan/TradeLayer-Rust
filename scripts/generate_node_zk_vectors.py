"""Signed spending transactions that carry ZkVerifyV1 frames, for tests/node_ingest.rs.

`tests/node_ingest.rs` (test `write_zk_frames`, run with `--ignored`) writes
`fixtures/node_zk_frames.tsv`: one row per transaction with the signer's key label, the parent
label, the template and the frame in hex. This script signs each row with the same public
throwaway scalars as generate_node_vectors.py and writes `fixtures/node_zk_vectors.tsv` in the
same columns as `fixtures/node_vectors.tsv`, with the frame (hex) as the payload column.

    python scripts/generate_node_zk_vectors.py --check

A frame is binary and longer than one direct push, so the OP_RETURN carries it behind
OP_PUSHDATA1 or OP_PUSHDATA2. No real key, block, node or funds are involved.
"""
import struct
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import bitcoin_supported_signing_reference as ref  # noqa: E402
import generate_node_vectors as base  # noqa: E402

FRAMES = HERE.parent / 'fixtures' / 'node_zk_frames.tsv'
OUT = HERE.parent / 'fixtures' / 'node_zk_vectors.tsv'


def carrier(data):
    assert 0 < len(data) < 65536
    return b'\x6a' + ref.push(data)


def transaction(witness, parent, vout, data, change_script):
    return {
        'version': struct.pack('<I', 2),
        'inputs': [[parent + struct.pack('<I', vout), b'', struct.pack('<I', 0xfffffffd)]],
        'outputs': [[0, carrier(data)], [9000, change_script]],
        'stacks': [[]] if witness else [],
        'lock': bytes(4),
        'witness': witness,
    }


def signed_row(name, template, key_label, parent_label, vout, frame):
    key = base.scalar(key_label)
    public = ref.pub(key)
    amount = 10000
    if template == 'P2wpkh':
        script = base.p2wpkh(ref.h160(public))
        change = base.p2wpkh(ref.h160(ref.pub(base.scalar('node-change'))))
        tx = transaction(True, base.parent_txid(parent_label), vout, frame, change)
        digest = ref.mode_digest(tx, 0, base.p2pkh(ref.h160(public)), amount, 1, True)[0]
        tx['stacks'][0] = [ref.sign(digest, key, name, False, 1), public]
    elif template == 'P2pkh':
        script = base.p2pkh(ref.h160(public))
        tx = transaction(False, base.parent_txid(parent_label), vout, frame, b'\x51')
        digest = ref.mode_digest(tx, 0, script, amount, 1, False)[0]
        tx['inputs'][0][1] = ref.push(ref.sign(digest, key, name, False, 1)) + ref.push(public)
    else:
        raise SystemExit(f'unknown template {template}')
    return [name, template, '1', str(amount), script.hex(), ref.serial(tx).hex(), frame.hex()]


def rows():
    out = []
    for line in FRAMES.read_text().splitlines():
        if not line or line.startswith('#'):
            continue
        name, template, key_label, parent_label, vout, frame_hex = line.split('\t')
        out.append(signed_row(name, template, key_label, parent_label, int(vout), bytes.fromhex(frame_hex)))
    return out


def main():
    text = '# name\ttemplate\thash_type\tparent_amount\tparent_script\tspending\tpayload\n'
    text += ''.join('\t'.join(r) + '\n' for r in rows())
    if '--check' in sys.argv:
        if OUT.read_bytes() != text.encode():
            raise SystemExit('fixtures/node_zk_vectors.tsv differs from the generator output')
        print('PASS: fixtures/node_zk_vectors.tsv is byte-identical to the generator output')
    else:
        OUT.write_bytes(text.encode())
        print('wrote', OUT, len(rows()), 'rows')


if __name__ == '__main__':
    main()
