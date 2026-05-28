<!-- markdownlint-disable-file -->

# Task Research Notes: LLM HTTP Proxy Mode

## Research Executed

### Tool Usage Ledger

- `read_file` loaded applicable skills and instructions.
  - Read Rust best-practice guidance for ownership, errors, and tests.
  - Read concise writing guidance for the research report.
  - Read global Copilot instructions, including the Hypergrep convention.
- `list_dir` and `file_search` inspected `.copilot-tracking/`.
  - Research directory exists.
  - Target file did not exist before this update.
  - Existing research files are unrelated to this LLM proxy task.
- `file_search` inspected workspace structure.
  - `crates/cloakpipe-proxy/src/` contains `handlers.rs`, `lib.rs`, `server.rs`, `state.rs`, `streaming.rs`, and `tree_handlers.rs`.
  - `crates/cloakpipe-core/src/` contains `config.rs`, `vault.rs`, `vault_sqlite.rs`, `replacer.rs`, `rehydrator.rs`, `format_preserving.rs`, `session.rs`, and detector modules.
  - No `.github/instructions/` or `copilot/` convention files were found in this workspace.
- `fetch_webpage` inspected Mirage public sources.
  - Fetched `https://github.com/chandika/mirage-proxy` for README and repository overview.
  - Fetched `https://github.com/chandika/mirage-proxy/tree/main/src` for source file names.
  - Fetched raw `README.md`, `Cargo.toml`, `mirage.default.yaml`, and the `v0.8.2` release page.
- Terminal command: `git status --short` and `hypergrep --model "" . | head -160`.
  - Workspace is already dirty in unrelated application files: `Cargo.lock`, `Cargo.toml`, `README.md`, several CLI/core files, and tests.
  - Hypergrep reported an 8-crate Rust workspace; hot spots include `crates/cloakpipe-cli/src/commands.rs`, `crates/cloakpipe-core/src/session.rs`, `crates/cloakpipe-core/src/format_preserving.rs`, `crates/cloakpipe-core/src/vault.rs`, and `crates/cloakpipe-proxy/src/handlers.rs`.
- Terminal command: Python GitHub API/raw-source inspection for `chandika/mirage-proxy`.
  - Verified source files: `src/audit.rs`, `src/config.rs`, `src/faker.rs`, `src/main.rs`, `src/patterns.rs`, `src/providers.rs`, `src/proxy.rs`, `src/redactor.rs`, `src/session.rs`, `src/stats.rs`, `src/update.rs`, `src/vault.rs`.
  - Captured source line ranges for routing, request mutation, response mutation, SSE streaming, provider mapping, wrappers, vault, session, and config behavior.
- `grep_search` inspected cloakpipe symbols and docs.
  - Confirmed current registered proxy routes and tests.
  - Confirmed `proxy.mode` and `masking_strategy` exist in config but `proxy.mode` is not used by current proxy handlers.
  - Confirmed `.gitignore` ignores `.env`.
- `semantic_search` inspected the workspace for less-obvious proxy/provider conventions.
  - Results pointed back to `crates/cloakpipe-proxy/src/handlers.rs`, `streaming.rs`, `docs/api.md`, and proxy tests.
- `file_search` checked for environment files.
  - No `.env*` files were found.
  - Research-only constraint prevented creating a placeholder `.env`; implementation should add one only if allowed by the implementation task.

### File Analysis

- `/Users/admin/dev/cloakpipe/Cargo.toml`
  - Rust workspace uses edition 2021 and version `0.14.0`.
  - Existing workspace dependencies already include `tokio`, `axum`, `hyper`, `reqwest` with `stream` and `rustls-tls`, `tower`, `tower-http`, `serde_json`, `bytes`, `futures`, `async-stream`, `http-body-util`, `aes-gcm`, `zeroize`, `rand`, `regex`, `anyhow`, `thiserror`, and `rusqlite`.
  - Mirage parity dependencies not currently present include `flate2`, `zstd`, `tokio-stream`, `argon2`, `serde_yaml`, `dirs-next`, and `md5`.
- `/Users/admin/dev/cloakpipe/crates/cloakpipe-proxy/Cargo.toml`
  - Proxy crate depends on `cloakpipe-core`, `cloakpipe-audit`, `cloakpipe-tree`, `axum`, `hyper`, `reqwest`, `tower`, `tower-http`, `bytes`, `futures`, `async-stream`, `tracing`, `uuid`, `anyhow`, and `http-body-util`.
- `/Users/admin/dev/cloakpipe/crates/cloakpipe-proxy/src/lib.rs`
  - Declares the proxy as an OpenAI-compatible HTTP proxy that detects and pseudonymizes prompts, forwards sanitized requests, and rehydrates responses.
