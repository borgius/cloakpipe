<!-- markdownlint-disable-file -->

# Task Details: LLM HTTP Proxy Mode

## Research Reference

**Source Research**: #file:../research/20260528-llm-http-proxy-mode-research.md

## Phase 1: Configuration and Routing Foundation

### Task 1.1: Add typed proxy mode and LLM proxy config

Introduce a backward-compatible typed proxy mode that keeps current `proxy` behavior unchanged and adds an opt-in `llm-http` mode for generic LLM traffic. Extend `[proxy]` with dry-run, bypass, provider-route, and auth-mode fields only as needed for this slice.

- **Files**:
  - `crates/cloakpipe-core/src/config.rs` - define typed proxy mode, parse string values, preserve existing TOML compatibility, and resolve the current default mismatch.
  - `crates/cloakpipe-cli/src/commands.rs` - update generated/default config only if the config change requires CLI default output changes.
  - `cloakpipe.toml` - update only if the implementation task explicitly wants the workspace sample to opt into `llm-http`.
- **Success**:
  - Existing configs with `mode = "proxy"` still load and preserve legacy fixed OpenAI-compatible routes.
  - `mode = "llm-http"` can be parsed from TOML and passed through `AppState` without stringly typed branching.
  - Dry-run/shadow, bypass, and auth-mode defaults are explicit and safe.
- **Research References**:
  - #file:../research/20260528-llm-http-proxy-mode-research.md (Lines 69-72) - current `ProxyConfig` fields and default mismatch.
  - #file:../research/20260528-llm-http-proxy-mode-research.md (Lines 365-375) - proposed `[proxy]` extension shape.
  - #file:../research/20260528-llm-http-proxy-mode-research.md (Lines 411-417) - mode switch, auth behavior, and wrapper deferral guidance.
- **Dependencies**:
  - Existing serde/TOML config loading.
  - No new runtime dependency required for this task.

### Task 1.2: Add provider routing module

Create a provider routing module that resolves explicit provider prefixes and OpenAI-compatible fallback paths while preserving path and query components. Start with OpenAI-compatible and Anthropic-prefixed traffic, then make the table easy to extend.

- **Files**:
  - `crates/cloakpipe-proxy/src/routing.rs` - new provider table, route resolution type, prefix stripping, fallback handling, and unit tests.
  - `crates/cloakpipe-proxy/src/lib.rs` - expose the new module if crate-level exports are needed.
- **Success**:
  - `/anthropic/v1/messages` resolves to Anthropic upstream with `/v1/messages` as the forwarded path.
  - `/v1/chat/completions`, `/responses`, `/chat/completions`, `/embeddings`, and `/models` resolve to OpenAI-compatible routes.
  - Unknown paths fail closed with a clear proxy error rather than silently forwarding to the wrong host.
- **Research References**:
  - #file:../research/20260528-llm-http-proxy-mode-research.md (Lines 191-196) - Mirage provider routing behavior.
  - #file:../research/20260528-llm-http-proxy-mode-research.md (Lines 377-382) - generic proxy routing requirements.
  - #file:../research/20260528-llm-http-proxy-mode-research.md (Lines 398-400) - recommended routing module and tests.
- **Dependencies**:
  - Task 1.1 config model if provider routes become configurable.

### Task 1.3: Register opt-in catch-all route without shadowing internal APIs

Add a raw-request catch-all route only when `proxy.mode = "llm-http"`. Register exact internal routes first so direct privacy, tree, session, and legacy OpenAI-compatible routes keep their current behavior.

- **Files**:
  - `crates/cloakpipe-proxy/src/server.rs` - branch router construction by typed proxy mode and append a catch-all after exact routes.
  - `crates/cloakpipe-proxy/src/state.rs` - add any shared mode/auth/bypass state only if direct config access is insufficient.
  - `crates/cloakpipe-proxy/src/llm_http.rs` - add the handler signature and minimal pass-through stub before request mutation work.
