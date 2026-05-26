<!-- markdownlint-disable-file -->

# Task Details: Global Config, Global Models, Cross-Surface Audit, And Global Sessions

## Research Reference

**Source Research**: #file:../research/20260526-global-config-model-audit-session-research.md

## Phase 1: Global Path And Config Discovery

### Task 1.1: Add shared global path helpers and naming compatibility

Create a single global-path contract for CloakPipe. Use canonical `~/.cloakpipe` and `cloakpipe.toml` names because the repository consistently uses `cloakpipe`; add explicit compatibility handling for the user-requested spellings `~/.cloackpipe` and `cloackpipe.toml` only if the implementation can do so without ambiguity.

- **Files**:
  - `crates/cloakpipe-core/src/paths.rs` - New shared path helper module for global home, global config path, global models directory, project config names, and config-relative path normalization helpers.
  - `crates/cloakpipe-core/src/lib.rs` - Export the new path helper module.
  - `crates/cloakpipe-cli/src/presets.rs` - Replace private config-home path logic with the shared global helper while preserving `CLOAKPIPE_CONFIG_HOME` compatibility for existing preset tests.
- **Success**:
  - Global home resolution supports a test override such as `CLOAKPIPE_HOME`, keeps `CLOAKPIPE_CONFIG_HOME` compatibility, and falls back to `~/.cloakpipe`.
  - Helper functions return `PathBuf` and use fallible `Result` paths instead of panics.
  - The code documents the `cloackpipe` spelling from the request as an alias or as an intentional non-canonical spelling.
- **Research References**:
  - #file:../research/20260526-global-config-model-audit-session-research.md (Lines 18-24) - Current preset and installer global-directory behavior.
  - #file:../research/20260526-global-config-model-audit-session-research.md (Lines 79-88) - Spelling decision, parent traversal, env, directory creation, and Cargo config precedent.
  - #file:../research/20260526-global-config-model-audit-session-research.md (Lines 143-155) - Recommended global path helper shape.
- **Dependencies**:
  - Existing `anyhow` usage in the workspace.
  - Rust `std::env::var_os`, `std::fs::create_dir_all`, and `PathBuf` APIs.

### Task 1.2: Replace implicit `--config` default with resolved config discovery

Change CLI config handling so omitted `--config` triggers discovery from the current directory through each parent. If no project config exists, load the global config. Keep explicit `--config` behavior for exact paths and bundled presets.

- **Files**:
  - `crates/cloakpipe-cli/src/main.rs` - Change the global config argument from a defaulted `String` to an optional explicit argument so command handlers can distinguish omitted and explicit config values.
  - `crates/cloakpipe-cli/src/commands.rs` - Replace `resolve_config_path`, `load_config`, and `load_config_or_default` call sites with a resolved-config API that returns config, source path, source kind, and base directory.
  - `crates/cloakpipe-cli/src/presets.rs` - Keep bundled preset resolution for explicit config names such as `dpdp.toml`.
- **Success**:
  - Omitted config checks nearest `cloakpipe.toml` in current directory and parent directories before global fallback.
  - Explicit `--config path/to/file.toml` loads that path and reports a clear error if it is missing.
  - Explicit `--config dpdp.toml` still resolves installed or bundled presets.
  - `start` no longer creates a local `cloakpipe.toml` simply because the default config argument was omitted.
- **Research References**:
  - #file:../research/20260526-global-config-model-audit-session-research.md (Lines 9-17) - Current CLI config default and command behavior.
  - #file:../research/20260526-global-config-model-audit-session-research.md (Lines 73-79) - Config discovery and spelling findings.
  - #file:../research/20260526-global-config-model-audit-session-research.md (Lines 126-140) - Recommended resolved config shape.
  - #file:../research/20260526-global-config-model-audit-session-research.md (Lines 201-205) - Technical requirements for config discovery.
- **Dependencies**:
  - Task 1.1 completion.
  - Existing preset installation and resolution tests.

### Task 1.3: Normalize relative paths against the selected config file

Normalize runtime paths after config load and before constructing vaults, audit sinks, detectors, tree storage, and local/vector components. Relative values in project config resolve against the project config directory; relative values in global config resolve under the global config directory.