- `/Users/admin/dev/cloakpipe/crates/cloakpipe-proxy/src/server.rs`
  - `build_router` registers explicit routes only.
  - Current LLM routes are `POST /v1/chat/completions` and `POST /v1/embeddings`.
  - Direct privacy routes, CloakTree routes, and session routes are also registered.
  - There is no catch-all LLM provider route today.
- `/Users/admin/dev/cloakpipe/crates/cloakpipe-proxy/src/handlers.rs`
  - `proxy_chat_completions` reads a JSON body, checks `stream`, optionally extracts a session ID, pseudonymizes `messages[*].content` when content is a string, forwards to `{proxy.upstream}/v1/chat/completions`, then rehydrates response content.
  - `proxy_embeddings` pseudonymizes `input` when it is a string or array of strings, forwards to `{proxy.upstream}/v1/embeddings`, and returns the upstream body without response rehydration.
  - The current proxy authenticates upstream with the server-side env var named by `proxy.api_key_env`; inbound `Authorization` is ignored for chat and embeddings.
  - Current proxy forwards `OpenAI-Organization` only.
  - Current proxy handlers call `Replacer::pseudonymize`, so they use token replacements even when config sets `masking_strategy = "similar"`.
- `/Users/admin/dev/cloakpipe/crates/cloakpipe-proxy/src/streaming.rs`
  - `rehydrate_stream` handles OpenAI-style SSE chunks by parsing `data: ...` lines and rewriting `choices[0].delta.content`.
  - It calls `response.text().await`, which buffers the entire upstream response before iterating lines. This is not true low-latency streaming.
  - It uses `Rehydrator::rehydrate_chunk`, whose current chunk logic recognizes token-like patterns such as `ORG_7`, not arbitrary plausible fake values.
- `/Users/admin/dev/cloakpipe/crates/cloakpipe-proxy/src/state.rs`
  - `AppState` stores config, detector, detection config, active profile, vault, audit sink, `reqwest::Client`, optional upstream API key, and session manager.
  - Client timeout comes from `config.proxy.timeout_seconds`.
- `/Users/admin/dev/cloakpipe/crates/cloakpipe-core/src/config.rs`
  - `ProxyConfig` has `listen`, `upstream`, `api_key_env`, `timeout_seconds`, `max_concurrent`, `mode`, and `masking_strategy`.
  - Config default function sets `mode` to `"cloaktree"`, while CLI default config sets `mode` to `"proxy"`.
  - `DetectionConfig` supports secrets, financial, dates, emails, phone numbers, IP addresses, internal URLs, NER, custom patterns, overrides, and resolver settings.
- `/Users/admin/dev/cloakpipe/crates/cloakpipe-core/src/lib.rs`
  - `MaskingStrategy` supports `Similar` by default, `Token`, and `FormatPreserving`.
  - Public entity and pseudonymization types are already suitable for reusing in HTTP middleware.
- `/Users/admin/dev/cloakpipe/crates/cloakpipe-core/src/vault.rs`
  - File vault stores forward original-to-token and reverse token-to-original mappings.
  - Supports `get_or_create`, `get_or_create_fp`, and `get_or_create_similar`.
  - Persists encrypted JSON using AES-256-GCM and atomic temp-file rename.
- `/Users/admin/dev/cloakpipe/crates/cloakpipe-core/src/vault_sqlite.rs`
  - SQLite vault exists with WAL mode, per-row encryption, user-scoped mapping support, and in-memory caches.
  - Current proxy state uses `Vault`, not `SqliteVault`.
- `/Users/admin/dev/cloakpipe/crates/cloakpipe-core/src/replacer.rs`
  - `pseudonymize_with_strategy` dispatches to token, format-preserving, or similar fake generation.
  - This is the right core entry point for an LLM mode that needs Mirage-style plausible fakes.
- `/Users/admin/dev/cloakpipe/crates/cloakpipe-core/src/rehydrator.rs`
  - Full-response rehydration sorts mappings by replacement length and uses token-boundary checks.
  - Chunk rehydration only detects `[A-Z]+_\d+` pseudo-token patterns.
- `/Users/admin/dev/cloakpipe/crates/cloakpipe-core/src/format_preserving.rs`
  - Similar fake generation already covers persons, organizations, locations, dates, percentages, amounts, phones, emails, IPs, URLs, secrets, SSNs, credit cards, account identifiers, usernames, device IDs, and other custom identifiers.
  - Tests verify plausible email, phone, URL, SSN, credit-card, IP, AWS key, and GitHub token outputs.
- `/Users/admin/dev/cloakpipe/crates/cloakpipe-core/src/detector/mod.rs`
  - Detector composes regex patterns, financial detection, optional NER backends, GLiNER-PII sidecar, custom rules, preserve list, force list, and overlapping-span deduplication.