- **Success**:
  - Existing direct endpoints keep routing exactly as documented.
  - Legacy `POST /v1/chat/completions` and `POST /v1/embeddings` tests pass in `proxy` mode.
  - In `llm-http` mode, non-internal LLM paths reach the new raw handler.
- **Research References**:
  - #file:../research/20260528-llm-http-proxy-mode-research.md (Lines 51-65) - current explicit routes and streaming limitations.
  - #file:../research/20260528-llm-http-proxy-mode-research.md (Lines 243-251) - current proxy limitations and need to isolate new mode.
  - #file:../research/20260528-llm-http-proxy-mode-research.md (Lines 379-380) - catch-all route registration requirement.
- **Dependencies**:
  - Task 1.1 typed mode.
  - Task 1.2 routing module for meaningful forwarding.

## Phase 2: Request Inspection and Mutation

### Task 2.1: Add conservative JSON filter rules

Create a JSON filtering module that mutates only content-bearing fields and preserves protocol, auth, IDs, metadata, encrypted envelopes, signatures, and binary-like payloads. Keep rules conservative to avoid corrupting LLM tool schemas and provider control fields.

- **Files**:
  - `crates/cloakpipe-proxy/src/json_filter.rs` - new recursive JSON filter, content-key allowlist, skip-key list, signed-thinking skip, data URL/base64 guard, and unit tests.
  - `crates/cloakpipe-proxy/src/lib.rs` - expose the module only if needed.
- **Success**:
  - Strings under keys such as `content`, `text`, `messages`, `system`, `input`, `instructions`, `description`, `prompt`, and tool result keys are eligible for mutation.
  - Values under keys such as `authorization`, `api_key`, `token`, `session_id`, `model`, `stream`, `max_tokens`, `temperature`, `id`, `role`, `signature`, `encrypted_content`, `ciphertext`, and `metadata` remain byte-for-byte unchanged.
  - Anthropic signed thinking objects and non-text data URLs are skipped.
- **Research References**:
  - #file:../research/20260528-llm-http-proxy-mode-research.md (Lines 198-206) - Mirage request mutation and skip rules.
  - #file:../research/20260528-llm-http-proxy-mode-research.md (Lines 383-389) - request mutation and preservation requirements.
  - #file:../research/20260528-llm-http-proxy-mode-research.md (Lines 404-406) - recommended `json_filter.rs` module.
- **Dependencies**:
  - Existing `serde_json` support.

### Task 2.2: Wire request mutation to cloakpipe privacy primitives

Implement raw-body request handling for textual JSON and plain text. Detect entities, call `Replacer::pseudonymize_with_strategy`, and use the configured `proxy.masking_strategy` so `similar` mode produces plausible fakes.

- **Files**:
  - `crates/cloakpipe-proxy/src/llm_http.rs` - read body bytes, classify textual payloads, apply JSON/plain-text mutation, forward original bytes in dry-run, and preserve non-text requests unchanged.
  - `crates/cloakpipe-proxy/src/state.rs` - ensure the handler can access detector, vault, config, audit, and client safely.
  - `crates/cloakpipe-core/src/rehydrator.rs` - add helper accessors only if response rehydration needs reverse mapping snapshots.
- **Success**:
  - `llm-http` mode uses `pseudonymize_with_strategy` rather than legacy token-only `pseudonymize`.
  - Dry-run/shadow mode detects and records what would be changed but forwards the original request body.
  - Non-text, multipart, binary, and undecodable compressed requests pass through unchanged.
- **Research References**:
  - #file:../research/20260528-llm-http-proxy-mode-research.md (Lines 83-91) - cloakpipe replacement and rehydration primitives.
  - #file:../research/20260528-llm-http-proxy-mode-research.md (Lines 253-260) - core fit and chunk gap.
  - #file:../research/20260528-llm-http-proxy-mode-research.md (Lines 413-415) - strategy wiring and auth recommendation.
- **Dependencies**:
  - Task 2.1 JSON filter.
  - Existing `cloakpipe-core` detector, replacer, vault, and masking strategy APIs.

### Task 2.3: Forward headers and auth safely

