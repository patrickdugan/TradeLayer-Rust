"""Execute official offline codec on derived inputs; no validity claim."""
from pathlib import Path
import hashlib,json,subprocess,argparse
ROOT=Path(__file__).resolve().parent
if ROOT.name=='scripts':ROOT=ROOT.parent
args_parser=argparse.ArgumentParser();args_parser.add_argument('--tool',type=Path,default=ROOT/'reference/litecoin-0.21.4/bin/litecoin-tx.exe');args_parser.add_argument('--check',action='store_true');args=args_parser.parse_args()
tool=args.tool
assert hashlib.sha256(tool.read_bytes()).hexdigest()=='050f54e1aadf3724d3188bd0901534e1e753bec295cba1aa00a2771b190ef469'
base=bytes.fromhex((ROOT/'fixtures/mweb_only_transfer.hex').read_text().strip())
kernel=bytes.fromhex((ROOT/'fixtures/mweb_only_transfer.kernel.hex').read_text().strip())
assert base[-4-len(kernel):-4]==kernel and kernel[0]==17
ib=base[74:270];ob=base[271:1168]
assert base[73]==1 and ib[0]==1 and base[270]==2 and ob[99]==1
def vi(n):
    b=[n&127]
    while n>127:n=(n>>7)-1;b.append((n&127)|128)
    return bytes(reversed(b))
def size(n):
    if n<253:return bytes([n])
    if n<=65535:return b'\xfd'+n.to_bytes(2,'little')
    return b'\xfe'+n.to_bytes(4,'little')
def kern(mask,fee=0,pegin=0,outs=None,height=0,extra=b''):
    b=bytes([mask])
    if mask&1:b+=vi(fee)
    if mask&2:b+=vi(pegin)
    if mask&4:
        outs=[] if outs is None else outs;b+=size(len(outs))
        for amount,script in outs:b+=vi(amount)+size(len(script))+script
    if mask&8:b+=vi(height)
    if mask&16:b+=kernel[3:36]
    if mask&32:b+=size(len(extra))+extra
    return b+kernel[-97:]
CASES=[]
def add(name,raw,scope='known-fields'):CASES.append((name,raw,scope))
def addkernel(name,**kw):add(name,base[:-4-len(kernel)]+kern(**kw)+base[-4:])
for name,kw in [
    ('kernel_zero_features',dict(mask=0)),('kernel_fee_zero',dict(mask=1)),
    ('kernel_fee_varint128',dict(mask=1,fee=128)),('kernel_pegin_zero',dict(mask=2)),
    ('kernel_pegin_large',dict(mask=2,pegin=8400000000000000)),
    ('kernel_empty_pegouts',dict(mask=4)),('kernel_one_empty_script',dict(mask=4,outs=[(0,b'')])),
    ('kernel_two_pegouts',dict(mask=4,outs=[(1,b'\x51'),(128,b'\x6a\x02tl')])),
    ('kernel_lock_zero',dict(mask=8)),('kernel_lock_max',dict(mask=8,height=2147483647)),
    ('kernel_stealth_only',dict(mask=16)),('kernel_extra_empty',dict(mask=32)),
    ('kernel_extra_1025',dict(mask=32,extra=b'k'*1025)),
    ('kernel_all_features',dict(mask=63,fee=128,pegin=1,outs=[(2,b'\x51')],height=128,extra=b'xyz')),
    ('kernel_all_empty_vectors',dict(mask=63))]:addkernel(name,**kw)
assert base[-5-len(kernel)]==1
for name,kernels in [('two_kernels_first_original',[kernel,kern(mask=0)]),('two_kernels_first_zero',[kern(mask=0),kernel])]:add(name,base[:-5-len(kernel)]+b'\x02'+b''.join(kernels)+base[-4:])
for mask in [0,2,3]:
    extra=b'' if mask==2 else b'in-extra';b=bytes([mask])+ib[1:99]
    if mask&1:b+=ib[99:132]
    if mask&2:b+=size(len(extra))+extra
    b+=ib[132:];add('input_features_'+str(mask),base[:74]+b+base[270:])
for mask in [0,2,3]:
    extra=b'' if mask==2 else b'out-extra';message=bytes([mask])
    if mask&1:message+=ob[100:158]
    if mask&2:message+=size(len(extra))+extra
    b=ob[:99]+message+ob[158:];add('output_features_'+str(mask),base[:271]+b+base[1168:])
for name,index,bit in [('input',74,4),('output',370,4),('kernel',len(base)-4-len(kernel),64)]:
    changed=bytearray(base);changed[index]|=bit;add('future_'+name+'_bit',bytes(changed),'unknown-feature-refusal')
add('truncated_signature',base[:-5],'malformed')
add('fee_i64_overflow',base[:-4-len(kernel)]+b'\x01'+vi(1<<63)+kernel[-97:]+base[-4:],'malformed')
add('height_i32_overflow',base[:-4-len(kernel)]+b'\x08'+vi(1<<31)+kernel[-97:]+base[-4:],'malformed')
results=[]
for name,raw,scope in CASES:
    def run(args,wire=raw):
        r=subprocess.run([str(tool),'-regtest',*args,wire.hex()],capture_output=True,text=True,creationflags=0x08000000)
        return r.returncode,r.stdout.strip(),r.stderr.strip()
    code,text,error=run(['-json']);cc,canonical,ce=run([])
    roundtrip_code,roundtrip_text,roundtrip_error=run(['-json'],bytes.fromhex(canonical)) if cc==0 and canonical!=raw.hex() else (code,text,error)
    results.append({'name':name,'scope':scope,'rawHex':raw.hex(),'decoderExit':code,'serializerExit':cc,'decoderError':error,'serializerError':ce,'canonicalHex':canonical if cc==0 else None,'decoded':json.loads(text)if code==0 else None,'canonicalDecoderExit':roundtrip_code,'canonicalDecoderError':roundtrip_error})
data=(json.dumps({'sourceCommit':'beae01d62292a0aab363b7a4d3f606708cea7260','binarySha256':'050f54e1aadf3724d3188bd0901534e1e753bec295cba1aa00a2771b190ef469','scope':'offline Core codec observations on derived inputs; no validity claim','cases':results},indent=2)+'\n').encode()
target=ROOT/'fixtures/litecoin_core_codec_observations.json'
if args.check:assert target.read_bytes()==data
else:target.write_bytes(data)
print('Observed',len(results),'cases;',sum(x['decoderExit']==0 for x in results),'Core decoder successes;',sum(x['serializerExit']==0 and x['canonicalHex']!=x['rawHex'] for x in results),'genuine serialization differences')
