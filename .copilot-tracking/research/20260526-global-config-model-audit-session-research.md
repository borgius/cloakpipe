<!-- markdownlint-disable-file -->

# Task Research Notes: Global Config, Global Models, Cross-Surface Audit, And Global Sessions

## Research Executed

### File Analysis

- `crates/cloakpipe-cli/src/main.rs`
  - The CLI defines a global `--config` string with default value `cloakpipe.toml`, so command handlers cannot tell whether the user supplied `--config` or accepted the default.
  - Most commands pass `&cli.config` to shared command functions, including `start`, `test`, `stats`, `mcp`, `tree`, `sessions`, and `scan`.
- `crates/cloakpipe-cli/src/commands.rs`
  - `resolve_config_path` checks only the supplied path, then installed presets, then returns `Missing(path)`.
  - `start` creates a local config at the missing supplied path, while `test`, `mcp`, `tree`, `scan`, and `sessions` load defaults when config is missing.
  - `init` and `setup` write local `cloakpipe.toml` files.
  - `default_config` uses relative paths such as `./vault.enc`, `./audit/`, and `./trees/`.
  - `download_distilbert_pii` finds a CloakPipe checkout and runs `tools/download_model.sh`; GLiNER helper state also uses project-local `.cloakpipe/gliner-pii-venv`.
- `crates/cloakpipe-cli/src/presets.rs`
  - Preset installation uses `CLOAKPIPE_CONFIG_HOME` when set.
  - Without the override, macOS defaults to `~/Library/Application Support/cloakpipe`, Windows uses `%APPDATA%/cloakpipe`, XDG uses `$XDG_CONFIG_HOME/cloakpipe`, and fallback uses `~/.config/cloakpipe`.
  - Presets install under a `policies/` subdirectory.
- `install.sh`
  - The installer installs the binary and calls `cloakpipe presets install` through `preinstall_bundled_presets`.
  - It does not create a global `cloakpipe.toml`, `cloackpipe.toml`, or `models/` directory directly.
- `crates/cloakpipe-core/src/config.rs`
  - Config structs contain no source path metadata and no path-normalization layer.
  - `AuditConfig` has `enabled`, `log_path`, `format`, `retention_days`, `log_entities`, `log_mappings`, and `backend`, but current proxy construction uses JSONL directly.
  - `SessionConfig` is part of the root config and defaults to disabled.
- `crates/cloakpipe-core/src/detector/distilbert_pii.rs`
  - The DistilBERT default model path is `models/distilbert-pii/quantized/model_quantized.onnx`.
  - The tokenizer is resolved from the model directory or its parent.
  - Detection already chunks long input and filters phone fragments with fewer than seven digits, so the model storage change should avoid regressing those behaviors.
- `crates/cloakpipe-core/src/detector/gliner.rs`
  - The GLiNER ONNX default model path is `models/gliner.onnx`.
- `crates/cloakpipe-core/src/detector/ner.rs`
  - The BERT NER default model path is `models/bert-ner.onnx`.
- `tools/download_model.sh`
  - The download script writes DistilBERT files under repository-local `models/distilbert-pii`.
  - The conversion fallback uses repository-local `.cloakpipe/gliner-pii-venv`.
- `crates/cloakpipe-audit/src/lib.rs`
  - `AuditLogger` appends JSONL entries with event, request ID, entity counts, categories, and error fields.
  - Entries do not include surface, session ID, user ID, or config source.
  - `AuditConfig.enabled` is not enforced by this logger.
- `crates/cloakpipe-audit/src/sqlite.rs`
  - `SqliteAuditLogger` exists and can store `user_id` and `session_id`.
  - It is not wired into `AppState` or MCP construction.
- `crates/cloakpipe-proxy/src/state.rs`
  - `AppState` stores `AuditLogger` and `Arc<SessionManager>`.
  - The constructor creates a session manager from config but does not create a global session at startup.
- `crates/cloakpipe-proxy/src/server.rs`
  - Direct API routes are `/pseudonymize`, `/rehydrate`, `/detect`, `/vault_stats`, `/configure`, `/session_context`, and matching `/v1/*` routes.
  - Proxy routes include `/v1/chat/completions` and `/v1/embeddings`.
- `crates/cloakpipe-proxy/src/handlers.rs`
  - Direct API pseudonymize and rehydrate log JSONL audit events, but direct detect, configure, vault stats, and session context do not log.
  - Proxy chat and embeddings log pseudonymization events; chat also logs rehydration events.
  - Proxy chat uses sessions only when session tracking is enabled and a request header supplies a session ID.
  - Direct API pseudonymize does not use session context.
- `crates/cloakpipe-mcp/src/lib.rs`
  - MCP server state contains detector, vault, detection config, active profile, and sessions.
  - MCP has no audit logger field and no audit logging in tools.
  - MCP pseudonymize does not record entities in a session, and `CloakPipeServer::new` does not create a global session.
- `crates/cloakpipe-core/src/session.rs`
  - `SessionManager::get_or_create` creates sessions even when `SessionConfig.enabled` is false; callers decide whether to use sessions.
  - The session module has no global session constant or helper.