Forward provider requests with hop-by-hop and body-size headers removed. Support pass-through auth by default in `llm-http` mode while preserving server-side `api_key_env` behavior for legacy fixed routes.

- **Files**:
  - `crates/cloakpipe-proxy/src/llm_http.rs` - implement header filtering, `accept-encoding: identity`, request method preservation, and upstream error mapping.
  - `docs/api.md` - document auth behavior during the implementation documentation phase.
- **Success**:
  - `Authorization`, provider-specific API-key headers, organization headers, and beta headers can pass through to the upstream.
  - `host`, `connection`, `transfer-encoding`, `content-length`, and inbound `accept-encoding` are not forwarded in unsafe form.
  - Legacy handlers still use configured server-side upstream API key behavior.
- **Research References**:
  - #file:../research/20260528-llm-http-proxy-mode-research.md (Lines 56-61) - current auth behavior and token-only masking.
  - #file:../research/20260528-llm-http-proxy-mode-research.md (Lines 177-189) - Mirage end-to-end forwarding pattern.
  - #file:../research/20260528-llm-http-proxy-mode-research.md (Lines 382-382) - auth behavior decision requirement.
- **Dependencies**:
  - Task 1.2 provider routing.
  - Task 2.2 request body handling.

## Phase 3: Response Rehydration and Streaming

### Task 3.1: Rehydrate complete textual responses provider-agnostically

For non-streaming responses, operate on textual response bodies rather than provider-specific JSON fields. Skip non-text and signed/encrypted responses, strip stale body-size headers, and use existing vault mappings to restore plausible fakes to originals.

- **Files**:
  - `crates/cloakpipe-proxy/src/llm_http.rs` - response classification, full-body rehydration, header cleanup, error mapping, and pass-through cases.
  - `crates/cloakpipe-core/src/rehydrator.rs` - adjust full-response mapping behavior only if similar-fake restoration requires boundary improvements.
- **Success**:
  - Complete text, JSON, NDJSON, XML, and JavaScript-like responses are eligible for rehydration.
  - Non-text, signed thinking, encrypted, and binary responses pass through unchanged.
  - `content-length` and `transfer-encoding` are removed when the body changes.
- **Research References**:
  - #file:../research/20260528-llm-http-proxy-mode-research.md (Lines 208-214) - Mirage complete and streaming response mutation.
  - #file:../research/20260528-llm-http-proxy-mode-research.md (Lines 385-386) - response rehydration and streaming requirements.
  - #file:../research/20260528-llm-http-proxy-mode-research.md (Lines 453-462) - streaming and collision risks.
- **Dependencies**:
  - Task 2.2 request mutation, because response rehydration depends on vault mappings created during request mutation.

### Task 3.2: Add provider-agnostic streaming rehydration with overlap buffering

Implement true byte streaming for `llm-http` responses. Rehydrate stream chunks using a safe overlap buffer so fake values split across chunks can be restored without buffering the whole upstream response.

- **Files**:
  - `crates/cloakpipe-proxy/src/streaming.rs` - keep legacy OpenAI SSE helper and add a provider-agnostic overlap-buffer stream helper for raw bytes.
  - `crates/cloakpipe-proxy/src/llm_http.rs` - choose streaming helper for `text/event-stream` and other chunked textual responses.
  - `crates/cloakpipe-core/src/rehydrator.rs` - add a mapping-snapshot or longest-match helper only if needed to avoid per-chunk vault locks.
- **Success**:
  - Streaming responses use `bytes_stream()` or equivalent and do not call `response.text().await` in the new path.
  - Fakes split across chunk boundaries are restored in tests.
  - Dry-run and compressed streams pass through safely if rehydration cannot be performed.
- **Research References**:
  - #file:../research/20260528-llm-http-proxy-mode-research.md (Lines 62-65) - current streaming helper buffers and token-only chunk behavior.
  - #file:../research/20260528-llm-http-proxy-mode-research.md (Lines 146-149) - Mirage streaming buffer behavior and edge case.
  - #file:../research/20260528-llm-http-proxy-mode-research.md (Lines 407-409) - recommended streaming module change.