- `/Users/admin/dev/cloakpipe/crates/cloakpipe-core/src/detector/patterns.rs`
  - Pattern detector includes AWS keys, OpenAI/generic keys, GitHub tokens, connection strings, JWTs, email, IP, SSN, Aadhaar, PAN, prefixed IDs, licenses, contextual passwords, PINs, IMEI, account numbers, credit cards, usernames, names, organizations, locations, phones, and URLs depending on config.
- `/Users/admin/dev/cloakpipe/crates/cloakpipe-core/src/session.rs`
  - Session manager supports header-derived session IDs, TTL, coreference resolution, sensitivity escalation, and safe session stats.
- `/Users/admin/dev/cloakpipe/crates/cloakpipe-cli/src/main.rs`
  - CLI has `start`, `test`, `stats`, `init`, `setup`, `presets`, `policy`, `mcp`, `tree`, `vector`, `sessions`, `ner`, `scan`, and `restore` commands.
  - There is no wrapper install or provider-list command today.
- `/Users/admin/dev/cloakpipe/crates/cloakpipe-cli/src/commands.rs`
  - `start` loads config, resolves vault key, builds detector, opens file vault, builds audit sink, resolves server-side upstream API key, and starts `cloakpipe_proxy::server`.
  - `scan` already uses `Replacer::pseudonymize_with_strategy` and exports `vault-mappings.json` for restore.
  - CLI writes global config and policies when needed; implementation tasks must avoid unexpected source-tree writes in tests.
- `/Users/admin/dev/cloakpipe/docs/api.md`
  - Documents actual current routes from `server.rs` and behavior from `handlers.rs`.
  - Notes that LLM-backed routes return `503` when `proxy.api_key_env` is unset.
  - Notes current limitations: only string `messages[].content` is masked, multimodal arrays are forwarded untouched, non-streaming rehydration rewrites `choices[].message.content`, streaming rehydration rewrites `choices[0].delta.content`, and leaked-PII scan is non-streaming only.
- `/Users/admin/dev/cloakpipe/docs/ARCHITECTURE.md`
  - Architecture document already anticipates `crates/cloakpipe-proxy/src/routing.rs`, `middleware.rs`, and `embeddings.rs`, but those files do not exist in current source.
- `/Users/admin/dev/cloakpipe/docs/TECH.md`
  - V2 API table mentions `/*` passthrough, but current `server.rs` does not implement catch-all passthrough.
- `/Users/admin/dev/cloakpipe/cloakpipe.toml`
  - Current project config uses `profile = "fintech"`, `proxy.listen = "127.0.0.1:8900"`, `proxy.upstream = "http://localhost:11434"`, `proxy.api_key_env = "OLLAMA_API_KEY"`, `proxy.mode = "proxy"`, and `proxy.masking_strategy = "similar"`.
  - Detection enables secrets, financial, dates, emails, phone numbers, IP addresses, internal URLs, and DistilBERT PII NER.
- `/Users/admin/dev/cloakpipe/crates/cloakpipe-proxy/tests/privacy_api.rs`
  - Tests build the axum router with `tower::ServiceExt::oneshot`.
  - Existing tests cover direct privacy endpoints, configure endpoint, session context, and `503` behavior for chat/embeddings/tree without upstream key.
- `/Users/admin/dev/cloakpipe/crates/cloakpipe-core/tests/integration.rs`
  - Includes streaming token rehydration tests.
- `/Users/admin/dev/cloakpipe/crates/cloakpipe-cli/tests/test_scan.rs`
  - CLI tests verify scan detect-only, mask output, no files, similar strategy output, restore roundtrip, and `--no-ner`.

### Code Search Results

- `proxy_chat_completions|proxy_embeddings|rehydrate_stream|pseudonymize_messages|pseudonymize_with_strategy|masking_strategy|mode`
  - Found current explicit LLM handlers in `crates/cloakpipe-proxy/src/handlers.rs` and `streaming.rs`.
  - Found `mode` only in config and default config creation; no runtime mode branch in proxy server.
  - Found `pseudonymize_with_strategy` used by CLI scan, not by proxy HTTP handlers.
- `chat/completions|stream|proxy|rehydrate|pseudonymize|vault|masking|upstream|API` in `docs/**`
  - `docs/api.md` is the most accurate current API reference.
  - `docs/TECH.md` includes planned catch-all passthrough that source has not implemented.
- `tokio|axum|reqwest|async-stream|tower-http|serde_json|hyper|http-body|eventsource`
  - Proxy crate already has enough HTTP/body primitives for a catch-all handler and response streaming.
  - Dedicated compression crates and `tokio-stream` are absent.
- `.env|CLOAKPIPE_VAULT_KEY|OPENAI_API_KEY|ANTHROPIC_API_KEY|OLLAMA_API_KEY`
  - `.gitignore` ignores `.env`.
  - No `.env*` file exists in the workspace.

### External Research

