"""Raw controls drive Core; source projections are assertion-only outputs."""
from pathlib import Path
import json,struct,hashlib,argparse
w=Path(__file__).resolve().parent
if w.name=='scripts':w=w.parent
parser=argparse.ArgumentParser();parser.add_argument('--source',type=Path);parser.add_argument('--check',action='store_true');args=parser.parse_args()
source=args.source or w/'fixtures/native_main_bootstrap_microtasks.jsonl'
assert hashlib.sha256(source.read_bytes()).hexdigest()=='0d6a8046edae6c7ea69e3ffa5c477c7ea15ce1c060ae89ab690e51a85ac51a24'
PROFILE='TL-MAIN-ORDERED-CALLABLE-PROMISES-13-2';UNDEFINED=object()
def untag(v):
 t=v[0]
 if t=='undefined':return UNDEFINED
 if t=='null':return None
 if t in ('string','boolean'):return v[1]
 if t=='number':return struct.unpack('>d',bytes.fromhex(v[1]))[0]
 if t=='array':return [untag(x)for x in v[1]]
 if t=='object':return {k:untag(x)for k,x in v[1]}
 raise ValueError(t)
def tag(v):
 if v is UNDEFINED:return ['undefined']
 if v is None:return ['null']
 if isinstance(v,bool):return ['boolean',v]
 if isinstance(v,str):return ['string',v]
 if isinstance(v,(float,int)):return ['number',struct.pack('>d',float(v)).hex()]
 if isinstance(v,list):return ['array',[tag(x)for x in v]]
 return ['object',[[k,tag(x)]for k,x in v.items()]]
