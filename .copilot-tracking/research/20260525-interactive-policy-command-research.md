<!-- markdownlint-disable-file -->

# Task Research Notes: Interactive Policy Command

## Research Executed

### File Analysis

- crates/cloakpipe-cli/src/main.rs
  - The CLI uses `clap::{Parser, Subcommand, ValueEnum}` with a top-level `Commands` enum and dispatches every command through `commands::*` async functions. `Presets`, `Tree`, `Vector`, `Sessions`, and `Ner` show the established nested-subcommand pattern.
- crates/cloakpipe-cli/src/commands.rs
  - `setup()` already uses `dialoguer::{Confirm, Select}` for interactive configuration and writes `cloakpipe.toml` through `toml::to_string_pretty(&config)`.
  - `resolve_config_path`, `load_config_file`, `load_config`, and `load_config_or_default` centralize config and bundled-preset resolution. New policy editing should reuse this path resolution style so `--config dpdp.toml` and normal files behave consistently.
  - `scan()` maps strategy strings to `MaskingStrategy::{Similar, FormatPreserving, Token}` and uses `load_config_or_default` before constructing `Detector::from_config(&config.detection)`.
- crates/cloakpipe-cli/src/presets.rs
  - Bundled presets are embedded in the CLI crate and installed under `CLOAKPIPE_CONFIG_HOME/policies`. `resolve_installed_preset()` accepts a one-component preset name or filename, installs presets on demand, and returns the installed editable path.
- crates/cloakpipe-core/src/config.rs
  - `CloakPipeConfig` serializes/deserializes full TOML policy files. Editable fields include `proxy.masking_strategy`, detection booleans, `[detection.ner]`, `custom.patterns`, `overrides.preserve`, `overrides.force`, resolver settings, audit settings, tree settings, vectors, local, vault, and session.
- crates/cloakpipe-core/src/detector/mod.rs
  - Detector construction uses `DetectionConfig` fields directly. Disabling a built-in detector means toggling its boolean off. `overrides.preserve` removes exact matched originals, and `overrides.force` inserts exact matches as forced custom entities.
- crates/cloakpipe-core/src/detector/custom.rs
  - Custom policy rules are `CustomPattern { name, regex, category }` values compiled into `Regex` at detector construction. A policy editor must validate or at least build a `Detector` after edits to catch invalid regexes.
- crates/cloakpipe-core/src/detector/financial.rs and crates/cloakpipe-core/src/detector/patterns.rs
  - Built-in rule families are controlled by detection booleans: `financial` and `dates` gate financial/date rules; `secrets`, `emails`, `ip_addresses`, `phone_numbers`, and `urls_internal` gate pattern rules. Identity/document patterns are currently always-on in `PatternDetector::new` and are not independently configurable through the existing schema.
- crates/cloakpipe-core/src/profiles.rs
  - Industry profiles generate full `DetectionConfig` values for general, legal, healthcare, and fintech. Existing setup uses profiles as a starting point rather than a runtime partial patch.
- README.md, policies/README.md, docs/ARCHITECTURE.md, docs/mcp.md
  - Policy files are documented as full `cloakpipe.toml`-compatible files used with `--config`. Documentation already names policy toggles for secrets, financial, dates, emails, phone numbers, IP addresses, and internal URLs. MCP `configure` applies profile, enable, disable, and detector rebuild semantics at runtime.

### Code Search Results

- `dialoguer` usage
  - `commands::setup()` imports and uses `Confirm`, `Select`, and `dialoguer::Input`, so no new interactive prompt dependency is needed for an interactive policy editor.
- `toml::to_string_pretty`
  - Used by `start()`, `init()`, and `setup()` to write config files. Reusing this serializer matches existing behavior, with the trade-off that comments in existing TOML are not preserved.
- `PresetCommands`
  - Current preset management supports only `install` and `list`; a separate `policy` command is cleaner than overloading preset installation because editing can target any `--config` policy, including installed bundled presets.
- `DetectionConfig` toggles
  - Existing schema already supports top-level toggles for `secrets`, `financial`, `dates`, `emails`, `phone_numbers`, `ip_addresses`, `urls_internal`, plus `ner.enabled`, custom rules, preserve/force overrides, resolver, and proxy replacement strategy.
- `MaskingStrategy`
  - CLI scan accepts `similar`, `format-preserving`, and `token`; config stores the default under `[proxy].masking_strategy`.

### External Research

