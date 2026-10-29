"""Public throwaway fixtures; actual Core VerifyScript supplies the oracle."""
from pathlib import Path
import copy,hashlib,json,struct,sys
HERE=Path(__file__).resolve().parent
sha=lambda b:hashlib.sha256(b).digest()
ds=lambda b:sha(sha(b))
P=2**256-2**32-977
N=0xfffffffffffffffffffffffffffffffebaaedce6af48a03bbfd25e8cd0364141
G=(0x79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798,0x483ada7726a3c4655da4fbfc0e1108a8fd17b448a68554199c47d08ffb10d4b8)
FLAGS=0xe15
def addp(a,b):
    if a is None:return b
    if b is None:return a
    x,y=a;u,v=b
    if x==u and (y+v)%P==0:return None
    m=((3*x*x)*pow(2*y,-1,P) if a==b else (v-y)*pow(u-x,-1,P))%P
    w=(m*m-x-u)%P
    return w,(m*(x-w)-y)%P
def mul(k,point=G):
    result=None
    while k:
        if k&1:result=addp(result,point)
        point=addp(point,point);k>>=1
    return result
def pub(k,compressed=True):
    x,y=mul(k)
    return bytes([2+(y&1)])+x.to_bytes(32,'big') if compressed else b'\x04'+x.to_bytes(32,'big')+y.to_bytes(32,'big')
def cs(n):
    if n<253:return bytes([n])
    if n<65536:return b'\xfd'+struct.pack('<H',n)
    return b'\xfe'+struct.pack('<I',n)
def var(b):return cs(len(b))+b
class Reader:
    def __init__(self,b):self.b=b;self.i=0
    def take(self,n):
        result=self.b[self.i:self.i+n];assert len(result)==n;self.i+=n;return result
    def size(self):
        n=self.take(1)[0]
        return n if n<253 else int.from_bytes(self.take({253:2,254:4,255:8}[n]),'little')
    def var(self):return self.take(self.size())
def parse(b):
    c=Reader(b);version=c.take(4);witness=c.b[c.i]==0
    if witness:assert c.take(2)==b'\x00\x01'
    inputs=[]
    for _ in range(c.size()):inputs.append([c.take(36),c.var(),c.take(4)])
    outputs=[]
    for _ in range(c.size()):outputs.append([int.from_bytes(c.take(8),'little'),c.var()])
    stacks=[]
    if witness:
        for _ in inputs:stacks.append([c.var() for _ in range(c.size())])
    lock=c.take(4);assert c.i==len(b)
    return {'version':version,'inputs':inputs,'outputs':outputs,'stacks':stacks,'lock':lock,'witness':witness}
def serial(t,strip=False):
    b=t['version']+(b'\x00\x01' if t['witness'] and not strip else b'')+cs(len(t['inputs']))
    for outpoint,script,sequence in t['inputs']:b+=outpoint+var(script)+sequence
    b+=cs(len(t['outputs']))
    for amount,script in t['outputs']:b+=struct.pack('<Q',amount)+var(script)
    if t['witness'] and not strip:
        for stack in t['stacks']:
            b+=cs(len(stack))
            for item in stack:b+=var(item)
    return b+t['lock']
def digest(spend,vin,script,amount,hash_type=1):
    return ds(spend['version']+ds(b''.join(x[0] for x in spend['inputs']))+ds(b''.join(x[2] for x in spend['inputs']))+spend['inputs'][vin][0]+var(script)+struct.pack('<Q',amount)+spend['inputs'][vin][2]+ds(b''.join(struct.pack('<Q',a)+var(s) for a,s in spend['outputs']))+spend['lock']+struct.pack('<I',hash_type))
