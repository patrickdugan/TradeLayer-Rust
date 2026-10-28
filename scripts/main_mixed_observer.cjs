
const path=require('node:path'),fs=require('node:fs'),cp=require('node:child_process');
const root=process.env.PROOF_REPO,stage=process.env.TL_MAIN_EVIDENCE_ROOT||root;
const args=[path.join(stage,'scripts/main_mixed_vectors.cjs'),'--check'];if(process.argv.includes('--lock'))args.push('--lock');
cp.execFileSync(process.execPath,args,{env:{...process.env,PROOF_REPO:root},maxBuffer:32*1024*1024,encoding:'utf8'});
const raw=fs.readFileSync(path.join(stage,'fixtures/main_mixed_vectors.jsonl'),'utf8');
const U=['undefined'],A=x=>['array',x],S=x=>['string',x],B=x=>['boolean',x],O=x=>['object',Object.entries(x)];
function get(v,key){return v?.[0]==='object'?(v[1].find(([k])=>k===key)?.[1]||U):U;}
function fallback(v,x){return v[0]==='undefined'?x:v;}
function norm(v){const t=v[0];if(t==='map')return A(v[1].map(([k,x])=>A([norm(k),norm(x)])));if(t==='set')return A(v[1].map(norm));if(t==='array')return A(v[1].map(norm));if(t==='object')return ['object',v[1].map(([k,x])=>[k,norm(x)])];if(t==='regexp'){return v[1]==='^tx'&&v[2]===''?S('^tx'):O({'$regexp':S(v[1]),'$flags':S(v[2])});}if(t==='function'||t==='cycle')throw Error('unadmitted identity transport');return v;}

function small(v,depth=0){if(depth>16)return false;let count=1;if(v[0]==='array'){for(const x of v[1]){const n=nodes(x,depth+1);if(n<0)return false;count+=n;}}else if(v[0]==='object'){for(const[,x]of v[1]){const n=nodes(x,depth+1);if(n<0)return false;count+=n;}}return count<=1000&&JSON.stringify(v).length<=131072;}
function nodes(v,depth=0){if(depth>16)return -1;let count=1;const xs=v[0]==='array'?v[1]:v[0]==='object'?v[1].map(x=>x[1]):[];for(const x of xs){const n=nodes(x,depth+1);if(n<0)return -1;count+=n;if(count>1000)return -1;}return count;}
function N(n){const b=Buffer.alloc(8);b.writeDoubleBE(n);return ['number',b.toString('hex')];}
// This frames ONLY complete test observations. Engine input bounds are not
// changed: every actual command still goes through the actual public runtime.
function emitParts(v,path=[]){if(small(v)){console.log(JSON.stringify(A([S('put'),A(path),v])));return;}if(v[0]==='object'){console.log(JSON.stringify(A([S('put'),A(path),O({})])));for(const[k,x]of v[1])emitParts(x,[...path,S(k)]);return;}if(v[0]==='array'){console.log(JSON.stringify(A([S('put'),A(path),A([])])));for(let i=0;i<v[1].length;i++)emitParts(v[1][i],[...path,N(i)]);return;}throw Error('single observation scalar outside existing diagnostic bound');}

for(const line of raw.trim().split('\n')){const r=JSON.parse(line),c=r.command;
 if(get(c,'profile')[1]==='full-import')continue;
 const st=r.state,src=get(st,'source'),t=get(st,'tally'),p=get(st,'property'),k=get(st,'contract'),ki=get(k,'instance'),o=get(src,'oracle'),oi=get(o,'instance'),cl=get(src,'clearing'),ch=get(st,'channels');
 const state=O({sourceReceiverEvidence:O({samePrototype:get(get(st,'receiver'),'samePrototype'),sameSingleton:get(get(st,'receiver'),'sameSingleton')}),ammInstances:get(st,'ammInstances'),propertyAmms:fallback(get(p,'ammIndex'),A([])),propertyRegistryBound:get(st,'propertyRegistryBound'),marginIdentity:get(st,'marginIdentity'),tables:get(src,'tables'),books:get(src,'instances'),margins:get(st,'marginInstances'),tally:fallback(get(t,'addresses'),A([])),tallyInitialized:get(t,'initialized'),tallyMod:get(t,'modFlag'),contractMod:get(k,'modFlag'),propertyCache:fallback(get(p,'propertyIndex'),A([])),propertyInitialized:B(p[0]!=='undefined'),contractSeries:fallback(get(ki,'contractSeries'),A([])),registryInitialized:B(ki[0]!=='undefined'),seriesInitialized:B(get(ki,'contractSeries')[0]!=='undefined'),activation:get(st,'activation'),channels:get(ch,'registry'),pendingWithdrawals:get(ch,'pending'),contexts:get(st,'clearingContexts'),blockTrades:get(cl,'blockTrades'),deleverageTrades:get(cl,'deleverageTrades'),liquidationRecords:get(cl,'liquidationRecords'),oracleMarks:get(cl,'latestOracleMarkById'),nativeMarks:get(cl,'latestNativeMarkById'),oracleCache:fallback(get(oi,'oracles'),A([])),oracleInitialized:B(oi[0]!=='null'&&oi[0]!=='undefined'),oracleWatermarks:get(o,'lastOracleUpdateBlock'),insurance:get(src,'insurance'),globals:get(st,'globals'),clearlist:get(st,'clearlist'),tallyInstance:fallback(get(t,'instance'),U),feeBuffer:fallback(get(get(src,'tally'),'feeAccrualBuffer'),A([])),feeBufferInitialized:B(get(get(src,'tally'),'feeAccrualBuffer')[0]!=='undefined'),vesting:get(st,'vesting'),vestingStatic:get(st,'vestingStatic'),consensusVector:get(st,'consensusVector'),insuranceInstances:get(st,'insuranceInstances'),unhandledRejections:get(st,'unhandledRejections')});
 const effects=A(r.effects[1].filter(v=>{const kind=get(v,'kind')[1];return ['database','db','uuid','random','rpc-clock','rpc','rpc-input','source-file','unhandled-rejection'].includes(kind)||(kind==='source-function'&&get(v,'target')[1]!=='Types.decodePayload');}));
 emitParts(norm(O({name:S(r.name),domain:S(r.domain),command:c,result:r.result,state,effects})));console.log(JSON.stringify(A([S('done')])));
}
