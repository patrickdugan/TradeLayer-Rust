// Loader for tradelayer_wasm.wasm (raw ABI, no imports). Works in browsers and Node 18+.
//
//   const tl = await loadTradeLayer(bytesOrUrl);
//   tl.call({ op: 'version' })  // -> { ok: true, crate: '0.1.0-rc.2' }
//
// `call` throws only on ABI misuse; operation failures come back as { ok: false, code, message }.

export async function loadTradeLayer(source) {
  let bytes = source;
  if (typeof source === 'string' || source instanceof URL) {
    bytes = await (await fetch(source)).arrayBuffer();
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
    exports: instance.exports,
    callRaw,
    call: (request) => JSON.parse(callRaw(JSON.stringify(request))),
  };
}