- #fetch:https://github.com/chandika/mirage-proxy
  - README states Mirage is a localhost proxy on `127.0.0.1:8686` that replaces secrets and PII with plausible fakes before provider egress, then restores originals in responses.
  - It supports Claude Code, Cursor, Cline, Codex CLI, Aider, Continue.dev, OpenClaw, and SDKs with configurable base URLs.
  - It supports shadow/dry-run mode, provider bypass list, SSE streaming, encrypted vault, wrappers, `--why`, and `--flag`.
- #fetch:https://raw.githubusercontent.com/chandika/mirage-proxy/main/README.md
  - README documents `mirage-proxy --setup`, wrapper scripts in `~/.mirage/bin/`, shadow mode as `--dry-run`, `MIRAGE_VAULT_KEY` for encrypted vault persistence, `/healthz`, `/why`, `/flag`, provider bypass config, and known skip classes.
  - README states JWTs, hex digests, SRI integrity values, Anthropic signed thinking blocks, Codex `encrypted_content` envelopes, and binary/multipart payloads are skipped.
  - README states streaming has a 128-byte boundary buffer and warns that a fake landing exactly at a chunk boundary can slip through.
- #fetch:https://github.com/chandika/mirage-proxy/tree/main/src
  - Source tree contains `audit.rs`, `config.rs`, `faker.rs`, `main.rs`, `patterns.rs`, `providers.rs`, `proxy.rs`, `redactor.rs`, `session.rs`, `stats.rs`, `update.rs`, and `vault.rs`.
- #fetch:https://raw.githubusercontent.com/chandika/mirage-proxy/main/Cargo.toml
  - Mirage uses `tokio`, `hyper`, `hyper-util`, `http-body-util`, `reqwest` with `stream`, `json`, and `rustls-tls`, `serde_yaml`, `regex`, `bytes`, `futures-util`, `tokio-stream`, `zstd`, `flate2`, `aes-gcm`, `sha2`, `argon2`, `md5`, and `ctrlc`.
- #fetch:https://raw.githubusercontent.com/chandika/mirage-proxy/main/mirage.default.yaml
  - Default config includes `target`, `bind`, `port`, `sensitivity`, `rules.always_redact`, `rules.mask`, `rules.warn_only`, `code_block_passthrough`, `allowlist`, `blocklist`, `audit`, `dry_run`, and `update_check`.
- #fetch:https://github.com/chandika/mirage-proxy/releases/tag/v0.8.2
  - Latest release tag is `v0.8.2`, commit `9f66f54`, with release title mentioning shadow mode, `--why`/`--flag`, confidence grading, and false-positive guards.
- #githubRepo:"chandika/mirage-proxy src/proxy.rs redactor.rs session.rs vault.rs providers.rs config.rs faker.rs main.rs"
  - GitHub API/raw-source command verified concrete source line ranges.
  - `src/proxy.rs` contains request collection, body inspection, non-text bypass, provider bypass, decompression/recompression, JSON recursive redaction, dry-run forwarding of original body, provider resolution, response rehydration, and streaming boundary buffering.
  - `src/providers.rs` contains explicit provider prefix routing and OpenAI/Codex auto-routing.
  - `src/faker.rs` contains per-session plausible fake generation and rehydration mappings.
  - `src/vault.rs` contains AES-GCM encrypted session-scoped mapping persistence.
  - `src/main.rs` contains CLI setup, wrappers, daemon/service install, `--shadow`, `--dry-run`, `--why`, `--flag`, and `--list-providers`.

### Project Conventions

- Standards referenced: Rust 2021 workspace conventions, existing `anyhow` use in CLI/proxy boundaries, existing `thiserror` availability for library errors, and current axum test style.
- Instructions followed: research-only mode; no application files modified; exactly one research file created under `.copilot-tracking/research/`.
- Local convention: `docs/api.md` explicitly treats `server.rs`, `handlers.rs`, and `tree_handlers.rs` as the source of truth for current HTTP routes.
- Test convention: proxy HTTP tests use `build_router` plus `tower::ServiceExt::oneshot`; CLI tests use `CARGO_BIN_EXE_cloakpipe` and temp dirs.

## Key Discoveries

### Mirage Proxy Pattern

Mirage implements a generic localhost LLM proxy, not a provider-specific OpenAI-only handler. Its core path is:

1. Accept any request on the local daemon.
2. Reserve local admin routes: `/healthz`, `/why`, and `/flag`.
3. Read request body bytes.
4. Skip non-text content types and configured bypass providers.
5. Decompress textual compressed request bodies for inspection when possible.
6. Parse JSON and recursively redact content values, or redact plain text if JSON parsing fails.
7. In dry-run/shadow mode, forward the original request body while logging would-be substitutions.
8. Resolve upstream provider from path prefix or known OpenAI/Codex paths.
9. Forward headers except hop-by-hop and body-size headers; force `accept-encoding: identity`.
10. Rehydrate response bodies for non-streaming responses.
11. Stream response bytes with a small overlap buffer for SSE and other chunked responses.

