#!/usr/bin/env node
'use strict';
// Independent bytes: fixed original diagnostic value tags, Node's SHA-256.
const crypto=require('node:crypto');
const S=x=>['string',x],N=x=>['number',x],A=x=>['array',x],O=x=>['object',Object.entries(x)];
function frame(value){const raw=Buffer.from(JSON.stringify(value));const length=Buffer.alloc(4);length.writeUInt32BE(raw.length);return Buffer.concat([length,raw]);}
function digest(policy,scope,commands){const count=Buffer.alloc(4);count.writeUInt32BE(commands.length);return crypto.createHash('sha256').update(Buffer.concat([Buffer.from('TradeLayer/SourceFullDispatchCommands/v4/'),Buffer.from([policy,scope]),count,...commands.map(frame)])).digest('hex');}
const source=[O({kind:S('SetHeight'),height:N('8000000000000000')}),O({kind:S('Invoke'),target:S('Tally.queueFeeAccrual'),arguments:A([N('401c000000000000'),N('3e45798ee2308c3a'),N('3ff0000000000000'),N('4024000000000000')])})];
const main=[O({kind:S('Invoke'),target:S('Main.saveMaxProcessedHeight'),arguments:A([N('4024000000000000')])})];
for(const policy of [9,10,11])for(const scope of [1,2])console.log(JSON.stringify({policy,scope,digest:digest(policy,scope,scope===1?source:main)}));
