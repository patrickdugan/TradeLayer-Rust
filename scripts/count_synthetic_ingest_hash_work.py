#!/usr/bin/env python3
"""Count SHA-256 compression calls in the pinned synthetic ingest workload.

This independently walks the fixture bytes; it imports no Rust cursor/layout.
It models make_block in candidate 9091bcd and the corrected main benchmark:
template 0 is coinbase, template 1 legacy, template 2 witness, with witness
selection (position * 37) % 100 < witness_percent. Changing prevout bytes
does not change hash lengths. These are operation counts, NOT timings or an
overall serial-work fraction. Classification, maps, allocation, parsing,
synchronization, and later consensus verification are excluded.
"""

import argparse
import hashlib
import json
from pathlib import Path


def parse_fixture(raw):
    cursor = 80

    def read(size):
        nonlocal cursor
        end = cursor + size
        if end > len(raw):
            raise ValueError("truncated fixture")
        result = raw[cursor:end]
        cursor = end
        return result

    def compact():
        first = read(1)[0]
        if first < 253:
            return first
        return int.from_bytes(read({253: 2, 254: 4, 255: 8}[first]), "little")

    def variable():
        return read(compact())

    rows = []
    for _ in range(compact()):
        start = cursor
        read(4)
        witness = raw[cursor] == 0
        if witness and read(2) != b"\x00\x01":
            raise ValueError("unsupported fixture witness flags")
        vin_start = cursor
        inputs = compact()
        for _ in range(inputs):
            read(36)
            variable()
            read(4)
        for _ in range(compact()):
            read(8)
            variable()
        outputs_end = cursor
        if witness:
            for _ in range(inputs):
                for _ in range(compact()):
                    variable()
        read(4)
        length = cursor - start
        rows.append({
            "raw_bytes": length,
            "stripped_bytes": 8 + outputs_end - vin_start if witness else length,
            "witness": witness,
        })
    if cursor != len(raw) or len(rows) < 3:
        raise ValueError("fixture boundary/template mismatch")
    return rows


def double_sha256_compressions(length):
    # First SHA: complete 64-byte blocks + padding with 1 marker and 8 length
    # bytes. Second SHA hashes a 32-byte digest in exactly one padded block.
    return (length + 8) // 64 + 2


def count_case(rows, count, witness_percent):
    if not 1 <= count <= 10000 or not 0 <= witness_percent <= 100:
        raise ValueError("invalid case")
    chosen = [rows[0]] + [
        rows[2 if (position * 37) % 100 < witness_percent else 1]
        for position in range(1, count)
    ]
    transaction_calls = sum(
        double_sha256_compressions(row["raw_bytes"])
        + (double_sha256_compressions(row["stripped_bytes"]) if row["witness"] else 0)
        for row in chosen
    )
    levels = []
    width = count
    while width > 1:
        width = (width + 1) // 2
        levels.append(width)
    merkle_calls = sum(levels) * double_sha256_compressions(64)
    return {
        "transaction_count": count,
        "witness_percent": witness_percent,
        "transaction_hash_compressions": transaction_calls,
        "serial_merkle_compressions": merkle_calls,
        "merkle_share_of_counted_compressions": merkle_calls / (transaction_calls + merkle_calls),
        "merkle_levels": len(levels),
        "windows_512": (count + 511) // 512,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fixture", type=Path, default=Path(__file__).resolve().parents[1]
                        / "fixtures" / "raw_block_ingest_synthetic.hex")
    parser.add_argument("--case", action="append", help="transaction_count,witness_percent")
    args = parser.parse_args()
    raw = bytes.fromhex(args.fixture.read_text().strip())
    if hashlib.sha256(raw).hexdigest() != "3e3113b5051e517ae188901511dd312bddc7c4eea0c3710c7094b8f73bb589f3":
        raise ValueError("pinned synthetic fixture SHA256 mismatch")
    rows = parse_fixture(raw)
    cases = [(int(count), int(percent)) for count, percent in
             (case.split(",") for case in args.case)] if args.case else [
                 (1000, 0), (1000, 50), (4000, 50), (10000, 95)]
    print(json.dumps({
        "scope": "SHA256_COMPRESSION_CALL_COUNTS_ONLY_NOT_WALL_TIME",
        "fixture_sha256": hashlib.sha256(raw).hexdigest(),
        "fixture_templates": rows,
        "cases": [count_case(rows, *case) for case in cases],
    }, indent=2))


if __name__ == "__main__":
    main()