### Mirage Routing

- `src/providers.rs` defines provider prefixes such as `/anthropic`, `/openai`, `/google`, `/vertex`, `/mistral`, `/cohere`, `/deepseek`, `/groq`, `/openrouter`, `/xai`, and others.
- `resolve_provider` strips explicit prefixes and returns `(upstream_base_url, remaining_path)`.
- It auto-routes `/v1/*`, `/responses`, `/chat/completions`, `/completions`, `/embeddings`, and `/models` to OpenAI-compatible paths.
- When `chatgpt-account-id` is present, Codex/ChatGPT account traffic goes to `https://chatgpt.com/backend-api/codex...` instead of `https://api.openai.com`.

### Mirage Request Mutation

- `src/proxy.rs` skips non-text request content types such as multipart, images, PDFs, and other binary bodies.
- It treats compressed request bodies by decompressing for inspection and recompressing after redaction.
- It detects JSON bodies, derives a session ID from `mirage_session`, `model`, or `default`, and recursively mutates strings.
- It only recurses into content-like keys such as `content`, `text`, `messages`, `system`, `input`, `instructions`, `description`, `prompt`, `tools`, and tool result keys.
- It skips auth/config/protocol fields such as `api_key`, `authorization`, `token`, `session_id`, `model`, `stream`, `max_tokens`, `temperature`, `id`, `role`, `signature`, `encrypted_content`, `ciphertext`, `metadata`, and `mirage_session`.
- It skips Anthropic signed thinking objects with `type = "thinking"` and `signature`.
- It skips large base64 blobs and non-text data URLs to avoid corrupting binary payloads.

### Mirage Response Mutation

- Non-streaming responses are read as bytes, skipped when content type is non-text, decompressed when needed, rehydrated through the session `Faker`, and recompressed when needed.
- Signed Anthropic thinking responses are passed through unchanged.
- Response `content-length` and `transfer-encoding` headers are stripped before returning mutated bodies.
- Streaming responses use `response.bytes_stream()` and a spawned task. Mirage holds a 128-byte leftover buffer to catch fake values split across chunks, then calls `faker.rehydrate` on safe chunks.
- Streaming rehydration is raw text replacement over chunk bytes; it does not require OpenAI-only SSE JSON parsing.

### Mirage Shadow And Bypass

- `--shadow` is an alias for `--dry-run`.
- In dry-run/shadow mode, Mirage runs detection and logging but forwards the original request body.
- `Config::is_bypassed` checks whether the resolved upstream URL contains any configured bypass pattern.
- README recommends bypassing `generativelanguage.googleapis.com` for Google TLS fingerprint issues.

### Mirage Vault And Mapping

- `Faker` keeps in-memory maps from original to fake and fake to original.
- `SessionManager` gives each session its own `Faker`.
- `Vault` persists session-scoped mappings as encrypted JSON: `session_id -> original -> fake`, plus a reverse map `fake -> (session_id, original)`.
- `Vault::key_from_passphrase` derives a 256-bit key with Argon2id from `MIRAGE_VAULT_KEY` or `--vault-key`.
- `Faker` generates plausible fake emails, phones, credit cards, SSNs, IPs, AWS keys, prefixed tokens, bearer tokens, connection strings, private keys, and high-entropy strings.
- For connection strings, Mirage also stores component-level reverse mappings for user, password, host, and database components so rehydration still works if the model rewrites the URI shape.
- `/why?decoy=...` returns kind, session, and MD5 fingerprint without returning the original.
- `/flag?decoy=...` puts the underlying original into a session allowlist and persists flags to `~/.mirage/flags.jsonl` per README.

### Mirage Wrappers

- `src/main.rs` defines wrapper targets for `claude`, `codex`, `cursor`, `aider`, and `opencode`.
- Wrappers set only the needed base URL environment variables.
- `claude` uses `ANTHROPIC_BASE_URL=http://127.0.0.1:${MIRAGE_PORT}/anthropic`.
- `codex`, `cursor`, and `opencode` use `OPENAI_BASE_URL=http://127.0.0.1:${MIRAGE_PORT}`.
- `aider` sets both Anthropic and OpenAI base URL variables.
- Direct wrappers such as `claude-direct` bypass Mirage by locating the real binary outside `~/.mirage/bin`.

### Cloakpipe Current Proxy

- Current cloakpipe proxy is OpenAI-compatible, not generic multi-provider.
- Route handling is JSON-extractor based for chat and embeddings, so it cannot currently proxy arbitrary methods, paths, headers, or non-JSON bodies.
- Chat request mutation is narrow: only string `messages[*].content` is pseudonymized.
- Response mutation is OpenAI-specific: non-streaming rewrites `choices[].message.content`; streaming rewrites `choices[0].delta.content`.
- Current streaming implementation buffers the full upstream response with `response.text().await`, which weakens streaming behavior.
- Current HTTP proxy does not use `proxy.masking_strategy`; it always uses token pseudonymization through `Replacer::pseudonymize`.
- Current docs already call out these limitations, so hidden tests may expect existing behavior unless new mode is isolated behind config.

