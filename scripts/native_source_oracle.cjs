'use strict';
// Test-only immutable source identity, never state-transition authority.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const COMMIT='dca0888b827c5bc28fa9189bc9e1d507f5c5670c';
const EXPECTED={
  "activation": {
    "bytes": 12976,
    "sha256": "268f08438eb80765587b8ce24d719f41de573648e108ef64df557faad870ea0e"
  },
  "vesting": {
    "bytes": 19178,
    "sha256": "4c134b6871f450d3190a0958b629731d960540069d0c8c37a6da33d6e1f70d2b"
  },
  "property": {
    "bytes": 17638,
    "sha256": "ef0ee7ba5c362227a8bfd44192e351c107871a93a3dc93b04ce6d1a5ebfc70b1"
  },
  "tally": {
    "bytes": 59507,
    "sha256": "cfde9e5845f810bf62d9f1a07d4db4ea011b11d3d2bb63f041d39bea557cddef"
  },
  "logic": {
    "bytes": 118868,
    "sha256": "f09d476e581d38cf7278f1424aadba7266f25e0fbc23763ceba4afa79c5b3e1d"
  },
  "main": {
    "bytes": 43435,
    "sha256": "006d59ba3d28d802b0e0c5be2d9d661b58caf1c919a704b3f0d40bc9613c4009"
  },
  "db": {
    "bytes": 4177,
    "sha256": "b92a05fee6652494b72bf6893d9633850c697be773e88b24d62798a8bc8f5e38"
  }
};
function verifyNativeSources(repo){
 const dir=path.join(repo,'fixtures/native_genesis');
 const pins=JSON.parse(fs.readFileSync(path.join(dir,'source-pins.json'),'utf8'));
 assert.equal(pins.commit,COMMIT);
 assert.deepEqual(pins.files.map(p=>path.basename(p.file,'.js')).sort(),Object.keys(EXPECTED).sort());
 for(const p of pins.files){
  const name=path.basename(p.file,'.js');assert.equal(p.file,'src/'+name+'.js');
  assert.equal(p.bytes,EXPECTED[name].bytes);assert.equal(p.sha256,EXPECTED[name].sha256);
  const bytes=fs.readFileSync(path.join(dir,'source',name+'.js'));
  assert.equal(bytes.length,p.bytes);assert.equal(crypto.createHash('sha256').update(bytes).digest('hex'),EXPECTED[name].sha256,name);
 }
 const extra=JSON.parse(fs.readFileSync(path.join(dir,'entitlement-pins.json'),'utf8'));
 assert.equal(extra.commit,COMMIT);assert.deepEqual(Object.keys(extra.files).sort(),['db','logic','main','tally','vesting']);
 for(const[name,p]of Object.entries(extra.files)){assert.equal(p.source,'src/'+name+'.js');assert.equal(p.bytes,EXPECTED[name].bytes);assert.equal(p.sha256,EXPECTED[name].sha256);}
}
module.exports={verifyNativeSources};
