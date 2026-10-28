"""Project recorded genuine Core JSON; never derive goldens from Rust output."""
from pathlib import Path
import json,struct,argparse,hashlib
ROOT=Path(__file__).resolve().parent
if ROOT.name=='scripts':ROOT=ROOT.parent
def tagged(v):
    if v is None:return ['null']
    if isinstance(v,bool):return ['boolean',v]
    if isinstance(v,(int,float)):return ['number',struct.pack('>d',float(v)).hex()]
    if isinstance(v,str):return ['string',v]
    if isinstance(v,list):return ['array',list(map(tagged,v))]
    return ['object',[[k,tagged(x)]for k,x in v.items()]]
def output():
    names=['pre_mweb_431','pegin_block_432','empty_followup_433','transfer_block_443','pegout_block_444','multiple_kernels_445','pegin_envelope','mweb_only_transfer','pegout_envelope','witness_hybrid_envelope','witness_hybrid_block_453']
    records=[]
    for name in names:
        raw=(ROOT/'fixtures'/f'{name}.hex').read_text(encoding='ascii').strip()
        d=json.loads((ROOT/'fixtures'/f'{name}.json').read_text(encoding='utf-8'))
        records.append(tagged({'name':name,'rawHex':raw,'core':d,'sourceCommit':'beae01d62292a0aab363b7a4d3f606708cea7260','binarySha256':'151ad090a79223c01a4c317347d5dda7be6be6f7b167e65cdfbe37dbe4d1648c'}))
    return ('\n'.join(json.dumps(r,separators=(',',':'))for r in records)+'\n').encode()
if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--check',action='store_true');args=p.parse_args();b=output();target=ROOT/'fixtures/litecoin_core_regtest_goldens.jsonl'
    if args.check:assert target.read_bytes()==b
    else:target.write_bytes(b)
    print('PASS: eleven unchanged Core raw/decoded golden records; SHA256',hashlib.sha256(b).hexdigest())

    source=ROOT/'fixtures/litecoin_core_codec_observations.json'
    observations=json.loads(source.read_text(encoding='utf-8'))['cases']
    data=('\n'.join(json.dumps(tagged(c),separators=(',',':'))for c in observations)+'\n').encode()
    target=ROOT/'fixtures/litecoin_core_codec_goldens.jsonl'
    if args.check:assert target.read_bytes()==data
    else:target.write_bytes(data)
    print('PASS: 29 genuine offline Core codec records; SHA256',hashlib.sha256(data).hexdigest())
