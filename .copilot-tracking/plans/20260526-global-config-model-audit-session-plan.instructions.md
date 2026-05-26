---
applyTo: ".copilot-tracking/changes/20260526-global-config-model-audit-session-changes.md"
---

<!-- markdownlint-disable-file -->

# Task Checklist: Global Config, Global Models, Cross-Surface Audit, And Global Sessions

## Overview

Implement hierarchical project/global configuration, global-only model storage, audit coverage for proxy/API/MCP, and startup global sessions for API and MCP.

## Objectives

- Add canonical global CloakPipe home handling with documented compatibility for the user-requested `cloackpipe` spelling.
- Make omitted config load the nearest project config from current directory or parents, then the global config.
- Bootstrap global config, policies, and model directories during install without overwriting user files.
- Store downloaded and default NER model files under the global models directory only.
- Make audit work across proxy, direct API, and MCP with surface and session metadata.
- Create and use a global session for direct API and MCP startup flows.

## Research Summary

### Project Files

- `crates/cloakpipe-cli/src/main.rs` - Current `--config` default prevents omitted-vs-explicit config detection.
- `crates/cloakpipe-cli/src/commands.rs` - Current config loading, local config creation, model download, MCP startup, and default config behavior.
- `crates/cloakpipe-cli/src/presets.rs` - Current preset config-home handling and `CLOAKPIPE_CONFIG_HOME` behavior.
- `install.sh` - Current installer preinstalls presets but does not create global config or model directories.
- `crates/cloakpipe-core/src/config.rs` - Current config structs, relative path defaults, audit config, and session config.
- `crates/cloakpipe-core/src/detector/distilbert_pii.rs` - Current DistilBERT model fallback and already-present chunking behavior.
- `crates/cloakpipe-audit/src/lib.rs` and `crates/cloakpipe-audit/src/sqlite.rs` - Current JSONL and SQLite audit implementations.
- `crates/cloakpipe-proxy/src/state.rs`, `crates/cloakpipe-proxy/src/handlers.rs`, and `crates/cloakpipe-proxy/src/server.rs` - Current API/proxy audit and session behavior.
- `crates/cloakpipe-mcp/src/lib.rs` - Current MCP tools, session state, and audit gap.

### External References

- #file:../research/20260526-global-config-model-audit-session-research.md - Verified project analysis and implementation guidance.
- #fetch:https://doc.rust-lang.org/std/path/struct.Path.html#method.ancestors - Standard parent traversal for project config discovery.
- #fetch:https://doc.rust-lang.org/std/env/fn.current_dir.html - Fallible current directory lookup for discovery startup.
- #fetch:https://doc.rust-lang.org/std/env/fn.var_os.html - Non-Unicode-assuming environment override lookup.
- #fetch:https://doc.rust-lang.org/std/fs/fn.create_dir_all.html - Recursive global directory creation.
- #fetch:https://doc.rust-lang.org/cargo/reference/config.html#hierarchical-structure - Hierarchical config precedent.
- #fetch:https://doc.rust-lang.org/cargo/reference/config.html#config-relative-paths - Config-relative path precedent.

### Standards References

- #file:../../.agents/skills/rust-best-practices/SKILL.md - Rust ownership, error handling, testing, and linting guidance.
- #file:../../.agents/skills/writing-clearly-and-concisely/SKILL.md - Clear technical prose guidance for docs and messages.

## Implementation Checklist

### [ ] Phase 1: Global Path And Config Discovery

- [ ] Task 1.1: Add shared global path helpers and naming compatibility

  - Details: .copilot-tracking/details/20260526-global-config-model-audit-session-details.md (Lines 11-30)

- [ ] Task 1.2: Replace implicit `--config` default with resolved config discovery

  - Details: .copilot-tracking/details/20260526-global-config-model-audit-session-details.md (Lines 31-52)

- [ ] Task 1.3: Normalize relative paths against the selected config file
  - Details: .copilot-tracking/details/20260526-global-config-model-audit-session-details.md (Lines 53-72)

