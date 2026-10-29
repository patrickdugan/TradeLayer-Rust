"""Deterministic raw inputs; independent actual Core calls produce outcomes."""
from pathlib import Path
import copy,hashlib,json,struct,subprocess
import argparse,runpy
HERE=Path(__file__).resolve().parent.parent
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--core-script-reference',type=Path,required=True)
parser.add_argument('--core-basic-reference',type=Path,required=True)
parser.add_argument('--check',action='store_true')
args=parser.parse_args()
scope=runpy.run_path(str(HERE/'scripts/bitcoin_supported_signing_reference.py'))
def write(name,value):
    path=HERE/'fixtures'/name
    raw=value.encode()
    if args.check:
        assert path.read_bytes()==raw,str(path)+' differs from actual regeneration'
    else:path.write_bytes(raw)

make,resign,serial,ds,cs,var=[scope[x] for x in ('make','resign','serial','ds','cs','var')]
MAX=21_000_000*100_000_000

def merkle(leaves):
    while len(leaves)>1:
        if len(leaves)%2:leaves=leaves+[leaves[-1]]
        leaves=[ds(leaves[i]+leaves[i+1]) for i in range(0,len(leaves),2)]
    return leaves[0]

def coinbase(script=b'\x51',amount=0):
    return {'version':struct.pack('<I',2),'inputs':[[bytes(32)+b'\xff'*4,b'\x01\x01',b'\xff'*4]],'outputs':[[amount,script]],'stacks':[],'lock':bytes(4),'witness':False}

def block(nodes,cb=None):
    cb=copy.deepcopy(cb or coinbase())
    witness=any(t['witness'] for t in nodes)
    if witness:
        witness_root=merkle([bytes(32)]+[ds(serial(t)) for t in nodes])
        cb['witness']=True;cb['stacks']=[[bytes(32)]]
        cb['outputs'].append([0,bytes.fromhex('6a24aa21a9ed')+ds(witness_root+bytes(32))])
    txs=[cb]+nodes
    header=struct.pack('<I',4)+b'\x33'*32+merkle([ds(serial(t,True)) for t in txs])+struct.pack('<III',1700000000,0x207fffff,0)
    return header+cs(len(txs))+b''.join(serial(t) for t in txs),ds(header)[::-1],witness

def scene(shapes,modes=None,amounts=None,out=None,vouts=None):
    count=len(shapes);modes=modes or [1]*count;amounts=amounts or [20000]*count;vouts=vouts or [0]*count
    funds=[];metas=[]
    for i,(shape,mode,vout) in enumerate(zip(shapes,modes,vouts)):
        keys=(i*3+1,i*3+2) if shape in ('p2sh','p2wsh','wrapped-p2wsh') else (i*3+1,)
        fund,_,meta=make(shape,modes=(mode,),vin=i,inputs=count,outputs=count,keys=keys,vout=vout)
        fund['outputs'][vout][0]=amounts[i];fund['lock']=struct.pack('<I',i+1)
        funds.append(fund);metas.append(meta)
    witness=any(m['witness'] for m in metas)
    tx={'version':struct.pack('<I',2),'inputs':[[ds(serial(f,True))+struct.pack('<I',v),b'',struct.pack('<I',0xfffffffd-i)] for i,(f,v) in enumerate(zip(funds,vouts))],
        'outputs':[[a,b'\x51'] for a in (out if out is not None else [19000]*count)],
        'stacks':[[] for _ in funds] if witness else [],'lock':bytes(4),'witness':witness}
    for f,m in zip(funds,metas):resign(f,tx,m)
    return funds,tx,metas

