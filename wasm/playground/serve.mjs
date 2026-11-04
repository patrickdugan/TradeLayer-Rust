// Static server for the playground: serves this folder and maps /tradelayer_wasm.wasm to a built
// file, so the binary never needs to be copied next to the sources.
//
//   node serve.mjs <path/to/tradelayer_wasm.wasm> [port=8080]
//   then open http://127.0.0.1:8080/
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { extname, join, normalize } from 'node:path';

const here = fileURLToPath(new URL('.', import.meta.url));
const TYPES = {
  '.html': 'text/html; charset=utf-8',
  '.mjs': 'text/javascript; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.wasm': 'application/wasm',
};

export function startServer(wasmPath, port = 0) {
  const server = createServer(async (req, res) => {
    try {
      const url = new URL(req.url, 'http://localhost');
      let path = decodeURIComponent(url.pathname);
      if (path === '/') path = '/index.html';
      let file;
      if (path === '/tradelayer_wasm.wasm') {
        file = wasmPath;
      } else {
        file = normalize(join(here, path));
        if (!file.startsWith(here)) throw Object.assign(new Error('forbidden'), { status: 403 });
      }
      const body = await readFile(file);
      res.writeHead(200, { 'content-type': TYPES[extname(file)] ?? 'application/octet-stream', 'cache-control': 'no-store' });
      res.end(body);
    } catch (error) {
      res.writeHead(error.status ?? 404);
      res.end(String(error.message));
    }
  });
  return new Promise((resolve) => server.listen(port, '127.0.0.1', () => resolve(server)));
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  const [wasmPath, port] = process.argv.slice(2);
  if (!wasmPath) {
    console.error('usage: node serve.mjs <tradelayer_wasm.wasm> [port]');
    process.exit(2);
  }
  const server = await startServer(wasmPath, Number(port ?? 8080));
  console.log(`playground at http://127.0.0.1:${server.address().port}/`);
}
