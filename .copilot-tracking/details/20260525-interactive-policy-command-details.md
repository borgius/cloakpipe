<!-- markdownlint-disable-file -->

# Task Details: Interactive Policy Command

## Research Reference

**Source Research**: #file:../research/20260525-interactive-policy-command-research.md

## Phase 1: CLI Surface And Policy Resolution

### Task 1.1: Add the top-level `policy edit` command

Add a new top-level `Policy` command variant in `crates/cloakpipe-cli/src/main.rs`, a public `PolicyCommands` enum with an `Edit` action, and dispatch from the existing `match cli.command` to a new `commands::policy(&cli.config, action).await` function. Keep the style aligned with existing nested subcommands such as `Presets`, `Tree`, `Vector`, `Sessions`, and `Ner`.

- **Files**:
  - crates/cloakpipe-cli/src/main.rs - Add `Policy` command, `PolicyCommands`, help text, and dispatch.
  - crates/cloakpipe-cli/src/commands.rs - Add the async policy command entry point.
- **Success**:
  - `cloakpipe policy edit --help` is available through clap.
  - The top-level dispatch compiles without changing unrelated commands.
- **Research References**:
  - #file:../research/20260525-interactive-policy-command-research.md (Lines 9-15) - Existing CLI command and preset resolution patterns.
  - #file:../research/20260525-interactive-policy-command-research.md (Lines 30-40) - Code search findings for command style and strategy names.
  - #fetch:https://docs.rs/clap/latest/clap/_derive/_tutorial/index.html - Clap nested subcommand pattern and `Command::debug_assert()` guidance.
- **Dependencies**:
  - Existing `clap` derive dependency.

### Task 1.2: Resolve editable policy targets safely

Create or expose helper logic in `commands.rs` that resolves the active `--config` target into an editable file path. Existing files should load in place. Missing local paths should start from `default_config()` and write to that requested path after confirmation. Bundled preset names such as `dpdp.toml` should resolve to installed user preset copies through the existing preset resolver, not modify repository preset sources.

- **Files**:
  - crates/cloakpipe-cli/src/commands.rs - Add editable policy resolution and config loading helpers.
  - crates/cloakpipe-cli/src/presets.rs - Reuse existing public preset resolution helpers; only modify if visibility or ergonomics require it.
- **Success**:
  - Editing `cloakpipe.toml` targets the local file.
  - Editing `--config dpdp.toml` targets the installed preset copy under `CLOAKPIPE_CONFIG_HOME/policies`.
  - Editing a missing local config path can create a full default config after user confirmation.
- **Research References**:
  - #file:../research/20260525-interactive-policy-command-research.md (Lines 11-15) - Config helper and installed-preset behavior.
  - #file:../research/20260525-interactive-policy-command-research.md (Lines 63-73) - Project structure and implementation patterns.
  - #file:../research/20260525-interactive-policy-command-research.md (Lines 203-207) - Recommended compatible editing approach.
- **Dependencies**:
  - Task 1.1 completion.
  - Existing `default_config()`, `resolve_config_path()`, and `resolve_installed_preset()` behavior.

## Phase 2: Interactive Policy Editing Experience

### Task 2.1: Implement detection rule family selection

Add a detection toggle model for the existing built-in policy fields: `secrets`, `financial`, `dates`, `emails`, `phone_numbers`, `ip_addresses`, and `urls_internal`. Present these in a `dialoguer::MultiSelect` prompt seeded from the current config, then apply the selected indices back to `config.detection`.

- **Files**:
  - crates/cloakpipe-cli/src/commands.rs - Add detection toggle descriptors and a prompt/helper to apply selections.
- **Success**:
  - The prompt shows every supported built-in detector family.
  - Existing enabled values are checked by default.
  - Deselected items are saved as `false`, selected items as `true`.
- **Research References**:
  - #file:../research/20260525-interactive-policy-command-research.md (Lines 17-25) - Existing config schema and detector toggle semantics.
  - #file:../research/20260525-interactive-policy-command-research.md (Lines 147-159) - Policy schema fields and category-level strategy constraint.
  - #fetch:https://docs.rs/dialoguer/latest/dialoguer/struct.MultiSelect.html - Multi-select prompt API with defaults and selected index return values.
