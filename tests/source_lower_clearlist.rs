//! Literal ClearList/Logic/Types over the same Core database and true Maps.
use tradelayer_core::source_contract_replay::{
    SourceContractCommand as C, SourceContractCommandOutcome as O, SourceContractReplay,
};
use tradelayer_core::source_contract_runtime::{
    SourceContractRuntime, SourceContractRuntimePolicy as P,
};
use tradelayer_core::validity_source::{
    DependencyCall, DependencyReturn, SourceOutcome, SourceRuleError, SourceValue as V,
};
fn s(value: &str) -> V {
    V::String(value.into())
}
fn obj(fields: &[(&str, V)]) -> V {
    V::Object(
        fields
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
    )
}
const REFERENCE: &str = r###"'use strict';
// Audit only: unchanged pinned business modules; real shared in-memory DB calls.
const fs=require('node:fs'),path=require('node:path'),Module=require('node:module'),vm=require('node:vm'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const stage=__dirname,root=process.env.PROOF_REPO||path.resolve(stage,'../../outputs/TradeLayer-Rust'),flags=process.argv.slice(2),lock=flags.includes('--lock');
const numeric=require(path.join(root,'scripts/legacy_numeric_oracle.cjs'));let BN=numeric.loadBigNumber();
if(lock){const packed=fs.readFileSync(path.join(root,'scripts/scaling_vectors.cjs'),'utf8').match(/const BIGNUMBER_GZIP="([^"]+)"/)[1],code=require('node:zlib').gunzipSync(Buffer.from(packed,'base64'));assert.equal(crypto.createHash('sha256').update(code).digest('hex'),'20db2d8d32fd388a927588beb9c1cdbb2c91ddf0273a180161334b28cb00902e');const loaded={exports:{}};vm.runInNewContext(code.toString('utf8'),{module:loaded,exports:loaded.exports},{timeout:1000});BN=loaded.exports;numeric.loadBigNumber=()=>BN;}
const file=path.join(root,'scripts/source_contract_execution_vectors.cjs');let code=fs.readFileSync(file,'utf8');code=code.slice(0,code.indexOf('const scenes=['));
function replace(before,after){assert.equal(code.split(before).length,2,'driver anchor '+before);code=code.replace(before,after);}
replace('return {types,logic,Orderbook,effects','return {context,storage,tables,sources,copy,types,logic,Orderbook,effects');replace('level,(...values)=>logs.push({level,values:values.map(value=>typeof value===\'string\'?value:String(value))})','level,(...values)=>{logs.push({level,values:values.map(value=>typeof value===\'string\'?value:String(value))});}');
replace("if(Object.hasOwn(adapters,normalized))return adapters[normalized];","if(normalized==='bitcore-lib-ltc'){if(scene.bitcoinFallback)throw Error('explicit Litecoin module-load failure');return external('bitcore-lib-ltc');} if(normalized==='bitcore-lib')return external('bitcore-lib'); if(Object.hasOwn(adapters,normalized))return adapters[normalized];");
code+='\nmodule.exports={environment};';
const driver=new Module(file,module);driver.filename=file;driver.paths=Module._nodeModulePaths(path.dirname(file));driver._compile(code,file);
const A='1BoatSLRHtKNngkdXEeobR76b53LETtpyT',B='1BitcoinEaterAddressDontSendf59kuE',C='LKKG9A9a8n7CeHK8Nk7RdNZiCuHZW2U789',records=[];let e,lastParams,libraryLocked=false;
function tag(v){if(v===undefined)return['undefined'];if(v===null)return['null'];if(typeof v==='string'||typeof v==='boolean')return[typeof v,v];if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return['number',Number.isNaN(v)?'NaN':v===Infinity?'Infinity':v===-Infinity?'-Infinity':b.toString('hex')];}if(BN.isBigNumber(v))return['bignumber',v.toString()];if(Object.prototype.toString.call(v)==='[object Map]')return tag([...v]);if(Array.isArray(v))return['array',v.map(tag)];return['object',Object.entries(v).map(([k,v])=>[k,tag(v)])];}
async function flush(){for(let i=0;i<5;i++)await new Promise(resolve=>setImmediate(resolve));}
function state(){const T=e.load('tally'),P=e.load('property'),Q=e.load('clearlist');return{tables:[...e.tables].map(([name,rows])=>({name,rows:structuredClone(rows)})),tally:T.addresses??new Map(),tallyInitialized:!!T.addresses,tallyMod:T.modFlag,propertyCache:P.instance?.propertyIndex??new Map(),propertyInitialized:!!P.instance,clearlist:{clearlistsMap:Q.clearlists,banlist:Q.banlist,xpubCache:Q.xpubCache,merkleXpubMap:Q.merkleXpubMap,libraryLocked,library:flags.includes('--fallback')?'bitcoin-fallback':'litecoin'},lastParams,unsupported:e.state().unsupported};}
function emit(name,command,result){const snapshot=e.copy(state());assert.deepEqual(snapshot.unsupported,[],name+' unowned boundary');records.push({name,command:e.copy(command),result:e.copy(result),state:snapshot,effects:e.effects.filter(v=>['source-function','database','db','uuid','random','rpc-clock','activation'].includes(v.kind)).map(v=>e.copy(v))});}
function observe(target,method,label){const original=target[method];assert.equal(typeof original,'function',label);target[method]=function(...args){if(label==='ClearList.deriveAddressesFromXpub')libraryLocked=true;e.effects.push({kind:'source-function',target:label,args:e.copy(args)});return Reflect.apply(original,this,args);};}
async function reset(name,options={}){options={...options,...(flags.includes('--fallback')?{bitcoinFallback:true}:{})};e=driver.exports.environment(options.entry==='tally-first'?{preload:'tally',...options}:options.entry==='volume-first'?{preload:'volumeIndex',...options}:options);lastParams=undefined;libraryLocked=false;for(const method of ['createClearList','updateAdmin','issueOrRevokeAttestation','updateBannedCountries'])observe(e.logic,method,'Logic.'+method);for(const method of ['createClearlist','createclearlist','loadClearlists','getClearlistById','getList','verifyAdmin','updateAdmin','getNextId','addAttestation','addAttestationWithXpub','deriveAddressesFromXpub','isAddressInClearlistOrDerived','storeMerkleRoot','registerXpubForRoot','verifyMerkleProof','buildMerkleTree','generateMerkleProof','revokeAttestation','getCountryCodeByAddress','setBanlist','setBanList','getBanlist','getAttestations','getAttestationHistory','isAddressInClearlist'])observe(e.load('clearlist'),method,'ClearList.'+method);await flush();e.effects.length=0;emit(name,{reset:options},{outcome:{returned:undefined}});}
function fail(f){if(f.op!=='getDatabase'){e.setFailure(f);return;}const original=e.storage.getDatabase;let skip=f.skip??0;e.storage.getDatabase=function(...args){if(args[0]===f.collection){if(skip>0)skip--;else{e.storage.getDatabase=original;const error=Error(f.message);e.effects.push({kind:'database',name:args[0],method:'getDatabase',args,thrown:{name:error.name,message:error.message}});return Promise.reject(error);}}return original.apply(this,args);};}
async function call(name,target,args=[],failure,unsupported=false){e.effects.length=0;if(failure)fail(failure);const [module,method]=target.split('.'),receiver=module==='Logic'?e.logic:e.load('clearlist');let transport='Immediate',outcome;try{let value=Reflect.apply(receiver[method],receiver,args);if(value&&typeof value.then==='function'){transport='Pending';try{outcome={returned:await value};}catch(error){transport='PendingThrow';outcome={thrown:{name:error.name,message:error.message}};}}else outcome={returned:value};}catch(error){transport='Throw';outcome={thrown:{name:error.name,message:error.message}};}await flush();emit(name,{target,args,failure,...(unsupported?{unsupported:true}:{})},{transport,outcome});}
async function raw(name,type,payload,sender=A,failure,block=10){e.effects.length=0;lastParams=undefined;if(failure)fail(failure);const args=[name,type,'tl',payload,sender,null,0,0,block];let outcome;try{lastParams=await e.types.decodePayload(...args);let value;if(lastParams.valid===true)value=await e.logic.typeSwitch(type,lastParams);outcome={params:lastParams,returned:value};}catch(error){outcome={params:lastParams,thrown:{name:error.name,message:error.message}};}await flush();emit(name,{raw:args,failure},{outcome});}
async function seed(name,collection,rows){e.effects.length=0;e.tables.set(collection,e.copy(rows));emit(name,{import:{collection,rows}},{outcome:{returned:undefined}});}
(async()=>{
await reset('first10-reset');
await raw('raw7-create',7,C+',LIST,url,description');
await raw('raw9-list1-add-address',9,'0,1,'+A+',ca');
await raw('raw9-list0-overwrites-address',9,'0,0,'+A+',gb');
await raw('raw8-whitelist-wrong-validation-collection',8,C+',1,0,0,1,0');
await call('direct-updateAdmin-string-shadow','Logic.updateAdmin',[true,false,false,1,C,false,10]);
await raw('raw9-reload-original-numeric-admin',9,'0,1,'+C+',ca');
await raw('raw9-admin-banlist',9,'0,0,BANLIST,CA|US','admin');
await call('direct-root-attestation','Logic.issueOrRevokeAttestation',[A,1,C,'xpub:invalid',false,11,'root']);
await call('direct-revoke-retains-root','Logic.issueOrRevokeAttestation',[A,1,C,'reason',true,12]);

for(const entry of ['main-subgraph','types-first','tally-first','volume-first']){
 await reset('entry-'+entry,{entry});
 await raw(entry+'-create',7,C+',NAME,u,d');
 await raw(entry+'-add',9,'0,1,'+B+',one');
 await raw(entry+'-revoke',9,'1,1,'+B+',reason');
 await raw(entry+'-revoked-again-invalid',9,'1,1,'+B+',reason');
 await raw(entry+'-missing-list',9,'0,99,'+B+',one');
 await raw(entry+'-outsider',9,'0,1,'+B+',one','other');
 await raw(entry+'-self-cert-not-self',9,'0,0,'+B+',ca');
 await raw(entry+'-self-cert-country-shape',9,'0,0,'+A+',CAN');
 await raw(entry+'-banlist-not-admin',9,'0,0,BANLIST,CA|US');
 await raw(entry+'-banlist-empty',9,'0,0,BANLIST,','admin');
 await raw(entry+'-missing-metadata-decoder-throw',9,'0,1,'+B);
 await raw(entry+'-uppercase-metadata-drops-xpub-prefix',9,'0,1,'+B+',xpub:abc');
 await call(entry+'-country-trim','ClearList.getCountryCodeByAddress',[A]);
 await call(entry+'-banlist-array-dot-data-bug','ClearList.getBanlist');
}
await reset('inactive',{inactive:true});await raw('inactive7',7,C+',NAME,u,d');await raw('inactive9',9,'0,0,'+A+',ca');await raw('inactive8',8,C+',1,0,0,1,0');
await reset('admin-stale-cache');
await call('direct-create','ClearList.createClearlist',[A,'L','ignored-url','description',B]);
await call('create-does-not-cache','ClearList.verifyAdmin',[1,A]);
await call('load-created','ClearList.getClearlistById',[1]);
await seed('raw8-validator-row','attestations',[{_id:1,data:{adminAddress:A,backupAddress:B}}]);
await raw('raw8-actual-whitelist-update',8,C+',1,0,0,1,0');
await call('cache-admin-now-C','ClearList.verifyAdmin',[1,C]);
await call('numeric-reload-old','ClearList.getClearlistById',[1]);
await call('string-shadow-new','ClearList.getClearlistById',['1']);
await raw('raw8-backup-actual-update',8,C+',1,0,0,1,1',B);
await call('direct-no-admin','Logic.createClearList',[0,'L']);
await call('direct-no-admin-undefined','Logic.createClearList',[]);
await call('alias-config-create','ClearList.createclearlist',[{adminAddress:'',admin:A,name:'alias',url:'ignored',description:{hello:1},backupAddress:false,backup:B,id:'12tail'}]);
await call('alias-positional-create','ClearList.createclearlist',[A,'alias2','url','d',B]);
await call('numeric-prefix-next','ClearList.getNextId');
await seed('clear-persisted-rows','clearlists',[]);await call('stale-load-keeps-cache','ClearList.loadClearlists');await call('stale-next-id','ClearList.getNextId');
await call('set-backup-truthy-array','ClearList.updateAdmin',[1,'new',[]]);
await call('missing-admin-write','ClearList.updateAdmin',[999,'new',false]);
for(const [name,target,args,collection,op,skip]of[
 ['create-load-factory','Logic.createClearList',[A,'L',null,'d',B,10],'clearlists','getDatabase',0],['create-load-find','Logic.createClearList',[A,'L',null,'d',B,10],'clearlists','find',0],['create-persist-factory','Logic.createClearList',[A,'L',null,'d',B,10],'clearlists','getDatabase',1],['create-update','Logic.createClearList',[A,'L',null,'d',B,10],'clearlists','update',0],
 ['admin-factory','ClearList.updateAdmin',[1,C,false],'clearlists','getDatabase',0],['admin-update','ClearList.updateAdmin',[1,C,false],'clearlists','update',0],
 ['load-factory','ClearList.getClearlistById',[1],'clearlists','getDatabase',0],['load-find','ClearList.getClearlistById',[1],'clearlists','find',0],
 ['attest-factory','Logic.issueOrRevokeAttestation',[A,1,C,'ca',false,10,'root'],'attestations','getDatabase',0],['attest-update','Logic.issueOrRevokeAttestation',[A,1,C,'ca',false,10,'root'],'attestations','update',0],['root-after-attestation','Logic.issueOrRevokeAttestation',[A,1,C,'ca',false,10,'root'],'attestations','update',1],
 ['revoke-read','ClearList.revokeAttestation',[1,C,'reason',11],'attestations','find',0],['revoke-write','ClearList.revokeAttestation',[1,C,'reason',11],'attestations','update',0],
 ['country-catches','ClearList.getCountryCodeByAddress',[C],'attestations','find',0],['banlist-rethrow','ClearList.setBanlist',[['CA'],10],'clearlists','update',0]
]){await reset('failure-'+name);if(name.startsWith('admin-')||name.startsWith('load-')){await call(name+'-create','ClearList.createClearlist',[A,'L',null,'d',B]);await call(name+'-hydrate','ClearList.loadClearlists');}if(name.startsWith('revoke-'))await call(name+'-attest','ClearList.addAttestation',[1,C,'meta',10]);await call(name+'-failed',target,args,{collection,op,skip,message:name});await call(name+'-retry',target,args);}
await reset('revocation-legacy-multi');await seed('legacy-attestation-rows','attestations',[{_id:'one',data:{address:C,listId:1,status:'active',timestamp:1}},{_id:'two',data:{address:C,clearlistId:1,status:'active',timestamp:1}},{_id:'attestation:'+B,data:{address:B,status:'active'}}]);
await call('revoke-partial-first-write','ClearList.revokeAttestation',[1,C,'why',20],{collection:'attestations',op:'update',skip:1,message:'second-revoke'});await call('revoke-only-remaining','ClearList.revokeAttestation',[1,C,'retry',21]);await call('revoke-legacy-id','ClearList.revokeAttestation',[99,B,'legacy',22]);await call('revoke-missing','ClearList.revokeAttestation',[1,'absent','why',23]);await call('all-list-attestations','ClearList.getAttestations',[1]);await call('ordered-history','ClearList.getAttestationHistory',[C,1]);
await reset('country-banlist');await seed('country-shapes','attestations',[{_id:'country1',data:{address:C,listId:0,status:'revoked',data:'us'}},{_id:'country2',data:{address:C,clearlistId:'0',data:{country:' gb '},timestamp:0,blockHeight:12}},{_id:'country3',data:{address:B,listId:null,data:' CA '}}]);
await call('legacy-country-object','ClearList.getCountryCodeByAddress',[C]);await call('null-list-is-Number-zero','ClearList.getCountryCodeByAddress',[B]);await call('set-alias-banlist','ClearList.setBanList',[['CA','GB'],30]);await call('get-banlist-literal-array-bug','ClearList.getBanlist');await call('direct-invalid-bancodes-return-console-undefined','Logic.updateBannedCountries',[['abc'],31]);await call('direct-normalize-banlist','Logic.issueOrRevokeAttestation',['admin',0,' banlist ','us;ca |gb',false,32]);await call('truthy-nonboolean-revoke-no-op','Logic.issueOrRevokeAttestation',[A,1,C,'meta','truthy',33]);
await seed('country-revoked-malformed-id-before-valid','attestations',[{_id:'bad-revoked',data:{address:C,status:'revoked',listId:{toString:1},data:'US'}},{_id:'later-active',data:{address:C,status:'active',listId:0,data:'CA'}}]);
await call('country-number-before-revoked-status-caught','ClearList.getCountryCodeByAddress',[C]);
assert.equal(records.at(-1).result.outcome.returned,null,'Number conversion must execute before revoked status guard');
await seed('country-after-malformed-row-removal','attestations',[{_id:'later-active',data:{address:C,status:'active',listId:0,data:'CA'}}]);
await call('country-valid-after-malformed-row-removal','ClearList.getCountryCodeByAddress',[C]);
assert.equal(records.at(-1).result.outcome.returned.countryCode,'CA');
const XPUB='xpub661MyMwAqRbcFtXgS5sYJABqqG9YLmC4Q1Rdap9gSE8NqtwybGhePY2gZ29ESFjqJoCu1Rupje8YtGqsefD265TMg7usUDFdp6W1EGMcet8';
await reset('xpub-roots');
await call('derive-actual-library','ClearList.deriveAddressesFromXpub',[XPUB,'ignored-network']);
const addresses=e.load('clearlist').xpubCache.get(XPUB);
await call('derive-cache-reuse-network-ignored','ClearList.deriveAddressesFromXpub',[XPUB,'testnet']);await call('derive-invalid-empty-no-cache','ClearList.deriveAddressesFromXpub',['bad']);await call('direct-xpub-prefix','ClearList.addAttestation',[1,'issuer','xpub:'+XPUB,40]);await call('inline-derived-first','ClearList.isAddressInClearlistOrDerived',[1,addresses[0]]);await call('inline-derived-last','ClearList.isAddressInClearlistOrDerived',[1,addresses[39]]);await call('inline-derived-absent','ClearList.isAddressInClearlistOrDerived',[2,addresses[39]]);
await call('explicit-xpub-row','ClearList.addAttestationWithXpub',[2,'issuer','meta',XPUB,41]);await call('explicit-derived','ClearList.isAddressInClearlistOrDerived',[2,addresses[2]]);
await call('build-single-tree','ClearList.buildMerkleTree',[[XPUB]]);const tree=e.load('clearlist').buildMerkleTree([XPUB]),proof=e.load('clearlist').generateMerkleProof([XPUB],XPUB);await call('generate-single-proof','ClearList.generateMerkleProof',[[XPUB],XPUB]);await call('verify-single-proof','ClearList.verifyMerkleProof',[XPUB,proof,tree.root]);await call('register-absent-root','ClearList.registerXpubForRoot',[tree.root,XPUB,proof]);await call('store-root','ClearList.storeMerkleRoot',[3,tree.root,'root-admin',42]);await call('register-verified-xpub','ClearList.registerXpubForRoot',[tree.root,XPUB,proof]);await call('register-duplicate-xpub','ClearList.registerXpubForRoot',[tree.root,XPUB,proof]);await call('root-derived-address','ClearList.isAddressInClearlistOrDerived',[3,addresses[4]]);await call('register-invalid-proof','ClearList.registerXpubForRoot',[tree.root,'other',proof]);await call('build-empty-tree','ClearList.buildMerkleTree',[[]]);await call('generate-absent-proof','ClearList.generateMerkleProof',[[XPUB],'other']);await call('three-leaf-tree','ClearList.buildMerkleTree',[['one','two','three']]);await call('three-leaf-proof','ClearList.generateMerkleProof',[['one','two','three'],'three']);


await reset('coercion-source-guards');
for(const [name,target,args]of[
 ['direct-target-own-toString','Logic.issueOrRevokeAttestation',[A,1,{toString:1},'meta',false,10]],
 ['direct-target-array-nested-toString','Logic.issueOrRevokeAttestation',[A,1,[{toString:null}],'meta',false,10]],
 ['country-thrown-number-caught','ClearList.getCountryCodeByAddress',[C]],
 ['create-bad-numeric-cache-data','ClearList.createClearlist',[A,'L',null,'d',B,1]]
])await call(name,target,args);
await seed('primitive-cache-data','clearlists',[{_id:1,data:2}]);await call('load-primitive-cache-data','ClearList.loadClearlists');await call('primitive-cache-admin-typeerror','ClearList.updateAdmin',[1,C,false]);
await call('banlist-JS-whitespace-only','Logic.issueOrRevokeAttestation',['admin',0,'BANLIST',String.fromCharCode(0x85)+'US'+String.fromCharCode(0x85),false,10]);

await reset('explicit-identity-domain');
await call('direct-target-plain-object-local-refusal','Logic.issueOrRevokeAttestation',[A,1,{},'meta',false,10],undefined,true);
for(const record of records)console.log(JSON.stringify(tag(record)));
})().catch(error=>{console.error(error.stack||error);process.exitCode=1});
"###;
fn text(value: V) -> String {
    let V::String(value) = value else {
        panic!("string {value:?}")
    };
    value
}
fn array(value: V) -> Vec<V> {
    let V::Array(value) = value else {
        panic!("array {value:?}")
    };
    value
}
fn cases(lock: bool, fallback: bool) -> Vec<V> {
    let output = std::process::Command::new("node")
        .env("PROOF_REPO", env!("CARGO_MANIFEST_DIR"))
        .args([
            "-e",
            REFERENCE,
            "clearlist-reference",
            if lock { "--lock" } else { "--baseline" },
            if fallback { "--fallback" } else { "--litecoin" },
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    std::str::from_utf8(&output.stdout)
        .unwrap()
        .lines()
        .map(|s| tradelayer_core::property_catalog::parse_diagnostic_value(s).unwrap())
        .collect()
}
fn returned(value: DependencyReturn) -> V {
    let (transport, result) = match value {
        DependencyReturn::Immediate(v) => ("Immediate", obj(&[("returned", v)])),
        DependencyReturn::Pending(v) => ("Pending", obj(&[("returned", v)])),
        DependencyReturn::Throw(e) => (
            "Throw",
            obj(&[(
                "thrown",
                obj(&[("name", s(&e.name)), ("message", s(&e.message))]),
            )]),
        ),
        DependencyReturn::PendingThrow(e) => (
            "PendingThrow",
            obj(&[(
                "thrown",
                obj(&[("name", s(&e.name)), ("message", s(&e.message))]),
            )]),
        ),
    };
    obj(&[("transport", s(transport)), ("outcome", result)])
}
fn failure(c: &V) -> Option<tradelayer_core::source_contract_runtime::StorageFailure> {
    let f = c.get("failure");
    if f == V::Undefined {
        return None;
    }
    let op = text(f.get("op"));
    Some(tradelayer_core::source_contract_runtime::StorageFailure {
        collection: text(f.get("collection")),
        operation: if op == "getDatabase" {
            op
        } else {
            format!("{op}Async")
        },
        skip: if let V::Number(v) = f.get("skip") {
            f64::from_bits(v) as usize
        } else {
            0
        },
        message: text(f.get("message")),
    })
}
fn command(c: &V) -> C {
    if let V::Object(_) = c.get("import") {
        let i = c.get("import");
        return C::Import {
            collection: text(i.get("collection")),
            rows: array(i.get("rows")),
        };
    }
    if let V::Array(args) = c.get("raw") {
        C::ExecuteContract(args)
    } else {
        C::Invoke(DependencyCall {
            target: text(c.get("target")),
            arguments: array(c.get("args")),
        })
    }
}
fn actual(runtime: &mut SourceContractRuntime, case: &V) -> V {
    let c = case.get("command");
    if let V::Object(_) = c.get("import") {
        let i = c.get("import");
        runtime
            .import_collection(&text(i.get("collection")), array(i.get("rows")))
            .unwrap();
        return obj(&[("outcome", obj(&[("returned", V::Undefined)]))]);
    }
    if let Some(f) = failure(&c) {
        runtime.fail_storage(f).unwrap();
    }
    if let V::Array(args) = c.get("raw") {
        let value = match runtime.execute_contract_unverified(&args).unwrap() {
            SourceOutcome::Returned(v) => v,
            SourceOutcome::Thrown(e) => obj(&[
                (
                    "params",
                    runtime
                        .last_contract_parameters()
                        .cloned()
                        .unwrap_or(V::Undefined),
                ),
                (
                    "thrown",
                    obj(&[("name", s(&e.name)), ("message", s(&e.message))]),
                ),
            ]),
        };
        obj(&[("outcome", value)])
    } else {
        returned(
            runtime
                .invoke_dependency(&DependencyCall {
                    target: text(c.get("target")),
                    arguments: array(c.get("args")),
                })
                .unwrap(),
        )
    }
}
fn assert_state(runtime: &SourceContractRuntime, case: &V) {
    let expected = case.get("state");
    let name = text(case.get("name"));
    let mut known = std::collections::BTreeSet::new();
    for table in array(expected.get("tables")) {
        let collection = text(table.get("name"));
        known.insert(collection.clone());
        assert_eq!(
            V::Array(runtime.collection(&collection).to_vec()),
            table.get("rows"),
            "{name} complete {collection}"
        );
    }
    let state = runtime.state_value().unwrap();
    for table in array(state.get("tables")) {
        if !known.contains(&text(table.get("name"))) {
            assert_eq!(
                table.get("rows"),
                V::Array(vec![]),
                "{name} no invented rows"
            );
        }
    }
    assert_eq!(state.get("runtimePolicy"), s("MainCallableV8"));
    assert_eq!(
        state.get("clearlistLifecycle"),
        expected.get("clearlist"),
        "{name} four genuine Maps/banlist and declared library"
    );
    for (key, wanted) in [
        ("tallyStaticInitialized", "tallyInitialized"),
        ("tallyMod", "tallyMod"),
        ("propertyCache", "propertyCache"),
    ] {
        assert_eq!(
            state.get("globalClearing").get(key),
            expected.get(wanted),
            "{name} inherited {key}"
        );
    }
    assert_eq!(
        state.get("proofLifecycle").get("propertyInitialized"),
        expected.get("propertyInitialized"),
        "{name} sole Property"
    );
    assert_eq!(
        runtime.tally_cache(),
        expected.get("tally"),
        "{name} sole Tally"
    );
    assert_eq!(
        runtime
            .last_contract_parameters()
            .cloned()
            .unwrap_or(V::Undefined),
        expected.get("lastParams"),
        "{name} actual raw fields"
    );
}
fn effects(values: Vec<V>) -> V {
    V::Array(values.into_iter().filter_map(|v|{
 if matches!(v.get("kind"),V::String(k)if["database","db","uuid","random","rpc-clock"].contains(&k.as_str())){Some(v)}
 else if v.get("kind")==s("dependency")&&matches!(v.get("target"),V::String(t)if t.starts_with("ClearList.")||["Logic.createClearList","Logic.updateAdmin","Logic.issueOrRevokeAttestation","Logic.updateBannedCountries"].contains(&t.as_str())){Some(obj(&[("kind",s("source-function")),("target",v.get("target")),("args",v.get("args"))]))}
 else if v.get("kind")==s("dependency")&&v.get("target")==s("Activation.isTxTypeActive"){Some(obj(&[("kind",s("activation")),("method",s("isTxTypeActive")),("args",v.get("args")),("returned",v.get("returned"))]))}
 else if v.get("kind")==s("dependency")&&v.get("target")==s("Activation.getAdmin"){Some(obj(&[("kind",s("activation")),("method",s("getAdmin")),("args",v.get("args")),("returned",v.get("returned"))]))}else{None}}).collect())
}
fn setup(runtime: &mut SourceContractRuntime, case: &V) {
    let options = case.get("command").get("reset");
    assert_eq!(
        runtime
            .invoke_dependency(&DependencyCall {
                target: "SourceMain.configure".into(),
                arguments: vec![obj(&[("activationProfile", s("declared-answer"))])]
            })
            .unwrap(),
        DependencyReturn::Immediate(V::Undefined)
    );
    runtime.take_effects();
    runtime
        .set_entry(&if options.get("entry") == V::Undefined {
            "main-subgraph".into()
        } else {
            text(options.get("entry"))
        })
        .unwrap();
    runtime.set_activation_answer(options.get("inactive") != V::Bool(true));
    if options.get("bitcoinFallback") == V::Bool(true) {
        assert_eq!(
            runtime
                .invoke_dependency(&DependencyCall {
                    target: "SourceClearList.configureLibrary".into(),
                    arguments: vec![s("bitcoin-fallback")]
                })
                .unwrap(),
            DependencyReturn::Immediate(V::Undefined)
        );
        runtime.take_effects();
    }
    for table in array(case.get("state").get("tables")) {
        runtime
            .import_collection(&text(table.get("name")), array(table.get("rows")))
            .unwrap();
    }
}
#[test]
fn full_unchanged_clearlist_graph_both_numeric_versions_and_real_library_paths() {
    for fallback in [false, true] {
        let baseline = cases(false, fallback);
        assert_eq!(baseline, cases(true, fallback));
        let mut runtime = SourceContractRuntime::with_policy(P::MainCallableV8);
        let (mut resets, mut imports, mut raw, mut direct, mut unsupported) = (0, 0, 0, 0, 0);
        for case in &baseline {
            let c = case.get("command");
            let name = text(case.get("name"));
            if c.get("reset") != V::Undefined {
                resets += 1;
                runtime = SourceContractRuntime::with_policy(P::MainCallableV8);
                setup(&mut runtime, case);
            } else if c.get("unsupported") == V::Bool(true) {
                unsupported += 1;
                let before = runtime.clone();
                assert_eq!(
                    runtime.invoke_dependency(&DependencyCall {
                        target: text(c.get("target")),
                        arguments: array(c.get("args"))
                    }),
                    Err(SourceRuleError::UnsupportedNumericDomain)
                );
                assert_eq!(
                    runtime, before,
                    "{name} explicit object-key domain rollback; source prefix not claimed"
                );
                continue;
            } else {
                if c.get("import") != V::Undefined {
                    imports += 1;
                } else if c.get("raw") != V::Undefined {
                    raw += 1;
                } else {
                    direct += 1;
                }
                assert_eq!(
                    actual(&mut runtime, case),
                    case.get("result"),
                    "{name} exact transport/return/error"
                );
            }
            assert_state(&runtime, case);
            assert_eq!(
                effects(runtime.take_effects()),
                case.get("effects"),
                "{name} full ordered DB/dependency effects"
            );
        }
        assert_eq!(
            (baseline.len(), resets, imports, raw, direct, unsupported),
            (204, 27, 7, 59, 110, 1)
        );
    }
}
#[test]
fn retained_admin_root_revoke_prefixes_and_real_xpub_caches_restore_and_continue() {
    for fallback in [false, true] {
        let records = cases(false, fallback);
        let mut count = 0;
        for start in records.iter().enumerate().filter_map(|(i, r)| {
            let name = text(r.get("name"));
            (r.get("command").get("reset") != V::Undefined
                && (name.starts_with("failure-")
                    || [
                        "first10-reset",
                        "admin-stale-cache",
                        "revocation-legacy-multi",
                        "country-banlist",
                        "xpub-roots",
                    ]
                    .contains(&name.as_str())))
            .then_some(i)
        }) {
            count += 1;
            let mut replay = SourceContractReplay::with_policy(P::MainCallableV8).unwrap();
            replay
                .apply(C::Invoke(DependencyCall {
                    target: "SourceMain.configure".into(),
                    arguments: vec![obj(&[("activationProfile", s("declared-answer"))])],
                }))
                .unwrap();
            if fallback {
                replay
                    .apply(C::Invoke(DependencyCall {
                        target: "SourceClearList.configureLibrary".into(),
                        arguments: vec![s("bitcoin-fallback")],
                    }))
                    .unwrap();
            }
            for table in array(records[start].get("state").get("tables")) {
                replay
                    .apply(C::Import {
                        collection: text(table.get("name")),
                        rows: array(table.get("rows")),
                    })
                    .unwrap();
            }
            for case in records[start + 1..]
                .iter()
                .take_while(|r| r.get("command").get("reset") == V::Undefined)
            {
                let c = case.get("command");
                if let Some(f) = failure(&c) {
                    replay.apply(C::FailStorage(f)).unwrap();
                }
                let receipt = replay.apply(command(&c)).unwrap();
                assert!(
                    !matches!(receipt.outcome, O::LocalError(_)),
                    "genuine source receipt"
                );
                assert_state(replay.runtime(), case);
                let name = text(case.get("name"));
                if !text(records[start].get("name")).starts_with("xpub-")
                    || [
                        "derive-actual-library",
                        "store-root",
                        "register-verified-xpub",
                        "root-derived-address",
                        "three-leaf-proof",
                    ]
                    .contains(&name.as_str())
                {
                    let bytes = replay.checkpoint().unwrap();
                    assert_eq!(&bytes[..5], b"TLCR\x08");
                    let restored =
                        SourceContractReplay::from_checkpoint(&bytes, replay.head()).unwrap();
                    assert_eq!(restored, replay);
                    assert_eq!(restored.checkpoint().unwrap(), bytes);
                    replay = restored;
                }
            }
        }
        assert_eq!(count, 20);
    }
}
#[test]
fn old_profiles_do_not_acquire_clearlist_mutators_or_new_state() {
    let mut current = SourceContractRuntime::with_policy(P::MainCallableV8);
    assert_eq!(
        current
            .invoke_dependency(&DependencyCall {
                target: "ClearList.deriveAddressesFromXpub".into(),
                arguments: vec![s("invalid")]
            })
            .unwrap(),
        DependencyReturn::Immediate(V::Array(vec![]))
    );
    assert_eq!(
        current
            .state_value()
            .unwrap()
            .get("clearlistLifecycle")
            .get("libraryLocked"),
        V::Bool(true)
    );
    let before = current.clone();
    assert_eq!(
        current.invoke_dependency(&DependencyCall {
            target: "SourceClearList.configureLibrary".into(),
            arguments: vec![s("bitcoin-fallback")]
        }),
        Err(SourceRuleError::InvalidInput)
    );
    assert_eq!(
        current, before,
        "failed key does not permit changing already-selected module identity"
    );
    for policy in [
        P::OnchainV1,
        P::GlobalClearingV2,
        P::SourceLifecycleV3,
        P::OptionLifecycleV4,
        P::DistributionLifecycleV5,
        P::ProofLifecycleV6,
        P::LowerLifecycleV7,
    ] {
        let mut runtime = SourceContractRuntime::with_policy(policy);
        assert_eq!(
            runtime.state_value().unwrap().get("clearlistLifecycle"),
            V::Undefined
        );
        for target in [
            "ClearList.createClearlist",
            "ClearList.updateAdmin",
            "ClearList.addAttestation",
            "ClearList.revokeAttestation",
            "ClearList.deriveAddressesFromXpub",
            "ClearList.storeMerkleRoot",
            "ClearList.registerXpubForRoot",
            "Logic.createClearList",
            "Logic.issueOrRevokeAttestation",
            "Logic.updateBannedCountries",
            "SourceClearList.configureLibrary",
        ] {
            let before = runtime.clone();
            assert_eq!(
                runtime.invoke_dependency(&DependencyCall {
                    target: target.into(),
                    arguments: vec![]
                }),
                Err(SourceRuleError::UnknownDependency)
            );
            assert_eq!(runtime, before);
        }
        for kind in [7., 9.] {
            let before = runtime.clone();
            assert_eq!(
                runtime.execute_contract_unverified(&[
                    s("old"),
                    V::number(kind),
                    s("tl"),
                    s(""),
                    s("sender"),
                    V::Null,
                    V::number(0.),
                    V::number(0.),
                    V::number(10.)
                ]),
                Err(SourceRuleError::InvalidInput)
            );
            assert_eq!(runtime, before);
        }
    }
}
