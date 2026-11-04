// Shared by the page (app.mjs) and the Node benchmark (bench.mjs): the wasm loader, the seeded
// generator, the oracle random walk and the market-maker quote ladder.

/** Loads tradelayer_wasm.wasm (raw ABI, no imports) from bytes or a URL. */
export async function loadEngine(source) {
  let bytes = source;
  if (typeof source === 'string' || source instanceof URL) {
    const response = await fetch(source);
    if (!response.ok) throw new Error(`cannot load ${source}: HTTP ${response.status}`);
    bytes = await response.arrayBuffer();
  }
  const { instance } = await WebAssembly.instantiate(bytes, {});
  const { memory, tl_alloc, tl_free, tl_call } = instance.exports;
  const encoder = new TextEncoder();
  const decoder = new TextDecoder();
  function callRaw(text) {
    const req = encoder.encode(text);
    const ptr = tl_alloc(req.length);
    new Uint8Array(memory.buffer, ptr, req.length).set(req);
    const out = tl_call(ptr, req.length);
    tl_free(ptr, req.length);
    const len = new DataView(memory.buffer).getUint32(out, true);
    const body = decoder.decode(new Uint8Array(memory.buffer, out + 4, len));
    tl_free(out, 4 + len);
    return body;
  }
  return {
    byteLength: bytes.byteLength,
    /** Parsed response: `{ ok: true, ... }` or `{ ok: false, code, message }`. */
    call: (request) => JSON.parse(callRaw(JSON.stringify(request))),
  };
}

export const ATOMS = 100_000_000;

/** mulberry32: the same generator the native benchmark uses. */
export function mulberry32(seed) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= (t + Math.imul(t ^ (t >>> 7), t | 61)) >>> 0;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/** One random-walk step. `volume` is the half-width of the relative move, e.g. 0.001. */
export function walkStep(price, u, volume) {
  return price * (1 + (u - 0.5) * 2 * volume);
}

export function toPrice1e4(price) {
  return Math.floor(price * 10000 + 0.5);
}

/** Integer cents as a decimal string. */
export function centsText(c) {
  return `${Math.floor(c / 100)}.${String(c % 100).padStart(2, '0')}`;
}

export function midCents(price1e4) {
  return Math.floor((price1e4 + 50) / 100);
}

/**
 * Symmetric ladder around the oracle price: cancel everything, then `levels` bids and asks at
 * offsets spreadBps + i * stepBps from the mid. Post-only so the engine prices a taker at the
 * quote. Returns engine ops.
 */
export function quoteOps(account, price1e4, { levels, size, spreadBps, stepBps }) {
  const ops = [{ t: 'cancel_all', account }];
  const mid = midCents(price1e4);
  for (let i = 0; i < levels; i += 1) {
    const offset = Math.floor((mid * (spreadBps + stepBps * i)) / 10000);
    ops.push({ t: 'order', account, side: 'buy', qty: String(size), price: centsText(mid - offset), post: true });
    ops.push({ t: 'order', account, side: 'sell', qty: String(size), price: centsText(mid + offset), post: true });
  }
  return ops;
}
