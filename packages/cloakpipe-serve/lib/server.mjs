import http from 'node:http';
import https from 'node:https';
import { createReadStream, existsSync, statSync } from 'node:fs';
import { extname, isAbsolute, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const MIME = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.mjs': 'text/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.json': 'application/json; charset=utf-8',
  '.svg': 'image/svg+xml',
  '.png': 'image/png',
  '.jpg': 'image/jpeg',
  '.ico': 'image/x-icon',
  '.woff2': 'font/woff2',
  '.map': 'application/json; charset=utf-8',
};

// Backend namespaces that must be reverse-proxied to the CloakPipe server.
const PROXY_PREFIXES = ['/admin', '/v1', '/tree', '/sessions', '/health'];

export function shouldProxy(pathname) {
  return PROXY_PREFIXES.some(
    (p) => pathname === p || pathname.startsWith(p + '/') || pathname.startsWith(p + '?'),
  );
}

/**
 * Resolve a request path to a safe absolute file inside `root`.
 * Returns null if the resolved path escapes `root` (path traversal) or contains
 * a NUL byte. Uses `path.relative` to confine the result strictly to `root`.
 */
export function resolveStatic(root, pathname) {
  let decoded;
  try {
    decoded = decodeURIComponent(pathname.split('?')[0]);
  } catch {
    return null;
  }
  if (decoded.includes('\0')) return null;
  const rootResolved = resolve(root);
  // Resolve relative to root so absolute-looking paths cannot escape it.
  const candidate = resolve(rootResolved, '.' + (decoded.startsWith('/') ? decoded : `/${decoded}`));
  const rel = relative(rootResolved, candidate);
  if (rel === '') return candidate;
  if (rel.startsWith('..') || isAbsolute(rel)) return null;
  return candidate;
}

function serveFile(filePath, res) {
  const stream = createReadStream(filePath);
  res.writeHead(200, { 'content-type': MIME[extname(filePath)] || 'application/octet-stream' });
  stream.pipe(res);
  stream.on('error', () => {
    res.writeHead(500);
    res.end('Internal error');
  });
}

function proxyRequest(req, res, backend) {
  // Pin the target host/protocol/port to the configured backend. Only the
  // path and query string come from the request, preventing request forgery
  // via absolute-form request targets (e.g. "GET http://evil/...").
  const base = new URL(backend);
  let incoming;
  try {
    incoming = new URL(req.url || '/', 'http://internal.invalid');
  } catch {
    res.writeHead(400);
    res.end('Bad request');
    return;
  }
  const target = new URL(base.toString());
  target.pathname = incoming.pathname;
  target.search = incoming.search;

  const client = target.protocol === 'https:' ? https : http;
  const headers = { ...req.headers, host: target.host };

  const proxyReq = client.request(
    { protocol: target.protocol, hostname: target.hostname, port: target.port, path: target.pathname + target.search, method: req.method, headers },
    (proxyRes) => {
      res.writeHead(proxyRes.statusCode || 502, proxyRes.headers);
      proxyRes.pipe(res);
    },
  );

  proxyReq.on('error', (err) => {
    res.writeHead(502, { 'content-type': 'application/json' });
    res.end(
      JSON.stringify({
        error: {
          code: 'backend_unreachable',
          message: `Cannot reach CloakPipe backend at ${backend}: ${err.message}. ` +
            'Is it running with `cloakpipe start server`?',
        },
      }),
    );
  });

  req.pipe(proxyReq);
}

/**
 * Create the admin UI HTTP server.
 * @param {{ root: string, backend: string }} opts
 */
export function createServer({ root, backend }) {
  const indexHtml = join(root, 'index.html');

  return http.createServer((req, res) => {
    const pathname = (req.url || '/').split('?')[0];

    if (shouldProxy(pathname)) {
      proxyRequest(req, res, backend);
      return;
    }

    const filePath = resolveStatic(root, pathname);
    if (filePath && existsSync(filePath) && statSync(filePath).isFile()) {
      serveFile(filePath, res);
      return;
    }

    // SPA fallback: serve index.html for client-side routes.
    if (existsSync(indexHtml)) {
      serveFile(indexHtml, res);
      return;
    }

    res.writeHead(404, { 'content-type': 'text/plain' });
    res.end('admin UI assets not found');
  });
}

/** Locate the built SPA assets bundled with this package, with a dev fallback. */
export function resolveAssetRoot() {
  const here = fileURLToPath(new URL('.', import.meta.url));
  const candidates = [
    resolve(here, '../public'),
    resolve(here, '../../../apps/admin-ui/dist'),
  ];
  return candidates.find((c) => existsSync(join(c, 'index.html'))) || candidates[0];
}
