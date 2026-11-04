// TradeLayer playground page: UI, chain clock, oracle feed and the JS market maker.
// The economic engine runs in tradelayer_wasm.wasm (same folder).
import { ATOMS, loadEngine, mulberry32, quoteOps, toPrice1e4, walkStep } from './engine.mjs';

const TIP_URL = 'https://mempool.space/testnet4/api/blocks/tip/height';
const PRICES_URL = 'https://mempool.space/api/v1/prices';
const POLL_MS = 30_000;
const LOCAL_ANCHOR = 1_000_000;
const MAX_CATCH_UP = 50;
const params = new URLSearchParams(location.search);
const OFFLINE = params.get('offline') === '1';

const $ = (id) => document.getElementById(id);
const el = (tag, text, cls) => {
  const node = document.createElement(tag);
  if (text !== undefined) node.textContent = text;
  if (cls) node.className = cls;
  return node;
};

const S = {
  engine: null,
  session: 'pg',
  mode: 'local',
  tip: null, // latest testnet4 tip height
  nextHeight: 0,
  state: null,
  pending: [], // queued user ops, applied in the next block
  price: 100000, // random-walk price in USD
  rng: mulberry32(1),
  flowRng: mulberry32(2),
  mempoolUsd: null,
  running: false,
  busy: false,
  stopFf: false,
  timings: [],
  stateMs: 0,
  blocksApplied: 0,
  prices: [],
  mmWarned: false,
  timer: null,
  lastBlock: null,
};

// ---- log ----------------------------------------------------------------------------------

function log(text, cls = 'info') {
  const box = $('log');
  const line = el('div', text, cls);
  box.appendChild(line);
  while (box.childElementCount > 400) box.removeChild(box.firstChild);
  box.scrollTop = box.scrollHeight;
}

// ---- formatting ---------------------------------------------------------------------------

const money = (atoms) => (atoms / ATOMS).toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 });
const usd = (price1e4) => (price1e4 / 10000).toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 });
const signedClass = (text) => (text.startsWith('-') ? 'neg' : text === '0' ? '' : 'pos');
const toAtoms = (tokens) => Math.round(Number(tokens) * ATOMS);

function kv(node, rows) {
  node.replaceChildren();
  for (const [key, value] of rows) {
    node.appendChild(el('span', key));
    node.appendChild(el('span', String(value)));
  }
}

// ---- engine calls -------------------------------------------------------------------------

function call(request) {
  const out = S.engine.call(request);
  return out;
}

async function newSession() {
  if (S.state) call({ op: 'pg_drop', session: S.session });
  const live = S.mode === 'live' && S.tip !== null;
  const anchor = live ? S.tip + 1 : (S.tip ?? LOCAL_ANCHOR - 1) + 1;
  const startPrice = S.mempoolUsd && $('osrc').value === 'mempool' ? S.mempoolUsd : 100000;
  S.price = startPrice;
  S.rng = mulberry32(Number($('seed').value) >>> 0);
  S.flowRng = mulberry32((Number($('seed').value) + 1) >>> 0);
  const out = call({
    op: 'pg_new',
    id: S.session,
    anchor_height: Math.max(3, anchor),
    leverage: 10,
    notional: '1',
    marks: $('marks').checked,
    price_1e4: toPrice1e4(startPrice),
    accounts: [
      { name: 'mm', tusd_atoms: 1_000_000 * ATOMS },
      { name: 'alice', tusd_atoms: 100_000 * ATOMS },
      { name: 'bob', tusd_atoms: 100_000 * ATOMS },
      { name: 'carol', tusd_atoms: 100_000 * ATOMS },
    ],
  });
  if (!out.ok) {
    log(`pg_new failed: ${out.code} ${out.message}`, 'err');
    return;
  }
  S.nextHeight = out.next_height;
  S.pending = [];
  S.timings = [];
  S.prices = [toPrice1e4(startPrice)];
  S.blocksApplied = 0;
  S.mmWarned = false;
  log(`new session: first block height ${out.next_height} (${S.mode === 'live' ? 'testnet4 tip + 1' : 'local heights'}), oracle ${usd(toPrice1e4(startPrice))}`, 'info');
  refresh();
}

function refresh() {
  const t0 = performance.now();
  const out = call({ op: 'pg_state', session: S.session, fills: 0 });
  S.stateMs = performance.now() - t0;
  if (!out.ok) {
    log(`pg_state failed: ${out.code} ${out.message}`, 'err');
    return;
  }
  S.state = out;
  S.nextHeight = out.next_height;
}