def integer(n):
    b=n.to_bytes(max(1,(n.bit_length()+7)//8),'big')
    if b[0]&128:b=b'\0'+b
    return b'\x02'+var(b)
def der(r,s,kind=1):
    b=integer(r)+integer(s)
    return b'\x30'+var(b)+bytes([kind])
def parts(item):
    c=Reader(item[2:-1]);assert c.take(1)==b'\x02';r=int.from_bytes(c.var(),'big');assert c.take(1)==b'\x02';s=int.from_bytes(c.var(),'big');return r,s
def sign(d,key,label,high=False,kind=1):
    nonce=int.from_bytes(sha(b'public-test-only/'+label.encode()+key.to_bytes(32,'big')+d),'big')%(N-1)+1
    r=mul(nonce)[0]%N;s=pow(nonce,-1,N)*(int.from_bytes(d,'big')+r*key)%N
    s=max(s,N-s) if high else min(s,N-s)
    return der(r,s,kind)
def sample(keys=(1,2),vin=0,inputs=1,high=(False,False),amount=20000,script=None,kind=1):
    script=script if script is not None else b'\x52'+b''.join(var(pub(k)) for k in keys)+b'\x52\xae'
    funding={'version':struct.pack('<I',2),'inputs':[[b'\x42'*32+bytes(4),b'',b'\xfe\xff\xff\xff']],'outputs':[[amount,b'\x00\x20'+sha(script)]],'stacks':[],'lock':bytes(4),'witness':False}
    spend={'version':struct.pack('<I',2),'inputs':[[bytes([0x30+i])*32+struct.pack('<I',i),b'',b'\xfd\xff\xff\xff'] for i in range(inputs)],'outputs':[[18000,b'\x51'],[1000,b'\x6a\x02TL']],'stacks':[[] for _ in range(inputs)],'lock':bytes(4),'witness':True}
    spend['inputs'][vin][0]=ds(serial(funding,True))+bytes(4)
    d=digest(spend,vin,script,amount,kind)
    spend['stacks'][vin]=[b'',sign(d,keys[0],'first',high[0],kind),sign(d,keys[1],'second',high[1],kind),script]
    return funding,spend,vin
h160=lambda b:hashlib.new('ripemd160',sha(b)).digest()
SHAPES=['p2pkh','p2wpkh','p2sh','p2wsh','wrapped-p2wpkh','wrapped-p2wsh']
TEMPLATES=dict(zip(SHAPES,['P2pkh','P2wpkh','P2shTwoOfTwo','P2wshTwoOfTwo','P2shP2wpkh','P2shP2wshTwoOfTwo']))
def push(b,nonminimal=False):
    if nonminimal or len(b)>75:
        return b'\x4c'+bytes([len(b)])+b if len(b)<256 else b'\x4d'+struct.pack('<H',len(b))+b
    return bytes([len(b)])+b
def pk_script(key):return b'\x76\xa9\x14'+h160(key)+b'\x88\xac'
def mode_digest(t,vin,script,amount,kind,witness):
    base=kind&31;alone=bool(kind&128)
    if witness:
        hp=bytes(32) if alone else ds(b''.join(x[0] for x in t['inputs']))
        hs=ds(b''.join(x[2] for x in t['inputs'])) if not alone and base==1 else bytes(32)
        ho=ds(b''.join(struct.pack('<Q',a)+var(s) for a,s in t['outputs'])) if base==1 else ds(struct.pack('<Q',t['outputs'][vin][0])+var(t['outputs'][vin][1])) if base==3 and vin<len(t['outputs']) else bytes(32)
        return ds(t['version']+hp+hs+t['inputs'][vin][0]+var(script)+struct.pack('<Q',amount)+t['inputs'][vin][2]+ho+t['lock']+struct.pack('<I',kind)),False
    if base==3 and vin>=len(t['outputs']):return b'\x01'+bytes(31),True
    inputs=[(i,x) for i,x in enumerate(t['inputs']) if not alone or i==vin]
    b=t['version']+cs(len(inputs))
    for i,(outpoint,_old,sequence) in inputs:
        b+=outpoint+var(script if i==vin else b'')+(bytes(4) if i!=vin and base in (2,3) else sequence)
    outputs=t['outputs'] if base==1 else [] if base==2 else [(2**64-1,b'')]*vin+[t['outputs'][vin]]
    b+=cs(len(outputs))+b''.join(struct.pack('<Q',a)+var(s) for a,s in outputs)+t['lock']+struct.pack('<I',kind)
    return ds(b),False
def make(shape,modes=(1,),vin=1,inputs=3,outputs=3,compressed=True,high=False,keys=None,vout=0):
    witness=shape not in ('p2pkh','p2sh');multi='wsh' in shape or shape=='p2sh'
    secrets=keys or ((1,2) if multi else (1,))
    public=[pub(key,compressed) for key in secrets]
    script=b'\x52'+b''.join(push(k) for k in public)+b'\x52\xae' if multi else pk_script(public[0])
    program=b'\x00\x20'+sha(script) if multi else b'\x00\x14'+h160(public[0])
    redeem=program if shape.startswith('wrapped-') else script
    spk=pk_script(public[0]) if shape=='p2pkh' else b'\xa9\x14'+h160(redeem)+b'\x87' if shape=='p2sh' or shape.startswith('wrapped-') else program
    funding={'version':struct.pack('<I',2),'inputs':[[b'\x42'*32+bytes(4),b'',b'\xfe\xff\xff\xff']],'outputs':[[100,b'\x51'] for _ in range(vout)]+[[20000,spk]],'stacks':[],'lock':bytes(4),'witness':False}
    spend={'version':struct.pack('<I',2),'inputs':[[bytes([0x30+i])*32+struct.pack('<I',i),b'',struct.pack('<I',0xfffffffd-i)] for i in range(inputs)],'outputs':[[18000+i,b'\x51'] for i in range(outputs)],'stacks':[[] for _ in range(inputs)] if witness else [],'lock':struct.pack('<I',19),'witness':witness}
    spend['inputs'][vin][0]=ds(serial(funding,True))+struct.pack('<I',vout)
    modes=tuple(modes if len(modes)==len(public) else modes*len(public))
    metadata={'shape':shape,'witness':witness,'script':script,'redeem':redeem,'public':public,'modes':modes,'vout':vout,'vin':vin,'secrets':secrets}
    resign(funding,spend,metadata,high=high)
    return funding,spend,metadata
def resign(f,s,m,high=False,amount=None):
    signatures=[sign(mode_digest(s,m['vin'],m['script'],f['outputs'][m['vout']][0] if amount is None else amount,kind,m['witness'])[0],key,'standard-'+str(i),high,kind) for i,(key,kind) in enumerate(zip(m['secrets'],m['modes']))]
    shape=m['shape'];vin=m['vin']
    if shape=='p2pkh':s['inputs'][vin][1]=push(signatures[0])+push(m['public'][0])
    elif shape=='p2sh':s['inputs'][vin][1]=b'\x00'+b''.join(push(sig) for sig in signatures)+push(m['script'])
    else:
        s['stacks'][vin]=[b'']+signatures+[m['script']] if 'wsh' in shape else signatures+[m['public'][0]]
        if shape.startswith('wrapped-'):s['inputs'][vin][1]=push(m['redeem'])
    m['signatures']=signatures
