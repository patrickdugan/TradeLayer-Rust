// Unauthenticated local simulation; never deploy or use for custody.
function vaultFactory(c,policy){
  // This simulation uses object-identity capabilities issued by a mock runtime.
  // Real NEAR receipts, predecessor authentication, storage staking and finality
  // are deliberately outside its authority.
  const assert=(b,m)=>{if(!b)throw Error(m)};
  const max=(1n<<64n)-1n;
  const completed=new Set(), pending=new Map(), deposits=[], exits=new Map();
  let D=0n,R=0n,holdings=0n,reserved=0n;
  const resolutionTickets=new WeakSet(),exitTickets=new WeakSet(),callbackTickets=new WeakSet();
  const state=()=>({D,R,holdings,reserved,count:deposits.length,root:c.tree(deposits.map(c.depositLeaf)).root});
  function check(){assert(D>=R&&holdings>=0n&&reserved>=0n&&reserved<=D-R,"reserve invariant");}
  function receive({predecessor,receipt,event,sender,recipientScriptHash,requested}){
    assert(predecessor===policy.token,"token predecessor");
    assert(typeof requested==="bigint"&&requested>0n&&requested<=max,"requested");
    const d={domain:policy.domain,receipt,event,sender,recipientScriptHash,index:0n,amount:requested};
    c.depositLeaf(d);const id=c.depositId(d);
    assert(!pending.has(id)&&!completed.has(id),"duplicate receipt event");
    pending.set(id,{...d,requested});return id;
  }
  // Mock FT resolution source. Never exposes a production verification method.
  function issueResolution(id,retained,terminal=true){
    const t=Object.freeze({id,retained,terminal});resolutionTickets.add(t);return t;
  }
  function finalizeDeposit(ticket){
    assert(resolutionTickets.has(ticket)&&ticket.terminal,"untrusted or nonterminal resolution");
    const p=pending.get(ticket.id);assert(p,"deposit missing");
    const n=ticket.retained;assert(typeof n==="bigint"&&n>=0n&&n<=p.requested,"retained");
    assert(D+n<=max&&(!n||deposits.length<c.CAP),"capacity");
    resolutionTickets.delete(ticket);pending.delete(ticket.id);completed.add(ticket.id);
    if(n===0n)return null;
    const d={domain:p.domain,receipt:p.receipt,event:p.event,sender:p.sender,recipientScriptHash:p.recipientScriptHash,index:BigInt(deposits.length),amount:n};
    deposits.push(Object.freeze(d));D+=n;holdings+=n;check();return {...d};
  }
  function issueExit(id,destination,amount){const t=Object.freeze({id,destination,amount});exitTickets.add(t);return t}
  function registerExit(t){
    assert(exitTickets.has(t)&&!exits.has(t.id),"unverified/replayed exit");
    assert(typeof t.amount==="bigint"&&t.amount>0n&&t.amount<=holdings-reserved,"reserve");
    assert(typeof t.destination==="string"&&/^[a-z0-9]+(?:[._-][a-z0-9]+)*$/.test(t.destination)&&t.destination.length>=2&&t.destination.length<=64,"destination");
    exitTickets.delete(t);exits.set(t.id,{...t,status:"pending",attempt:0,transferred:false});reserved+=t.amount;check();
  }
  function begin(id){
    const e=exits.get(id);assert(e&&e.status==="pending","exit not pending");
    assert(e.attempt<Number.MAX_SAFE_INTEGER,"attempt overflow");
    e.status="inflight";e.attempt++;e.transferred=false;return {id,attempt:e.attempt,destination:e.destination,amount:e.amount};
  }
  function simulateTransfer(id,attempt,success){
    const e=exits.get(id);assert(e&&e.status==="inflight"&&e.attempt===attempt&&!e.transferred,"invalid transfer attempt");
    assert(typeof success==="boolean","transfer result");
    assert(!success||holdings>=e.amount,"holdings");
    if(success)holdings-=e.amount;
    e.transferred=true;const t=Object.freeze({id,attempt,success});callbackTickets.add(t);return t;
  }
  function callback(t){
    const e=exits.get(t.id);assert(callbackTickets.has(t)&&e&&e.status==="inflight"&&e.attempt===t.attempt,"unauthorized callback");
    callbackTickets.delete(t);
    if(t.success){e.status="paid";reserved-=e.amount;R+=e.amount}
    else {e.status="pending";e.transferred=false}
    check();
  }
  return {receive,issueResolution,finalizeDeposit,issueExit,registerExit,begin,simulateTransfer,callback,state,depositProof:i=>({leaf:{...deposits[i]},path:c.tree(deposits.map(c.depositLeaf)).path(i),count:BigInt(deposits.length),root:state().root}),status:id=>exits.get(id)?.status};
}
export { vaultFactory };
