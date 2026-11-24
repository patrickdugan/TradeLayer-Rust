#!/usr/bin/env python3
"""Independent reference encoder, SHA-256 tagged hasher, and BIP-340 verifier
for TAP-VWAP Receipt/v1 (526-byte canonical layout) and AssetHTLC/v1 nullifiers.

Used solely to cross-check receipt.cjs and freeze golden fixtures. Never use
any variable-time Python EC arithmetic with secret keys.
"""

from __future__ import annotations

import csv
import hashlib
import json
from pathlib import Path
import struct
import sys

P = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEFFFFFC2F
N = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141
GX = 0x79BE667EF9DCBBAC55A06295CE870B07029BFCDB2DCE28D959F2815B16F81798
GY = 0x483ADA7726A3C4655DA4FBFC0E1108A8FD17B448A68554199C47D08FFB10D4B8
G = (GX, GY)

FIELDS_32 = (
    "genesis",
    "policy",
    "asset",
    "edgeKey",
    "btcPeerKey",
    "assetPeerKey",
    "rfqId",
    "quoteTranscript",
    "paymentHash",
    "assetPre",
    "assetPost",
    "btcPre",
    "btcPost",
)


def tagged_hash(tag: str, data: bytes) -> bytes:
    th = hashlib.sha256(tag.encode("ascii")).digest()
    return hashlib.sha256(th + th + data).digest()


def point_add(p1: tuple[int, int] | None, p2: tuple[int, int] | None) -> tuple[int, int] | None:
    if p1 is None:
        return p2
    if p2 is None:
        return p1
    x1, y1 = p1
    x2, y2 = p2
    if x1 == x2 and (y1 + y2) % P == 0:
        return None
    if x1 == x2:
        lam = (3 * x1 * x1 * pow(2 * y1, P - 2, P)) % P
    else:
        lam = ((y2 - y1) * pow(x2 - x1, P - 2, P)) % P
    x3 = (lam * lam - x1 - x2) % P
    y3 = (lam * (x1 - x3) - y1) % P
    return (x3, y3)


def point_mul(p: tuple[int, int] | None, k: int) -> tuple[int, int] | None:
    r = None
    a = p
    while k > 0:
        if k & 1:
            r = point_add(r, a)
        a = point_add(a, a)
        k >>= 1
    return r


