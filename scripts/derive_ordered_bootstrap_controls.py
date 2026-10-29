"""Project only init/caller/notification observations from unchanged source.
Inputs come exclusively from the source corpus's explicit raw input control.
No expected table/financial/validity state is imported or supplied to Runtime.
"""
from pathlib import Path
import json,struct,hashlib,sys
w=Path(__file__).resolve().parents[1]
args=[arg for arg in sys.argv[1:]if arg!='--check']
checking='--check'in sys.argv[1:]
source=Path(args[0])if args else w/'fixtures/native_main_bootstrap_failures.jsonl'
assert hashlib.sha256(source.read_bytes()).hexdigest()=='088be935ca15e2c1f7db14069d3bd3ebb49ace5b7d1c682357cddbd4008e5200','independent unchanged-source corpus pin'
PROFILE='TL-MAIN-ORDERED-CALLABLE-BOOTSTRAP-13-1'
UNDEFINED=object()
def untag(v):
 t=v[0]
 if t=='undefined':return UNDEFINED
 if t=='null':return None
 if t in ('string','boolean'):return v[1]
 if t=='number':return struct.unpack('>d',bytes.fromhex(v[1]))[0]
 if t=='array':return [untag(x) for x in v[1]]
 if t=='object':return {k:untag(x) for k,x in v[1]}
 raise ValueError(t)
def tag(v):
 if v is None:return ['null']
 if isinstance(v,bool):return ['boolean',v]
 if isinstance(v,str):return ['string',v]
 if isinstance(v,(int,float)):return ['number',struct.pack('>d',float(v)).hex()]
 if isinstance(v,list):return ['array',[tag(x)for x in v]]
 return ['object',[[k,tag(x)]for k,x in v.items()]]
def action(name,**rest):return {'profile':PROFILE,'action':name,**rest}
specs={};rows=[];full_boundary=None
for raw in source.read_text(encoding="utf-8").splitlines():
 f=json.loads(raw)
 if 'bindings' not in f:
  assert f['case']=='full-main-module-unavailable-reorg-boundary';full_boundary=f;continue
 case=f['case'];point=f['point'];control=untag(f['control'])
 commands=[];config=None
 if point=='raw-reference-construction':
  spec=control['input'];specs[case]=spec
  calls=[]
  for role in ('import','index'):
   mode=spec.get(role,'resolve');fact={'mode':mode}
   if mode!='resolve':fact['rejection']={'name':'RecordedClientInitError','message':'recorded '+role+' initialization rejection','code':'ECLIENT_'+role.upper()}
   calls.append(fact)
  config={'profile':PROFILE,'production':{'client':{'chain':control['rawRPCFacts']['chain'],'test':control['rawRPCFacts']['test']},'io':{'height':control['rawRPCFacts']['height'],**({'failures':[{'method':'Client.getBlockCount','ordinal':1,'name':'RecordedRpcError','message':'recorded height RPC rejection','code':'ERPC_HEIGHT'}]}if spec.get('rpcFailure')else{})}},'clientCalls':calls}
 elif point=='immediately-after-full-txutils-import':commands=[action('ImportTxUtils')]
 elif point=='after-full-txindex-import-before-construction':pass
 elif point=='import-settled-before-main':commands=[action(('Resolve'if specs[case]['settleBeforeMain']=='resolve'else'Reject')+'Import'),action('EndTurn')]
 elif point=='import-turn-before-main':commands=[action('EndTurn')]
 elif point=='main-constructor-returned-static-init-pending':commands=[action('ConstructMain')]
 elif point=='main-height-call-settled-before-turn-notification':commands=[action('CallMainHeight')]
 elif point=='main-invocation-jobs-and-notifications-drained':commands=[action('EndTurn')]
 elif point=='import-settled-after-main':commands=[action(('Resolve'if specs[case]['settleAfterMain']=='resolve'else'Reject')+'Import'),action('EndTurn')]
 elif point=='index-settled-after-main':commands=[action('RejectIndex'),action('EndTurn')]
 elif point.startswith('reject-'):
  role=point.split('-')[1];commands=[action('Reject'+role.title()),action('EndTurn')]
 elif point=='literal-txutils-use-after-rejected-import':commands=[action('CallTxUtilsRaw',txid=control['args'][0])]
 elif point=='explicit-late-caller-handler':commands=[action('AttachIndexHandler'),action('EndTurn')]
 else:raise AssertionError(point)
 bindings=f['bindings']
 expected={
  'importClientOwn':bindings['txUtilsClientOwn'],'importClientBound':bindings['txUtilsClient'] is not None,
  'classClientOwn':bindings['indexClassClientOwn'],'classClientBound':bindings['indexClassClient'] is not None,
  'mainConstructed':bindings['sameMainSingleton'],'indexSingleton':bindings['sameIndexInstance'],
  'notifications':[{'code':e['reason'].get('code'),'name':e['reason'].get('name'),'message':e['reason'].get('message'),'isMainIndexPromise':e['isMainIndexPromise']}for e in f['unhandled']],
  'rejectionHandled':len(f['rejectionHandled']),
  'requests':[{'ordinal':e['ordinal'],'args':untag(e['args'])}for e in f['trace']if e['kind']=='client-input'],
  'callers':[],
 }
 for e in f['trace']:
  if e['kind']!='caller-result':continue
  if 'returned'in e:expected['callers'].append({'status':'Fulfilled','value':untag(e['returned'])})
  else:expected['callers'].append({'status':'Rejected','name':e['thrown']['name'],'message':e['thrown']['message'],**({'code':e['thrown']['code']}if 'code'in e['thrown']else{})})
 if point=='literal-txutils-use-after-rejected-import':
  thrown=control['result']['thrown'];expected['callers'].append({'status':'Rejected','name':thrown['name'],'message':thrown['message'],**({'code':thrown['code']}if 'code'in thrown and thrown['code'] is not UNDEFINED else{})})
 rows.append({'case':case,'point':point,'rawConfiguration':config,'rawControls':commands,'expectedProjection':expected})
assert len(specs)==14 and len(rows)==98
out=w/'fixtures'
def emit(path,content):
 if checking:assert path.read_text(encoding='utf-8')==content,str(path)
 else:path.write_text(content,encoding='utf-8',newline='\n')
data='\n'.join(json.dumps(tag(row),separators=(',',':'))for row in rows)+'\n'
emit(out/'ordered_bootstrap_controls.jsonl',data)
assert full_boundary is not None
emit(out/'ordered_bootstrap_full_main_boundary.json',json.dumps(full_boundary,indent=2)+'\n')
emit(out/'ordered_bootstrap_source_projection.json',json.dumps({'sourceSHA256':hashlib.sha256(source.read_bytes()).hexdigest(),'cases':list(specs),'observations':len(rows),'projectionScope':'initializer requests, actual client fields, completed Main caller results, actual unhandled/late-handled ordering; full financial tables/prototypes/logs/global Main IIFEs not claimed','inputOrigin':'only raw input/spec/RPC control fields, never expectedProjection'},indent=2)+'\n')
print('14 cases /98 projected source observations',hashlib.sha256(data.encode()).hexdigest())