- **Files**:
  - `crates/cloakpipe-cli/src/commands.rs` - Apply normalization in the resolved-config load path before returning config to command handlers.
  - `crates/cloakpipe-core/src/config.rs` - Add helper methods only if the normalization belongs next to the config structs; otherwise keep structs unchanged and document external normalization.
  - `crates/cloakpipe-core/src/detector/distilbert_pii.rs`, `crates/cloakpipe-core/src/detector/gliner.rs`, `crates/cloakpipe-core/src/detector/ner.rs` - Ensure detector constructors receive normalized model paths when config omits or supplies relative paths.
- **Success**:
  - `vault.path`, `audit.log_path`, `tree.storage_path`, local vector/database paths, and `detection.ner.model` no longer depend on process current directory.
  - Config-relative path behavior matches the documented Cargo precedent.
  - Existing serialized config format remains compatible.
- **Research References**:
  - #file:../research/20260526-global-config-model-audit-session-research.md (Lines 25-32) - Current config metadata gap and relative detector defaults.
  - #file:../research/20260526-global-config-model-audit-session-research.md (Lines 83-88) - External path traversal and config-relative precedent.
  - #file:../research/20260526-global-config-model-audit-session-research.md (Lines 168-174) - Additive schema and optional NER model guidance.
  - #file:../research/20260526-global-config-model-audit-session-research.md (Lines 201-208) - Technical requirements for path normalization and global models.
- **Dependencies**:
  - Task 1.2 completion.

## Phase 2: Global Bootstrap And Installer Integration

### Task 2.1: Ensure install/bootstrap creates global config and models directory

Extend the existing install path so installation creates the global home directory, `models/`, `policies/`, and a default global `cloakpipe.toml` without overwriting an existing user config.

- **Files**:
  - `install.sh` - Keep binary installation intact and call a CLI bootstrap path that creates the global layout.
  - `crates/cloakpipe-cli/src/commands.rs` - Add a global bootstrap helper or command path that writes the default global config and creates the global models directory.
  - `crates/cloakpipe-cli/src/presets.rs` - Ensure bundled preset installation and global layout creation share the same home directory.
- **Success**:
  - Fresh install creates `~/.cloakpipe/models`, `~/.cloakpipe/policies`, and `~/.cloakpipe/cloakpipe.toml`.
  - Existing global config files are preserved.
  - Installer output states where the global config and presets were installed.
- **Research References**:
  - #file:../research/20260526-global-config-model-audit-session-research.md (Lines 18-24) - Current presets and installer behavior.
  - #file:../research/20260526-global-config-model-audit-session-research.md (Lines 176-197) - Global config example.
  - #file:../research/20260526-global-config-model-audit-session-research.md (Lines 201-206) - Bootstrap and discovery requirements.
- **Dependencies**:
  - Phase 1 global path helpers.

### Task 2.2: Define local override creation for `init` and `setup`

Keep `init` and interactive `setup` useful for project-level overrides. They should write local project config only when the user asks for local initialization and should not conflict with global bootstrap.

- **Files**:
  - `crates/cloakpipe-cli/src/commands.rs` - Update `init`, `setup`, and start-up messages to distinguish global fallback from project override creation.
  - `README.md` - Document how a project `cloakpipe.toml` overrides the global config.
- **Success**:
  - `cloakpipe init` and `cloakpipe setup` behavior is documented as project-local override creation.
  - App startup reports whether it loaded explicit, project, global, or preset config.
- **Research References**:
  - #file:../research/20260526-global-config-model-audit-session-research.md (Lines 12-17) - Current local config creation behavior.
  - #file:../research/20260526-global-config-model-audit-session-research.md (Lines 214-220) - Recommended no-merge override model.
- **Dependencies**:
  - Task 1.2 completion.
  - Task 2.1 global bootstrap behavior.

## Phase 3: Global Model Storage

### Task 3.1: Redirect DistilBERT downloads to the global models directory

Update model download behavior so DistilBERT files are written only under the global models directory. Avoid source-checkout-local `models/` writes during normal CLI use.