def lift_x(x: int) -> tuple[int, int] | None:
    if x < 0 or x >= P:
        return None
    c = (pow(x, 3, P) + 7) % P
    y = pow(c, (P + 1) // 4, P)
    if (y * y) % P != c:
        return None
    return (x, P - y if (y & 1) else y)


def verify_schnorr_32(pub: bytes, msg: bytes, sig: bytes) -> bool:
    if len(pub) != 32 or len(msg) != 32 or len(sig) != 64:
        return False
    p = lift_x(int.from_bytes(pub, "big"))
    r = int.from_bytes(sig[:32], "big")
    s = int.from_bytes(sig[32:], "big")
    if p is None or r >= P or s >= N:
        return False
    e = int.from_bytes(tagged_hash("BIP0340/challenge", sig[:32] + pub + msg), "big") % N
    pt = point_add(point_mul(G, s), point_mul((p[0], P - p[1]), e))
    if pt is None or (pt[1] & 1) != 0 or pt[0] != r:
        return False
    return True


def parse_hex_exact(val: object, byte_len: int, name: str) -> bytes:
    if not isinstance(val, str) or len(val) != byte_len * 2:
        raise ValueError(f"invalid {name}")
    if any(ch not in "0123456789abcdef" for ch in val):
        raise ValueError(f"invalid {name}")
    return bytes.fromhex(val)


def parse_u64_be(val: object, name: str, positive: bool = False) -> bytes:
    if not isinstance(val, str) or not val:
        raise ValueError(f"invalid {name}")
    if val != "0" and (val[0] == "0" or not val.isdigit() or len(val) > 20):
        raise ValueError(f"invalid {name}")
    if val == "0" and positive:
        raise ValueError(f"invalid {name}")
    n = int(val)
    if n < 0 or n > 0xFFFFFFFFFFFFFFFF or (positive and n == 0):
        raise ValueError(f"invalid {name}")
    return struct.pack(">Q", n)


def parse_outpoint(val: object, name: str) -> bytes:
    raw = parse_hex_exact(val, 36, name)
    txid, vout = raw[:32], struct.unpack(">I", raw[32:])[0]
    if txid == b"\x00" * 32 or vout == 0xFFFFFFFF:
        raise ValueError(f"unresolved {name}")
    return raw


def parse_bit(val: object, name: str) -> bytes:
    if type(val) is not int or val not in (0, 1):
        raise ValueError(f"invalid {name}")
    return bytes([val])


def encode_receipt_v1(r: dict) -> bytes:
    if r.get("splicePending") is True or r.get("spliceUnresolved") is True:
        raise ValueError("unresolved splice identity")
    if "fundingResolved" in r and r["fundingResolved"] is not True:
        raise ValueError("unresolved funding identity")
    f32 = [parse_hex_exact(r.get(k), 32, k) for k in FIELDS_32]
    keys = [r["edgeKey"], r["btcPeerKey"], r["assetPeerKey"]]
    if len(set(keys)) != 3:
        raise ValueError("distinct roles required")
    for k in keys:
        if lift_x(int.from_bytes(bytes.fromhex(k), "big")) is None:
            raise ValueError("invalid curve key")
    if r.get("assetOutpoint") == r.get("btcOutpoint"):
        raise ValueError("distinct channels required")
    if r.get("assetPre") == r.get("assetPost") or r.get("btcPre") == r.get("btcPost"):
        raise ValueError("unchanged state commitment")
    d = parse_bit(r.get("direction"), "direction")
    aes = parse_bit(r.get("assetEdgeSide"), "asset edge side")
    bes = parse_bit(r.get("btcEdgeSide"), "btc edge side")
    direction = r["direction"]
    want_asset = (1 - r["assetEdgeSide"]) if direction == 0 else r["assetEdgeSide"]
    want_btc = r["btcEdgeSide"] if direction == 0 else (1 - r["btcEdgeSide"])
    if r.get("assetOfferer") != want_asset or r.get("btcOfferer") != want_btc:
        raise ValueError("offerer/direction")
    parts = [
        b"\x01",
        *f32,
        d,
        aes,
        bes,
        parse_outpoint(r.get("assetOutpoint"), "asset outpoint"),
        parse_bit(r.get("assetOfferer"), "asset offerer"),
        parse_u64_be(r.get("assetHtlc"), "asset htlc", False),
        parse_outpoint(r.get("btcOutpoint"), "btc outpoint"),
        parse_bit(r.get("btcOfferer"), "btc offerer"),
        parse_u64_be(r.get("btcHtlc"), "btc htlc", False),
        parse_u64_be(r.get("assetAtoms"), "asset atoms", True),
        parse_u64_be(r.get("grossBtcMsat"), "gross btc msat", True),
    ]
    out = b"".join(parts)
    assert len(out) == 526
    return out


def receipt_digest_v1(r: dict) -> bytes:
    return tagged_hash("TradeLayer/TAP-VWAP/Receipt/v1", encode_receipt_v1(r))


def receipt_nullifier_v1(r: dict) -> bytes:
    encode_receipt_v1(r)
    payload = (
        parse_hex_exact(r["genesis"], 32, "genesis")
        + parse_outpoint(r["assetOutpoint"], "asset outpoint")
        + parse_bit(r["assetOfferer"], "asset offerer")
        + parse_u64_be(r["assetHtlc"], "asset htlc", False)
    )
    assert len(payload) == 77
    return tagged_hash("TradeLayer/TAP-VWAP/AssetHTLC/v1", payload)


def xonly_hex(scalar: int) -> str:
    pt = point_mul(G, scalar)
    assert pt is not None
    return pt[0].to_bytes(32, "big").hex()


def bip340_sign_deterministic_fixture(seckey: int, msg32: bytes, aux32: bytes) -> str:
    """Official BIP-0340 signing algorithm for deterministic test vector generation only."""
    assert 1 <= seckey < N and len(msg32) == 32 and len(aux32) == 32
    p = point_mul(G, seckey)
    assert p is not None
    d = seckey if (p[1] & 1) == 0 else (N - seckey)
    t_bytes = bytes(a ^ b for a, b in zip(d.to_bytes(32, "big"), tagged_hash("BIP0340/aux", aux32)))
    rand = tagged_hash("BIP0340/nonce", t_bytes + p[0].to_bytes(32, "big") + msg32)
    k0 = int.from_bytes(rand, "big") % N
    assert k0 != 0
    r_pt = point_mul(G, k0)
    assert r_pt is not None
    k = k0 if (r_pt[1] & 1) == 0 else (N - k0)
    rx = r_pt[0].to_bytes(32, "big")
    px = p[0].to_bytes(32, "big")
    e = int.from_bytes(tagged_hash("BIP0340/challenge", rx + px + msg32), "big") % N
    sig = rx + ((k + e * d) % N).to_bytes(32, "big")
    assert verify_schnorr_32(px, msg32, sig)
    return sig.hex()


def build_golden_bundle(bip340_csv_path: Path | None = None) -> dict:
    purchase = {
        "genesis": ("01" * 32),
        "policy": ("02" * 32),
        "asset": ("03" * 32),
        "edgeKey": xonly_hex(3),
        "btcPeerKey": xonly_hex(5),
        "assetPeerKey": xonly_hex(7),
        "rfqId": ("04" * 32),
        "quoteTranscript": ("05" * 32),
        "paymentHash": ("06" * 32),
        "assetPre": ("07" * 32),
        "assetPost": ("08" * 32),
        "btcPre": ("09" * 32),
        "btcPost": ("0a" * 32),
        "direction": 0,
        "assetEdgeSide": 0,
        "btcEdgeSide": 1,
        "assetOutpoint": ("0b" * 32) + "00000001",
        "assetOfferer": 1,
        "assetHtlc": "13",
        "btcOutpoint": ("0c" * 32) + "00000002",
        "btcOfferer": 1,
        "btcHtlc": "17",
        "assetAtoms": "400000",
        "grossBtcMsat": "80500000",
    }
    sale = {
        "genesis": ("01" * 32),
        "policy": ("02" * 32),
        "asset": ("03" * 32),
        "edgeKey": xonly_hex(11),
        "btcPeerKey": xonly_hex(13),
        "assetPeerKey": xonly_hex(19),
        "rfqId": ("14" * 32),
        "quoteTranscript": ("15" * 32),
        "paymentHash": ("06" * 32),
        "assetPre": ("17" * 32),
        "assetPost": ("18" * 32),
        "btcPre": ("19" * 32),
        "btcPost": ("1a" * 32),
        "direction": 1,
        "assetEdgeSide": 1,
        "btcEdgeSide": 0,
        "assetOutpoint": ("1b" * 32) + "00000000",
        "assetOfferer": 1,
        "assetHtlc": "29",
        "btcOutpoint": ("0c" * 32) + "00000002",
        "btcOfferer": 1,
        "btcHtlc": "17",
        "assetAtoms": "350000",
        "grossBtcMsat": "71250000",
    }
    fixtures = []
    for name, rec, scalars in (
        ("purchase_edge_side0_btc_side1", purchase, (3, 5, 7)),
        ("sale_edge_side1_btc_side0", sale, (11, 13, 19)),
    ):
        enc = encode_receipt_v1(rec)
        dig = receipt_digest_v1(rec)
        nul = receipt_nullifier_v1(rec)
        sigs = {
            "edge": bip340_sign_deterministic_fixture(scalars[0], dig, b"\x01" * 32),
            "btcPeer": bip340_sign_deterministic_fixture(scalars[1], dig, b"\x02" * 32),
            "assetPeer": bip340_sign_deterministic_fixture(scalars[2], dig, b"\x03" * 32),
        }
        fixtures.append({
            "name": name,
            "receipt": rec,
            "encoded_hex": enc.hex(),
            "encoded_sha256": hashlib.sha256(enc).hexdigest(),
            "digest_hex": dig.hex(),
            "nullifier_hex": nul.hex(),
            "signatures": sigs,
        })

    vectors = []
    if bip340_csv_path is not None and bip340_csv_path.is_file():
        with bip340_csv_path.open("r", encoding="utf-8") as fh:
            reader = csv.DictReader(fh)
            for row in reader:
                msg_hex = row["message"].lower()
                if len(msg_hex) != 64:
                    continue
                pub_hex = row["public key"].lower()
                sig_hex = row["signature"].lower()
                valid = row["verification result"].strip().upper() == "TRUE"
                assert verify_schnorr_32(bytes.fromhex(pub_hex), bytes.fromhex(msg_hex), bytes.fromhex(sig_hex)) == valid
                vectors.append({
                    "index": int(row["index"]),
                    "pubkey": pub_hex,
                    "message": msg_hex,
                    "signature": sig_hex,
                    "valid": valid,
                    "comment": row["comment"],
                })
    return {
        "schema": "tap-vwap-receipt-v1-golden",
        "wire_version": 1,
        "wire_byte_length": 526,
        "bip340_official_32b_vectors": vectors,
        "receipt_fixtures": fixtures,
    }


def main() -> int:
    out_path = Path(__file__).resolve().parent / "golden-fixtures.json"
    if len(sys.argv) > 1 and sys.argv[1] == "--generate":
        csv_path = Path(sys.argv[2])
        bundle = build_golden_bundle(csv_path)
        out_path.write_text(json.dumps(bundle, indent=2) + "\n", encoding="utf-8", newline="\n")
        print(f"Wrote {len(bundle['bip340_official_32b_vectors'])} BIP-340 vectors and {len(bundle['receipt_fixtures'])} golden receipt fixtures")
        return 0
    bundle = json.loads(out_path.read_text(encoding="utf-8"))
    for v in bundle["bip340_official_32b_vectors"]:
        got = verify_schnorr_32(bytes.fromhex(v["pubkey"]), bytes.fromhex(v["message"]), bytes.fromhex(v["signature"]))
        if got != v["valid"]:
            raise AssertionError(f"BIP340 vector {v['index']} mismatch")
    for fx in bundle["receipt_fixtures"]:
        enc = encode_receipt_v1(fx["receipt"])
        dig = receipt_digest_v1(fx["receipt"])
        nul = receipt_nullifier_v1(fx["receipt"])
        assert enc.hex() == fx["encoded_hex"]
        assert dig.hex() == fx["digest_hex"]
        assert nul.hex() == fx["nullifier_hex"]
        for role, key_field in (("edge", "edgeKey"), ("btcPeer", "btcPeerKey"), ("assetPeer", "assetPeerKey")):
            assert verify_schnorr_32(bytes.fromhex(fx["receipt"][key_field]), dig, bytes.fromhex(fx["signatures"][role]))
    print(f"PASS independent Python encoder & BIP340 check ({len(bundle['bip340_official_32b_vectors'])} vectors, {len(bundle['receipt_fixtures'])} fixtures)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
