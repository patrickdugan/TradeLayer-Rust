#!/usr/bin/env python3
"""Independent constructed-serialization and hashlib oracle, not consensus validation."""
import argparse
import hashlib
from pathlib import Path
import struct

ROOT = Path(__file__).resolve().parents[1]


def compact(n):
    if n < 253:
        return bytes([n])
    if n <= 65535:
        return b"\xfd" + struct.pack("<H", n)
    if n <= 0xFFFFFFFF:
        return b"\xfe" + struct.pack("<I", n)
    return b"\xff" + struct.pack("<Q", n)


def sha256d(raw):
    return hashlib.sha256(hashlib.sha256(raw).digest()).digest()


def transaction(stacks, outputs=1, first_script_size=0, output_script=b"\x51", witness=True, locktime=0, first_prevout=None):
    """Construct separate full/stripped serializations; do not parse Rust offsets."""
    version = struct.pack("<I", 2)
    vin = bytearray(compact(len(stacks)))
    offsets = {"vin-count": 6 if witness else 4}
    for i in range(len(stacks)):
        prevhash, previndex = first_prevout if i == 0 and first_prevout else (bytes([17 + i % 239]) * 32, i)
        vin.extend(prevhash)
        vin.extend(struct.pack("<I", previndex))
        script = b"\x51" * (first_script_size if i == 0 else 0)
        if i == 0:
            offsets["scriptsig-length"] = (6 if witness else 4) + len(vin)
        vin.extend(compact(len(script)) + script + b"\xfe\xff\xff\xff")
    vout = bytearray(compact(outputs))
    offsets["vout-count"] = (6 if witness else 4) + len(vin)
    for i in range(outputs):
        vout.extend(struct.pack("<Q", i + 1))
        if i == 0:
            offsets["outputscript-length"] = (6 if witness else 4) + len(vin) + len(vout)
        vout.extend(compact(len(output_script)) + output_script)
    witnesses = bytearray()
    for i, stack in enumerate(stacks):
        if i == 0:
            offsets["witness-count"] = 6 + len(vin) + len(vout) + len(witnesses)
        witnesses.extend(compact(len(stack)))
        for j, item in enumerate(stack):
            if i == 0 and j == 0:
                offsets["witnessitem-length"] = 6 + len(vin) + len(vout) + len(witnesses)
            witnesses.extend(compact(len(item)) + item)
    locktime_bytes = struct.pack("<I", locktime)
    stripped = version + vin + vout + locktime_bytes
    raw = version + b"\x00\x01" + vin + vout + witnesses + locktime_bytes if witness else stripped
    return bytes(raw), bytes(stripped), offsets


def vectors():
    rows = []
    valid = []

    def accepted(label, stacks, **kwargs):
        # Distinct stripped identities allow all variants in the same block.
        kwargs["locktime"] = len(valid)
        raw, stripped, _ = transaction(stacks, **kwargs)
        txid, wtxid = sha256d(stripped), sha256d(raw)
        rows.append([label, "ok", str(len(stacks)), str(kwargs.get("outputs", 1)),
                     str(len(stripped)), txid[::-1].hex(), wtxid[::-1].hex(), raw.hex()])
        valid.append((raw, stripped, txid))
        return txid

    def rejected(label, status, raw):
        rows.append([label, status, "-", "-", "-", "-", "-", raw.hex()])

    accepted("one-empty-item", [ [b""] ])
    accepted("two-empty-items", [ [b"", b""] ])
    accepted("empty-item-before-second-nonempty", [ [b""], [b"\x99"] ])
    accepted("zero-stack-before-empty-item", [ [], [b""] ])
    accepted("unknown-v2-program-output-empty-witness-item", [ [b""] ], output_script=b"\x52\x02\xab\xcd")
    parent = accepted("legacy-unknown-v2-parent", [[]], output_script=b"\x52\x02\xab\xcd", witness=False)
    accepted("unknown-v2-parent-spend-empty-item", [[b""]], first_prevout=(parent, 0))
    accepted("vin253-vout253-scriptsig253", [ [b""] ] + [[]] * 252, outputs=253, first_script_size=253)
    accepted("witness253-empty-items", [[b""] * 253])
    accepted("witness-item253-bytes", [[b"\x42" * 253]])
    accepted("legacy-vin253-vout253", [[]] * 253, outputs=253, witness=False)
    raw, _, offsets = transaction([[b""]])
    for name, offset in offsets.items():
        number = raw[offset]
        assert number < 253
        for prefix, packed in [(253, struct.pack("<H", number)),
                               (254, struct.pack("<I", number)),
                               (255, struct.pack("<Q", number))]:
            rejected(f"noncanonical-{name}-{prefix}", "NonCanonicalCompactSize",
                     raw[:offset] + bytes([prefix]) + packed + raw[offset + 1:])
    for flag in [0, 2, 3, 255]:
        rejected(f"invalid-optional-flag-{flag}", "Invalid", raw[:5] + bytes([flag]) + raw[6:])
    for stacks in [[], [[]], [[], []]]:
        empty, _, _ = transaction(stacks)
        rejected(f"all-empty-stacks-{len(stacks)}", "Invalid", empty)
    for length in range(len(raw)):
        rejected(f"truncated-{length}", "Truncated", raw[:length])
    rejected("trailing-byte", "TrailingBytes", raw + b"\x00")
    vin_offset = offsets["vin-count"]
    rejected("excessive-input-count", "Invalid", raw[:vin_offset] + compact(100001) + raw[vin_offset + 1:])
    vout_offset = offsets["vout-count"]
    rejected("zero-output-count", "Invalid", raw[:vout_offset] + b"\x00" + raw[vout_offset + 1:])
    text = "# Independent Python serializer + hashlib SHA256d. Synthetic syntax observations, not valid spends.\n"
    text += "# label\tstatus\tinputs\toutputs\tstripped-bytes\ttxid-display\twtxid-display\traw-hex\n"
    text += "\n".join("\t".join(row) for row in rows) + "\n"
    level = [entry[2] for entry in valid]
    while len(level) > 1:
        if len(level) % 2:
            level.append(level[-1])
        level = [sha256d(level[i] + level[i + 1]) for i in range(0, len(level), 2)]
    header = struct.pack("<I", 2) + bytes(32) + level[0] + bytes(12)
    block = header + compact(len(valid)) + b"".join(item[0] for item in valid)
    return text, block.hex() + "\n", len(rows), len(valid)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--write", action="store_true")
    args = parser.parse_args()
    text, block, count, accepted = vectors()
    for name, content in [("tx_parser_review.tsv", text), ("tx_parser_review_block.hex", block)]:
        target = ROOT / "fixtures" / name
        if args.write:
            target.write_text(content, encoding="ascii", newline="\n")
        else:
            assert target.read_text(encoding="ascii") == content, f"independent fixture mismatch: {name}"
    print(f"Verified {count} parser vectors ({accepted} accepted syntax) and one independently hashed block.")


if __name__ == "__main__":
    main()
