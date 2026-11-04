// Headless browser check over the Chrome DevTools Protocol: loads the page against a built wasm,
// fast-forwards 200 blocks, issues/grants/redeems tUSD, opens and closes a trader position, and
// asserts a non-empty book, balanced positions and no console errors.
//
//   node check_browser.mjs <chrome.exe> <tradelayer_wasm.wasm> [screenshot.png]
import { spawn } from 'node:child_process';
import { mkdtemp, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { startServer } from './serve.mjs';

const [chrome, wasm, shot] = process.argv.slice(2);
if (!chrome || !wasm) {
  console.error('usage: node check_browser.mjs <chrome> <tradelayer_wasm.wasm> [screenshot.png]');
  process.exit(2);
}
const server = await startServer(wasm);
const port = server.address().port;
const profile = await mkdtemp(join(tmpdir(), 'pg-chrome-'));
const debugPort = 9300 + Math.floor(Math.random() * 500);
const proc = spawn(
  chrome,
  [
    '--headless=new',
    `--remote-debugging-port=${debugPort}`,
    `--user-data-dir=${profile}`,
    '--no-first-run',
    '--no-default-browser-check',
    '--disable-gpu',
    '--window-size=1400,1800',
    'about:blank',
  ],
  { stdio: 'ignore' },
);

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
async function pageSocket() {
  for (let i = 0; i < 100; i += 1) {
    try {
      const list = await (await fetch(`http://127.0.0.1:${debugPort}/json/list`)).json();
      const page = list.find((t) => t.type === 'page');
      if (page) return page.webSocketDebuggerUrl;
    } catch {
      // the browser is still starting
    }
    await sleep(100);
  }
  throw new Error('chrome did not start');
}

const ws = new WebSocket(await pageSocket());
await new Promise((resolve, reject) => {
  ws.onopen = resolve;
  ws.onerror = reject;
});
let nextId = 1;
const waiting = new Map();
const consoleErrors = [];
ws.onmessage = (event) => {
  const msg = JSON.parse(event.data);
  if (msg.id && waiting.has(msg.id)) {
    waiting.get(msg.id)(msg);
    waiting.delete(msg.id);
  } else if (msg.method === 'Runtime.consoleAPICalled' && ['error', 'assert'].includes(msg.params.type)) {
    consoleErrors.push(msg.params.args.map((a) => a.value ?? a.description).join(' '));
  } else if (msg.method === 'Runtime.exceptionThrown') {
    consoleErrors.push(`exception: ${msg.params.exceptionDetails.text} ${msg.params.exceptionDetails.exception?.description ?? ''}`);
  } else if (msg.method === 'Log.entryAdded' && msg.params.entry.level === 'error') {
    consoleErrors.push(`log: ${msg.params.entry.text} ${msg.params.entry.url ?? ''}`);
  }
};
const send = (method, params = {}) =>
  new Promise((resolve) => {
    const id = nextId++;
    waiting.set(id, resolve);
    ws.send(JSON.stringify({ id, method, params }));
  });
const evaluate = async (expression) => {
  const reply = await send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true });
  if (reply.result.exceptionDetails) throw new Error(`eval failed: ${JSON.stringify(reply.result.exceptionDetails)}`);
  return reply.result.result.value;
};
async function until(expression, what, timeoutMs = 120000) {
  const end = Date.now() + timeoutMs;
  while (Date.now() < end) {
    if (await evaluate(expression)) return;
    await sleep(100);
  }
  throw new Error(`timeout waiting for ${what}`);
}