- `crates/cloakpipe-proxy/tests/privacy_api.rs`
  - Existing tests cover direct API roundtrip, detect/vault stats/configure/session_context shapes, and missing upstream API key behavior.
  - The session context test manually creates a session before inspecting it.
- `crates/cloakpipe-cli/tests/test_presets.rs`
  - Tests expect `CLOAKPIPE_CONFIG_HOME` to control where `policies/` is installed.
- `crates/cloakpipe-cli/tests/test_ner_install.rs` and `crates/cloakpipe-cli/tests/test_ner_start.rs`
  - Tests assert project-local `.cloakpipe/gliner-pii-venv` behavior for GLiNER install/start helpers.

### Code Search Results

- Config discovery is centralized in CLI command helpers, not in `cloakpipe-core`.
- Model default paths are currently embedded in detector constructors, so global model storage requires either pre-normalizing `detection.ner.model` or moving default model path resolution into shared core path helpers.
- Audit JSONL and SQLite implementations exist, but the proxy uses only JSONL and MCP uses no audit implementation.
- Session context machinery exists and is tested, but direct API and MCP do not use it for pseudonymization.
- The user requested `~/.cloackpipe` and `cloackpipe.toml`; the repository, binary, docs, active config, and package names consistently use `cloakpipe` and `cloakpipe.toml`. Implementation should either honor the request literally or document a compatibility decision. The lowest-risk product decision is to make canonical paths `~/.cloakpipe` and `cloakpipe.toml`, while accepting the requested misspellings `~/.cloackpipe` and `cloackpipe.toml` as compatibility aliases if feasible.

### External Research

- #fetch:https://doc.rust-lang.org/std/path/struct.Path.html#method.ancestors - Rust `Path::ancestors` produces an iterator over a path and each parent, which fits current-directory-to-root config probing.
- #fetch:https://doc.rust-lang.org/std/env/fn.current_dir.html - `std::env::current_dir()` returns a `Result<PathBuf>` and can fail when the current directory is invalid or inaccessible, so discovery must surface contextual errors.
- #fetch:https://doc.rust-lang.org/std/env/fn.var_os.html - `std::env::var_os()` reads environment variables without assuming valid Unicode, useful for home and override paths.
- #fetch:https://doc.rust-lang.org/std/fs/fn.create_dir_all.html - `std::fs::create_dir_all()` recursively creates parent directories, treats concurrent self-creation as success, and is appropriate for global config/model directories.
- #fetch:https://doc.rust-lang.org/cargo/reference/config.html#hierarchical-structure - Cargo documents a close precedent: it probes current directory and parents for local config, then a global home config, with closer files taking precedence.
- #fetch:https://doc.rust-lang.org/cargo/reference/config.html#config-relative-paths - Cargo documents config-relative path behavior, supporting a design where relative paths in a config file resolve relative to that config file's directory.

### Project Conventions

- Standards referenced: `.agents/skills/rust-best-practices/SKILL.md`.
  - Prefer borrowed parameters such as `&Path` and `&str`, use `Result` for fallible path discovery, avoid unnecessary clones, and add focused tests.
- Writing standards referenced: `writing-clearly-and-concisely` and Strunk composition guidance.
  - Use concrete task language, active voice, and concise paragraphs in planning artifacts.
- Planning mode constraints.
  - Research and planning files must stay under `.copilot-tracking/research/`, `.copilot-tracking/plans/`, `.copilot-tracking/details/`, and `.copilot-tracking/prompts/`.

## Key Discoveries

### Project Structure

The feature spans four workspace crates and one installer script. `cloakpipe-cli` owns command-line config loading and installation hooks. `cloakpipe-core` owns config structs, detector defaults, and sessions. `cloakpipe-audit` owns audit persistence. `cloakpipe-proxy` and `cloakpipe-mcp` own API/proxy/MCP surface behavior.

### Implementation Patterns

Config loading currently returns a bare `CloakPipeConfig`, losing the path that supplied it. A global/project discovery design needs a resolved-config wrapper, such as `ResolvedConfig { config, source_path, source_kind, base_dir }`, so callers can normalize relative paths and report which config was used.

The existing `find_gliner_project_root` helper already demonstrates a parent-directory traversal using `Path::ancestors`. The config lookup can reuse this standard-library pattern, but should look for config files instead of `tools/gliner-pii-server.py`.

The audit layer should move behind a cloneable or shared sink. `AppState` can hold `Arc<AuditSink>`, and MCP can hold the same type without fighting `Clone` requirements. The sink can wrap JSONL, SQLite behind a mutex, or a no-op disabled implementation.

The session layer already separates session storage from caller policy. A global session can be introduced without rewriting `SessionManager`: add a constant such as `GLOBAL_SESSION_ID`, call `get_or_create(GLOBAL_SESSION_ID)` during API and MCP startup, and route direct API/MCP pseudonymize flows through that ID.

### Complete Examples

