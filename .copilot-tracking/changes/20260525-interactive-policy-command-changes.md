<!-- markdownlint-disable-file -->

# Interactive Policy Command Changes

## Status

- [x] Phase 1: CLI Surface And Policy Resolution
- [x] Phase 2: Interactive Policy Editing Experience
- [x] Phase 3: Validation, Persistence, And User-Safe Save Flow
- [x] Phase 4: Tests And Documentation

## Implementation Log

- Created the changes tracking file required by the implementation prompt.
- Added the top-level `cloakpipe policy edit` command, command dispatch, and clap command-definition tests.
- Added editable policy resolution for existing files, missing local paths, and installed bundled preset copies under `CLOAKPIPE_CONFIG_HOME/policies`.
- Added an interactive policy editor for detection rule families, replacement strategy, NER settings, preserve/force lists, and custom regex patterns.
- Added validation-before-write that serializes pretty TOML, round-trips through `CloakPipeConfig`, and constructs a `Detector` before overwriting the target file.
- Added confirmation/cancellation behavior and save summaries for changed policy sections.
- Updated `MaskingStrategy` and `NerBackend` serde names so saved policies use documented values while accepting previous aliases.
- Added CLI helper tests, a `policy edit --help` integration test, and core serialization tests.
- Documented `cloakpipe policy edit` in `README.md` and `policies/README.md`.
- Verified with `cargo test -p cloakpipe-cli` and `cargo test -p cloakpipe-core`.
- Deleted `.copilot-tracking/prompts/implement-interactive-policy-command.prompt.md` as required by cleanup.
