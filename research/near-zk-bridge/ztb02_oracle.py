"""Independent stdlib byte-encoding and Merkle vector oracle."""
import hashlib
import json
import struct
from pathlib import Path

def H(*parts):
    return hashlib.sha256(b"".join(parts)).digest()

def tagged(tag, *parts):
    return H(tag.encode("ascii"), b"\x00", *parts)

def named(tag, account):
    a = account.encode("ascii")
    return tagged(tag, struct.pack("<B", len(a)), a)

def h(n):
    return bytes([n]) * 32

domain = tagged("TLBRIDGE/DOMAIN/V1", h(1), h(2), h(3), h(4),
                named("TLBRIDGE/TOKEN/V1", "usdc.testnet"),
                named("TLBRIDGE/VAULT/V1", "bridge.testnet"),
                struct.pack("<I", 77))
wire = b"TLSTMT01" + domain + b"".join(h(n) for n in range(5, 14)) + struct.pack("<8Q", 1,100,107,2,1,10000000,10000000,2000000)
assert len(wire) == 392
d = H(wire)
def leaf(index, receipt, amount):
    dep = tagged("TLBRIDGE/DEPOSIT-ID/V1", domain, h(receipt), struct.pack("<I", 0))
    return tagged("TLBRIDGE/DEPOSIT-LEAF/V1", domain, struct.pack("<Q", index), dep,
                  named("TLBRIDGE/SENDER/V1", "alice.testnet"), h(42), struct.pack("<Q", amount))

def node(left, right):
    return tagged("TLBRIDGE/DEPOSIT-NODE/V1", left, right)

empty = tagged("TLBRIDGE/DEPOSIT-EMPTY/V1")
root = node(leaf(0,20,8000000), leaf(1,21,2000000))
empty = node(empty,empty)
for _ in range(1,16):
    root = node(root, empty)
    empty = node(empty,empty)
root = tagged("TLBRIDGE/DEPOSIT-ROOT/V1", struct.pack("<Q",2), root)
expected = dict(domain=domain.hex(), statement_hex=wire.hex(), digest=d.hex(),
                packet=(b"TLBRDG01"+d).hex(),
                limbs=[str(int.from_bytes(d[i:i+16],"big")) for i in (0,16)],
                deposit_root=root.hex())
assert expected == json.loads(Path(__file__).with_name("ztb02_vector.json").read_text())
print("independent Python vector: PASS")
