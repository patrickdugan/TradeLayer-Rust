"""Deterministic signed TradeLayer spending transactions for tests/admission.rs.

Public throwaway scalars derived from fixed labels. No real key, block, node or
funds are involved. The primitives come from bitcoin_supported_signing_reference.py.

    python scripts/generate_admission_vectors.py --check
"""
import hashlib
import struct
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import bitcoin_supported_signing_reference as ref  # noqa: E402

OUT = HERE.parent / 'fixtures' / 'admission_vectors.tsv'
PAYLOAD = b'\x6a\x0etl20;alice;5;2'


def scalar(label):
    digest = hashlib.sha256(b'tradelayer-admission-test/' + label.encode()).digest()
    return int.from_bytes(digest, 'big') % (ref.N - 1) + 1


def parent_txid(label):
    return hashlib.sha256(b'tradelayer-admission-parent/' + label.encode()).digest()


def p2wpkh(key_hash):
    return b'\x00\x14' + key_hash


def p2pkh(key_hash):
    return b'\x76\xa9\x14' + key_hash + b'\x88\xac'


def transaction(witness, parent_txid, parent_vout, extra_inputs=0, change_script=b'\x51'):
    inputs = [[parent_txid + struct.pack('<I', parent_vout), b'', struct.pack('<I', 0xfffffffd)]]
    for index in range(extra_inputs):
        inputs.append([bytes([0x60 + index]) * 32 + struct.pack('<I', 0), b'', struct.pack('<I', 0xfffffffd)])
    return {
        'version': struct.pack('<I', 2),
        'inputs': inputs,
        'outputs': [[0, PAYLOAD], [9000, change_script]],
        'stacks': [[] for _ in inputs] if witness else [],
        'lock': bytes(4),
        'witness': witness,
    }


def row(name, template, kind, tx, amount, script):
    return [name, template, str(kind), str(amount), script.hex(), ref.serial(tx).hex()]


def p2wpkh_row(name, kind=1, extra_inputs=0):
    key = scalar(name.split('-')[0] + '-key')
    public = ref.pub(key)
    script = p2wpkh(ref.h160(public))
    amount = 10000
    tx = transaction(True, parent_txid('p2wpkh'), 1, extra_inputs, p2wpkh(ref.h160(ref.pub(scalar('change')))))
    digest = ref.mode_digest(tx, 0, p2pkh(ref.h160(public)), amount, kind, True)[0]
    tx['stacks'][0] = [ref.sign(digest, key, name, False, kind), public]
    for index in range(1, extra_inputs + 1):
        tx['stacks'][index] = [b'\x01', b'\x02']
    return row(name, 'P2wpkh', kind, tx, amount, script)


def p2pkh_row(name, kind=1):
    key = scalar(name.split('-')[0] + '-key')
    public = ref.pub(key)
    script = p2pkh(ref.h160(public))
    amount = 10000
    tx = transaction(False, parent_txid('p2pkh'), 0)
    digest = ref.mode_digest(tx, 0, script, amount, kind, False)[0]
    tx['inputs'][0][1] = ref.push(ref.sign(digest, key, name, False, kind)) + ref.push(public)
    return row(name, 'P2pkh', kind, tx, amount, script)


def wrapped_row(name):
    key = scalar('wrapped-key')
    public = ref.pub(key)
    redeem = p2wpkh(ref.h160(public))
    script = b'\xa9\x14' + ref.h160(redeem) + b'\x87'
    amount = 10000
    tx = transaction(True, parent_txid('wrapped'), 2)
    digest = ref.mode_digest(tx, 0, p2pkh(ref.h160(public)), amount, 1, True)[0]
    tx['inputs'][0][1] = ref.push(redeem)
    tx['stacks'][0] = [ref.sign(digest, key, name, False, 1), public]
    return row(name, 'P2shP2wpkh', 1, tx, amount, script)


def rows():
    return [
        p2wpkh_row('p2wpkh-all'),
        p2wpkh_row('p2wpkh-two-inputs', extra_inputs=1),
        p2pkh_row('p2pkh-all'),
        p2wpkh_row('p2wpkh-none', kind=2),
        p2wpkh_row('p2wpkh-single', kind=3),
        p2pkh_row('p2pkh-none', kind=2),
        wrapped_row('wrapped-p2wpkh-all'),
    ]


def main():
    text = '# name\ttemplate\thash_type\tparent_amount\tparent_script\tspending\n'
    text += ''.join('\t'.join(r) + '\n' for r in rows())
    if '--check' in sys.argv:
        if OUT.read_bytes() != text.encode():
            raise SystemExit('fixtures/admission_vectors.tsv differs from the generator output')
        print('PASS: fixtures/admission_vectors.tsv is byte-identical to the generator output')
    else:
        OUT.write_bytes(text.encode())
        print('wrote', OUT, len(rows()), 'rows')


if __name__ == '__main__':
    main()