- **Files**:
  - `crates/cloakpipe-cli/src/commands.rs` - Update `download_distilbert_pii` to target the global model directory and avoid treating the repository root as the model destination.
  - `tools/download_model.sh` - Accept a target directory or `CLOAKPIPE_HOME`/`CLOAKPIPE_MODEL_DIR` override and default to the global models path.
  - `Taskfile.yaml` - Update clean semantics if they still remove repository-local `models/` as a runtime artifact.
- **Success**:
  - `cloakpipe ner download` writes DistilBERT model files below the global models directory.
  - The script no longer writes runtime models to repository-local `models/` unless explicitly invoked for development.
  - Existing model conversion fallback uses the global or managed runtime directory, not project-local `.cloakpipe` by default.
- **Research References**:
  - #file:../research/20260526-global-config-model-audit-session-research.md (Lines 29-39) - Current model defaults and download destinations.
  - #file:../research/20260526-global-config-model-audit-session-research.md (Lines 70-71) - Tests that currently assert project-local GLiNER state.
  - #file:../research/20260526-global-config-model-audit-session-research.md (Lines 201-208) - Global model storage requirements.
- **Dependencies**:
  - Phase 1 global path helpers.
  - Phase 2 global models directory creation.

### Task 3.2: Resolve default model paths globally for all NER backends

Set or resolve model paths for DistilBERT PII, GLiNER, and BERT NER under the global models directory when `detection.ner.model` is omitted. Preserve explicitly configured absolute paths.

- **Files**:
  - `crates/cloakpipe-core/src/detector/distilbert_pii.rs` - Replace repository-local fallback with normalized global model fallback.
  - `crates/cloakpipe-core/src/detector/gliner.rs` - Replace `models/gliner.onnx` fallback with normalized global model fallback.
  - `crates/cloakpipe-core/src/detector/ner.rs` - Replace `models/bert-ner.onnx` fallback with normalized global model fallback.
  - `crates/cloakpipe-cli/src/commands.rs` - Ensure config normalization sets `detection.ner.model` consistently before detector construction where possible.
- **Success**:
  - Omitted model paths never resolve to repository-local `models/` in application mode.
  - Relative model paths in project configs still resolve relative to the project config file.
  - DistilBERT long-input chunking and phone-fragment filtering remain unchanged.
- **Research References**:
  - #file:../research/20260526-global-config-model-audit-session-research.md (Lines 29-36) - Current detector default model paths.
  - #file:../research/20260526-global-config-model-audit-session-research.md (Lines 75-79) - Model default path discovery and spelling compatibility.
  - #file:../research/20260526-global-config-model-audit-session-research.md (Lines 168-174) - Optional NER model normalization guidance.
- **Dependencies**:
  - Task 1.3 completion.
  - Task 3.1 completion.

### Task 3.3: Update NER install/start tests for global runtime paths

Adjust CLI tests that currently expect project-local `.cloakpipe` or repository model state so they use temporary global homes.

- **Files**:
  - `crates/cloakpipe-cli/tests/test_ner_install.rs` - Assert global GLiNER virtualenv/model messages and files using a temporary `CLOAKPIPE_HOME`.
  - `crates/cloakpipe-cli/tests/test_ner_start.rs` - Assert GLiNER start uses global managed runtime state or clearly documented source-checkout server script behavior.
  - `crates/cloakpipe-cli/tests/test_presets.rs` - Preserve compatibility with `CLOAKPIPE_CONFIG_HOME` and add coverage for `CLOAKPIPE_HOME` if introduced.
- **Success**:
  - NER tests do not touch the developer's real home directory.
  - Tests prove model and virtualenv artifacts are global, not project-local.
- **Research References**:
  - #file:../research/20260526-global-config-model-audit-session-research.md (Lines 65-71) - Existing tests and project-local expectations.
  - #file:../research/20260526-global-config-model-audit-session-research.md (Lines 222-226) - Overall implementation guidance and success criteria.
- **Dependencies**:
  - Task 3.1 completion.
  - Task 3.2 completion.

## Phase 4: Cross-Surface Audit

### Task 4.1: Introduce an audit sink with enabled, backend, surface, and session support

