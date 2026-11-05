"""Deterministic signed TradeLayer spending transactions for tests/node_ingest.rs.

Same conventions as generate_admission_vectors.py (public throwaway scalars
derived from fixed labels, primitives from bitcoin_supported_signing_reference.py)
with real TradeLayer payloads: an activation of type 2, sends of property 7,
and a P2PKH holder's send. No real key, block, node or funds are involved.

    python scripts/generate_node_vectors.py --check
"""
import hashlib
import struct
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import bitcoin_supported_signing_reference as ref  # noqa: E402

OUT = HERE.parent / 'fixtures' / 'node_vectors.tsv'
RECIPIENT = '1BitcoinEaterAddressDontSendf59kuE'
ACTIVATE = 'tl02,' + '11' * 32


def scalar(label):
    digest = hashlib.sha256(b'tradelayer-node-test/' + label.encode()).digest()
    return int.from_bytes(digest, 'big') % (ref.N - 1) + 1


def parent_txid(label):
    return hashlib.sha256(b'tradelayer-node-parent/' + label.encode()).digest()


def p2wpkh(key_hash):
    return b'\x00\x14' + key_hash


def p2pkh(key_hash):
    return b'\x76\xa9\x14' + key_hash + b'\x88\xac'


def carrier(text):
    data = text.encode()
    assert len(data) < 76
    return b'\x6a' + bytes([len(data)]) + data


def transaction(witness, parent, vout, payload, change_script):
    return {
        'version': struct.pack('<I', 2),
        'inputs': [[parent + struct.pack('<I', vout), b'', struct.pack('<I', 0xfffffffd)]],
        'outputs': [[0, carrier(payload)], [9000, change_script]],
        'stacks': [[]] if witness else [],
        'lock': bytes(4),
        'witness': witness,
    }


def row(name, template, tx, amount, script, payload):
    return [name, template, '1', str(amount), script.hex(), ref.serial(tx).hex(), payload]


def p2wpkh_row(name, key_label, parent_label, vout, payload):
    key = scalar(key_label)
    public = ref.pub(key)
    script = p2wpkh(ref.h160(public))
    amount = 10000
    change = p2wpkh(ref.h160(ref.pub(scalar('node-change'))))
    tx = transaction(True, parent_txid(parent_label), vout, payload, change)
    digest = ref.mode_digest(tx, 0, p2pkh(ref.h160(public)), amount, 1, True)[0]
    tx['stacks'][0] = [ref.sign(digest, key, name, False, 1), public]
    return row(name, 'P2wpkh', tx, amount, script, payload)


def p2pkh_row(name, key_label, parent_label, vout, payload):
    key = scalar(key_label)
    public = ref.pub(key)
    script = p2pkh(ref.h160(public))
    amount = 10000
    tx = transaction(False, parent_txid(parent_label), vout, payload, b'\x51')
    digest = ref.mode_digest(tx, 0, script, amount, 1, False)[0]
    tx['inputs'][0][1] = ref.push(ref.sign(digest, key, name, False, 1)) + ref.push(public)
    return row(name, 'P2pkh', tx, amount, script, payload)


def rows():
    return [
        p2wpkh_row('admin-activate', 'admin-key', 'activate', 0, ACTIVATE),
        p2wpkh_row('send-p2wpkh-a', 'admin-key', 'send-a', 1, f'tl20;{RECIPIENT};7;a'),
        p2wpkh_row('send-p2wpkh-b', 'admin-key', 'send-b', 2, f'tl20;{RECIPIENT};7;5'),
        p2pkh_row('send-p2pkh', 'holder-key', 'send-holder', 0, f'tl20;{RECIPIENT};7;3'),
    ]


def main():
    text = '# name\ttemplate\thash_type\tparent_amount\tparent_script\tspending\tpayload\n'
    text += ''.join('\t'.join(r) + '\n' for r in rows())
    if '--check' in sys.argv:
        if OUT.read_bytes() != text.encode():
            raise SystemExit('fixtures/node_vectors.tsv differs from the generator output')
        print('PASS: fixtures/node_vectors.tsv is byte-identical to the generator output')
    else:
        OUT.write_bytes(text.encode())
        print('wrote', OUT, len(rows()), 'rows')


if __name__ == '__main__':
    main()
