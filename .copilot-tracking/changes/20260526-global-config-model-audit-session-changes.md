# Changes: Global Config, Global Models, Cross-Surface Audit, And Global Sessions

## Notes

- Created this tracker for the implementation run on 2026-05-26.
- The prompt references `.github/instructions/task-implementation.instructions.md`, but that file is not present in this workspace. Implementation follows the available tracked plan, details, research, Rust best practices, and repository conventions.

## Progress

- Started implementation from `.copilot-tracking/plans/20260526-global-config-model-audit-session-plan.instructions.md`.
- Added shared global path helpers in `crates/cloakpipe-core/src/paths.rs` with canonical `~/.cloakpipe` handling, `CLOAKPIPE_HOME`, backward-compatible `CLOAKPIPE_CONFIG_HOME`, and compatibility aliases for `~/.cloackpipe` / `cloackpipe.toml`.
- Reworked CLI config loading so omitted `--config` discovers nearest project config (`cloakpipe.toml`, then `cloackpipe.toml`) and falls back to global `~/.cloakpipe/cloakpipe.toml`; explicit missing config paths now error.
- Added global bootstrap for config, `models/`, and `policies/` from install/preset/init/setup flows without overwriting existing user config.
- Normalized relative vault, audit, tree storage, local vector DB, and NER model paths relative to the selected config file.
- Moved default NER model paths and managed GLiNER sidecar virtualenv to the global CloakPipe home; updated `tools/download_model.sh` to accept a target directory.
- Added a shared audit sink abstraction with disabled, JSONL, and SQLite backends plus surface/user/session metadata.
- Wired audit metadata through direct API, proxy chat/embeddings, and MCP tools.
- Added the `global` session helper and startup creation for direct API/proxy state and MCP; direct API and MCP pseudonymize paths record into the global session.
- Updated tests and docs for global config discovery, global runtime paths, audit metadata, and global session behavior.

## Validation

- `cargo fmt --all` passed.
- `cargo check --workspace` passed.
- `cargo test --workspace` passed.