// ---- oracle -------------------------------------------------------------------------------

function nextOraclePrice1e4() {
  if ($('osrc').value === 'mempool' && S.mempoolUsd) {
    S.price = S.mempoolUsd;
  } else {
    const volume = Math.max(0, Number($('vol').value)) / 10000;
    S.price = Math.max(1000, walkStep(S.price, S.rng(), volume));
  }
  return toPrice1e4(S.price);
}

async function fetchMempoolPrice() {
  if (OFFLINE) return;
  try {
    const response = await fetch(PRICES_URL, { cache: 'no-store' });
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    const body = await response.json();
    if (typeof body.USD === 'number' && body.USD > 0) {
      S.mempoolUsd = body.USD;
    }
  } catch (error) {
    log(`mempool.space price fetch failed: ${error.message}`, 'info');
  }
}

// ---- market maker -------------------------------------------------------------------------

function mmConfig() {
  return {
    levels: Math.max(1, Math.floor(Number($('mmLevels').value))),
    size: Number($('mmSize').value),
    spreadBps: Math.max(1, Math.floor(Number($('mmSpread').value))),
    stepBps: Math.max(0, Math.floor(Number($('mmStep').value))),
  };
}

function mmOpsFor(price1e4) {
  if (!$('mmOn').checked || !S.state) return [];
  const cfg = mmConfig();
  if (!(cfg.size > 0)) {
    if (!S.mmWarned) log('MM idle: size must be positive', 'err');
    S.mmWarned = true;
    return [];
  }
  // Fixed margin: each resting order reserves size * margin_per_contract tUSD.
  const row = S.state.accounts.find((a) => a.name === 'mm');
  const budget = row ? row.available_atoms + row.reserved_atoms : 0;
  const perLevel = 2 * Math.ceil(cfg.size * Number(S.state.contract.margin_per_contract) * ATOMS);
  const affordable = Math.floor((budget * 0.85) / perLevel);
  if (affordable < 1) {
    if (!S.mmWarned) log('MM out of tUSD: grant it more to resume quoting', 'err');
    S.mmWarned = true;
    return [{ t: 'cancel_all', account: 'mm' }];
  }
  S.mmWarned = false;
  cfg.levels = Math.min(cfg.levels, affordable);
  return quoteOps('mm', price1e4, cfg);
}

/** Random taker flow: with probability 0.5 one market order of 0.1 to 0.3 contract per block. */
function noiseOps(price1e4) {
  if (!$('noise').checked || !S.state || S.flowRng() >= 0.5) return [];
  const account = ['alice', 'bob', 'carol'][Math.floor(S.flowRng() * 3)];
  const side = S.flowRng() < 0.5 ? 'buy' : 'sell';
  const qty = `0.${1 + Math.floor(S.flowRng() * 3)}`;
  return [{ t: 'order', account, side, qty, price: (price1e4 / 10000).toFixed(2), market: true }];
}

// ---- block application --------------------------------------------------------------------

function recordTiming(ms) {
  S.timings.push(ms);
  if (S.timings.length > 400) S.timings.shift();
}

function sendBlock(height, ops) {
  const t0 = performance.now();
  const out = call({ op: 'pg_block', session: S.session, height, ops });
  const ms = performance.now() - t0;
  return { out, ms };
}

