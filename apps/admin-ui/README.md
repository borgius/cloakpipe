# CloakPipe Admin UI

A self-hosted, client-rendered admin SPA for **local** CloakPipe. Manage
profiles, policies, detection categories/rules, audit logs and vault secrets
against a CloakPipe instance running in `server` mode.

Built with **React + TypeScript + Vite** and **TanStack** Router / Query / Table.

## Requirements

CloakPipe must be running in **server** mode, which exposes the admin API under
`/admin/api/*`:

```bash
cloakpipe start server
```

## Quick start (production)

The easiest way to run the UI is the bundled npm package, which serves the
compiled SPA and reverse-proxies the backend:

```bash
npx cloakpipe serve            # see packages/cloakpipe-serve
```

## Development

```bash
cd apps/admin-ui
npm install
cp .env.example .env           # optional: point at your backend
npm run dev                    # http://localhost:5273
```

In dev, Vite proxies `/admin`, `/v1`, `/tree`, `/sessions`, `/health` to
`VITE_CLOAKPIPE_BASE_URL` (default `http://127.0.0.1:8400`).

### Scripts

| Script            | Description                                            |
| ----------------- | ------------------------------------------------------ |
| `npm run dev`     | Start the Vite dev server                              |
| `npm run build`   | Type-check and build to `dist/`                        |
| `npm run test`    | Run Vitest unit/component tests                        |
| `npm run lint`    | Type-check only (`tsc --noEmit`)                       |
| `npm run gen:api` | Regenerate API types from `openapi/admin-api.yaml`     |

## Environment variables

| Variable                 | Default       | Purpose                                            |
| ------------------------ | ------------- | -------------------------------------------------- |
| `VITE_CLOAKPIPE_BASE_URL`| same-origin   | Base URL of the CloakPipe server-mode instance.    |

Leave it empty when serving via `npx cloakpipe serve` (same-origin proxying).

## API contract

`openapi/admin-api.yaml` is the source of truth for the admin HTTP API. Frontend
types in `src/api/schema.ts` are generated from it via `openapi-typescript`
(`npm run gen:api`). The hand-written typed client lives in `src/api/client.ts`.

## Pages

- **Overview** – runtime/config status (mode, profile, NER, audit, vault).
- **Profiles** – built-in industry templates; activate applies detection live.
- **Policies** – disk-backed `cloakpipe.toml` configs; edit, validate, save,
  activate, delete. Unsaved-change guard included.
- **Categories & Rules** – detection families + custom regex rule CRUD.
- **Audit Logs** – query/filter/sort/paginate audit events; CSV export.
- **Vault & Secrets** – mapping inspection, **redacted by default**; revealing
  originals requires explicit confirmation and is audited.
- **Sessions** – runtime session diagnostics.

## Limitations

- **No built-in authentication** — intended for trusted/local operators or to be
  fronted by an external auth proxy.
- **Audit querying** is fully supported for the **SQLite** backend. The **JSONL**
  backend is read on a best-effort basis; a **disabled** backend returns a clear
  unsupported state.
- Listener/upstream/masking changes from a policy require a **restart**; detection
  settings apply live.