def action(name,**fields):return {'profile':PROFILE,'action':name,**fields}
def role_action(role,kind):return action(('Reject'if kind=='reject'else'Resolve')+('Import'if role=='import'else'Index'))
specs={};index_sent=set();rows=[]
for line in source.read_text(encoding="utf-8").splitlines():
 frame=json.loads(line)
 if 'bindings'not in frame:continue
 case=frame['case'];point=frame['point'];control=untag(frame['control']);commands=[];configuration=None
 if point=='raw-reference-construction':
  spec=control['input'];specs[case]=spec;calls=[]
  for role in ('import','index'):
   mode=spec.get(role,'resolve');fact={'mode':mode}
   if mode!='resolve':
    if spec.get('sharedReason'):fact['rejectionRef']='one-original-error'
    else:fact['rejection']={'name':'RecordedClientInitError','message':'recorded '+role+' initialization rejection','code':'ECLIENT_'+role.upper()}
   calls.append(fact)
  configuration={'profile':PROFILE,'production':{'client':{'chain':control['rawRPCFacts']['chain'],'test':control['rawRPCFacts']['test']},'io':{'height':control['rawRPCFacts']['height']}},'clientCalls':calls}
  if spec.get('sharedReason'):configuration['rejectionObjects']=[{'id':'one-original-error','error':{'name':'RecordedSharedClientError','message':'same recorded rejected object','code':'ECLIENT_SHARED'}}]
 elif point=='immediately-after-full-txutils-import':commands=[action('ImportTxUtils')]
 elif point=='after-full-txindex-import-before-construction':pass
 elif point=='main-constructor-returned-static-init-pending':commands=[action('ConstructMain')]
 elif point=='handler-attached-before-first-source-jobs':commands=[action('AttachIndexHandler',handler='constructor-return-before-await')]
 elif point=='main-height-call-settled-before-turn-notification':commands=[action('CallMainHeight')]
 elif point=='main-invocation-jobs-and-notifications-drained':commands=[action('EndTurn')]
 elif point=='handler-before-client-settlement':commands=[action('AttachIndexHandler',handler='before-client-rejection')]
 elif point=='handler-after-client-rejection-before-microtasks':commands=[action('RejectIndex'),action('AttachIndexHandler',handler='after-reject-before-any-drain')];index_sent.add(case)
 elif point=='index-settled-after-main':commands=([action('RejectIndex')]if case not in index_sent else[])+[action('EndTurn')]
 elif point.startswith('client-')and point.endswith('-without-drain'):
  assert control['kind']=='SettleActualClientWithoutDrain';commands=[role_action(control['label'],control['action'])]
 elif point.startswith('microtask-checkpoint-'):commands=[action('HostAwaitCheckpoint')]
 elif point=='same-turn-client-settlements-notified':commands=[action('EndTurn')]
 elif point=='explicit-late-caller-handler':commands=[action('AttachIndexHandler',handler='late-after-notification'),action('EndTurn')]
 elif point=='second-late-handler-does-not-renotify':commands=[action('AttachIndexHandler',handler='second-late-catch'),action('EndTurn')]
 elif point.startswith('repeat-turn-'):commands=[action('EndTurn')]
 else:raise ValueError(point)
 binding=frame['bindings'];trace=frame['trace'];groups={}
 for event in trace:
  reason=event.get('thrown')
  if isinstance(reason,dict)and reason.get('object')is not None:groups.setdefault(reason['object'],len(groups)+1)
 def error(value):
  return {'name':value['name'],'message':value['message'],**({'code':value['code']}if'code'in value else{}),'group':groups[value['object']]}
 expected={
  'importClientOwn':binding['txUtilsClientOwn'],'importClientBound':binding['txUtilsClient']==1,
  'classClientOwn':binding['indexClassClientOwn'],'classClientBound':binding['indexClassClient']==1,
  'mainConstructed':binding['sameMainSingleton'],'indexSingleton':binding['sameIndexInstance'],
  'notifications':[{'code':e['reason'].get('code',UNDEFINED),'name':e['reason']['name'],'message':e['reason']['message'],'isMainIndexPromise':e['isMainIndexPromise']}for e in frame['unhandled']],
  'rejectionHandled':len(frame['rejectionHandled']),
  'requests':[{'ordinal':e['ordinal'],'args':untag(e['args'])}for e in trace if e['kind']=='client-input'],
  'callers':[],
 }
 visible=[]
 for event in trace:
  kind=event['kind']
  if kind=='client-input':visible.append({'kind':'client-request','ordinal':event['ordinal'],'args':untag(event['args'])})
  elif kind=='caller-result':
   result={'status':'Fulfilled','value':untag(event['returned'])}if'returned'in event else{'status':'Rejected',**{k:v for k,v in error(event['thrown']).items()if k!='group'}}
   expected['callers'].append(result);visible.append({'kind':'caller-result','result':result})
  elif kind in ('explicit-catch-result','explicit-late-catch-result'):
   visible.append({'kind':'caller-catch','handler':event.get('label','late-after-notification'),'error':error(event['reason'])})
  elif kind=='unhandled-rejection':visible.append({'kind':kind,'isMainIndexPromise':event['isMainIndexPromise'],'error':error(event['reason'])})
  elif kind=='rejection-handled':visible.append({'kind':kind,'isMainIndexPromise':event['isMainIndexPromise']})
 expected['visibleTrace']=visible
 rows.append({'case':case,'point':point,'rawConfiguration':configuration,'rawControls':commands,'expectedProjection':expected})
assert len(specs)==12 and len(rows)==142
out=w/'fixtures';out.mkdir(exist_ok=True)
data='\n'.join(json.dumps(tag(row),separators=(',',':'))for row in rows)+'\n'
meta=json.dumps({'originalSourceSHA256':hashlib.sha256(source.read_bytes()).hexdigest(),'cases':list(specs),'projections':142,'sourceObservations':143,'inputOrigin':'explicit original raw controls/client/RPC facts only; no projected result/state/table drives execution','scope':'initializer fields, original Main caller, notification/catch ordering and shared error identity; full original source tables retained in source corpus, not imported into Core'},indent=2)+'\n'
for name,value in [('ordered_bootstrap_promises_controls.jsonl',data),('ordered_bootstrap_promises_source_projection.json',meta)]:
 if args.check:assert (out/name).read_text(encoding="utf-8")==value,name
 else:(out/name).write_text(value,encoding='utf-8',newline='\n')
print('12 scenes /142 assertion-only source projections',hashlib.sha256(data.encode()).hexdigest())