/** Applies one block at the next height: oracle, queued user inputs, MM requote. */
function applyBlock({ renderEvery = true } = {}) {
  const height = S.nextHeight;
  const price1e4 = nextOraclePrice1e4();
  const user = S.pending;
  const plain = user.filter((o) => o.t !== 'order');
  const orders = user.filter((o) => o.t === 'order');
  const mm = mmOpsFor(price1e4);
  const flow = noiseOps(price1e4);
  const oracle = { t: 'oracle', price_1e4: price1e4 };
  // Attempts from fullest to emptiest; the engine applies a block atomically, so a rejected
  // attempt changed nothing and the next one starts from the same state.
  const attempts = [
    { label: 'full', ops: [oracle, ...plain, ...mm, ...orders, ...flow] },
    ...(user.length || flow.length ? [{ label: 'without your inputs and random flow', ops: [oracle, ...mm] }] : []),
    ...(mm.length ? [{ label: 'oracle only', ops: [oracle] }] : []),
  ];
  let applied = null;
  for (const attempt of attempts) {
    const { out, ms } = sendBlock(height, attempt.ops);
    if (out.ok) {
      applied = { out, ms, attempt };
      break;
    }
    log(`block ${height} rejected (${attempt.label}): ${out.code} - ${out.message}`, 'err');
  }
  if (!applied) {
    log(`block ${height} could not be applied; session unchanged`, 'err');
    return null;
  }
  S.pending = [];
  S.blocksApplied += 1;
  S.prices.push(price1e4);
  if (S.prices.length > 300) S.prices.shift();
  recordTiming(applied.ms);
  S.lastBlock = { height, ms: applied.ms, fills: applied.out.fill_count };
  const quotes = applied.attempt.ops.filter((o) => o.t === 'order' && o.account === 'mm').length;
  log(`block ${height}  oracle ${usd(price1e4)}  inputs ${applied.attempt.ops.length}  mm quotes ${quotes}  fills ${applied.out.fill_count}  ${applied.ms.toFixed(2)} ms`, 'info');
  for (const f of applied.out.fills) {
    log(`  fill ${f.qty} @ ${f.price}  ${f.buyer} buys from ${f.seller}`, 'fill');
  }
  if (applied.out.fill_count > applied.out.fills.length) log(`  ... ${applied.out.fill_count - applied.out.fills.length} more fills`, 'fill');
  refresh();
  if (renderEvery) render();
  return applied;
}

async function catchUp(tip) {
  // Live mode: one TradeLayer block per testnet4 block.
  if (S.busy) return;
  S.busy = true;
  try {
    if (tip + 1 < S.nextHeight) return;
    const missing = tip - S.nextHeight + 1;
    if (missing > MAX_CATCH_UP) {
      log(`testnet4 is ${missing} blocks ahead of the session; starting a new session at the tip`, 'info');
      await newSession();
      return;
    }
    for (let h = S.nextHeight; h <= tip; h += 1) {
      if (!applyBlock({ renderEvery: h === tip })) break;
      if (!S.running) break;
      await new Promise((r) => setTimeout(r, 0));
    }
  } finally {
    S.busy = false;
  }
}

// ---- clock --------------------------------------------------------------------------------

async function pollTip() {
  if (OFFLINE) return;
  try {
    const response = await fetch(TIP_URL, { cache: 'no-store' });
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    const tip = Number.parseInt(await response.text(), 10);
    if (!Number.isFinite(tip)) throw new Error('unexpected tip response');
    const first = S.tip === null;
    S.tip = tip;
    if (first && S.mode === 'live' && !S.state) await newSession();
    if (S.mode === 'live' && S.running && S.state) await catchUp(tip);
  } catch (error) {
    log(`testnet4 tip fetch failed: ${error.message}`, 'info');
  }
  if ($('osrc').value === 'mempool') await fetchMempoolPrice();
  render();
}

function setRunning(on) {
  S.running = on;
  $('startstop').textContent = on ? 'Pause' : 'Start';
  clearInterval(S.timer);
  S.timer = null;
  if (!on) return;
  if (S.mode === 'live') {
    pollTip();
  } else {
    const ms = Math.max(0, Number($('interval').value));
    S.timer = setInterval(() => {
      if (!S.busy) applyBlock();
    }, ms);
  }
}

async function fastForward() {
  if (S.mode !== 'local') {
    log('fast-forward needs Local heights', 'err');
    return;
  }
  if (S.busy) return;
  const n = Math.max(1, Math.min(5000, Math.floor(Number($('ffn').value))));
  S.busy = true;
  S.stopFf = false;
  $('ff').disabled = true;
  $('ffstop').disabled = false;
  const t0 = performance.now();
  let done = 0;
  try {
    for (; done < n && !S.stopFf; done += 1) {
      if (!applyBlock({ renderEvery: false })) break;
      if (done % 25 === 24) {
        render();
        await new Promise((r) => setTimeout(r, 0));
      }
    }
  } finally {
    S.busy = false;
    $('ff').disabled = false;
    $('ffstop').disabled = true;
    render();
    log(`fast-forward: ${done} blocks in ${((performance.now() - t0) / 1000).toFixed(2)} s`, 'info');
  }
}

// ---- user actions -------------------------------------------------------------------------

function queue(op, text) {
  S.pending.push(op);
  log(`queued: ${text}`, 'info');
  if (S.mode === 'local' && $('auto').checked && !S.busy) {
    applyBlock();
  } else {
    render();
  }
}

function acct(id) {
  return $(id).value;
}

function amount(id) {
  const tokens = Number($(id).value);
  if (!(tokens > 0)) {
    log('amount must be positive', 'err');
    return null;
  }
  return toAtoms(tokens);
}