Replace direct JSONL-only construction with a shared audit sink that can represent disabled audit, JSONL audit, and SQLite audit. Add optional surface, session ID, and user ID metadata to audit entries.

- **Files**:
  - `crates/cloakpipe-audit/src/lib.rs` - Extend `AuditEntry`, add context-aware log methods, preserve existing simple methods as wrappers, and add a disabled/no-op path.
  - `crates/cloakpipe-audit/src/sqlite.rs` - Add surface support and make SQLite usable through the shared sink.
  - `crates/cloakpipe-proxy/src/state.rs` - Store a shared audit sink rather than a concrete JSONL logger.
  - `crates/cloakpipe-cli/src/commands.rs` - Construct audit from `AuditConfig` for proxy/API and MCP startup.
- **Success**:
  - `audit.enabled = false` prevents audit writes across all surfaces.
  - JSONL entries include `surface` and `session_id` when available.
  - SQLite audit stores equivalent surface/session metadata or has documented parity tests.
  - Existing call sites compile through compatibility wrappers during migration.
- **Research References**:
  - #file:../research/20260526-global-config-model-audit-session-research.md (Lines 40-46) - Current JSONL and SQLite audit state.
  - #file:../research/20260526-global-config-model-audit-session-research.md (Lines 77-78) - Audit wiring gaps.
  - #file:../research/20260526-global-config-model-audit-session-research.md (Lines 111-112) - Recommended shared audit sink pattern.
  - #file:../research/20260526-global-config-model-audit-session-research.md (Lines 158-170) - Recommended audit context and additive schema.
- **Dependencies**:
  - Task 1.3 normalized audit log path.

### Task 4.2: Fill API and proxy audit gaps with surface-aware calls

Update proxy handlers so all direct API endpoints and proxy endpoints log through the shared audit sink with `surface = "api"` or `surface = "proxy"`.

- **Files**:
  - `crates/cloakpipe-proxy/src/handlers.rs` - Add context-aware audit calls to direct API pseudonymize, rehydrate, detect, configure, vault stats, session context, proxy chat, and proxy embeddings where appropriate.
  - `crates/cloakpipe-proxy/tests/privacy_api.rs` - Add assertions for API audit entries, surface values, and session IDs.
- **Success**:
  - Direct API pseudonymize and rehydrate keep their existing audit semantics and gain surface/session metadata.
  - Direct API detect/configure/vault/session endpoints produce safe metadata audit records without logging raw PII unless `log_entities` allows it.
  - Proxy chat and embeddings log `surface = "proxy"` and include header-derived session IDs when present.
- **Research References**:
  - #file:../research/20260526-global-config-model-audit-session-research.md (Lines 50-57) - Current API/proxy routes and audit/session gaps.
  - #file:../research/20260526-global-config-model-audit-session-research.md (Lines 168-172) - Audit schema and session stats safety.
  - #file:../research/20260526-global-config-model-audit-session-research.md (Lines 207-210) - Audit technical requirements.
- **Dependencies**:
  - Task 4.1 completion.

### Task 4.3: Add MCP audit wiring and tool-level audit records

Pass the shared audit sink into MCP server construction and log MCP tool activity with `surface = "mcp"`.

- **Files**:
  - `crates/cloakpipe-mcp/src/lib.rs` - Add audit state, update `CloakPipeServer::new`, update `serve_stdio`, and log pseudonymize, rehydrate, detect, configure, and session context tool calls.
  - `crates/cloakpipe-cli/src/commands.rs` - Create the audit sink in the `mcp` command and pass it to `cloakpipe_mcp::serve_stdio`.
  - MCP tests in `crates/cloakpipe-mcp/src/lib.rs` or a new test module - Verify MCP audit records include `surface = "mcp"` and global session metadata where relevant.
- **Success**:
  - MCP uses the same `audit.enabled`, `audit.backend`, `audit.log_path`, and `audit.log_entities` settings as proxy/API.
  - MCP audit entries include safe operation metadata even for detect/configure calls.