### Cloakpipe Core Fit

- Cloakpipe already has the key privacy primitives needed for Mirage-style behavior.
- `MaskingStrategy::Similar` and `Vault::get_or_create_similar` support plausible fakes.
- `Replacer::pseudonymize_with_strategy` is ready for request mutation in a new LLM HTTP proxy mode.
- Full-response `Rehydrator::rehydrate` works with reverse mappings and length ordering, so it can restore similar fakes in complete responses.
- Chunk rehydration needs work for similar fake mode because it currently buffers around token regexes only.
- Session manager already supports header-derived sessions and context-aware behavior, but Mirage derives session from JSON `mirage_session` or `model`. Cloakpipe should support both header and body-derived IDs if wrapper mode is added.

### Complete Examples

Source-derived flow from Mirage; this is pseudocode, not production code:

```rust
async fn llm_proxy_flow(request) {
    if request.path in ["/healthz", "/why", "/flag"] {
        return local_admin_response(request);
    }

    let body = read_body_bytes(request).await?;
    if non_text_content_type(request.headers) || provider_is_bypassed(request.path) {
        return forward_unmodified(request, body).await;
    }

    let inspected = decompress_if_needed(body, request.headers)?;
    let session = derive_session_from_json_model_or_header(&inspected);
    let mutated = if is_json(&inspected) {
        redact_only_content_keys(inspected, session)
    } else {
        redact_plain_text(inspected, session)
    };

    let outbound_body = if shadow_mode { body } else { recompress_if_needed(mutated) };
    let upstream = resolve_provider_prefix_or_openai_path(request.path)?;
    let response = forward_with_filtered_headers(upstream, outbound_body).await?;

    if response.is_streaming() {
        stream_with_boundary_rehydration(response, session).await
    } else {
        rehydrate_textual_response(response, session).await
    }
}
```

Current cloakpipe chat proxy flow, based on `crates/cloakpipe-proxy/src/handlers.rs`:

```rust
async fn current_cloakpipe_chat_flow(json_body) {
    let session_id = header_session_if_enabled();
    pseudonymize_messages_string_content_only(json_body.messages, session_id);
    let upstream = format!("{}/v1/chat/completions", config.proxy.upstream);
    let response = post_json_with_server_side_api_key(upstream, json_body).await?;
    if json_body.stream == true {
        return openai_sse_delta_rehydration(response).await;
    }
    scan_unexpected_response_pii_then_rehydrate_choices_message_content(response).await
}
```

### API and Schema Documentation

- Mirage local endpoints:
  - `GET /healthz` for liveness and counters.
  - `GET /why?decoy=<value>` for substitution explanation without disclosing originals.
  - `POST /flag?decoy=<value>` for session pass-through/forgiveness.
- Mirage CLI flags relevant to cloakpipe planning:
  - `--port`, `--bind`, `--config`, `--sensitivity`, `--shadow`, `--dry-run`, `--why`, `--flag`, `--vault-key`, `--list-providers`, `--setup`, `--wrapper-install`, `--service-install`.
- Mirage config schema from `mirage.default.yaml`:
  - `bind`, `port`, `sensitivity`, `rules.always_redact`, `rules.mask`, `rules.warn_only`, `bypass`, `audit.enabled`, `audit.path`, `audit.log_values`, `dry_run`, `update_check`.
- Cloakpipe current HTTP endpoints:
  - `GET /health`.
  - Direct privacy endpoints: `/v1/pseudonymize`, `/v1/rehydrate`, `/v1/detect`, `/v1/vault_stats`, `/v1/configure`, `/v1/session_context`, with root aliases.
  - LLM endpoints: `POST /v1/chat/completions`, `POST /v1/embeddings`.
  - Tree and session management endpoints.
- Cloakpipe current config schema relevant to this task:
  - `[proxy] listen`, `upstream`, `api_key_env`, `timeout_seconds`, `max_concurrent`, `mode`, `masking_strategy`.
  - `[detection]` rules, `[detection.ner]`, custom patterns, overrides, resolver.
  - `[session] enabled`, `id_from`, `ttl_seconds`, `coreference`, `sensitivity_escalation`, `session_threshold`.

### Configuration Examples

Current cloakpipe project proxy config:

```toml
[proxy]
listen = "127.0.0.1:8900"
upstream = "http://localhost:11434"
api_key_env = "OLLAMA_API_KEY"
timeout_seconds = 120
max_concurrent = 256
mode = "proxy"
masking_strategy = "similar"
```