function placeOrder(side) {
  const account = acct('tacct');
  const qty = Number($('oqty').value);
  if (!(qty > 0)) return log('contracts must be positive', 'err');
  const market = $('otype').value === 'market';
  const mark = S.state?.oracle.price_1e4 ?? null;
  let price;
  if (market) {
    price = mark === null ? null : (mark / 10000).toFixed(2);
  } else {
    price = Number($('oprice').value);
    if (!(price > 0)) return log('limit price must be positive', 'err');
    price = price.toFixed(2);
  }
  const op = { t: 'order', account, side, qty: String(qty), price, market };
  if (price === null) delete op.price;
  queue(op, `${account} ${side} ${qty} ${market ? 'at market' : `limit ${price}`}`);
}

function closePosition() {
  const account = acct('tacct');
  const position = S.state?.positions.find((p) => p.account === account);
  if (!position) return log(`${account} has no open position`, 'err');
  const contracts = Number(position.contracts);
  const side = contracts > 0 ? 'sell' : 'buy';
  const qty = String(Math.abs(contracts));
  const mark = (S.state.oracle.price_1e4 / 10000).toFixed(2);
  queue({ t: 'order', account, side, qty, price: mark, market: true }, `${account} close ${side} ${qty} at market`);
}

function renderMyOrders() {
  const account = acct('tacct');
  const tbody = $('myorders').tBodies[0];
  tbody.replaceChildren();
  if (!S.state) return;
  const mine = [...S.state.book.bids, ...S.state.book.asks].filter((o) => o.account === account);
  for (const o of mine) {
    const tr = el('tr');
    tr.append(el('td', `#${o.id}`), el('td', o.side, o.side === 'buy' ? 'bid' : 'ask'), el('td', o.market ? 'market' : o.price), el('td', o.qty));
    const cell = el('td');
    const button = el('button', 'Cancel');
    button.addEventListener('click', () => queue({ t: 'cancel', account, order_id: o.id }, `${account} cancel order #${o.id}`));
    cell.appendChild(button);
    tr.appendChild(cell);
    tbody.appendChild(tr);
  }
}

// ---- rendering ----------------------------------------------------------------------------

function percentile(sorted, q) {
  return sorted.length ? sorted[Math.min(sorted.length - 1, Math.floor(q * sorted.length))] : 0;
}

function drawLine(canvas, values, color) {
  const ctx = canvas.getContext('2d');
  const { width, height } = canvas;
  ctx.clearRect(0, 0, width, height);
  if (values.length < 2) return;
  const lo = Math.min(...values);
  const hi = Math.max(...values);
  const span = hi - lo || 1;
  ctx.strokeStyle = color;
  ctx.lineWidth = 1.5;
  ctx.beginPath();
  values.forEach((v, i) => {
    const x = (i / (values.length - 1)) * (width - 2) + 1;
    const y = height - 4 - ((v - lo) / span) * (height - 8);
    if (i === 0) ctx.moveTo(x, y);
    else ctx.lineTo(x, y);
  });
  ctx.stroke();
}

function fillSelect(select, names, keep = true) {
  const current = select.value;
  select.replaceChildren(...names.map((n) => el('option', n)));
  if (keep && names.includes(current)) select.value = current;
}