- **Research References**:
  - #file:../research/20260526-global-config-model-audit-session-research.md (Lines 58-61) - Current MCP audit and session gaps.
  - #file:../research/20260526-global-config-model-audit-session-research.md (Lines 111-113) - Audit sink and global session patterns.
  - #file:../research/20260526-global-config-model-audit-session-research.md (Lines 207-211) - Cross-surface audit and global session requirements.
- **Dependencies**:
  - Task 4.1 completion.

## Phase 5: API And MCP Global Sessions

### Task 5.1: Add a global session helper and create it at API/MCP startup

Add a single global session identifier and ensure API and MCP constructors create the session on startup.

- **Files**:
  - `crates/cloakpipe-core/src/session.rs` - Add `GLOBAL_SESSION_ID` and a helper such as `ensure_global_session`.
  - `crates/cloakpipe-proxy/src/state.rs` - Call the helper from `AppState::new` so direct API routes have a global session immediately.
  - `crates/cloakpipe-mcp/src/lib.rs` - Call the helper from `CloakPipeServer::new`.
- **Success**:
  - API and MCP startup create the global session even before the first request or tool call.
  - The helper has documented behavior when `session.enabled = false`; the plan recommends creating the session but letting callers decide whether to use coreference features.
- **Research References**:
  - #file:../research/20260526-global-config-model-audit-session-research.md (Lines 47-64) - Current proxy/MCP/session startup state.
  - #file:../research/20260526-global-config-model-audit-session-research.md (Lines 113-114) - Recommended global session pattern.
  - #file:../research/20260526-global-config-model-audit-session-research.md (Lines 170-172) - Existing safe session response shapes.
  - #file:../research/20260526-global-config-model-audit-session-research.md (Lines 209-211) - Global session requirements.
- **Dependencies**:
  - No source dependency beyond existing session manager.

### Task 5.2: Use the global session in direct API and MCP pseudonymize flows

Route direct API and MCP pseudonymize operations through the global session so entity memory, coreference resolution, and audit session metadata work consistently outside proxy chat.

- **Files**:
  - `crates/cloakpipe-proxy/src/handlers.rs` - Use the global session ID in direct API pseudonymize and include it in audit context.
  - `crates/cloakpipe-mcp/src/lib.rs` - Use the global session ID in MCP pseudonymize and include it in audit context.
  - `crates/cloakpipe-core/src/session.rs` - Add helper methods only if needed to avoid duplicated session code in API and MCP.
- **Success**:
  - Direct API pseudonymize records detected entities and generated tokens in the global session.
  - MCP pseudonymize records detected entities and generated tokens in the global session.
  - Rehydrate and detect audit records include the global session ID when they operate in API or MCP global context.
- **Research References**:
  - #file:../research/20260526-global-config-model-audit-session-research.md (Lines 53-61) - Direct API and MCP pseudonymize session gaps.
  - #file:../research/20260526-global-config-model-audit-session-research.md (Lines 113-114) - Global session pattern.
  - #file:../research/20260526-global-config-model-audit-session-research.md (Lines 207-211) - Audit and session requirements.
- **Dependencies**:
  - Task 5.1 completion.
  - Task 4.2 and Task 4.3 audit context APIs.

### Task 5.3: Update session context tests for the global session

Update API and MCP tests so global session visibility is explicit and intentional.

- **Files**:
  - `crates/cloakpipe-proxy/tests/privacy_api.rs` - Assert `/session_context` can list or inspect the global session after startup and after API pseudonymize.
  - `crates/cloakpipe-mcp/src/lib.rs` tests or new MCP tests - Assert MCP `session_context` returns the global session after startup and shows entity counts after pseudonymize.
- **Success**:
  - Tests no longer rely only on manually inserted sessions for API context coverage.
  - Global session behavior is stable whether `session.enabled` is true or false according to the documented choice from Task 5.1.
- **Research References**:
  - #file:../research/20260526-global-config-model-audit-session-research.md (Lines 62-67) - Current session manager behavior and tests.
  - #file:../research/20260526-global-config-model-audit-session-research.md (Lines 170-172) - Existing safe session response shapes.
  - #file:../research/20260526-global-config-model-audit-session-research.md (Lines 222-226) - Overall success criteria.
