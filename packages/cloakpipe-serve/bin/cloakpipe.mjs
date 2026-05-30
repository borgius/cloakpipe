#!/usr/bin/env node
import { existsSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { createServer, resolveAssetRoot } from '../lib/server.mjs';

const HELP = `cloakpipe — serve the self-hosted CloakPipe admin UI

Usage:
  npx cloakpipe serve [options]

Options:
  --port, -p <port>       Port to listen on (default: 8420, env PORT)
  --host <host>           Host to bind (default: 127.0.0.1, env HOST)
  --backend, -b <url>     CloakPipe server-mode base URL to proxy
                          (default: http://127.0.0.1:8400, env CLOAKPIPE_BASE_URL)
  --help, -h              Show this help

The admin UI requires CloakPipe to be running in server mode:
  cloakpipe start server

Security: the admin API has no built-in authentication. Bind to localhost or
front it with an authenticating reverse proxy. Do not expose it to untrusted
networks — it can read/modify policies and reveal vault secrets.
`;

function parseArgs(argv) {
  const args = { _: [] };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    switch (a) {
      case '--help':
      case '-h':
        args.help = true;
        break;
      case '--port':
      case '-p':
        args.port = argv[++i];
        break;
      case '--host':
        args.host = argv[++i];
        break;
      case '--backend':
      case '-b':
        args.backend = argv[++i];
        break;
      default:
        args._.push(a);
    }
  }
  return args;
}

function main() {
  const args = parseArgs(process.argv.slice(2));
  const command = args._[0] || 'serve';

  if (args.help || command === 'help') {
    process.stdout.write(HELP);
    return;
  }

  if (command !== 'serve') {
    process.stderr.write(`Unknown command: ${command}\n\n${HELP}`);
    process.exit(1);
  }

  const port = Number(args.port || process.env.PORT || 8420);
  const host = args.host || process.env.HOST || '127.0.0.1';
  const backend = (
    args.backend ||
    process.env.CLOAKPIPE_BASE_URL ||
    'http://127.0.0.1:8400'
  ).replace(/\/$/, '');

  const root = resolveAssetRoot();
  if (!existsSync(join(root, 'index.html'))) {
    process.stderr.write(
      `Admin UI assets not found at ${root}.\n` +
        'Build them first: (cd apps/admin-ui && npm install && npm run build)\n' +
        'or run `npm run build` inside packages/cloakpipe-serve.\n',
    );
    process.exit(1);
  }
  // Guard against a stray file named index.html in an unexpected location.
  statSync(join(root, 'index.html'));

  const server = createServer({ root, backend });
  server.listen(port, host, () => {
    process.stdout.write(
      `CloakPipe admin UI → http://${host}:${port}\n` +
        `Proxying /admin, /v1, /tree, /sessions, /health → ${backend}\n` +
        'Press Ctrl+C to stop.\n',
    );
  });

  const shutdown = () => {
    server.close(() => process.exit(0));
  };
  process.on('SIGINT', shutdown);
  process.on('SIGTERM', shutdown);
}

main();