- **Dependencies**:
  - Phase 1 config loading helpers.

### Task 2.2: Implement replacement strategy and NER selectors

Add interactive selection for `[proxy].masking_strategy` using existing strategy names `similar`, `format-preserving`, and `token`. Add NER editing for `enabled`, backend selection from existing `NerBackend` variants, `confidence_threshold`, `entity_types`, `sidecar_url`, and optional model path.

- **Files**:
  - crates/cloakpipe-cli/src/commands.rs - Add strategy and NER prompt helpers.
  - crates/cloakpipe-core/src/config.rs - Modify only if existing enum serialization or helper display behavior needs a small public helper.
- **Success**:
  - Users can choose the default replacement strategy stored in `[proxy].masking_strategy`.
  - Users can enable or disable NER and adjust current NER fields without invalid TOML output.
  - Existing config values are used as prompt defaults.
- **Research References**:
  - #file:../research/20260525-interactive-policy-command-research.md (Lines 38-40) - Existing detection toggles and strategy mapping.
  - #file:../research/20260525-interactive-policy-command-research.md (Lines 45-51) - Dialoguer `Select`, clap, and TOML external docs.
  - #file:../research/20260525-interactive-policy-command-research.md (Lines 75-145) - Complete examples for command, prompt, and validation patterns.
- **Dependencies**:
  - Task 2.1 completion.
  - Existing `MaskingStrategy` and `NerBackend` serialization.

### Task 2.3: Implement override and custom pattern management

Add interactive menu actions for `[detection.overrides].preserve`, `[detection.overrides].force`, and `[[detection.custom.patterns]]`. Support viewing current values, adding entries, removing selected entries, and editing custom pattern `name`, `regex`, and `category`. Keep changes in memory until the final save confirmation.

- **Files**:
  - crates/cloakpipe-cli/src/commands.rs - Add override-list and custom-pattern prompt helpers.
- **Success**:
  - Users can manage exact values that should be preserved or forced.
  - Users can add, edit, and remove custom regex rules.
  - Custom pattern edits are validated before writing the final policy.
- **Research References**:
  - #file:../research/20260525-interactive-policy-command-research.md (Lines 19-23) - Detector preserve/force and custom regex semantics.
  - #file:../research/20260525-interactive-policy-command-research.md (Lines 161-190) - Existing TOML shape for custom patterns and overrides.
  - #file:../research/20260525-interactive-policy-command-research.md (Lines 192-199) - Technical requirements for custom pattern and override management.
- **Dependencies**:
  - Phase 1 config loading helpers.
  - Existing `CustomPattern` and `OverrideConfig` structs.

## Phase 3: Validation, Persistence, And User-Safe Save Flow

### Task 3.1: Validate edited policies before writing

Add a validation helper that serializes the edited `CloakPipeConfig` to TOML and constructs `Detector::from_config(&config.detection)` before writing. Return contextual errors for invalid TOML serialization or invalid custom regexes. Avoid partially writing a file when validation fails.

- **Files**:
  - crates/cloakpipe-cli/src/commands.rs - Add validation and save helper.
- **Success**:
  - Invalid custom regexes fail before the target file is overwritten.
  - Valid edits serialize as pretty TOML and round-trip through `CloakPipeConfig`.
  - Error messages include enough context to identify the invalid policy area.
- **Research References**:
  - #file:../research/20260525-interactive-policy-command-research.md (Lines 21-23) - Custom regex compilation point.
  - #file:../research/20260525-interactive-policy-command-research.md (Lines 140-145) - Validation-before-write example.
  - #fetch:https://docs.rs/toml/latest/toml/fn.to_string_pretty.html - Pretty TOML serialization behavior.
- **Dependencies**:
  - Phase 2 editing helpers.
  - Existing `Detector::from_config` validation behavior.

### Task 3.2: Add confirmation and cancellation behavior

Wrap the editing flow in a main menu that lets users edit specific policy aspects, preview the destination path, save, or exit without saving. Require confirmation before overwriting an existing file and before creating a missing file.

