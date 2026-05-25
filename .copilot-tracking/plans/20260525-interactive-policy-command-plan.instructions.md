---
applyTo: '.copilot-tracking/changes/20260525-interactive-policy-command-changes.md'
---

<!-- markdownlint-disable-file -->

# Task Checklist: Interactive Policy Command

## Overview

Add an interactive `cloakpipe policy edit` command that creates or edits a policy file and lets users select supported detection, replacement, NER, override, and custom-rule settings.

## Objectives

- Add a discoverable top-level CLI command for interactive policy editing.
- Let users create or edit the active `--config` policy file without modifying bundled source presets.
- Let users enable or disable built-in detection rule families and choose the default replacement strategy.
- Let users manage NER settings, custom regex patterns, preserve list, and force list.
- Validate edited policies before writing and cover the behavior with focused tests and documentation.

## Research Summary

### Project Files

- crates/cloakpipe-cli/src/main.rs - CLI command definitions, nested subcommand enums, and top-level dispatch.
- crates/cloakpipe-cli/src/commands.rs - Existing config loading, preset-aware resolution, interactive setup, TOML serialization, and scan strategy mapping.
- crates/cloakpipe-cli/src/presets.rs - Installed bundled preset resolution that should be reused for editable preset targets.
- crates/cloakpipe-core/src/config.rs - Serializable `CloakPipeConfig`, `DetectionConfig`, `NerConfig`, `CustomPattern`, `OverrideConfig`, and proxy masking strategy fields.
- crates/cloakpipe-core/src/detector/mod.rs - Detector construction, preserve/force semantics, and validation path for edited configs.
- crates/cloakpipe-core/src/detector/custom.rs - Custom regex rule compilation behavior that should reject invalid edited custom rules.
- README.md - User-facing policy file documentation target.
- policies/README.md - Bundled preset documentation target.

### External References

- #file:../research/20260525-interactive-policy-command-research.md - Validated project research, schema mapping, external tool documentation, and recommended compatible approach.
- #fetch:https://docs.rs/dialoguer/latest/dialoguer/struct.MultiSelect.html - `MultiSelect` API for selecting enabled detection rule families with default checked states.
- #fetch:https://docs.rs/dialoguer/latest/dialoguer/struct.Select.html - `Select` API for menus, strategy selection, and NER/backend choices.
- #fetch:https://docs.rs/clap/latest/clap/_derive/_tutorial/index.html - Clap derive nested subcommand and command definition testing guidance.
- #fetch:https://docs.rs/toml/latest/toml/fn.to_string_pretty.html - TOML pretty serialization behavior used by existing commands.

### Standards References

- #file:../../.agents/skills/rust-best-practices/SKILL.md - Rust ownership, error handling, validation, and testing conventions.
- #file:../../.github/agents/task-researcher.agent.md - Research documentation standard followed before planning.

## Implementation Checklist

### [x] Phase 1: CLI Surface And Policy Resolution

- [x] Task 1.1: Add the top-level `policy edit` command
  - Details: .copilot-tracking/details/20260525-interactive-policy-command-details.md (Lines 11-27)

- [x] Task 1.2: Resolve editable policy targets safely
  - Details: .copilot-tracking/details/20260525-interactive-policy-command-details.md (Lines 28-45)

### [x] Phase 2: Interactive Policy Editing Experience

- [x] Task 2.1: Implement detection rule family selection
  - Details: .copilot-tracking/details/20260525-interactive-policy-command-details.md (Lines 49-65)

- [x] Task 2.2: Implement replacement strategy and NER selectors
  - Details: .copilot-tracking/details/20260525-interactive-policy-command-details.md (Lines 66-84)

- [x] Task 2.3: Implement override and custom pattern management
  - Details: .copilot-tracking/details/20260525-interactive-policy-command-details.md (Lines 85-101)

### [x] Phase 3: Validation, Persistence, And User-Safe Save Flow

- [x] Task 3.1: Validate edited policies before writing
  - Details: .copilot-tracking/details/20260525-interactive-policy-command-details.md (Lines 105-122)

- [x] Task 3.2: Add confirmation and cancellation behavior
  - Details: .copilot-tracking/details/20260525-interactive-policy-command-details.md (Lines 123-139)

### [x] Phase 4: Tests And Documentation

- [x] Task 4.1: Add CLI and helper tests
  - Details: .copilot-tracking/details/20260525-interactive-policy-command-details.md (Lines 143-162)

- [x] Task 4.2: Document the new policy editor
  - Details: .copilot-tracking/details/20260525-interactive-policy-command-details.md (Lines 163-180)

## Dependencies

- Rust 2021 workspace and existing Cargo test workflow.
- Existing workspace dependencies: `clap`, `dialoguer`, `toml`, `anyhow`, `regex`, and `serde`.
- Existing `CloakPipeConfig`, `Detector::from_config`, and preset resolution helpers.
- Writable target policy path or writable `CLOAKPIPE_CONFIG_HOME` for installed bundled presets.

## Success Criteria

- `cloakpipe policy edit --help` is available and the clap command definition validates.
- The command can create a missing local policy from defaults or edit an existing active `--config` policy.
- Editing a bundled preset name modifies the installed user copy under `CLOAKPIPE_CONFIG_HOME/policies`.
- Users can select detection rule families, replacement strategy, NER settings, custom patterns, preserve list, and force list.
- Invalid custom regexes are rejected before writing.
- Edited policy TOML round-trips through `CloakPipeConfig` and builds a detector.
- New policy tests, existing preset tests, and relevant core tests pass.