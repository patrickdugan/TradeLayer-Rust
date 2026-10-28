"""Verify pinned official bytes; independent hashlib IDs, no chain validity verdict."""
from pathlib import Path
import hashlib
import json
import re
import struct
import argparse

ROOT = Path(__file__).resolve().parent
if ROOT.name == "scripts":
    ROOT = ROOT.parent


def sha256d(data):
    return hashlib.sha256(hashlib.sha256(data).digest()).digest()


class Reader:
    def __init__(self, raw):
        self.raw, self.pos = raw, 0

    def read(self, length):
        end = self.pos + length
        assert end <= len(self.raw)
        result = self.raw[self.pos:end]
        self.pos = end
        return result

    def size(self):
        x = self.read(1)[0]
        if x < 253:
            return x
        n = int.from_bytes(self.read({253: 2, 254: 4, 255: 8}[x]), "little")
        assert n >= {253: 253, 254: 65536, 255: 4294967296}[x]
        return n

    def data(self):
        return self.read(self.size())

    def varint(self):
        n = 0
        while True:
            c = self.read(1)[0]
            n = (n << 7) | (c & 127)
            if c & 128:
                n += 1
            else:
                return n


def tx(reader):
    start = reader.pos
    version = reader.read(4)
    flags = 0
    if reader.raw[reader.pos] == 0:
        reader.read(1)
        flags = reader.read(1)[0]
        assert flags in (1, 8, 9)
    body_start = reader.pos
    inputs = reader.size()
    assert inputs
    for _ in range(inputs):
        reader.read(36)
        reader.data()
        reader.read(4)
    outputs = reader.size()
    assert outputs
    for _ in range(outputs):
        reader.read(8)
        reader.data()
    body_end = reader.pos
    if flags & 1:
        assert inputs
        for _ in range(inputs):
            for _ in range(reader.size()):
                reader.data()
    witness_end = reader.pos
    hogex = bool(flags & 8)
    if flags & 8:
        assert reader.read(1) == b"\0", "official example is explicitly a null-MWEB HogEx"
    locktime = reader.read(4)
    stripped = version + reader.raw[body_start:body_end] + locktime
    with_witness = version + b"\x00\x01" + reader.raw[body_start:witness_end] + locktime if flags & 1 else stripped
    return dict(start=start, end=reader.pos, txid=sha256d(stripped)[::-1].hex(),
                wtxid=sha256d(with_witness)[::-1].hex(), hogex=hogex,
                inputs=inputs, outputs=outputs, strippedBytes=len(stripped), transparentBytes=len(with_witness))


def block(raw):
    reader = Reader(raw)
    header = reader.read(80)
    txs = [tx(reader) for _ in range(reader.size())]
    hashes = [bytes.fromhex(x["txid"])[::-1] for x in txs]
    while len(hashes) > 1:
        if len(hashes) % 2:
            hashes.append(hashes[-1])
        hashes = [sha256d(a + b) for a, b in zip(hashes[::2], hashes[1::2])]
    assert hashes[0] == header[36:68]
    mw = None
    if len(txs) >= 2 and txs[-1]["hogex"]:
        start = reader.pos
        assert reader.read(1) == b"\x01"
        height = reader.varint()
        assert height <= 2147483647
        reader.read(5 * 32)
        outputs, kernels = reader.varint(), reader.varint()
        assert [reader.size(), reader.size(), reader.size()] == [0, 0, 0]
        mw = dict(start=start, end=reader.pos, height=height, outputMmrSize=outputs, kernelMmrSize=kernels)
    assert reader.pos == len(raw)
    return dict(blockId=sha256d(header)[::-1].hex(), merkleRoot=header[36:68][::-1].hex(), transactions=txs, mweb=mw)


def outputs():
    source = ROOT / "fixtures/litecoin_core_mining_changes_source.txt"
    raw_source = source.read_bytes()
    assert hashlib.sha1(b"blob " + str(len(raw_source)).encode() + b"\0" + raw_source).hexdigest() == "468c29ef17fa8979e9f52afb2f8212884ab5a7a2"
    text = source.read_text(encoding="utf-8")
    response = json.loads(re.search(r"GBT Response:\s*```\s*(\{.*?\})\s*```", text, re.S)[1])
    raw = bytes.fromhex(re.search(r'submitblock "([0-9a-f]+)"', text)[1])
    result = block(raw)
    assert len(result["transactions"]) == 3
    for actual, expected in zip(result["transactions"][1:], response["transactions"]):
        assert actual["txid"] == expected["txid"]
        assert actual["wtxid"] == expected["hash"]
        assert 3 * actual["strippedBytes"] + actual["transparentBytes"] == expected["weight"]
    assert result["mweb"]["height"] == response["height"]
    expected = dict(pin="beae01d62292a0aab363b7a4d3f606708cea7260", source="doc/mweb/mining-changes.md",
                    sourceGitBlob="468c29ef17fa8979e9f52afb2f8212884ab5a7a2", classification="official submitted-block example; chain inclusion unverified",
                    rawSha256=hashlib.sha256(raw).hexdigest(), **result)
    return {"litecoin_official_post_mweb.hex": (raw.hex() + "\n").encode(),
            "litecoin_official_post_mweb.json": (json.dumps(expected, indent=2) + "\n").encode()}


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    directory = ROOT / "fixtures"
    directory.mkdir(exist_ok=True)
    for name, data in outputs().items():
        path = directory / name
        if args.check:
            assert path.read_bytes() == data
        else:
            path.write_bytes(data)
    print("PASS: official full block, both declared TXID/witness IDs, exact transparent Merkle root and empty MWEB header/body boundary")