- #fetch:https://docs.rs/dialoguer/latest/dialoguer/struct.MultiSelect.html
  - `MultiSelect` supports `.items(...)`, `.defaults(&[bool])`, `.with_prompt(...)`, and `.interact()` returning `Vec<usize>`. This is a direct fit for allowing users to select enabled/disabled detection rule families in one prompt.
- #fetch:https://docs.rs/dialoguer/latest/dialoguer/struct.Select.html
  - `Select` supports `.items(...)`, `.default(index)`, `.with_prompt(...)`, and `.interact()` returning the selected index. This matches strategy/profile/menu selection.
- #fetch:https://docs.rs/clap/latest/clap/_derive/_tutorial/index.html
  - Clap derive supports nested subcommands with `#[derive(Subcommand)]` and `#[command(subcommand)]`. The same documentation recommends `Command::debug_assert()` tests for CLI definitions.
- #fetch:https://docs.rs/toml/latest/toml/fn.to_string_pretty.html
  - `toml::to_string_pretty` serializes Serde data structures as pretty TOML. The docs note that `toml_edit::DocumentMut` is the customization path for preserving richer document formatting, but the current project already uses `toml::to_string_pretty`.

### Project Conventions

- Standards referenced: .agents/skills/rust-best-practices/SKILL.md
  - Keep APIs fallible with `Result`, prefer borrowed parameters, avoid unnecessary clones, and add focused tests for behavior and error paths.
- Instructions followed: .github/agents/task-researcher.agent.md
  - Research is documented only in `.copilot-tracking/research/` before planning and uses verified project and external findings.

## Key Discoveries

### Project Structure

The CLI crate owns user-facing commands and already depends on `dialoguer`, `clap`, and `toml`. The core crate owns the serializable `CloakPipeConfig` and all detector schema. Policy files are full config files, not a smaller policy-only type, so a policy editor should load and write `CloakPipeConfig` rather than introducing a parallel schema.

### Implementation Patterns

New commands are added by extending `Commands` in `crates/cloakpipe-cli/src/main.rs`, adding any nested `Subcommand` enums there, and dispatching to `commands::*` in the top-level match. Command implementation functions in `commands.rs` return `anyhow::Result<()>`, use existing helper functions for config path handling, and print concise status messages.

Interactive flows should follow `setup()`: use `dialoguer` prompts in `commands.rs`, mutate a `CloakPipeConfig`, serialize with `toml::to_string_pretty`, write with `std::fs::write`, and return `Ok(())`. The editor should rebuild `Detector::from_config(&config.detection)` after mutation to validate detection flags and custom regex compilation before writing.

Preset names such as `dpdp.toml` resolve to installed copies through `resolve_installed_preset()`, so editing a bundled preset should modify the installed user copy, not source files in `crates/cloakpipe-cli/presets/` or `policies/`.

### Complete Examples

```rust
// Existing CLI pattern to extend.
#[derive(Subcommand)]
enum Commands {
    /// Manage bundled configuration presets
    Presets {
        #[command(subcommand)]
        action: PresetCommands,
    },
}

match cli.command {
    Commands::Presets { action } => commands::presets(action).await,
    // New policy command should dispatch the same way.
}
```

```rust
// Existing interactive write pattern from setup().
use dialoguer::{Confirm, Select};

let audit_enabled = Confirm::new()
    .with_prompt("Enable audit logging?")
    .default(true)
    .interact()?;

let mut config = default_config();
config.audit = cloakpipe_core::config::AuditConfig {
    enabled: audit_enabled,
    ..Default::default()
};

let toml_str = toml::to_string_pretty(&config)?;
std::fs::write("cloakpipe.toml", &toml_str)?;
```

```rust
// Recommended policy toggle shape using existing dependencies.
use dialoguer::MultiSelect;

struct DetectionToggle {
    key: &'static str,
    label: &'static str,
    enabled: fn(&cloakpipe_core::config::DetectionConfig) -> bool,
    set: fn(&mut cloakpipe_core::config::DetectionConfig, bool),
}

let items: Vec<&str> = toggles.iter().map(|toggle| toggle.label).collect();
let defaults: Vec<bool> = toggles
    .iter()
    .map(|toggle| (toggle.enabled)(&config.detection))
    .collect();
let selected = MultiSelect::new()
    .with_prompt("Select detection rule families to enable")
    .items(&items)
    .defaults(&defaults)
    .interact()?;

for (index, toggle) in toggles.iter().enumerate() {
    (toggle.set)(&mut config.detection, selected.contains(&index));
}
```

```rust
// Validation should fail before writing when custom regexes are invalid.
let _detector = cloakpipe_core::detector::Detector::from_config(&config.detection)?;
let serialized = toml::to_string_pretty(&config)?;
std::fs::write(&policy_path, serialized)?;
```