Mirage default behavior expressed compactly:

```yaml
bind: "127.0.0.1"
port: 8686
sensitivity: medium
bypass: []
rules:
  always_redact: [SSN, CREDIT_CARD, PRIVATE_KEY, AWS_KEY, GITHUB_TOKEN, API_KEY, BEARER_TOKEN, CONNECTION_STRING, SECRET]
  mask: [EMAIL, PHONE]
  warn_only: [IP_ADDRESS]
audit:
  enabled: true
  path: "./mirage-audit.jsonl"
  log_values: false
dry_run: false
```

Implementation-facing cloakpipe config shape should extend existing `[proxy]` instead of introducing a new root config file:

```toml
[proxy]
mode = "llm-http"
listen = "127.0.0.1:8900"
masking_strategy = "similar"
dry_run = false
bypass = ["generativelanguage.googleapis.com"]
auth_mode = "pass-through"
```

### Technical Requirements

- Add a generic LLM HTTP proxy path that can receive raw `axum` requests rather than JSON-only extractors.
- Keep existing admin/direct/tree/session routes exact and register generic catch-all after them.
- Resolve providers from path prefixes and common OpenAI-compatible paths.
- Decide and document auth behavior. Mirage forwards inbound auth; current cloakpipe uses only a server-side key. Multi-provider wrapper mode likely needs inbound auth pass-through.
- Apply request mutation recursively to content-bearing JSON fields, but skip protocol/auth/signature/encrypted fields.
- Use `Replacer::pseudonymize_with_strategy` so configured plausible fake mode works in HTTP proxy traffic.
- Implement response rehydration for complete textual bodies independent of provider-specific JSON shape.
- Replace current full-buffer SSE handling for the new mode with true byte streaming plus overlap-buffer rehydration.
- Add dry-run/shadow mode that logs detections and audit metadata while forwarding original traffic.
- Add provider bypass configuration.
- Preserve non-text and signed/encrypted payloads byte-for-byte.
- Add tests for routing, body mutation, skip behavior, dry-run, bypass, response rehydration, streaming boundary splits, and missing auth behavior.

## Recommended Approach

Add a new `llm-http` proxy mode inside `crates/cloakpipe-proxy`, reusing `cloakpipe-core` privacy primitives and preserving current OpenAI-compatible routes as the default/legacy behavior.

The best implementation path is to introduce dedicated proxy modules rather than expanding `handlers.rs` further:

- `crates/cloakpipe-proxy/src/routing.rs`
  - Provider prefix table and `resolve_provider` logic, modeled on Mirage but adapted to cloakpipe config.
  - Unit tests for explicit prefixes, OpenAI-compatible auto-routing, unknown paths, and query preservation.
- `crates/cloakpipe-proxy/src/llm_http.rs`
  - Raw request handler for generic LLM traffic.
  - Handles body bytes, textual detection, JSON mutation, dry-run, bypass, upstream forwarding, and response rehydration.
- `crates/cloakpipe-proxy/src/json_filter.rs`
  - Content-key allowlist and skip-key rules.
  - Keeps signed thinking blocks, `encrypted_content`, auth values, IDs, model parameters, and metadata intact.
- `crates/cloakpipe-proxy/src/streaming.rs`
  - Keep existing OpenAI SSE helper for legacy routes.
  - Add a provider-agnostic overlap-buffer stream rehydrator for `llm-http` mode.

Use `ProxyConfig.mode` as the feature switch, but make it typed and backward-compatible. Keep existing `"proxy"` semantics unchanged. Add a new value such as `"llm-http"` for catch-all multi-provider behavior. The current string default mismatch (`config.rs` default `"cloaktree"`, CLI default `"proxy"`) should be resolved as part of the config change.

Implement request mutation with `Replacer::pseudonymize_with_strategy(&text, &entities, &mut vault, state.config.proxy.masking_strategy)`. This matches Mirage’s “plausible fakes” goal while using existing cloakpipe generators and vault mappings. Full-response rehydration can use `Rehydrator::rehydrate`; streaming needs a new overlap-buffer mapping replacement path because `Rehydrator::rehydrate_chunk` is token-pattern-specific.

Implement provider auth as pass-through by default for `llm-http` mode. Keep server-side `api_key_env` for the existing fixed OpenAI route and optionally support provider-specific env fallback later. This avoids breaking wrappers and SDKs that already send provider credentials.

Do not add wrappers in the first production slice unless the task explicitly includes CLI setup. Mirage wrappers are useful evidence, but cloakpipe can first support configurable base URLs directly. A later CLI task can add `cloakpipe proxy wrappers install` for `OPENAI_BASE_URL` and `ANTHROPIC_BASE_URL`.

## Implementation Guidance

