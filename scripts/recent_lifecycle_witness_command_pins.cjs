'use strict';
const crypto = require('node:crypto');
const domains = ['tl-source-number-bits-bounded-decimal-bignumber-v1', 'tl-source-owned-db-data-only-rpc-clock-rng-v1'];
const frames = ['8000000000000000','4024000000000000'].map(bits => Buffer.from(JSON.stringify(['object', [['kind',['string','SetHeight']],['height',['number',bits]]]])));
function u16(n) { const b=Buffer.alloc(2);b.writeUInt16BE(n);return b; }
function u32(n) { const b=Buffer.alloc(4);b.writeUInt32BE(n);return b; }
for (const [policy,scopes] of [[7,[1,2]],[8,[1,2,3]]]) for (const scope of scopes) {
  const b=[Buffer.from('TradeLayer/SourceRecentLifecycleTransitionCommands/v3/'),Buffer.from([policy,scope])];
  for(const domain of domains)b.push(u16(Buffer.byteLength(domain)),Buffer.from(domain));
  b.push(u32(frames.length));for(const f of frames)b.push(u32(f.length),f);
  console.log(policy,scope,crypto.createHash('sha256').update(Buffer.concat(b)).digest('hex'));
}