- **Dependencies**:
  - Task 5.1 completion.
  - Task 5.2 completion.

## Phase 6: Tests, Documentation, And Validation

### Task 6.1: Add config discovery, bootstrap, and model path tests

Add focused tests for config precedence, global bootstrap, path normalization, and global model storage.

- **Files**:
  - `crates/cloakpipe-cli/tests/test_presets.rs` - Add global bootstrap and `CLOAKPIPE_HOME` coverage while keeping `CLOAKPIPE_CONFIG_HOME` compatibility.
  - New or existing CLI tests - Add omitted-config project discovery, parent discovery, global fallback, explicit config, preset, and config-relative path cases.
  - `crates/cloakpipe-cli/tests/test_ner_install.rs` and `crates/cloakpipe-cli/tests/test_ner_start.rs` - Cover global model/runtime paths.
- **Success**:
  - Tests use temporary global homes and never touch the real user home.
  - Discovery tests prove the precedence order: explicit config, nearest project config, parent config, global config.
  - Model tests prove runtime artifacts stay under the global directory.
- **Research References**:
  - #file:../research/20260526-global-config-model-audit-session-research.md (Lines 65-71) - Existing test coverage and project-local assumptions.
  - #file:../research/20260526-global-config-model-audit-session-research.md (Lines 201-211) - Config, model, audit, and session technical requirements.
- **Dependencies**:
  - Phases 1 through 5 as applicable.

### Task 6.2: Update user-facing documentation

Document the new global/project configuration model, global model directory, audit coverage, and API/MCP global sessions.

- **Files**:
  - `README.md` - Add global config location, project override discovery, model storage, and startup behavior.
  - `docs/api.md` - Document API audit/session behavior and global session context.
  - `docs/mcp.md` - Document MCP audit/session behavior and global session context.
  - `policies/README.md` - Note where bundled presets are installed globally.
- **Success**:
  - Docs explicitly mention canonical `~/.cloakpipe/cloakpipe.toml` and explain any `cloackpipe` compatibility alias.
  - Docs explain that project config overrides global config by presence and does not merge with it.
  - Docs state models download to the global models directory only.
- **Research References**:
  - #file:../research/20260526-global-config-model-audit-session-research.md (Lines 79-88) - Spelling and external config precedence findings.
  - #file:../research/20260526-global-config-model-audit-session-research.md (Lines 214-220) - Recommended override model and naming decision.
  - #file:../research/20260526-global-config-model-audit-session-research.md (Lines 222-226) - Implementation guidance and success criteria.
- **Dependencies**:
  - Implementation decisions from Phases 1 through 5.

### Task 6.3: Run workspace validation and fix regressions

Validate the full Rust workspace after implementation and fix regressions tied to the planned changes.

- **Files**:
  - Workspace manifests and tests touched by implementation - Ensure all changes compile and tests pass.
- **Success**:
  - `cargo fmt --all` has no pending formatting changes.
  - `cargo test --workspace` passes.
  - Add `cargo clippy --all-targets --all-features -- -D warnings` if the project supports the lint baseline.
- **Research References**:
  - #file:../research/20260526-global-config-model-audit-session-research.md (Lines 90-97) - Rust and planning conventions.
  - #file:../research/20260526-global-config-model-audit-session-research.md (Lines 222-226) - Success criteria.
- **Dependencies**:
  - All implementation phases complete.

## Dependencies

- Rust standard library path, environment, and file-system APIs.
- Existing `anyhow` error handling and workspace crate dependencies.
- Existing JSONL and SQLite audit implementations.
- Existing `SessionManager` and session context response shapes.
- Temporary global-home environment overrides for tests.

## Success Criteria

- Omitted-config commands discover nearest project config, then parent config, then global config.
- Install/bootstrap creates the global config, policies, and models directory without overwriting user config.
- Relative runtime paths resolve against the selected config file directory.
- Model downloads and default NER model paths use the global models directory only.
- Audit entries work for proxy, direct API, and MCP and include surface/session metadata.
- API and MCP create and use a global session from startup.
- `cargo fmt --all` and `cargo test --workspace` pass after implementation.