<!-- markdownlint-disable-file -->

# LLM HTTP Proxy Mode Changes

## Status Checklist

- [x] Phase 1: Configuration and Routing Foundation
  - [x] Task 1.1: Add typed proxy mode and LLM proxy config
  - [x] Task 1.2: Add provider routing module
  - [x] Task 1.3: Register opt-in catch-all route without shadowing internal APIs
- [x] Phase 2: Request Inspection and Mutation
  - [x] Task 2.1: Add conservative JSON filter rules
  - [x] Task 2.2: Wire request mutation to cloakpipe privacy primitives
  - [x] Task 2.3: Forward headers and auth safely
- [x] Phase 3: Response Rehydration and Streaming
  - [x] Task 3.1: Rehydrate complete textual responses provider-agnostically
  - [x] Task 3.2: Add provider-agnostic streaming rehydration with overlap buffering
- [x] Phase 4: Tests, Documentation, and Validation
  - [x] Task 4.1: Add focused unit and integration tests
  - [x] Task 4.2: Update docs and run validation commands

## Task Log

- Loaded the planning, detail, and research artifacts plus the relevant proxy, core, CLI, docs, and test files before implementation.
- Added typed proxy configuration for `proxy.mode`, `proxy.auth_mode`, `proxy.dry_run`, `proxy.bypass`, and `proxy.provider_routes`, updated CLI defaults, and preserved backward compatibility by accepting legacy `mode = "cloaktree"` values.
- Added opt-in raw `llm-http` routing with provider resolution for OpenAI-compatible and Anthropic-prefixed traffic while preserving legacy `proxy` mode behavior.
- Implemented conservative JSON and text mutation rules that skip signed thinking blocks, encrypted envelopes, binary-like payloads, auth/config fields, IDs, and metadata.
- Reused cloakpipe detector, replacer, vault, session, audit, and rehydrator primitives for request mutation, complete-response rehydration, and overlap-buffer streaming rehydration.
- Added local mock upstream coverage for routing, auth forwarding, dry-run, bypass, full-response rehydration, and chunk-boundary streaming restoration.
- Updated API docs for proxy modes, auth behavior, catch-all routing, and llm-http limitations.
- Fixed bundled default policy/preset mode drift by synchronizing `policies/default.toml` and `crates/cloakpipe-cli/presets/default.toml` back to `mode = "proxy"`.
- Resolved validation-driven lint and formatting issues in touched Rust crates so the final targeted test, fmt, and clippy bundle completes cleanly.

## Files Changed

- `.copilot-tracking/changes/20260528-llm-http-proxy-mode-changes.md` - implementation tracking log for this task.
- `crates/cloakpipe-core/src/config.rs` - typed proxy config, defaults, aliases, and serialization/deserialization tests.
- `crates/cloakpipe-cli/src/commands.rs` - default config output for the new proxy settings.
- `policies/default.toml` - synced bundled default policy mode.
- `crates/cloakpipe-cli/presets/default.toml` - synced embedded default preset mode.
- `crates/cloakpipe-proxy/src/lib.rs` - module exports for new llm-http support code.
- `crates/cloakpipe-proxy/src/server.rs` - router branching between legacy `proxy` mode and opt-in `llm-http` catch-all routing.
- `crates/cloakpipe-proxy/src/state.rs` - shared helpers for auth behavior, upstream key handling, and bypass checks.
- `crates/cloakpipe-proxy/src/routing.rs` - provider resolution for OpenAI-compatible and Anthropic-prefixed traffic.
- `crates/cloakpipe-proxy/src/json_filter.rs` - conservative request/response JSON mutation guards.
- `crates/cloakpipe-proxy/src/llm_http.rs` - raw multi-provider llm-http proxy implementation.
- `crates/cloakpipe-proxy/src/streaming.rs` - provider-agnostic text streaming rehydration with overlap buffering.
- `crates/cloakpipe-proxy/tests/privacy_api.rs` - mock-upstream llm-http integration coverage.
- `docs/api.md` - user-facing documentation for proxy modes and llm-http routing.
- `crates/cloakpipe-audit/src/lib.rs` - justified clippy expectations for wide audit-entry helper signatures.
- `crates/cloakpipe-core/src/detector/distilbert_pii.rs` - lint-driven helper simplification during validation.
- `crates/cloakpipe-core/src/format_preserving.rs` - lint-driven URL helper cleanup during validation.
- `crates/cloakpipe-core/src/rehydrator.rs` - lint-driven boundary helper cleanup during validation.

## Validation Log

- `cargo test -p cloakpipe-core` ✅
  - 112 core unit tests passed.
  - 22 core integration tests passed.
  - 0 doc tests failed.
- `cargo test -p cloakpipe-proxy` ✅
  - 8 proxy unit tests passed.
  - 12 `privacy_api` integration tests passed.
  - 0 doc tests failed.
- `cargo fmt --all --check` ✅
- `cargo clippy -p cloakpipe-core -p cloakpipe-proxy --all-targets -- -D warnings` ✅
- Notable debug detour resolved: bundled policy parsing initially failed on legacy `mode = "cloaktree"`; fixed by syncing the bundled defaults back to `proxy` and adding a `ProxyMode::Proxy` serde alias for older configs.

## Implementation Decisions

- Preserve legacy `proxy` mode behavior and isolate the new catch-all behavior behind typed `proxy.mode = "llm-http"`.
- Prefer pass-through auth for `llm-http` mode; keep server-side `api_key_env` behavior for legacy fixed OpenAI-compatible routes.
- Resolve OpenAI-compatible routes from `proxy.upstream` and Anthropic-prefixed routes from configurable provider routes so custom/local upstreams keep working.
- Mutate only conservative text-bearing fields and never touch signed thinking blocks, encrypted envelopes, binary bodies, multipart uploads, non-text data URLs, auth fields, model config, IDs, or metadata.
- In dry-run mode, inspect and audit bodies but forward the original request unchanged; in bypass mode, skip both mutation and response rehydration.
- Reuse cloakpipe core detector, replacer, vault, session, audit, and rehydrator primitives instead of copying Mirage internals.

## References

- Plan: `.copilot-tracking/plans/20260528-llm-http-proxy-mode-plan.instructions.md`
- Details: `.copilot-tracking/details/20260528-llm-http-proxy-mode-details.md`
- Research: `.copilot-tracking/research/20260528-llm-http-proxy-mode-research.md`
