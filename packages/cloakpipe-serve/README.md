# cloakpipe (admin UI server)

Serve the self-hosted **CloakPipe admin UI** and reverse-proxy a local CloakPipe
server-mode instance — with zero runtime dependencies.

```bash
# 1. Start CloakPipe in server mode (exposes /admin/api/*)
cloakpipe start server

# 2. Serve the admin UI (in another terminal)
npx cloakpipe serve
# → CloakPipe admin UI → http://127.0.0.1:8420
```

## Usage

```
npx cloakpipe serve [options]

Options:
  --port, -p <port>     Port to listen on (default: 8420, env PORT)
  --host <host>         Host to bind (default: 127.0.0.1, env HOST)
  --backend, -b <url>   CloakPipe server base URL to proxy
                        (default: http://127.0.0.1:8400, env CLOAKPIPE_BASE_URL)
  --help, -h            Show help
```

The server serves the built SPA and reverse-proxies the backend namespaces
`/admin`, `/v1`, `/tree`, `/sessions`, and `/health` to `--backend`, so the UI
runs same-origin (no CORS, no separate `VITE_CLOAKPIPE_BASE_URL` needed).

## Building locally

The published package bundles the compiled SPA in `public/`. To build from a
git checkout:

```bash
cd packages/cloakpipe-serve
npm run build      # builds apps/admin-ui and copies dist → public/
node bin/cloakpipe.mjs serve
```

## Security

- **Authentication is optional.** The admin API can read/modify policies and
  reveal vault secrets. Start the backend with `CLOAKPIPE_ADMIN_TOKEN` set to
  require a bearer token on `/admin/api/*`, bind to `127.0.0.1` (default), and/or
  place an authenticating reverse proxy in front. Do **not** expose an
  unauthenticated admin API to untrusted networks.
- Requires CloakPipe running in **`server`** mode; other modes do not expose
  `/admin/api/*`.