```rust
// Current ancestor traversal pattern in commands.rs.
for dir in start_dir.ancestors() {
    if dir.join(GLINER_SERVER_SCRIPT).exists() {
        return Ok(dir.to_path_buf());
    }
}
```

```rust
// Recommended config discovery shape.
pub enum ConfigSourceKind {
    ExplicitPath,
    BundledPreset,
    Project,
    Global,
}

pub struct ResolvedConfig {
    pub config: CloakPipeConfig,
    pub path: PathBuf,
    pub base_dir: PathBuf,
    pub source: ConfigSourceKind,
}
```

```rust
// Recommended global path helpers.
pub const CANONICAL_CONFIG_FILE: &str = "cloakpipe.toml";
pub const LEGACY_REQUESTED_CONFIG_FILE: &str = "cloackpipe.toml";

pub fn global_home() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("CLOAKPIPE_HOME") {
        return Ok(PathBuf::from(path));
    }
    let home = std::env::var_os("HOME")
        .ok_or_else(|| anyhow::anyhow!("Cannot determine home directory"))?;
    Ok(PathBuf::from(home).join(".cloakpipe"))
}
```

```rust
// Recommended audit context shape.
pub struct AuditContext<'a> {
    pub surface: &'a str,
    pub request_id: &'a str,
    pub session_id: Option<&'a str>,
    pub user_id: Option<&'a str>,
}
```

### API and Schema Documentation

`AuditEntry` can be extended additively with optional `surface`, `session_id`, and `user_id` fields without breaking existing JSONL consumers. Existing `AuditEvent` variants cover pseudonymize, rehydrate, proxy request, and error. Detect/configure/vault/session operations can use a request-style event or new additive variants if tests and docs cover them.

`SessionStats` already exposes safe metadata without raw PII, so global-session visibility through `/session_context` and MCP `session_context` can reuse existing response shapes.

`NerConfig.model` is optional. Setting it during config normalization preserves detector constructor APIs and lets each backend use a global model path without changing serialized config files.

### Configuration Examples

```toml
# Global config target after bootstrap.
# Canonical path: ~/.cloakpipe/cloakpipe.toml
# Compatibility alias, if implemented: ~/.cloackpipe/cloackpipe.toml

[vault]
path = "vault.enc"
encryption = "aes-256-gcm"
key_env = "CLOAKPIPE_VAULT_KEY"
backend = "file"

[audit]
enabled = true
log_path = "audit"
backend = "jsonl"
log_entities = true

[detection.ner]
enabled = false
backend = "distilbert_pii"
model = "models/distilbert-pii/quantized/model_quantized.onnx"
```

### Technical Requirements

- Add global path helpers that resolve the global CloakPipe home, global config path, and global models directory with testable environment overrides.
- Change CLI config handling so omitted `--config` triggers project discovery from current directory through parents, then global fallback.
- Preserve explicit `--config` behavior for exact paths and bundled presets; this likely requires changing the CLI field to `Option<String>` or otherwise detecting whether the flag was supplied.
- Create global config and `models/` during install/bootstrap without overwriting an existing global config.
- Resolve relative config paths against the directory containing the config file, not the process current directory.
- Download and default model files under the global models directory only.
- Honor `audit.enabled`, route JSONL and SQLite through one audit sink, and add surface/session metadata.
- Add audit logging to MCP and fill direct API gaps.
- Create and use a global session for direct API and MCP startup and pseudonymize flows.
- Update tests that currently assume project-local `.cloakpipe` or source-checkout model paths.

## Recommended Approach

Implement the feature in six phases. First, add path and resolved-config infrastructure so every later change has a single global-home and config-source contract. Second, bootstrap the global directory from the installer path already used by `install.sh`. Third, move model download/default paths to the global models directory. Fourth, introduce a shared audit sink with surface and session metadata. Fifth, create and use a global session for API and MCP. Sixth, update tests and docs.

Use `~/.cloakpipe` and `cloakpipe.toml` as canonical names because the codebase and product already use that spelling. Treat `~/.cloackpipe` and `cloackpipe.toml` from the request as compatibility aliases or document the decision if the implementation intentionally rejects aliases.

Do not merge project and global config values unless a separate requirement asks for merging. The user's requested model is override by presence: nearest project config wins; global config applies only when no project config exists.

## Implementation Guidance

- **Objectives**: Add global config fallback, project override discovery, global-only models, audit coverage across proxy/API/MCP, and API/MCP global sessions.
- **Key Tasks**: Add path helpers, update CLI config resolution, bootstrap global files, normalize path fields, redirect model download/default paths, introduce an audit sink, wire MCP audit, create global sessions in API/MCP constructors, and extend tests/docs.
- **Dependencies**: Rust standard library path/env/fs APIs, existing `anyhow` error handling, existing config structs, existing session manager, existing JSONL/SQLite audit modules, and current Cargo workspace tests.
- **Success Criteria**: `cargo test --workspace` passes; omitted-config commands discover nearest project config then global config; install/bootstrap creates the global config and models directory; model downloads never write repo-local `models/`; JSONL or SQLite audit entries include surface/session metadata for proxy/API/MCP; API and MCP session context shows the global session after startup.