records=[];core_rows=[];basic_rows=[]
def record(name,funds,tx,metas,expected='OK',external=None,nodes=None,position=1,cb=None):
    ext=funds if external is None else external
    raw,display,witness=block(nodes if nodes is not None else [tx],cb)
    total=sum(f['outputs'][m['vout']][0] for f,m in zip(funds,metas))
    records.append([name,str(position),raw.hex(),display.hex(),str(witness).lower(),'|'.join(serial(f).hex() for f in ext) or '-',expected,str(len(tx['inputs'])),str(total),str(sum(a for a,_ in tx['outputs']))])
    basic_rows.append([name,serial(tx).hex()])
    for f,m in zip(funds,metas):
        amount,spk=f['outputs'][m['vout']]
        core_rows.append([name+'/'+str(m['vin']),str(m['vout']),str(m['vin']),'3605',serial(f).hex(),serial(tx).hex(),str(amount),spk.hex()])

for shape in scope['SHAPES']:
    for mode in [1,2,3,129,130,131]:
        record(shape+'-mode-'+str(mode),*scene([shape,shape],[mode,mode]))
record('mixed-three-shapes',*scene(['p2pkh','wrapped-p2wpkh','p2wsh'],[1,130,131]))
record('nonzero-funding-vouts',*scene(['p2wpkh','p2sh'],[1,1],vouts=[1,2]))
for shape in ['p2pkh','p2wpkh','p2sh','p2wsh','wrapped-p2wpkh','wrapped-p2wsh']:
    f,t,m=scene([shape,shape],[3,3],out=[19000]);record(shape+'-single-last-output-missing',f,t,m)
f,t,m=scene(['p2wpkh','p2wpkh']);b=bytearray(t['stacks'][1][0]);b[-3]^=1;t['stacks'][1][0]=bytes(b);record('later-invalid-signature',f,t,m,'InputScript1')
for name,out,expected in [('negative-fee',[40001],'InputsBelowOutputs'),('output-max-plus-one',[MAX+1],'OutputTooLarge'),('negative-output',[2**64-1],'NegativeOutput'),('output-total-too-large',[MAX,1],'OutputTotalTooLarge')]:
    record(name,*scene(['p2wpkh','p2wpkh'],out=out),expected)
record('funding-max-plus-one',*scene(['p2wpkh','p2wpkh'],amounts=[20000,MAX+1],out=[1]),'InvalidFundingValue1')
record('funding-negative',*scene(['p2wpkh','p2wpkh'],amounts=[20000,2**64-1],out=[1]),'InvalidFundingValue1')
record('input-total-too-large',*scene(['p2wpkh','p2wpkh'],amounts=[MAX,1],out=[1]),'InputTotalTooLarge1')
record('max-money-positive',*scene(['p2wpkh'],amounts=[MAX],out=[MAX]))
f,t,m=scene(['p2wpkh','p2wpkh']);f[1]=copy.deepcopy(f[0]);m[1]=copy.deepcopy(m[0]);m[1]['vin']=1;t['inputs'][1][0]=t['inputs'][0][0]
for fund,meta in zip(f,m):resign(fund,t,meta)
record('duplicate-prevout',f,t,m,'DuplicateInput')
f,t,m=scene(['p2wpkh','p2wpkh']);record('missing-funding',f,t,m,'MissingFunding1',external=f[:1]);record('duplicate-funding',f,t,m,'DuplicateFunding1',external=[f[0],f[0],f[1]])
extra,_,_=scene(['p2pkh']);record('extra-funding',f,t,m,'UnexpectedFunding2',external=f+extra)
record('same-block-earlier',f,t,m,external=[],nodes=f+[t],position=3)
record('same-block-forward',f,t,m,'ForwardReference',external=[],nodes=[t]+f)
record('same-block-redundant-external',f,t,m,'UnexpectedFunding0',external=f,nodes=f+[t],position=3)
f,t,m=scene(['p2wpkh','p2wpkh']);f[1]['outputs'][0][1]=b'\x51';t['inputs'][1][0]=ds(serial(f[1],True))+bytes(4)
for fund,meta in zip(f,m):resign(fund,t,meta)
t['stacks'][1]=[] # Genuine Core-valid OP_TRUE, explicitly outside six-template profile.
record('later-unsupported-script',f,t,m,'UnsupportedScript1')
f,t,m=scene(['p2pkh']);cb=coinbase(f[0]['outputs'][0][1],20000);f[0]=cb;t['inputs'][0][0]=ds(serial(cb,True))+bytes(4);resign(cb,t,m[0]);record('same-block-coinbase',f,t,m,'SameBlockCoinbase',external=[],cb=cb)
f,t,m=scene(['p2wpkh','p2wpkh']);f[0]['outputs'].append(copy.deepcopy(f[0]['outputs'][0]));f[1]=f[0];m[1]=copy.deepcopy(m[0]);m[1]['vin']=1;m[1]['vout']=1
for i in range(2):t['inputs'][i][0]=ds(serial(f[0],True))+struct.pack('<I',i)
for fund,meta in zip(f,m):resign(fund,t,meta)
record('two-vouts-one-funding',f,t,m,external=f[:1])
f,t,m=scene(['p2wpkh','p2wpkh']);t['inputs'][0][0]=t['inputs'][0][0][:32]+struct.pack('<I',100)
for fund,meta in zip(f,m):resign(fund,t,meta)
record('nonexistent-funding-vout',f,t,m,'FundingOutput0')
f,t,m=scene(['p2wpkh','p2wpkh']);t['inputs'][0][0]=b'\x44'*32+bytes(4)
for fund,meta in zip(f,m):resign(fund,t,meta)
record('wrong-funding-txid',f,t,m,'MissingFunding0')
f,t,m=scene(['p2wpkh','p2wpkh'],amounts=[20000,MAX+1],out=[1]);b=bytearray(t['stacks'][0][0]);b[-3]^=1;t['stacks'][0][0]=bytes(b)
record('complete-value-preflight-before-script',f,t,m,'InvalidFundingValue1')
record('zero-value-consensus-no-dust-policy',*scene(['p2wpkh','p2wpkh'],amounts=[0,0],out=[0]))

