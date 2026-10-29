'use strict';
const fs=require('node:fs'),path=require('node:path'),cp=require('node:child_process'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const root=path.resolve(__dirname,'..'),locked=process.argv.includes('--lock');
const result=cp.spawnSync(process.execPath,[path.join(__dirname,'channel_queue_large_financial.cjs'),...(locked?['--lock']:[])],{encoding:'utf8',windowsHide:true,maxBuffer:64*1024*1024});
assert.equal(result.error,undefined);assert.equal(result.status,0,result.stderr);assert.equal(result.stderr,'');
const file=path.join(root,'fixtures/channel_queue_large_financial'+(locked?'_locked':'')+'.jsonl');
if(process.argv.includes('--write'))fs.writeFileSync(file,result.stdout);else assert.equal(fs.readFileSync(file,'utf8').replace(/\r\n/g,'\n'),result.stdout);
console.log(JSON.stringify({profile:'TL-ORIGINAL-LARGE-CHANNEL-FINANCIAL-1',numeric:locked?'genuine9.1.2':'genuine9.3.1',records:result.stdout.trim().split('\n').length,sha256:crypto.createHash('sha256').update(result.stdout).digest('hex')}));