- **Objectives**: Add a generic HTTP proxy mode that watches LLM traffic, replaces secrets/PII in requests with configured cloakpipe pseudonyms or plausible fakes, restores mapped fakes in responses, and preserves provider protocol correctness.
- **Key Tasks**:
  - Add typed proxy mode config and optional fields: `dry_run`, `bypass`, provider routes, and auth behavior.
  - Add provider routing module with explicit prefixes and OpenAI-compatible fallbacks.
  - Build a new raw-body catch-all route that activates only in `llm-http` mode and does not shadow existing internal routes.
  - Add JSON content filtering that mutates content fields and skips auth, IDs, model config, signed thinking, encrypted content, and binary data.
  - Wire request mutation to `pseudonymize_with_strategy`.
  - Add provider-agnostic complete response rehydration over textual bodies.
  - Add provider-agnostic streaming rehydration with overlap buffering for similar fake strings.
  - Add dry-run/shadow logging and audit events without modifying traffic.
  - Add tests in `crates/cloakpipe-proxy/tests/privacy_api.rs` and unit tests beside new modules.
- **Dependencies**:
  - Already present: `axum`, `hyper`, `reqwest` with `stream`, `bytes`, `futures`, `async-stream`, `http-body-util`, `serde_json`, `regex`, `aes-gcm`, `tracing`, `uuid`, `anyhow`.
  - Add only if needed: `flate2` and `zstd` for compressed body mutation, `tokio-stream` for a Mirage-like `ReceiverStream`, `argon2` only if cloakpipe adds passphrase-derived vault keys, and `md5` only if adding a `/why` fingerprint endpoint.
  - Avoid `serde_yaml` unless a separate YAML config is explicitly desired; cloakpipe already uses TOML.
- **Target Files**:
  - `crates/cloakpipe-core/src/config.rs` for proxy mode/config fields.
  - `crates/cloakpipe-proxy/src/server.rs` for conditional router construction and catch-all registration.
  - `crates/cloakpipe-proxy/src/state.rs` for any new shared state such as bypass/provider route data or counters.
  - `crates/cloakpipe-proxy/src/routing.rs` for provider/base URL handling.
  - `crates/cloakpipe-proxy/src/llm_http.rs` for raw proxy request/response handling.
  - `crates/cloakpipe-proxy/src/streaming.rs` for provider-agnostic streaming rehydration.
  - `crates/cloakpipe-cli/src/commands.rs` only if new config defaults or CLI flags are required.
  - `docs/api.md` and `README.md` only in the implementation/doc task, not during research.
- **Tests**:
  - Add routing unit tests for prefix stripping and OpenAI-compatible auto-routing.
  - Add JSON filter tests for content-key mutation and skip-key preservation.
  - Add proxy integration tests with a local mock upstream axum server for non-stream request and response mutation.
  - Add dry-run tests proving original body reaches upstream while audit/log counters record detections.
  - Add bypass tests proving selected upstreams pass through without mutation or response rehydration.
  - Add streaming tests for fake values split across chunks, including plausible fake emails/secrets, not only `ORG_1`-style tokens.
  - Keep existing `privacy_api.rs` tests passing for direct endpoints and missing-key behavior.
- **Risks And Edge Cases**:
  - Current `Rehydrator::rehydrate_chunk` cannot safely restore arbitrary plausible fakes split across chunks.
  - Current `streaming.rs` buffers the whole response; new mode must avoid this for SSE latency.
  - Recursive JSON redaction can break tool schemas, model config, IDs, signatures, encrypted envelopes, and auth unless skip rules are conservative.
  - Similar fake replacements can collide with normal text; boundary-aware and longest-match replacement is required.
  - Provider APIs differ: OpenAI, Anthropic, Gemini, OpenRouter, and Codex have different request/response shapes. Provider-agnostic text-body replacement is safer than OpenAI-only JSON field rewriting for the new mode.
  - Inbound auth pass-through changes the trust model from current server-side-key proxy behavior; document it clearly.
  - Non-text bodies, multipart uploads, data URLs, and base64 blobs must be forwarded unchanged.
  - Compressed request/response support is optional but must either be implemented correctly or avoided by forcing `accept-encoding: identity` and forwarding compressed requests unmodified.
  - Vault lock contention can become visible under streaming if the lock is held per chunk. Prefer snapshotting reverse mappings per response when possible.
  - The workspace has no `.env`; implementation should not hardcode provider keys and should use placeholders if an implementation task permits creating `.env`.
- **Success Criteria**:
  - Existing proxy/direct API tests continue to pass.
  - New mode proxies at least OpenAI-compatible and Anthropic-prefixed requests through a local mock upstream.
  - Request bodies are modified when enforcement is enabled and unchanged in dry-run/shadow mode.
  - Responses restore fakes to originals for both complete and streaming responses.
  - Bypass and skip rules preserve signed/encrypted/binary content unchanged.
  - Config defaults are documented and backwards-compatible.