- **Files**:
  - crates/cloakpipe-cli/src/commands.rs - Add main policy edit loop and confirmation prompts.
- **Success**:
  - Users can edit any supported policy aspect from a single command.
  - Exiting without saving leaves the file unchanged.
  - Saving prints the path written and a concise summary of changed areas.
- **Research References**:
  - #file:../research/20260525-interactive-policy-command-research.md (Lines 67-73) - Existing command implementation patterns.
  - #file:../research/20260525-interactive-policy-command-research.md (Lines 192-211) - Technical requirements and recommended approach.
  - #fetch:https://docs.rs/dialoguer/latest/dialoguer/struct.Select.html - Select prompt API for menu selection.
- **Dependencies**:
  - Task 3.1 completion.
  - Existing `dialoguer::Confirm` pattern from `setup()`.

## Phase 4: Tests And Documentation

### Task 4.1: Add CLI and helper tests

Add tests that cover clap command definition, editable path resolution, config mutation helpers, serialization round-trip, invalid custom regex rejection, and installed preset target resolution. Keep direct terminal-interaction testing out of scope by extracting pure helper functions for state changes.

- **Files**:
  - crates/cloakpipe-cli/src/main.rs - Add a clap `debug_assert` unit test or equivalent command definition test.
  - crates/cloakpipe-cli/src/commands.rs - Add unit tests for helpers where private helper access is useful.
  - crates/cloakpipe-cli/tests/test_policy.rs - Add integration tests for non-interactive behavior that can be exercised through helpers or CLI-visible effects.
- **Success**:
  - New tests run without needing a real interactive terminal.
  - Existing preset tests still pass.
  - Invalid custom regex policy edits are covered by a regression test.
- **Research References**:
  - #file:../research/20260525-interactive-policy-command-research.md (Lines 49-51) - Clap testing guidance.
  - #file:../research/20260525-interactive-policy-command-research.md (Lines 192-199) - Required test coverage areas.
  - #file:../research/20260525-interactive-policy-command-research.md (Lines 209-213) - Success criteria for test commands and policy behavior.
- **Dependencies**:
  - Phases 1 through 3 completion.
  - Existing CLI integration test style.

### Task 4.2: Document the new policy editor

Update user-facing policy documentation with the new command, including examples for editing the default policy, editing an installed bundled preset, and the supported editable aspects. Note that the command edits existing schema-compatible fields and does not currently support per-category replacement strategies.

- **Files**:
  - README.md - Add policy editor usage near the Policy Files section.
  - policies/README.md - Add a short note for editing bundled presets through installed user copies.
- **Success**:
  - Documentation includes `cloakpipe policy edit` and `cloakpipe --config dpdp.toml policy edit` examples.
  - Documentation names supported aspects: detection toggles, replacement strategy, NER settings, custom patterns, preserve list, and force list.
  - Documentation avoids implying legal certification or per-category replacement support.
- **Research References**:
  - #file:../research/20260525-interactive-policy-command-research.md (Lines 26-28) - Existing policy documentation and MCP toggle behavior.
  - #file:../research/20260525-interactive-policy-command-research.md (Lines 147-159) - Supported policy schema and replacement-strategy boundary.
  - #file:../research/20260525-interactive-policy-command-research.md (Lines 192-199) - Documentation requirement.
- **Dependencies**:
  - Final command behavior from Phases 1 through 3.

## Dependencies

- Rust 2021 workspace and existing `cargo` tooling.
- Existing workspace dependencies: `clap`, `dialoguer`, `toml`, `anyhow`, `regex`, `serde`.
- Existing config schema in `cloakpipe-core`.
- Existing bundled preset installation and resolution helpers.

## Success Criteria

- `cloakpipe policy edit --help` is available and clap command validation passes.
- The command can create or edit the active `--config` policy file interactively.
- Users can toggle built-in detection rule families, replacement strategy, NER, custom patterns, preserve list, and force list.
- Invalid custom regexes are rejected before writing.
- Edited policies round-trip through `CloakPipeConfig` and can build a `Detector`.
- New CLI tests, policy helper tests, `cargo test -p cloakpipe-cli test_presets`, and relevant core tests pass.