function render() {
  const st = S.state;
  const queuedText = S.pending.length ? `${S.pending.length} queued for height ${S.nextHeight}` : 'none';
  kv($('clockinfo'), [
    ['testnet4 tip', S.tip ?? (OFFLINE ? 'offline' : 'unknown')],
    ['next TL height', S.nextHeight || '-'],
    ['TL blocks applied', S.blocksApplied],
    ['oracle (USD)', st ? usd(st.oracle.price_1e4) : '-'],
    ['oracle source', $('osrc').value === 'mempool' ? `mempool.space${S.mempoolUsd ? '' : ' (waiting)'}` : 'seeded random walk'],
    ['queued inputs', queuedText],
    ['wasm', S.engine ? `${(S.engine.byteLength / 1024).toFixed(0)} KiB` : 'loading'],
  ]);
  drawLine($('chart'), S.prices, '#3fb6ff');
  if (!st) return;

  fillSelect($('grantTo'), st.accounts.map((a) => a.name).filter((n) => n !== 'oracle' && n !== 'admin'));
  fillSelect($('redeemFrom'), st.accounts.map((a) => a.name).filter((n) => n !== 'oracle' && n !== 'admin'));
  fillSelect($('tacct'), ['alice', 'bob', 'carol']);
  if (!$('oprice').value) $('oprice').value = (st.oracle.price_1e4 / 10000).toFixed(2);

  const c = st.custody;
  kv($('supplyinfo'), [
    ['tUSD supply', money(c.supply_atoms)],
    ['issuer treasury', money(st.accounts.find((a) => a.name === st.issuer).available_atoms)],
    ['custody balanced', c.balanced ? 'yes' : 'NO'],
  ]);
  kv($('custody'), [
    ['available', money(c.available_atoms)],
    ['margin', money(c.margin_atoms)],
    ['order reserves', money(c.reserved_atoms)],
    ['fees', money(c.fees_atoms)],
    ['insurance pending', money(c.insurance_atoms)],
    ['loss pool', money(c.loss_pool_atoms)],
    ['unfunded loss', money(c.unfunded_loss_atoms)],
    ['sum equals supply', c.balanced ? 'yes' : 'NO'],
  ]);

  const mm = st.accounts.find((a) => a.name === 'mm');
  const mmPos = st.positions.find((p) => p.account === 'mm');
  const mmOrders = [...st.book.bids, ...st.book.asks].filter((o) => o.account === 'mm').length;
  kv($('mminfo'), [
    ['equity', mm ? money(mm.equity_atoms) : '-'],
    ['available', mm ? money(mm.available_atoms) : '-'],
    ['inventory (contracts)', mmPos ? mmPos.contracts : '0'],
    ['resting quotes', mmOrders],
  ]);

  // Book, aggregated by price: asks above the spread, bids below.
  const levels = (orders) => {
    const map = new Map();
    for (const o of orders) {
      const key = o.market ? 'market' : o.price;
      const level = map.get(key) ?? { price: key, qty: 0, count: 0 };
      level.qty += Number(o.qty);
      level.count += 1;
      map.set(key, level);
    }
    return [...map.values()];
  };
  const tbody = $('book').tBodies[0];
  tbody.replaceChildren();
  const asks = levels(st.book.asks).slice(0, 8).reverse();
  const bids = levels(st.book.bids).slice(0, 8);
  for (const [rows, cls] of [[asks, 'ask'], [bids, 'bid']]) {
    for (const l of rows) {
      const tr = el('tr');
      tr.append(el('td', l.price, cls), el('td', String(Number(l.qty.toFixed(8)))), el('td', String(l.count)));
      tbody.appendChild(tr);
    }
    if (cls === 'ask') {
      const spread = el('tr');
      const bestAsk = st.book.asks[0];
      const bestBid = st.book.bids[0];
      const label = bestAsk && bestBid && !bestAsk.market && !bestBid.market ? `spread ${(Number(bestAsk.price) - Number(bestBid.price)).toFixed(2)}` : 'spread -';
      const td = el('td', `mark ${usd(st.oracle.price_1e4)}   ${label}`);
      td.colSpan = 3;
      td.style.textAlign = 'center';
      spread.appendChild(td);
      tbody.appendChild(spread);
    }
  }

  const ptbody = $('positions').tBodies[0];
  ptbody.replaceChildren();
  for (const p of st.positions) {
    const tr = el('tr');
    tr.append(
      el('td', p.account),
      el('td', p.contracts, signedClass(p.contracts)),
      el('td', p.entry ? Number(p.entry).toFixed(2) : '-'),
      el('td', money(p.margin_atoms)),
      el('td', Number(p.unrealized_pnl).toFixed(2), signedClass(p.unrealized_pnl)),
      el('td', Number(p.realized_pnl).toFixed(2), signedClass(p.realized_pnl)),
    );
    ptbody.appendChild(tr);
  }
  kv($('posinfo'), [
    ['net contracts', st.net_contracts],
    ['open interest (long side)', st.open_interest],
    ['maintenance calls', st.maintenance_calls.length],
    ['fills so far', st.fill_total],
  ]);

  const atbody = $('accounts').tBodies[0];
  atbody.replaceChildren();
  for (const a of st.accounts.filter((x) => x.name !== 'admin' && x.name !== 'oracle')) {
    const tr = el('tr');
    tr.append(el('td', a.name), el('td', money(a.available_atoms)), el('td', money(a.margin_atoms)), el('td', money(a.reserved_atoms)), el('td', money(a.equity_atoms)));
    atbody.appendChild(tr);
  }
  renderMyOrders();

  const sorted = [...S.timings].sort((a, b) => a - b);
  const total = S.timings.reduce((a, b) => a + b, 0);
  const median = percentile(sorted, 0.5);
  kv($('timinginfo'), [
    ['last pg_block', S.lastBlock ? `${S.lastBlock.ms.toFixed(2)} ms (height ${S.lastBlock.height})` : '-'],
    ['median / p95 (last 400)', `${median.toFixed(2)} / ${percentile(sorted, 0.95).toFixed(2)} ms`],
    ['mean', S.timings.length ? `${(total / S.timings.length).toFixed(2)} ms` : '-'],
    ['throughput at median', median ? `${(1000 / median).toFixed(0)} blocks/s` : '-'],
    ['last pg_state', `${S.stateMs.toFixed(2)} ms`],
  ]);
  drawLine($('tchart'), S.timings, '#d29922');
}