### API and Schema Documentation

The editable policy surface should map to existing schema fields:

- File target: global `--config` value, defaulting to `cloakpipe.toml`, resolved with existing config helpers.
- Replacement behavior: `[proxy].masking_strategy` with `similar`, `format-preserving`, and `token` values.
- Built-in detection rule families: `[detection] secrets`, `financial`, `dates`, `emails`, `phone_numbers`, `ip_addresses`, and `urls_internal`.
- NER: `[detection.ner] enabled`, `backend`, `confidence_threshold`, `entity_types`, `sidecar_url`, and optional `model`.
- Custom rules: `[[detection.custom.patterns]]` entries with `name`, `regex`, and `category`.
- Overrides: `[detection.overrides] preserve` and `force` arrays.
- Resolver: `[detection.resolver]` values for fuzzy matching and alias groups.

The phrase “allow/disable rules to replace” maps to enabling/disabling detection rule families plus preserving or forcing exact values. Existing replacement strategy is policy-level, not category-level, so category-specific replacement strategies would require new core schema and replacer behavior and is outside the minimal compatible design.

### Configuration Examples

```toml
[proxy]
masking_strategy = "similar"

[detection]
secrets = true
financial = true
dates = true
emails = true
phone_numbers = false
ip_addresses = true
urls_internal = true

[detection.ner]
enabled = true
backend = "distilbert_pii"
confidence_threshold = 0.4
entity_types = []

[[detection.custom.patterns]]
name = "swift_bic"
regex = '\b[A-Z]{4}[A-Z]{2}[A-Z0-9]{2}(?:[A-Z0-9]{3})?\b'
category = "SWIFT_CODE"

[detection.overrides]
preserve = ["NYSE", "NASDAQ"]
force = []
```

### Technical Requirements

- Add a top-level `policy` command with an `edit` action that runs an interactive editor against the active `--config` policy.
- Keep writes scoped to user-selected config paths and installed preset copies resolved through existing helpers.
- Let users select enabled detection families in a `MultiSelect` prompt seeded with current values.
- Let users select the default replacement strategy in a `Select` prompt backed by existing `MaskingStrategy` values.
- Let users manage exact preserve and force lists and custom regex patterns through interactive prompts.
- Validate edited policies by serializing TOML and rebuilding `Detector::from_config(&config.detection)` before writing.
- Add tests for CLI parsing, non-interactive helper behavior, policy file write/roundtrip, invalid custom-regex rejection, and preset-path resolution without needing real interactive terminal input.
- Document the new command in README policy usage and/or `policies/README.md`.

## Recommended Approach

Implement a focused `cloakpipe policy edit` command that loads the active policy from the global `--config` path, presents an interactive menu for detection toggles, replacement strategy, custom patterns, preserve list, force list, and NER settings, validates the edited `CloakPipeConfig`, and writes the same TOML file back after user confirmation.

Use `dialoguer` because it is already a workspace dependency and already used by `setup()`. Use `toml::to_string_pretty` because existing commands already serialize policies that way. Keep the first implementation schema-compatible by editing existing fields only; do not introduce category-level replacement strategy unless a future core schema change is explicitly requested.

## Implementation Guidance

- **Objectives**: Provide a discoverable interactive command for creating or editing a policy, enable/disable detection rule families, choose replacement strategy, manage exact preserve/force values, manage custom patterns, and validate policies before saving.
- **Key Tasks**: Add `PolicyCommands` to the CLI, add policy editing helpers in `commands.rs`, reuse config resolution and TOML serialization, implement `dialoguer` prompts, validate detector construction, add CLI/helper tests, and update policy documentation.
- **Dependencies**: Existing `clap`, `dialoguer`, `toml`, `anyhow`, `regex`, `CloakPipeConfig`, `Detector::from_config`, bundled preset resolution helpers, and Rust workspace test tooling.
- **Success Criteria**: `cargo test -p cloakpipe-cli test_presets`, new policy command tests, and `cargo test -p cloakpipe-core` pass; `cloakpipe policy edit` can create a missing `cloakpipe.toml` from defaults; `cloakpipe --config dpdp.toml policy edit` edits the installed preset copy; invalid custom regexes are rejected before write; edited policy TOML round-trips through `CloakPipeConfig`.
- **Planning Validation**: `.github/instructions/task-implementation.instructions.md` is not present in this workspace; implementation should rely on the task plan, details, research document, existing project conventions, and #file:../../.agents/skills/rust-best-practices/SKILL.md.