#!/usr/bin/env node
'use strict';
// Seeded multi-block contract histories over the unchanged composed driver
// scripts/source_contract_execution_vectors.cjs. Anchored transforms swap its
// scene list for generated histories and its fixture for a gzip one; the
// genuine module composition, actions and state snapshots are the driver's own.
// Usage: source_contract_history_vectors.cjs --check|--write --module-root <tradelayer.js>
const assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), Module = require('node:module');
const driver = path.join(__dirname, 'source_contract_execution_vectors.cjs');
let code = fs.readFileSync(driver, 'utf8').replace(/\r\n/g, '\n');
function replace(before, after) {
  assert.equal(code.split(before).length, 2, 'driver anchor ' + before.slice(0, 60));
  code = code.replace(before, () => after);
}
// An oracle random walk, one to three random A/B orders per block, a drain
// every block and a fee flush every fifth block.
replace('function compactSummary(state,sourceOrders){', `function seeded(seed){return ()=>{seed=seed+0x6D2B79F5|0;let t=Math.imul(seed^seed>>>15,1|seed);t=t+Math.imul(t^t>>>7,61|t)^t;return((t^t>>>14)>>>0)/4294967296;};}
function history(seed,blocks){
  const r=seeded(seed),actions=[];let price=100;
  for(let h=10;h<10+blocks;h++){
    price=Math.max(90,Math.min(110,price+Math.round((r()-0.5)*4)));
    if(h>10&&r()<0.7)actions.push({publish:{price,height:h}});
    for(let k=1+Math.floor(r()*3);k>0;k--){
      const sell=r()<0.5,amount=1+Math.floor(r()*2),quote=price+Math.round((r()-0.5)*4);
      actions.push({raw:body(amount,sell,{price:quote}),sender:r()<0.5?A:B,block:h});
    }
    actions.push({drain:h});
    if((h-9)%5===0)actions.push({flushFees:h});
  }
  return {name:\`history-seed-\${seed}\`,history:true,actions};
}
const historyScenes=[history(1,15),history(2,15),history(3,15)];
function compactSummary(state,sourceOrders){`);
// Genuine source throws are part of the observed history and are compared.
replace('function verifyScene(scene,observations){\n', `function verifyScene(scene,observations){
  const byKind={};
  observations.forEach(({outcome,state},i)=>{
    assert.deepEqual(state.unsupported,[],\`\${scene.name}: unowned runtime boundary\`);
    if(outcome.thrown){const key=\`\${Object.keys(scene.actions[i]).find(k=>k!=='sender'&&k!=='block')}: \${outcome.thrown.message}\`;byKind[key]=(byKind[key]||0)+1;}
  });
  if(args.includes('--summary-throws'))console.log(scene.name,JSON.stringify(byKind));
  return;
`);
replace('  for(const scene of scenes){', '  for(const scene of historyScenes){');
// The Rust projection reads commands, outcomes and state; effects stay empty.
replace('hex(initial),hex(e.effects.splice(0)),', 'hex(initial),hex((e.effects.splice(0),[])),');
replace('hex(snapshot),hex(effects),', 'hex(snapshot),hex([]),');
replace("scripts/source_contract_execution_vectors.cjs\\n'", "scripts/source_contract_history_vectors.cjs over scripts/source_contract_execution_vectors.cjs\\n'");
replace('ordered effects diagnostic JSON hex\\t', 'ordered effects diagnostic JSON hex (empty)\\t');
replace("const target=path.join(root,'fixtures/source_contract_execution_vectors.tsv');", "const target=path.join(root,'fixtures/source_contract_history_vectors.tsv.gz');");
replace("if(args.includes('--write'))fs.writeFileSync(target,output);else assert.equal(fs.readFileSync(target,'utf8').replace(/\\r\\n/g,'\\n'),output);",
  "const zlib=require('node:zlib');if(args.includes('--write'))fs.writeFileSync(target,zlib.gzipSync(output,{level:9}));else assert.equal(zlib.gunzipSync(fs.readFileSync(target)).toString('utf8'),output);");
replace('console.log(`PASS ${scenes.length} actual composed contract scenes', 'console.log(`PASS ${historyScenes.length} actual composed contract history scenes');
const mod = new Module(driver, module);
mod.filename = driver;
mod.paths = Module._nodeModulePaths(path.dirname(driver));
mod._compile(code, driver);