// ---- wiring -------------------------------------------------------------------------------

function wire() {
  $('startstop').addEventListener('click', () => setRunning(!S.running));
  $('step').addEventListener('click', () => {
    if (!S.busy) applyBlock();
  });
  $('ff').addEventListener('click', fastForward);
  $('ffstop').addEventListener('click', () => {
    S.stopFf = true;
  });
  $('reset').addEventListener('click', () => newSession());
  $('mode').addEventListener('change', async () => {
    setRunning(false);
    S.mode = $('mode').value;
    $('ff').disabled = S.mode !== 'local';
    $('step').disabled = S.mode !== 'local';
    log(`heights: ${S.mode === 'live' ? 'following the testnet4 tip' : 'local'}; starting a new session`, 'info');
    await newSession();
  });
  $('osrc').addEventListener('change', async () => {
    if ($('osrc').value === 'mempool') await fetchMempoolPrice();
    render();
  });
  $('grant').addEventListener('click', () => {
    const atoms = amount('grantAmt');
    if (atoms) queue({ t: 'grant', to: acct('grantTo'), atoms }, `issuer grants ${$('grantAmt').value} tUSD to ${acct('grantTo')}`);
  });
  $('redeem').addEventListener('click', () => {
    const atoms = amount('redeemAmt');
    if (atoms) queue({ t: 'redeem', from: acct('redeemFrom'), atoms }, `${acct('redeemFrom')} redeems ${$('redeemAmt').value} tUSD`);
  });
  $('deposit').addEventListener('click', () => {
    const atoms = amount('depAmt');
    if (atoms) queue({ t: 'send', from: S.state.issuer, to: acct('tacct'), atoms }, `deposit ${$('depAmt').value} tUSD to ${acct('tacct')}`);
  });
  $('mmFund').addEventListener('click', () => {
    const atoms = amount('mmFundAmt');
    if (atoms) queue({ t: 'grant', to: 'mm', atoms }, `issuer grants ${$('mmFundAmt').value} tUSD to mm`);
  });
  $('buy').addEventListener('click', () => placeOrder('buy'));
  $('sell').addEventListener('click', () => placeOrder('sell'));
  $('close').addEventListener('click', closePosition);
  $('tacct').addEventListener('change', renderMyOrders);
  for (const id of ['mmOn', 'mmSpread', 'mmStep', 'mmLevels', 'mmSize']) {
    $(id).addEventListener('change', () => {
      S.mmWarned = false;
    });
  }
}

async function main() {
  S.mode = params.get('mode') === 'local' || OFFLINE ? 'local' : 'live';
  $('mode').value = S.mode;
  $('ff').disabled = S.mode !== 'local';
  $('step').disabled = S.mode !== 'local';
  wire();
  try {
    S.engine = await loadEngine(new URL('./tradelayer_wasm.wasm', import.meta.url));
  } catch (error) {
    log(`cannot load tradelayer_wasm.wasm: ${error.message}`, 'err');
    return;
  }
  log(`engine loaded (${(S.engine.byteLength / 1024).toFixed(0)} KiB)`, 'info');
  window.__playground = { S, applyBlock, fastForward, newSession, queue, render };
  if (S.mode === 'live') {
    await pollTip();
    if (!S.state) {
      log('testnet4 unreachable: switching to local heights', 'info');
      S.mode = 'local';
      $('mode').value = 'local';
      $('ff').disabled = false;
      $('step').disabled = false;
      await newSession();
    } else {
      setRunning(true);
    }
    setInterval(pollTip, POLL_MS);
  } else {
    await newSession();
    setInterval(pollTip, POLL_MS);
  }
  render();
}

main().catch((error) => log(`startup failed: ${error.message}`, 'err'));