# Direct context-free Core controls include branches not reachable through a
# selected non-coinbase block transaction. No expected results enter execution.
for length in [1,2,100,101]:
    cb=coinbase();cb['inputs'][0][1]=b'\x01'*length;basic_rows.append(['coinbase-script-'+str(length),serial(cb).hex()])
f,t,m=scene(['p2pkh','p2pkh']);t['inputs'][0][0]=bytes(32)+b'\xff'*4;basic_rows.append(['ordinary-null-prevout',serial(t).hex()])

(HERE/'fixtures').mkdir(exist_ok=True)
write('bitcoin_supported_transaction.tsv','\n'.join('\t'.join(r) for r in records)+'\n')
write('bitcoin_supported_input_core.tsv','\n'.join('\t'.join(r) for r in core_rows)+'\n')
write('bitcoin_context_free_raw.tsv','\n'.join('\t'.join(r) for r in basic_rows)+'\n')
core=args.core_script_reference
assert hashlib.sha256(core.read_bytes()).hexdigest()=='6e1b7c303c8986dabcda4968456fedce388be288a82873ba0231e10d8ea9efa4'
result=subprocess.run([str(core),str(HERE/'fixtures/bitcoin_supported_input_core.tsv')],capture_output=True,text=True,check=True)
write('bitcoin_supported_input_core_outcomes.tsv',result.stdout)
basic_reference=args.core_basic_reference
assert hashlib.sha256(basic_reference.read_bytes()).hexdigest()=='c38edd026b190b526eb440458b17eb9e5e47a3af4a4b9dd1eabd9db2f2b76828'
result=subprocess.run([str(basic_reference),str(HERE/'fixtures/bitcoin_context_free_raw.tsv')],capture_output=True,text=True,check=True)
write('bitcoin_context_free_core_outcomes.tsv',result.stdout)
assert len(result.stdout.splitlines())==len(basic_rows)
print(json.dumps({'transactionScenes':len(records),'coreScriptCalls':len(core_rows),'contextFreeRawCalls':len(basic_rows)}))