- **Dependencies**:
  - Task 3.1 complete response rehydration.
  - Existing `reqwest` stream, `bytes`, `futures`, and `async-stream` dependencies.

## Phase 4: Tests, Documentation, and Validation

### Task 4.1: Add focused unit and integration tests

Add tests that lock down routing, JSON filtering, request mutation, dry-run, bypass, auth forwarding, complete-response rehydration, and streaming boundary restoration. Use local mock upstreams rather than real provider calls.

- **Files**:
  - `crates/cloakpipe-proxy/src/routing.rs` - route resolution unit tests.
  - `crates/cloakpipe-proxy/src/json_filter.rs` - JSON filter unit tests.
  - `crates/cloakpipe-proxy/tests/privacy_api.rs` - router-level tests with a local mock upstream.
  - `crates/cloakpipe-core/tests/integration.rs` - add rehydration helper coverage only if core helpers change.
- **Success**:
  - Existing direct API and legacy route tests continue to pass.
  - New tests prove `llm-http` mode modifies requests in enforce mode and preserves original bodies in dry-run mode.
  - Tests cover both complete and streaming responses, including plausible fake values split across chunks.
- **Research References**:
  - #file:../research/20260528-llm-http-proxy-mode-research.md (Lines 116-122) - current test locations and conventions.
  - #file:../research/20260528-llm-http-proxy-mode-research.md (Lines 445-452) - recommended test matrix.
  - #file:../research/20260528-llm-http-proxy-mode-research.md (Lines 464-470) - overall success criteria.
- **Dependencies**:
  - Phases 1 through 3 complete.

### Task 4.2: Update docs and run validation commands

Document the new mode, provider base URL examples, auth behavior, dry-run/shadow mode, bypass rules, and known limitations. Run the workspace checks needed for a Rust proxy change.

- **Files**:
  - `docs/api.md` - update route and mode documentation; keep current route source-of-truth style.
  - `README.md` - add a short user-facing section only if implementation scope includes README changes.
  - `.env` - create with placeholder provider key names only if implementation introduces required environment variables and no `.env` exists.
- **Success**:
  - Docs state that `llm-http` is opt-in and explain pass-through auth versus legacy server-side auth.
  - Validation includes targeted proxy/core tests, formatting, and clippy where feasible.
  - No real provider secrets, personal identifiers, or machine-specific values are hardcoded.
- **Research References**:
  - #file:../research/20260528-llm-http-proxy-mode-research.md (Lines 105-115) - current docs and workspace config.
  - #file:../research/20260528-llm-http-proxy-mode-research.md (Lines 136-138) - `.env` status and gitignore behavior.
  - #file:../research/20260528-llm-http-proxy-mode-research.md (Lines 432-435) - dependency guidance.
- **Dependencies**:
  - Task 4.1 tests complete enough to document verified behavior.

## Dependencies

- Rust 2021 workspace with existing `axum`, `reqwest`, `hyper`, `bytes`, `futures`, `async-stream`, `http-body-util`, `serde_json`, `tracing`, `anyhow`, and `uuid` dependencies.
- Existing cloakpipe detector, replacer, vault, rehydrator, session manager, and audit sink.
- Optional dependencies only if the implementation explicitly supports compressed mutation or Mirage-like channel streams: `flate2`, `zstd`, and possibly `tokio-stream`.

## Success Criteria

- `proxy.mode = "proxy"` preserves current behavior and tests.
- `proxy.mode = "llm-http"` proxies OpenAI-compatible and Anthropic-prefixed requests through a local mock upstream.
- Requests are pseudonymized with the configured masking strategy in enforce mode and left unchanged in dry-run/shadow mode.
- Complete and streaming responses restore mapped plausible fakes to originals.
- Bypass, non-text, signed, encrypted, and binary payloads pass through unchanged.
- Documentation and validation commands describe the new trust model and prove the implementation works without hardcoded secrets.