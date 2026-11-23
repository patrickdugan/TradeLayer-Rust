"""Independent SHA-256 ABI and fixed-depth accumulator audit (research only)."""
from hashlib import sha256
from struct import pack

DEPTH = 16
CAP = 1 << DEPTH

def h(tag, *parts):
    return sha256(tag.encode('ascii') + b'\x00' + b''.join(parts)).digest()

def node(left, right):
    return h('TLBRIDGE/DEPOSIT-NODE/V1', left, right)

ZERO = [h('TLBRIDGE/DEPOSIT-EMPTY/V1')]
for _ in range(DEPTH):
    ZERO.append(node(ZERO[-1], ZERO[-1]))

def counted(n, root):
    return h('TLBRIDGE/DEPOSIT-ROOT/V1', pack('<Q', n), root)

class Frontier:
    def __init__(self):
        self.frontier = [None] * DEPTH
        self.count = 0
        self.root = ZERO[DEPTH]

    def append(self, leaf):
        if len(leaf) != 32 or self.count >= CAP:
            raise ValueError('leaf/capacity')
        x, index = leaf, self.count
        for level in range(DEPTH):
            if index & 1:
                x = node(self.frontier[level], x)
            else:
                self.frontier[level] = x
                x = node(x, ZERO[level])
            index >>= 1
        self.count += 1
        self.root = x
        return counted(self.count, x)

    def commitment(self):
        return counted(self.count, self.root)

def reference(leaves):
    level = list(leaves)
    for depth in range(DEPTH):
        if len(level) & 1:
            level.append(ZERO[depth])
        level = [node(level[i], level[i + 1]) for i in range(0, len(level), 2)]
    return counted(len(leaves), level[0] if level else ZERO[DEPTH])

def run():
    # Reproduce the previous frozen domain, ABI and two actual deposit leaves.
    b = lambda n: bytes([n])*32
    named = lambda t, a: h(t, bytes([len(a)]), a.encode('ascii'))
    domain = h('TLBRIDGE/DOMAIN/V1', b(1), b(2), b(3), b(4), named('TLBRIDGE/TOKEN/V1','usdc.testnet'), named('TLBRIDGE/VAULT/V1','bridge.testnet'), pack('<I',77))
    assert domain.hex() == '76dcd42c7d41733b0f6f0fbbef7ec8489d634e0e12fab06ef5aeb701567fa99a'
    wire = b'TLSTMT01'+domain+b''.join(b(n) for n in range(5,14))+pack('<8Q',1,100,107,2,1,10000000,10000000,2000000)
    digest = sha256(wire).digest()
    assert len(wire)==392 and digest.hex()=='b14e0c2eac85dbce4a56d2d9e7aa0208f09288cdcaaa192745c319d596826ddd'
    assert [int.from_bytes(digest[i:i+16],'big') for i in (0,16)] == [235678601495699792362129670830845198856,319775569041926914538904181906161692125]
    def deposit(i, r, a):
        dep = h('TLBRIDGE/DEPOSIT-ID/V1', domain, b(r), pack('<I',0))
        return h('TLBRIDGE/DEPOSIT-LEAF/V1',domain,pack('<Q',i),dep,named('TLBRIDGE/SENDER/V1','alice.testnet'),b(42),pack('<Q',a))
    f = Frontier()
    f.append(deposit(0,20,8000000))
    f.append(deposit(1,21,2000000))
    assert f.commitment().hex() == '948897c64dbc116f71e31d8eff0740db3eeb1368e11fee6cdc70de60662578ec'
    print('ZTB-02A independent Python ABI/domain/digest/limbs/Merkle-root: PASS')
    f = Frontier()
    leaves = []
    assert f.commitment() == reference(leaves)
    matched = 1
    for i in range(CAP):
        item = h('ZTB02B/TEST/V1', pack('<Q',i))
        leaves.append(item)
        root = f.append(item)
        n = i+1
        if n <= 256 or n in {511,512,513,1023,1024,1025,4095,4096,4097,32767,32768,32769,CAP-1,CAP}:
            assert root == reference(leaves), n
            matched += 1
    before = f.commitment()
    try:
        f.append(bytes(32))
    except ValueError:
        pass
    else:
        raise AssertionError('accepted capacity overflow')
    assert f.count == CAP and f.commitment() == before
    print(f'Frontier/reference match: {matched}/{matched} checkpoints, {CAP} appends, capacity refusal PASS')

if __name__ == '__main__':
    run()