### [ ] Phase 2: Global Bootstrap And Installer Integration

- [ ] Task 2.1: Ensure install/bootstrap creates global config and models directory

  - Details: .copilot-tracking/details/20260526-global-config-model-audit-session-details.md (Lines 75-93)

- [ ] Task 2.2: Define local override creation for `init` and `setup`
  - Details: .copilot-tracking/details/20260526-global-config-model-audit-session-details.md (Lines 94-110)

### [ ] Phase 3: Global Model Storage

- [ ] Task 3.1: Redirect DistilBERT downloads to the global models directory

  - Details: .copilot-tracking/details/20260526-global-config-model-audit-session-details.md (Lines 113-132)

- [ ] Task 3.2: Resolve default model paths globally for all NER backends

  - Details: .copilot-tracking/details/20260526-global-config-model-audit-session-details.md (Lines 133-153)

- [ ] Task 3.3: Update NER install/start tests for global runtime paths
  - Details: .copilot-tracking/details/20260526-global-config-model-audit-session-details.md (Lines 154-171)

### [ ] Phase 4: Cross-Surface Audit

- [ ] Task 4.1: Introduce an audit sink with enabled, backend, surface, and session support

  - Details: .copilot-tracking/details/20260526-global-config-model-audit-session-details.md (Lines 174-195)

- [ ] Task 4.2: Fill API and proxy audit gaps with surface-aware calls

  - Details: .copilot-tracking/details/20260526-global-config-model-audit-session-details.md (Lines 196-213)

- [ ] Task 4.3: Add MCP audit wiring and tool-level audit records
  - Details: .copilot-tracking/details/20260526-global-config-model-audit-session-details.md (Lines 214-231)

### [ ] Phase 5: API And MCP Global Sessions

- [ ] Task 5.1: Add a global session helper and create it at API/MCP startup

  - Details: .copilot-tracking/details/20260526-global-config-model-audit-session-details.md (Lines 234-252)

- [ ] Task 5.2: Use the global session in direct API and MCP pseudonymize flows

  - Details: .copilot-tracking/details/20260526-global-config-model-audit-session-details.md (Lines 253-272)

- [ ] Task 5.3: Update session context tests for the global session
  - Details: .copilot-tracking/details/20260526-global-config-model-audit-session-details.md (Lines 273-290)

### [ ] Phase 6: Tests, Documentation, And Validation

- [ ] Task 6.1: Add config discovery, bootstrap, and model path tests

  - Details: .copilot-tracking/details/20260526-global-config-model-audit-session-details.md (Lines 293-310)

- [ ] Task 6.2: Update user-facing documentation

  - Details: .copilot-tracking/details/20260526-global-config-model-audit-session-details.md (Lines 311-330)

- [ ] Task 6.3: Run workspace validation and fix regressions
  - Details: .copilot-tracking/details/20260526-global-config-model-audit-session-details.md (Lines 331-346)

## Dependencies

- Rust 2021 workspace with existing `anyhow`, `tokio`, `axum`, `rusqlite`, `serde`, `toml`, and detector dependencies.
- Existing `cloakpipe-core` config, detector, and session modules.
- Existing `cloakpipe-audit` JSONL and SQLite implementations.
- Existing `cloakpipe-proxy` direct API and proxy routes.
- Existing `cloakpipe-mcp` stdio server implementation.
- Temporary global-home environment overrides for tests.

## Success Criteria

- Omitted-config commands use nearest project config, then parent config, then global config.
- Explicit config paths and bundled preset names keep predictable behavior.
- Install/bootstrap creates global config, policies, and models directories without overwriting user config.
- Relative runtime paths resolve against the selected config file directory.
- NER model downloads and default model paths use the global models directory only.
- Audit works for proxy, direct API, and MCP with surface/session metadata and respects `audit.enabled`.
- API and MCP create a global session at startup and use it for pseudonymize flows.
- Workspace formatting and tests pass after implementation.