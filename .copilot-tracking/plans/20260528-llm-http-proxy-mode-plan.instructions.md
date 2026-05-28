---
applyTo: ".copilot-tracking/changes/20260528-llm-http-proxy-mode-changes.md"
---

<!-- markdownlint-disable-file -->

# Task Checklist: LLM HTTP Proxy Mode

## Overview

Add an opt-in generic LLM HTTP proxy mode that watches provider traffic, replaces secrets and PII in requests with cloakpipe mappings or plausible fakes, and restores mapped values in provider responses.

## Objectives

- Preserve the current fixed OpenAI-compatible proxy behavior for `proxy.mode = "proxy"`.
- Add `proxy.mode = "llm-http"` for raw provider traffic with request mutation, response rehydration, dry-run, bypass, and safe auth forwarding.
- Reuse cloakpipe detector, replacer, vault, session, and rehydrator primitives instead of duplicating Mirage internals.
- Support OpenAI-compatible and Anthropic-prefixed traffic through local mock upstream tests.
- Add provider-agnostic complete-response and streaming rehydration without corrupting signed, encrypted, binary, or non-text payloads.

## Research Summary

### Project Files

- `crates/cloakpipe-core/src/config.rs` - owns `ProxyConfig`, `mode`, `masking_strategy`, and detection config fields.
- `crates/cloakpipe-proxy/src/server.rs` - registers current exact HTTP routes and will own opt-in catch-all registration.
- `crates/cloakpipe-proxy/src/handlers.rs` - current fixed chat and embeddings proxy behavior to preserve.
- `crates/cloakpipe-proxy/src/streaming.rs` - current OpenAI SSE helper plus target location for provider-agnostic stream rehydration.
- `crates/cloakpipe-core/src/replacer.rs` - contains `pseudonymize_with_strategy`, the required request mutation entry point.
- `crates/cloakpipe-core/src/rehydrator.rs` - contains full-response rehydration and current token-specific chunk behavior.
- `crates/cloakpipe-proxy/tests/privacy_api.rs` - existing router-level proxy test location.

### External References

- #file:../research/20260528-llm-http-proxy-mode-research.md - validated task research with Mirage and cloakpipe findings.
- #fetch:https://github.com/chandika/mirage-proxy - Mirage README and repository overview for the proxy threat model and user-facing behavior.
- #fetch:https://github.com/chandika/mirage-proxy/tree/main/src - upstream source file list for Mirage implementation modules.
- #fetch:https://github.com/chandika/mirage-proxy/blob/main/src/proxy.rs - Mirage raw proxy flow, request mutation, forwarding, and response handling.
- #fetch:https://github.com/chandika/mirage-proxy/blob/main/src/providers.rs - Mirage provider prefix routing and OpenAI-compatible fallbacks.
- #fetch:https://github.com/chandika/mirage-proxy/blob/main/src/faker.rs - Mirage fake mapping and rehydration approach.
- #fetch:https://github.com/chandika/mirage-proxy/blob/main/src/vault.rs - Mirage encrypted session-scoped mapping persistence.
- #githubRepo:"chandika/mirage-proxy src/proxy.rs providers.rs faker.rs vault.rs session.rs config.rs main.rs" - upstream code search scope used in research.

### Standards References

- #file:/Users/admin/dev/cloakpipe/.agents/skills/rust-best-practices/SKILL.md - Rust ownership, errors, testing, and linting practices.
- #file:/Users/admin/.agents/skills/writing-clearly-and-concisely/SKILL.md - concise documentation and prompt writing standards.
- #file:../research/20260528-llm-http-proxy-mode-research.md (Lines 166-171) - verified local project conventions and test style.

## Implementation Checklist

### [ ] Phase 1: Configuration and Routing Foundation

- [ ] Task 1.1: Add typed proxy mode and LLM proxy config

  - Details: .copilot-tracking/details/20260528-llm-http-proxy-mode-details.md (Lines 11-29)

- [ ] Task 1.2: Add provider routing module

  - Details: .copilot-tracking/details/20260528-llm-http-proxy-mode-details.md (Lines 31-47)

- [ ] Task 1.3: Register opt-in catch-all route without shadowing internal APIs

  - Details: .copilot-tracking/details/20260528-llm-http-proxy-mode-details.md (Lines 49-67)

### [ ] Phase 2: Request Inspection and Mutation

- [ ] Task 2.1: Add conservative JSON filter rules

  - Details: .copilot-tracking/details/20260528-llm-http-proxy-mode-details.md (Lines 71-87)

- [ ] Task 2.2: Wire request mutation to cloakpipe privacy primitives

  - Details: .copilot-tracking/details/20260528-llm-http-proxy-mode-details.md (Lines 89-107)

- [ ] Task 2.3: Forward headers and auth safely

  - Details: .copilot-tracking/details/20260528-llm-http-proxy-mode-details.md (Lines 109-126)

### [ ] Phase 3: Response Rehydration and Streaming

- [ ] Task 3.1: Rehydrate complete textual responses provider-agnostically

  - Details: .copilot-tracking/details/20260528-llm-http-proxy-mode-details.md (Lines 130-146)

- [ ] Task 3.2: Add provider-agnostic streaming rehydration with overlap buffering

  - Details: .copilot-tracking/details/20260528-llm-http-proxy-mode-details.md (Lines 148-166)

### [ ] Phase 4: Tests, Documentation, and Validation

- [ ] Task 4.1: Add focused unit and integration tests

  - Details: .copilot-tracking/details/20260528-llm-http-proxy-mode-details.md (Lines 170-188)

- [ ] Task 4.2: Update docs and run validation commands

  - Details: .copilot-tracking/details/20260528-llm-http-proxy-mode-details.md (Lines 190-207)

## Dependencies

- Rust 2021 workspace and existing proxy/core crates.
- Existing dependencies: `axum`, `reqwest`, `hyper`, `bytes`, `futures`, `async-stream`, `http-body-util`, `serde_json`, `tracing`, `anyhow`, and `uuid`.
- Existing cloakpipe detector, replacer, vault, rehydrator, session manager, and audit sink.
- Optional dependencies only if implementation chooses compressed mutation or Mirage-like channel streams: `flate2`, `zstd`, and possibly `tokio-stream`.

## Success Criteria

- Existing direct API, tree, session, chat, and embeddings tests continue to pass in legacy `proxy` mode.
- `llm-http` mode proxies OpenAI-compatible and Anthropic-prefixed requests through local mock upstreams.
- Request bodies are modified when enforcement is enabled and unchanged in dry-run/shadow mode.
- Complete and streaming responses restore plausible fake values to originals.
- Bypass rules and skip guards preserve signed, encrypted, binary, multipart, and non-text payloads unchanged.
- Docs describe mode selection, auth behavior, provider base URLs, dry-run/shadow behavior, and known limitations without hardcoded secrets.