const results = [];
function check(name, ok, detail = '') {
  results.push({ name, ok, detail });
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${name}${detail ? `  (${detail})` : ''}`);
}

try {
  await send('Runtime.enable');
  await send('Log.enable');
  await send('Page.enable');
  await send('Page.navigate', { url: `http://127.0.0.1:${port}/index.html?mode=local&offline=1` });
  await until('!!(window.__playground && window.__playground.S.state)', 'engine and first session');
  const P = 'window.__playground.S';
  check('wasm engine loaded and session created', true, `${await evaluate(`${P}.engine.byteLength`)} bytes`);

  // Fast-forward 200 local blocks.
  const t0 = Date.now();
  await evaluate("document.getElementById('ffn').value = '200'; document.getElementById('ff').click(); 0");
  await until(`${P}.blocksApplied >= 200 && !${P}.busy`, '200 blocks');
  const ffMs = Date.now() - t0;
  const st = JSON.parse(await evaluate(`JSON.stringify(${P}.state)`));
  check('fast-forward applied 200 blocks', st.blocks === 200, `${ffMs} ms wall`);
  check('book is non-empty', st.book.bid_count > 0 && st.book.ask_count > 0, `${st.book.bid_count} bids, ${st.book.ask_count} asks`);
  check('positions balanced (net contracts 0)', st.net_contracts === '0', `net ${st.net_contracts}, ${st.fill_total} fills`);
  check('custody balanced after fast-forward', st.custody.balanced === true);
  check('taker flow produced fills', st.fill_total > 0, `${st.fill_total} fills`);
  // The remaining steps need exact positions, so switch the random flow off.
  await evaluate("document.getElementById('noise').checked = false; 0");

  // Issuer: grant then redeem.
  const supply = async () => JSON.parse(await evaluate(`JSON.stringify(${P}.state.custody.supply_atoms)`));
  const s0 = await supply();
  await evaluate("document.getElementById('grantTo').value = 'alice'; document.getElementById('grantAmt').value = '2500'; document.getElementById('grant').click(); 0");
  await until(`${P}.pending.length === 0 && !${P}.busy`, 'grant block');
  const s1 = await supply();
  check('grant mints 2500 tUSD', s1 - s0 === 2500e8, `supply +${(s1 - s0) / 1e8}`);
  await evaluate("document.getElementById('redeemFrom').value = 'alice'; document.getElementById('redeemAmt').value = '1000'; document.getElementById('redeem').click(); 0");
  await until(`${P}.pending.length === 0 && !${P}.busy`, 'redeem block');
  const s2 = await supply();
  check('redeem burns 1000 tUSD', s1 - s2 === 1000e8, `supply -${(s1 - s2) / 1e8}`);

  // Trader: open then close a position.
  const bobPosition = async () => JSON.parse(await evaluate(`JSON.stringify(${P}.state.positions.find((p) => p.account === 'bob') ?? null)`));
  const before = await bobPosition();
  await evaluate("document.getElementById('tacct').value = 'bob'; document.getElementById('otype').value = 'market'; document.getElementById('oqty').value = '1'; document.getElementById('buy').click(); 0");
  await until(`${P}.pending.length === 0 && !${P}.busy`, 'buy block');
  const open = await bobPosition();
  const delta = Number(open?.contracts ?? 0) - Number(before?.contracts ?? 0);
  check('trader buy opened a position', delta === 1, `bob contracts ${before?.contracts ?? 0} -> ${open?.contracts}`);
  await evaluate("document.getElementById('close').click(); 0");
  await until(`${P}.pending.length === 0 && !${P}.busy`, 'close block');
  const closed = await bobPosition();
  check('close flattened the position', closed === null, `bob after close: ${closed ? closed.contracts : 'flat'}`);
  const end = JSON.parse(await evaluate(`JSON.stringify(${P}.state)`));
  check('net contracts still zero', end.net_contracts === '0', end.net_contracts);
  check('custody balanced at the end', end.custody.balanced === true);

  const verified = await evaluate(`${P}.engine.call({ op: 'pg_state', session: 'pg', verify: true }).ok`);
  check('engine verifier and ledger conservation pass', verified === true);

  console.log(`timing panel:\n${await evaluate("document.getElementById('timinginfo').innerText")}`);
  console.log(`log tail:\n${await evaluate("[...document.querySelectorAll('#log div')].slice(-6).map((d) => d.textContent).join('\\n')")}`);
  if (shot) {
    const { result } = await send('Page.captureScreenshot', { format: 'png', captureBeyondViewport: true });
    await writeFile(shot, Buffer.from(result.data, 'base64'));
    console.log(`screenshot ${shot}`);
  }
  check('no console errors', consoleErrors.length === 0, consoleErrors.join(' | '));
} catch (error) {
  check('check script', false, error.message);
} finally {
  ws.close();
  proc.kill();
  server.close();
  await sleep(300);
  await rm(profile, { recursive: true, force: true }).catch(() => {});
}
const failed = results.filter((r) => !r.ok);
console.log(failed.length ? `FAILED ${failed.length} of ${results.length}` : `ALL ${results.length} CHECKS PASSED`);
process.exit(failed.length ? 1 : 0);
