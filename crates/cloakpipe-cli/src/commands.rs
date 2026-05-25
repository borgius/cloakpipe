//! CLI command implementations.

use crate::presets::{
    bundled_presets, install_bundled_presets, installed_preset_dir, resolve_installed_preset,
    ResolvedPreset,
};
use anyhow::{bail, Context, Result};
use cloakpipe_audit::AuditLogger;
use cloakpipe_core::{
    config::CloakPipeConfig, detector::Detector, replacer::Replacer, vault::Vault,
};
use cloakpipe_proxy::{server, state::AppState};
use std::path::{Path, PathBuf};
use std::process::Stdio;

const GLINER_PIP_PACKAGE: &str = "gliner";
const GLINER_SIDECAR_URL: &str = "http://127.0.0.1:9111";
const GLINER_VENV_DIR: &str = ".cloakpipe/gliner-pii-venv";
const GLINER_SERVER_SCRIPT: &str = "tools/gliner-pii-server.py";

enum ConfigSource {
    Existing(PathBuf),
    BundledPreset(ResolvedPreset),
    Missing(PathBuf),
}

/// Load configuration from TOML file.
fn load_config_file(path: &Path) -> Result<CloakPipeConfig> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Cannot read config: {}", path.display()))?;
    toml::from_str(&content).with_context(|| format!("Invalid config in: {}", path.display()))
}

fn resolve_config_path(config_path: &str) -> Result<ConfigSource> {
    let path = PathBuf::from(config_path);
    if path.exists() {
        return Ok(ConfigSource::Existing(path));
    }

    if let Some(preset) = resolve_installed_preset(config_path)? {
        return Ok(ConfigSource::BundledPreset(preset));
    }

    Ok(ConfigSource::Missing(path))
}

fn load_config(config_path: &str) -> Result<CloakPipeConfig> {
    match resolve_config_path(config_path)? {
        ConfigSource::Existing(path) | ConfigSource::Missing(path) => load_config_file(&path),
        ConfigSource::BundledPreset(preset) => load_config_file(&preset.path),
    }
}

fn load_config_or_default(config_path: &str) -> Result<CloakPipeConfig> {
    match resolve_config_path(config_path)? {
        ConfigSource::Existing(path) => load_config_file(&path),
        ConfigSource::BundledPreset(preset) => load_config_file(&preset.path),
        ConfigSource::Missing(_) => Ok(default_config()),
    }
}

/// Resolve the vault encryption key from environment variable.
fn resolve_vault_key(config: &CloakPipeConfig) -> Result<Vec<u8>> {
    let env_var = config
        .vault
        .key_env
        .as_deref()
        .unwrap_or("CLOAKPIPE_VAULT_KEY");
    match std::env::var(env_var) {
        Ok(hex_key) => {
            let bytes = hex_decode(&hex_key)
                .with_context(|| format!("{} must be a 64-char hex string (32 bytes)", env_var))?;
            if bytes.len() != 32 {
                bail!(
                    "{} must be 32 bytes (got {} bytes). Use a 64-char hex string.",
                    env_var,
                    bytes.len()
                );
            }
            Ok(bytes)
        }
        Err(_) => {
            tracing::warn!(
                "No {} set — generating ephemeral vault key (mappings won't persist across restarts)",
                env_var
            );
            let mut key = vec![0u8; 32];
            rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut key);
            Ok(key)
        }
    }
}

fn resolve_proxy_api_key(config: &CloakPipeConfig) -> Option<String> {
    let env_var = config.proxy.api_key_env.as_str();

    match std::env::var(env_var) {
        Ok(api_key) if !api_key.trim().is_empty() => Some(api_key),
        Ok(_) | Err(_) => {
            tracing::warn!(
                env_var = env_var,
                "No upstream API key configured — direct privacy endpoints remain available, but chat, embeddings, and upstream-backed tree routes will return errors until the variable is set"
            );
            None
        }
    }
}

/// Simple hex decoder.
fn hex_decode(hex: &str) -> Result<Vec<u8>> {
    let hex = hex.trim();
    if !hex.len().is_multiple_of(2) {
        bail!("Hex string must have even length");
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&hex[i..i + 2], 16)
                .with_context(|| format!("Invalid hex at position {}", i))
        })
        .collect()
}

/// Start the proxy server.
pub async fn start(config_path: &str) -> Result<()> {
    let config = match resolve_config_path(config_path)? {
        ConfigSource::Existing(path) => load_config_file(&path)?,
        ConfigSource::BundledPreset(preset) => {
            tracing::info!(
                preset = preset.name,
                path = %preset.path.display(),
                "Using bundled preset"
            );
            load_config_file(&preset.path)?
        }
        ConfigSource::Missing(path) => {
            tracing::info!("No config found, creating {} with defaults", path.display());
            let config = default_config();
            let toml_str = toml::to_string_pretty(&config)?;
            std::fs::write(&path, toml_str)?;
            config
        }
    };
    let key = resolve_vault_key(&config)?;
    let detector = Detector::from_config(&config.detection)?;
    let vault = Vault::open(&config.vault.path, key)?;
    let audit = AuditLogger::new(&config.audit.log_path, config.audit.log_entities)?;
    let api_key = resolve_proxy_api_key(&config);

    tracing::info!(
        listen = %config.proxy.listen,
        upstream = %config.proxy.upstream,
        "Starting CloakPipe proxy"
    );

    let state = AppState::new(config, detector, vault, audit, api_key);
    server::start(state).await
}

/// Test detection on sample text.
pub async fn test(config_path: &str, text: Option<String>, file: Option<String>) -> Result<()> {
    let input = match (text, file) {
        (Some(t), _) => t,
        (_, Some(f)) => {
            std::fs::read_to_string(&f).with_context(|| format!("Cannot read file: {}", f))?
        }
        (None, None) => {
            // Default sample text
            "Tata Motors reported revenue of $1.2M in Q3 2025. Contact: cfo@tatamotors.com. \
             AWS key: AKIAIOSFODNN7EXAMPLE. Server: 192.168.1.100"
                .to_string()
        }
    };

    let config = load_config_or_default(config_path)?;

    let detector = Detector::from_config(&config.detection)?;
    let mut vault = Vault::ephemeral();

    println!("\n--- Input ---");
    println!("{}", input);

    let entities = detector.detect(&input)?;
    println!("\n--- Detected Entities ({}) ---", entities.len());
    for e in &entities {
        println!(
            "  [{:?}] \"{}\" (confidence: {:.0}%, source: {:?})",
            e.category,
            e.original,
            e.confidence * 100.0,
            e.source,
        );
    }

    let result = Replacer::pseudonymize(&input, &entities, &mut vault)?;
    println!("\n--- Pseudonymized ---");
    println!("{}", result.text);

    let rehydrated = cloakpipe_core::rehydrator::Rehydrator::rehydrate(&result.text, &vault)?;
    println!("\n--- Rehydrated ---");
    println!("{}", rehydrated.text);
    println!("\n  Tokens rehydrated: {}", rehydrated.rehydrated_count);

    let roundtrip_ok = rehydrated.text == input;
    println!(
        "  Roundtrip match: {}",
        if roundtrip_ok { "YES" } else { "NO" }
    );

    Ok(())
}

/// Show vault statistics.
pub async fn stats(config_path: &str) -> Result<()> {
    let config = load_config(config_path)?;
    let key = resolve_vault_key(&config)?;
    let vault = Vault::open(&config.vault.path, key)?;
    let stats = vault.stats();

    println!("Vault: {}", config.vault.path);
    println!("Total mappings: {}", stats.total_mappings);
    if !stats.categories.is_empty() {
        println!("Categories:");
        for (cat, count) in &stats.categories {
            println!("  {}: {}", cat, count);
        }
    }

    Ok(())
}

/// Initialize a new config file.
pub async fn init() -> Result<()> {
    let path = "cloakpipe.toml";
    if std::path::Path::new(path).exists() {
        bail!("{} already exists", path);
    }

    let config = default_config();
    let toml_str = toml::to_string_pretty(&config)?;
    std::fs::write(path, toml_str)?;
    let preset_dir = install_bundled_presets()?;
    println!("Created {}", path);
    println!("Bundled presets installed in {}", preset_dir.display());
    println!("\nNext steps:");
    println!("  1. Set OPENAI_API_KEY (or your upstream API key)");
    println!("  2. Set CLOAKPIPE_VAULT_KEY (64-char hex string for encryption)");
    println!("  3. Run: cloakpipe start");
    println!("  4. Or run a bundled preset directly: cloakpipe --config dpdp.toml start");

    Ok(())
}

/// Bundled preset management commands.
pub async fn presets(action: crate::PresetCommands) -> Result<()> {
    match action {
        crate::PresetCommands::Install => {
            let preset_dir = install_bundled_presets()?;
            println!("Bundled presets installed in {}", preset_dir.display());
            for preset in bundled_presets() {
                println!("  {} — {}", preset.file_name, preset.description);
            }
            println!("\nUse them with:");
            println!("  cloakpipe --config dpdp.toml start");
        }
        crate::PresetCommands::List => {
            let preset_dir = installed_preset_dir()?;
            println!("Bundled presets:");
            for preset in bundled_presets() {
                let installed_path = preset_dir.join(preset.file_name);
                let status = if installed_path.exists() {
                    format!("installed at {}", installed_path.display())
                } else {
                    format!(
                        "embedded; installs to {} on first use or via `cloakpipe presets install`",
                        installed_path.display()
                    )
                };

                println!(
                    "  {} — {} ({})",
                    preset.file_name, preset.description, status
                );
            }
        }
    }

    Ok(())
}

/// Interactive guided setup.
pub async fn setup() -> Result<()> {
    use cloakpipe_core::profiles::IndustryProfile;
    use dialoguer::{Confirm, Select};

    println!("CloakPipe Setup\n");

    // 1. Industry profile
    let profiles = IndustryProfile::all();
    let _profile_names: Vec<&str> = profiles.iter().map(|p| p.name()).collect();
    let profile_descriptions = [
        "General — balanced defaults for most use cases",
        "Legal — NER for names, case numbers, SSNs; preserves numeric reasoning",
        "Healthcare — HIPAA-aware: MRN, NPI, DEA numbers; NER for patient names",
        "Fintech — financial data, SWIFT/ISIN/IBAN; IP and internal URL detection",
    ];

    let profile_idx = Select::new()
        .with_prompt("What industry are you in?")
        .items(&profile_descriptions)
        .default(0)
        .interact()?;
    let profile = profiles[profile_idx];

    // 2. Upstream provider
    let upstreams = [
        "OpenAI (https://api.openai.com)",
        "Azure OpenAI",
        "Anthropic (https://api.anthropic.com)",
        "Ollama / local (http://localhost:11434)",
        "Custom URL",
    ];
    let upstream_idx = Select::new()
        .with_prompt("Which LLM provider?")
        .items(&upstreams)
        .default(0)
        .interact()?;
    let (upstream, api_key_env) = match upstream_idx {
        0 => ("https://api.openai.com".to_string(), "OPENAI_API_KEY"),
        1 => (
            "https://YOUR_RESOURCE.openai.azure.com".to_string(),
            "AZURE_OPENAI_API_KEY",
        ),
        2 => ("https://api.anthropic.com".to_string(), "ANTHROPIC_API_KEY"),
        3 => ("http://localhost:11434".to_string(), "OLLAMA_API_KEY"),
        _ => {
            let url: String = dialoguer::Input::new()
                .with_prompt("Enter upstream URL")
                .interact_text()?;
            (url, "API_KEY")
        }
    };

    // 3. Vault backend
    let backends = ["File (vault.enc)", "SQLite (vault.db)"];
    let backend_idx = Select::new()
        .with_prompt("Vault storage backend?")
        .items(&backends)
        .default(0)
        .interact()?;
    let (vault_backend, vault_path) = match backend_idx {
        0 => ("file", "./vault.enc"),
        _ => ("sqlite", "./vault.db"),
    };

    // 4. Audit logging
    let audit_enabled = Confirm::new()
        .with_prompt("Enable audit logging?")
        .default(true)
        .interact()?;

    // Build config
    let detection = profile.detection_config();
    let mut config = default_config();
    config.profile = Some(profile.name().to_string());
    config.proxy.upstream = upstream;
    config.proxy.api_key_env = api_key_env.into();
    config.vault.backend = vault_backend.into();
    config.vault.path = vault_path.into();
    config.detection = detection;
    config.audit = cloakpipe_core::config::AuditConfig {
        enabled: audit_enabled,
        ..Default::default()
    };

    let path = "cloakpipe.toml";
    let toml_str = toml::to_string_pretty(&config)?;
    std::fs::write(path, &toml_str)?;
    let preset_dir = install_bundled_presets()?;

    println!("\nCreated {} with profile: {}", path, profile);
    println!("Bundled presets installed in {}", preset_dir.display());
    println!("\nNext steps:");
    println!("  1. Set {} (your API key)", api_key_env);
    println!("  2. Set CLOAKPIPE_VAULT_KEY=$(openssl rand -hex 32)");
    println!("  3. Run: cloakpipe start");
    println!("  4. Or use a bundled preset: cloakpipe --config dpdp.toml start");

    Ok(())
}

/// NER management commands.
pub async fn ner(action: crate::NerCommands) -> Result<()> {
    match action {
        crate::NerCommands::Install {
            backend,
            dry_run,
            python,
            no_verify,
        } => match backend {
            crate::NerInstallBackend::GlinerPii => install_gliner_pii(python, dry_run, no_verify),
        },
        crate::NerCommands::Start {
            backend,
            python,
            host,
            port,
            threshold,
            dry_run,
        } => match backend {
            crate::NerInstallBackend::GlinerPii => {
                start_gliner_pii(python, host, port, threshold, dry_run)
            }
        },
    }
}

fn install_gliner_pii(python: Option<String>, dry_run: bool, no_verify: bool) -> Result<()> {
    let python = PathBuf::from(match python {
        Some(python) => python,
        None if dry_run => "python3".to_string(),
        None => detect_python_interpreter()?,
    });
    let venv_dir = default_gliner_venv_dir();

    let install_args = ["-m", "pip", "install", GLINER_PIP_PACKAGE];

    if dry_run {
        println!("NER backend: gliner-pii");
        println!("Would run: {}", format_command(&python, &install_args));
        let venv_dir_arg = path_arg(&venv_dir);
        let venv_args = ["-m", "venv", venv_dir_arg.as_str()];
        let venv_python = virtualenv_python_path(&venv_dir);
        println!("If pip is blocked by PEP 668, CloakPipe will fall back to:");
        println!("  {}", format_command(&python, &venv_args));
        println!("  {}", format_command(&venv_python, &install_args));
        print_gliner_next_steps();
        return Ok(());
    }

    println!("Installing GLiNER-PII sidecar dependency...");
    println!("  {}", format_command(&python, &install_args));

    let install_output = std::process::Command::new(&python)
        .args(install_args)
        .output()
        .with_context(|| {
            format!(
                "Failed to launch {}",
                format_command(&python, &install_args)
            )
        })?;

    let install_python = if install_output.status.success() {
        python
    } else if is_externally_managed_environment(&install_output) {
        println!("Detected an externally managed Python environment.");
        println!(
            "Falling back to a local virtualenv at {}",
            venv_dir.display()
        );

        let venv_python = ensure_virtualenv(&python, &venv_dir)?;
        println!("  {}", format_command(&venv_python, &install_args));

        let venv_install_output = std::process::Command::new(&venv_python)
            .args(install_args)
            .output()
            .with_context(|| {
                format!(
                    "Failed to launch {}",
                    format_command(&venv_python, &install_args)
                )
            })?;

        if !venv_install_output.status.success() {
            bail!(
                "GLiNER install failed in the local virtualenv.\n{}",
                render_process_output(&venv_install_output)
            );
        }

        venv_python
    } else {
        bail!(
            "GLiNER install failed.\n{}",
            render_process_output(&install_output)
        );
    };

    if !no_verify {
        verify_gliner_import(&install_python)?;
    }

    println!("Installed {} successfully.", GLINER_PIP_PACKAGE);
    if no_verify {
        println!("Skipped import verification (--no-verify).");
    }
    print_gliner_next_steps();

    Ok(())
}

fn start_gliner_pii(
    python: Option<String>,
    host: String,
    port: u16,
    threshold: f64,
    dry_run: bool,
) -> Result<()> {
    let current_dir = std::env::current_dir().context("Cannot determine current directory")?;
    let project_root = find_gliner_project_root(&current_dir)?;
    let script_path = project_root.join(GLINER_SERVER_SCRIPT);
    let python = resolve_gliner_start_python(python, &project_root, dry_run)?;

    let args = vec![
        path_arg(&script_path),
        "--host".to_string(),
        host,
        "--port".to_string(),
        port.to_string(),
        "--threshold".to_string(),
        threshold.to_string(),
    ];

    println!("Starting GLiNER-PII sidecar...");
    println!("  {}", format_command(&python, &args));

    if dry_run {
        return Ok(());
    }

    ensure_gliner_import_available(&python)?;

    let status = std::process::Command::new(&python)
        .args(&args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("Failed to launch {}", format_command(&python, &args)))?;

    if status.success() {
        Ok(())
    } else if let Some(code) = status.code() {
        bail!("GLiNER-PII sidecar exited with status code {}", code)
    } else {
        bail!("GLiNER-PII sidecar terminated before exiting cleanly")
    }
}

fn detect_python_interpreter() -> Result<String> {
    for candidate in ["python3", "python", "py"] {
        if command_available(candidate) {
            return Ok(candidate.to_string());
        }
    }

    bail!("No Python interpreter found. Install Python 3 or rerun with --python <path>.")
}

fn command_available(command: &str) -> bool {
    std::process::Command::new(command)
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn default_gliner_venv_dir() -> PathBuf {
    PathBuf::from(GLINER_VENV_DIR)
}

fn gliner_venv_dir(project_root: &Path) -> PathBuf {
    project_root.join(GLINER_VENV_DIR)
}

fn find_gliner_project_root(start_dir: &Path) -> Result<PathBuf> {
    for dir in start_dir.ancestors() {
        if dir.join(GLINER_SERVER_SCRIPT).exists() {
            return Ok(dir.to_path_buf());
        }
    }

    bail!(
        "Could not find {} from {} or any parent directory. Run this command from a CloakPipe checkout.",
        GLINER_SERVER_SCRIPT,
        start_dir.display()
    )
}

fn resolve_gliner_start_python(
    python: Option<String>,
    project_root: &Path,
    dry_run: bool,
) -> Result<PathBuf> {
    if let Some(python) = python {
        return Ok(PathBuf::from(python));
    }

    let managed_python = virtualenv_python_path(&gliner_venv_dir(project_root));
    if managed_python.exists() {
        return Ok(managed_python);
    }

    if dry_run {
        return Ok(PathBuf::from("python3"));
    }

    Ok(PathBuf::from(detect_python_interpreter()?))
}

fn ensure_virtualenv(base_python: &Path, venv_dir: &Path) -> Result<PathBuf> {
    let venv_python = virtualenv_python_path(venv_dir);
    if venv_python.exists() {
        return Ok(venv_python);
    }

    if let Some(parent) = venv_dir.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Cannot create runtime directory: {}", parent.display()))?;
    }

    let venv_dir_arg = path_arg(venv_dir);
    let venv_args = ["-m", "venv", venv_dir_arg.as_str()];
    println!("  {}", format_command(base_python, &venv_args));

    let create_output = std::process::Command::new(base_python)
        .args(venv_args)
        .output()
        .with_context(|| {
            format!(
                "Failed to launch {}",
                format_command(base_python, &venv_args)
            )
        })?;

    if !create_output.status.success() {
        bail!(
            "Failed to create a local GLiNER virtualenv.\n{}",
            render_process_output(&create_output)
        );
    }

    if !venv_python.exists() {
        bail!(
            "Created virtualenv at {}, but no Python interpreter was found at {}.",
            venv_dir.display(),
            venv_python.display()
        );
    }

    Ok(venv_python)
}

fn verify_gliner_import(python: &Path) -> Result<()> {
    let verify_output = gliner_import_check(python)?;

    if !verify_output.status.success() {
        bail!(
            "Installed package but verification failed.\n{}",
            render_process_output(&verify_output)
        );
    }

    Ok(())
}

fn ensure_gliner_import_available(python: &Path) -> Result<()> {
    let verify_output = gliner_import_check(python)?;

    if !verify_output.status.success() {
        bail!(
            "Python at {} cannot import gliner. Run `cloakpipe ner install` first or pass --python <path>.\n{}",
            python.display(),
            render_process_output(&verify_output)
        );
    }

    Ok(())
}

fn gliner_import_check(python: &Path) -> Result<std::process::Output> {
    let verify_args = ["-c", "from gliner import GLiNER; print('GLiNER import OK')"];
    std::process::Command::new(python)
        .args(verify_args)
        .output()
        .with_context(|| format!("Failed to launch {}", format_command(python, &verify_args)))
}

fn virtualenv_python_path(venv_dir: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        venv_dir.join("Scripts").join("python.exe")
    }

    #[cfg(not(windows))]
    {
        venv_dir.join("bin").join("python")
    }
}

fn is_externally_managed_environment(output: &std::process::Output) -> bool {
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
    .to_ascii_lowercase();

    combined.contains("externally-managed-environment")
        || combined.contains("externally managed")
        || combined.contains("pep 668")
}

fn render_process_output(output: &std::process::Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

    match (stdout.is_empty(), stderr.is_empty()) {
        (true, true) => "Process exited without output.".to_string(),
        (false, true) => stdout,
        (true, false) => stderr,
        (false, false) => format!("stdout:\n{}\n\nstderr:\n{}", stdout, stderr),
    }
}

fn format_command<S: AsRef<str>>(command: &Path, args: &[S]) -> String {
    let mut parts = Vec::with_capacity(args.len() + 1);
    parts.push(path_arg(command));
    parts.extend(args.iter().map(|arg| arg.as_ref().to_string()));

    parts
        .iter()
        .map(|arg| format_cli_arg(arg))
        .collect::<Vec<_>>()
        .join(" ")
}

fn path_arg(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn format_cli_arg(arg: &str) -> String {
    if arg.chars().any(char::is_whitespace) {
        format!("\"{}\"", arg.replace('"', "\\\""))
    } else {
        arg.to_string()
    }
}

fn print_gliner_next_steps() {
    println!("\nNext steps:");
    println!("  1. Start the sidecar: cloakpipe ner start");
    println!("  2. Enable this in cloakpipe.toml:");
    println!("     [detection.ner]");
    println!("     enabled = true");
    println!("     backend = \"gliner_pii\"");
    println!("     sidecar_url = \"{}\"", GLINER_SIDECAR_URL);
    println!("     confidence_threshold = 0.4");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    use std::os::unix::process::ExitStatusExt;
    #[cfg(windows)]
    use std::os::windows::process::ExitStatusExt;

    #[test]
    fn detects_externally_managed_python_errors() {
        let output = std::process::Output {
            status: failure_status(),
            stdout: Vec::new(),
            stderr: b"error: externally-managed-environment".to_vec(),
        };

        assert!(is_externally_managed_environment(&output));
    }

    #[test]
    fn default_gliner_venv_dir_is_project_local() {
        assert_eq!(
            default_gliner_venv_dir(),
            PathBuf::from(".cloakpipe/gliner-pii-venv")
        );
    }

    #[test]
    fn gliner_venv_dir_is_relative_to_project_root() {
        assert_eq!(
            gliner_venv_dir(Path::new("/tmp/cloakpipe")),
            PathBuf::from("/tmp/cloakpipe/.cloakpipe/gliner-pii-venv")
        );
    }

    fn failure_status() -> std::process::ExitStatus {
        #[cfg(unix)]
        {
            std::process::ExitStatus::from_raw(1)
        }

        #[cfg(windows)]
        {
            std::process::ExitStatus::from_raw(1)
        }
    }
}

/// Start as MCP server (stdio transport).
pub async fn mcp(config_path: &str) -> Result<()> {
    let config = load_config_or_default(config_path)?;

    let key = resolve_vault_key(&config)?;
    let vault = cloakpipe_core::vault::Vault::open(&config.vault.path, key)?;
    let detector = cloakpipe_core::detector::Detector::from_config(&config.detection)?;

    cloakpipe_mcp::serve_stdio(config, detector, vault).await
}

/// CloakTree commands — vectorless document retrieval.
pub async fn tree(config_path: &str, action: crate::TreeCommands) -> Result<()> {
    let config = load_config_or_default(config_path)?;

    let tree_config = &config.tree;

    match action {
        crate::TreeCommands::Index { file, no_summaries } => {
            let api_key = std::env::var(&config.proxy.api_key_env).unwrap_or_default();
            let mut tc = tree_config.clone();
            if no_summaries {
                tc.add_node_summaries = false;
            }

            let indexer =
                cloakpipe_tree::TreeIndexer::new(tc, api_key, config.proxy.upstream.clone());

            let tree_index = indexer.build_index(&file).await?;
            let path =
                cloakpipe_tree::storage::TreeStorage::save(&tree_index, &tree_config.storage_path)?;

            println!("Tree index created:");
            println!("  ID:     {}", tree_index.id);
            println!("  Source: {}", tree_index.source);
            println!("  Nodes:  {}", tree_index.node_count());
            println!("  Depth:  {}", tree_index.max_depth());
            println!("  Pages:  {}", tree_index.total_pages);
            if let Some(desc) = &tree_index.description {
                println!("  Desc:   {}", desc);
            }
            println!("  Saved:  {}", path);
        }

        crate::TreeCommands::Search { index, query } => {
            let api_key = std::env::var(&config.proxy.api_key_env)
                .context("API key required for tree search")?;

            let tree_index = cloakpipe_tree::storage::TreeStorage::load(&index)?;
            let searcher = cloakpipe_tree::TreeSearcher::new(
                api_key,
                config.proxy.upstream.clone(),
                tree_config.search_model.clone(),
            );

            let result = searcher.search(&tree_index, &query).await?;

            println!("Search results for: {}", query);
            println!("  Reasoning: {}", result.reasoning);
            if let Some(conf) = result.confidence {
                println!("  Confidence: {:.0}%", conf * 100.0);
            }
            println!("  Matching nodes:");
            for id in &result.node_ids {
                if let Some(node) = tree_index.find_node(id) {
                    println!(
                        "    [{}] {} (pages {}-{})",
                        id, node.title, node.pages.0, node.pages.1
                    );
                    if let Some(summary) = &node.summary {
                        println!("          {}", summary.text);
                    }
                }
            }
        }

        crate::TreeCommands::List => {
            let trees = cloakpipe_tree::storage::TreeStorage::list(&tree_config.storage_path)?;
            if trees.is_empty() {
                println!("No tree indices found in {}", tree_config.storage_path);
                println!("Create one with: cloakpipe tree index <file>");
            } else {
                println!("Tree indices ({}):", trees.len());
                for (id, source) in &trees {
                    println!("  {} -> {}", id, source);
                }
            }
        }

        crate::TreeCommands::Query { file, question } => {
            let api_key = std::env::var(&config.proxy.api_key_env)
                .context("API key required for tree query")?;

            // If file is a .json, load existing index; otherwise build one
            let (tree_index, pages) = if file.ends_with(".json") {
                let tree_index = cloakpipe_tree::storage::TreeStorage::load(&file)?;
                let pages = cloakpipe_tree::parser::parse_document(&tree_index.source)?;
                (tree_index, pages)
            } else {
                let indexer = cloakpipe_tree::TreeIndexer::new(
                    tree_config.clone(),
                    api_key.clone(),
                    config.proxy.upstream.clone(),
                );
                let tree_index = indexer.build_index(&file).await?;
                let pages = cloakpipe_tree::parser::parse_document(&file)?;

                // Save for future use
                let path = cloakpipe_tree::storage::TreeStorage::save(
                    &tree_index,
                    &tree_config.storage_path,
                )?;
                println!("Index saved: {}\n", path);
                (tree_index, pages)
            };

            // Search
            let searcher = cloakpipe_tree::TreeSearcher::new(
                api_key.clone(),
                config.proxy.upstream.clone(),
                tree_config.search_model.clone(),
            );
            let result = searcher.search(&tree_index, &question).await?;

            // Extract content from matching nodes
            let content = cloakpipe_tree::extractor::ContentExtractor::extract(
                &tree_index,
                &result.node_ids,
                &pages,
            )?;

            let context_text: String = content
                .iter()
                .map(|c| format!("[{}] {}\n{}", c.node_id, c.title, c.text))
                .collect::<Vec<_>>()
                .join("\n\n---\n\n");

            // Send to LLM for final answer
            let prompt = format!(
                "Based on the following document excerpts, answer the question.\n\n\
                 EXCERPTS:\n{}\n\n\
                 QUESTION: {}\n\n\
                 Answer concisely based only on the provided excerpts.",
                context_text, question
            );

            let body = serde_json::json!({
                "model": tree_config.search_model,
                "messages": [
                    {"role": "system", "content": "You answer questions based on provided document excerpts. Be precise and cite section titles when relevant."},
                    {"role": "user", "content": prompt}
                ],
                "max_tokens": 1000,
                "temperature": 0.3
            });

            let url = format!(
                "{}/v1/chat/completions",
                config.proxy.upstream.trim_end_matches('/')
            );
            let client = reqwest::Client::new();
            let response = client
                .post(&url)
                .header("Authorization", format!("Bearer {}", api_key))
                .json(&body)
                .send()
                .await?
                .json::<serde_json::Value>()
                .await?;

            let answer = response["choices"][0]["message"]["content"]
                .as_str()
                .unwrap_or("No answer generated");

            println!("Question: {}\n", question);
            println!("Sources ({}):", result.node_ids.len());
            for c in &content {
                println!(
                    "  [{}] {} (pages {}-{})",
                    c.node_id, c.title, c.pages.0, c.pages.1
                );
            }
            println!("\nAnswer:\n{}", answer);
        }

        crate::TreeCommands::Show { index } => {
            let tree_index = cloakpipe_tree::storage::TreeStorage::load(&index)?;

            println!("Tree Index: {}", tree_index.id);
            println!("  Source:  {}", tree_index.source);
            println!("  Model:   {}", tree_index.model);
            println!("  Pages:   {}", tree_index.total_pages);
            println!("  Nodes:   {}", tree_index.node_count());
            println!("  Depth:   {}", tree_index.max_depth());
            println!("  Created: {}", tree_index.created_at);
            if let Some(desc) = &tree_index.description {
                println!("  Desc:    {}", desc);
            }
            println!("\nTree structure:");
            for entry in tree_index.navigation_map() {
                println!("  {}", entry);
            }
        }
    }

    Ok(())
}

/// ADCPE vector encryption commands.
pub async fn vector(action: crate::VectorCommands) -> Result<()> {
    match action {
        crate::VectorCommands::Encrypt { input, output, dim } => {
            let key = resolve_vector_key()?;
            let config = cloakpipe_vector::AdcpeConfig {
                dimensions: dim,
                noise_scale: 0.0,
            };
            let mut enc = cloakpipe_vector::AdcpeEncryptor::new(&key, &config)?;

            let data = std::fs::read_to_string(&input)
                .with_context(|| format!("Cannot read: {}", input))?;
            let vectors: Vec<Vec<f64>> = serde_json::from_str(&data)
                .context("Input must be a JSON array of float arrays")?;

            let encrypted = enc.encrypt_batch(&vectors)?;
            let json = serde_json::to_string_pretty(&encrypted)?;
            std::fs::write(&output, json)?;

            println!(
                "Encrypted {} vectors (dim={}) -> {}",
                vectors.len(),
                dim,
                output
            );
        }

        crate::VectorCommands::Decrypt { input, output, dim } => {
            let key = resolve_vector_key()?;
            let config = cloakpipe_vector::AdcpeConfig {
                dimensions: dim,
                noise_scale: 0.0,
            };
            let enc = cloakpipe_vector::AdcpeEncryptor::new(&key, &config)?;

            let data = std::fs::read_to_string(&input)
                .with_context(|| format!("Cannot read: {}", input))?;
            let encrypted: Vec<Vec<f64>> = serde_json::from_str(&data)
                .context("Input must be a JSON array of float arrays")?;

            let decrypted = enc.decrypt_batch(&encrypted)?;
            let json = serde_json::to_string_pretty(&decrypted)?;
            std::fs::write(&output, json)?;

            println!(
                "Decrypted {} vectors (dim={}) -> {}",
                encrypted.len(),
                dim,
                output
            );
        }

        crate::VectorCommands::Test { dim } => {
            let key = resolve_vector_key()?;
            let config = cloakpipe_vector::AdcpeConfig {
                dimensions: dim,
                noise_scale: 0.0,
            };
            let mut enc = cloakpipe_vector::AdcpeEncryptor::new(&key, &config)?;

            // Generate sample vectors
            use rand::Rng;
            let mut rng = rand::thread_rng();
            let a: Vec<f64> = (0..dim).map(|_| rng.gen::<f64>() - 0.5).collect();
            let b: Vec<f64> = (0..dim).map(|_| rng.gen::<f64>() - 0.5).collect();

            let cos_orig = cloakpipe_vector::adcpe::cosine_similarity(&a, &b);

            let ea = enc.encrypt(&a)?;
            let eb = enc.encrypt(&b)?;
            let cos_enc = cloakpipe_vector::adcpe::cosine_similarity(&ea, &eb);

            let da = enc.decrypt(&ea)?;
            let max_err: f64 = a
                .iter()
                .zip(da.iter())
                .map(|(x, y)| (x - y).abs())
                .fold(0.0, f64::max);

            println!("ADCPE Test (dim={})", dim);
            println!("  Cosine similarity (original):  {:.6}", cos_orig);
            println!("  Cosine similarity (encrypted): {:.6}", cos_enc);
            println!(
                "  Distance preserved: {}",
                if (cos_orig - cos_enc).abs() < 1e-10 {
                    "YES"
                } else {
                    "NO"
                }
            );
            println!("  Roundtrip max error: {:.2e}", max_err);
            println!(
                "  Roundtrip exact: {}",
                if max_err < 1e-10 { "YES" } else { "NO" }
            );
        }
    }

    Ok(())
}

/// Resolve the ADCPE vector encryption key from env.
fn resolve_vector_key() -> Result<[u8; 32]> {
    let env_var = "CLOAKPIPE_VECTOR_KEY";
    match std::env::var(env_var) {
        Ok(hex_key) => {
            let bytes = hex_decode(&hex_key)
                .with_context(|| format!("{} must be a 64-char hex string", env_var))?;
            if bytes.len() != 32 {
                bail!("{} must be 32 bytes (got {})", env_var, bytes.len());
            }
            let mut key = [0u8; 32];
            key.copy_from_slice(&bytes);
            Ok(key)
        }
        Err(_) => {
            tracing::warn!("No {} set — generating ephemeral key", env_var);
            let mut key = [0u8; 32];
            rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut key);
            Ok(key)
        }
    }
}

fn default_config() -> CloakPipeConfig {
    CloakPipeConfig {
        proxy: cloakpipe_core::config::ProxyConfig {
            listen: "127.0.0.1:8900".into(),
            upstream: "https://api.openai.com".into(),
            api_key_env: "OPENAI_API_KEY".into(),
            timeout_seconds: 120,
            max_concurrent: 256,
            mode: "proxy".into(),
            masking_strategy: cloakpipe_core::MaskingStrategy::Token,
        },
        vault: cloakpipe_core::config::VaultConfig {
            path: "./vault.enc".into(),
            encryption: "aes-256-gcm".into(),
            key_env: Some("CLOAKPIPE_VAULT_KEY".into()),
            key_keyring: false,
            backend: "file".into(),
        },
        profile: None,
        detection: cloakpipe_core::config::DetectionConfig {
            secrets: true,
            financial: true,
            dates: true,
            emails: true,
            phone_numbers: false,
            ip_addresses: false,
            urls_internal: false,
            ner: Default::default(),
            custom: Default::default(),
            overrides: Default::default(),
            resolver: Default::default(),
        },
        tree: Default::default(),
        vectors: Default::default(),
        local: Default::default(),
        audit: Default::default(),
        session: Default::default(),
    }
}

/// RAG pipeline scan — detect and optionally mask PII across files/directories.
pub async fn scan(
    config_path: &str,
    input: String,
    output: Option<String>,
    strategy: String,
    detect_only: bool,
    min_confidence: f64,
) -> Result<()> {
    let config = load_config_or_default(config_path)?;

    let detector = Detector::from_config(&config.detection)?;
    let masking_strategy = match strategy.as_str() {
        "format-preserving" | "fp" => cloakpipe_core::MaskingStrategy::FormatPreserving,
        _ => cloakpipe_core::MaskingStrategy::Token,
    };

    let input_path = std::path::Path::new(&input);
    let files = collect_scannable_files(input_path)?;

    if files.is_empty() {
        println!("No scannable files found in: {}", input);
        return Ok(());
    }

    let output_dir = if detect_only {
        None
    } else {
        let dir = output.unwrap_or_else(|| format!("{}-masked", input.trim_end_matches('/')));
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("Cannot create output dir: {}", dir))?;
        Some(dir)
    };

    let mut vault = Vault::ephemeral();
    let mut total_entities = 0usize;
    let mut total_files = 0usize;
    let mut file_results: Vec<(String, usize)> = Vec::new();

    println!("Scanning {} files for PII...\n", files.len());

    for file_path in &files {
        let content = std::fs::read_to_string(file_path)
            .with_context(|| format!("Cannot read: {}", file_path.display()))?;

        let mut entities = detector.detect(&content)?;
        entities.retain(|e| e.confidence >= min_confidence);

        let entity_count = entities.len();
        total_entities += entity_count;

        let rel_path = file_path
            .strip_prefix(input_path)
            .unwrap_or(file_path)
            .to_string_lossy()
            .to_string();

        if entity_count > 0 {
            total_files += 1;
            file_results.push((rel_path.clone(), entity_count));

            if detect_only {
                println!("  {} — {} entities", rel_path, entity_count);
                for e in &entities {
                    println!(
                        "    [{:?}] \"{}\" (confidence: {:.0}%, source: {:?})",
                        e.category,
                        e.original,
                        e.confidence * 100.0,
                        e.source,
                    );
                }
            }
        }

        if let Some(ref out_dir) = output_dir {
            entities.sort_by_key(|e| e.start);

            let masked_content = if entities.is_empty() {
                content.clone()
            } else {
                Replacer::pseudonymize_with_strategy(
                    &content,
                    &entities,
                    &mut vault,
                    masking_strategy,
                )?
                .text
            };

            let out_path = std::path::Path::new(out_dir).join(&rel_path);
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&out_path, masked_content)?;
        }
    }

    println!("\n--- Scan Summary ---");
    println!("  Files scanned:  {}", files.len());
    println!("  Files with PII: {}", total_files);
    println!("  Total entities: {}", total_entities);
    println!("  Strategy:       {}", strategy);
    println!("  Min confidence: {:.0}%", min_confidence * 100.0);

    if let Some(ref out_dir) = output_dir {
        // Save vault mappings as JSON for rehydration
        let mappings_path = format!("{}/vault-mappings.json", out_dir);
        let mappings = vault.reverse_mappings();
        let json = serde_json::to_string_pretty(&mappings)?;
        std::fs::write(&mappings_path, json)?;

        println!("  Output dir:     {}", out_dir);
        println!("  Vault mappings: {}", mappings_path);
    }

    if !file_results.is_empty() && !detect_only {
        println!("\n  Files masked:");
        for (path, count) in &file_results {
            println!("    {} ({} entities)", path, count);
        }
    }

    Ok(())
}

/// Collect scannable text files from a path (file or directory).
fn collect_scannable_files(path: &std::path::Path) -> Result<Vec<std::path::PathBuf>> {
    let extensions = [
        "txt", "md", "json", "csv", "toml", "yaml", "yml", "xml", "html",
    ];

    if path.is_file() {
        return Ok(vec![path.to_path_buf()]);
    }

    if !path.is_dir() {
        bail!("Path does not exist: {}", path.display());
    }

    let mut files = Vec::new();
    collect_files_recursive(path, &extensions, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_files_recursive(
    dir: &std::path::Path,
    extensions: &[&str],
    files: &mut Vec<std::path::PathBuf>,
) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_files_recursive(&path, extensions, files)?;
        } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if extensions.contains(&ext) {
                files.push(path);
            }
        }
    }
    Ok(())
}

/// Session management commands — talks to the running proxy's HTTP API.
pub async fn sessions(config_path: &str, action: crate::SessionCommands) -> Result<()> {
    let config = load_config_or_default(config_path)?;

    let base = format!("http://{}", config.proxy.listen);
    let client = reqwest::Client::new();

    match action {
        crate::SessionCommands::List => {
            let resp = client
                .get(format!("{}/sessions", base))
                .send()
                .await
                .context("Cannot reach proxy — is it running?")?
                .json::<serde_json::Value>()
                .await?;

            let sessions = resp.as_array().map(|a| a.len()).unwrap_or(0);
            if sessions == 0 {
                println!("No active sessions.");
                println!("Sessions are created when requests include x-session-id header.");
            } else {
                println!("Active sessions ({}):\n", sessions);
                for sess in resp.as_array().unwrap() {
                    println!(
                        "  {} | {} msgs | {} entities | sensitivity: {} | last: {}",
                        sess["session_id"].as_str().unwrap_or("?"),
                        sess["message_count"],
                        sess["entity_count"],
                        sess["sensitivity"].as_str().unwrap_or("normal"),
                        sess["last_activity"].as_str().unwrap_or("?"),
                    );
                }
            }
        }

        crate::SessionCommands::Inspect { session_id } => {
            let resp = client
                .get(format!("{}/sessions/{}", base, session_id))
                .send()
                .await
                .context("Cannot reach proxy — is it running?")?;

            if resp.status() == 404 {
                bail!("Session {} not found", session_id);
            }

            let stats: serde_json::Value = resp.json().await?;
            println!("Session: {}", session_id);
            println!("  Messages:      {}", stats["message_count"]);
            println!("  Entities:      {}", stats["entity_count"]);
            println!("  Coreferences:  {}", stats["coreference_count"]);
            println!(
                "  Sensitivity:   {}",
                stats["sensitivity"].as_str().unwrap_or("normal")
            );
            if let Some(keywords) = stats["escalation_keywords"].as_array() {
                if !keywords.is_empty() {
                    let kw: Vec<&str> = keywords.iter().filter_map(|k| k.as_str()).collect();
                    println!("  Keywords:      {}", kw.join(", "));
                }
            }
            if let Some(cats) = stats["categories"].as_object() {
                println!("  Categories:");
                for (cat, count) in cats {
                    println!("    {}: {}", cat, count);
                }
            }
            println!(
                "  Created:       {}",
                stats["created_at"].as_str().unwrap_or("?")
            );
            println!(
                "  Last activity: {}",
                stats["last_activity"].as_str().unwrap_or("?")
            );
        }

        crate::SessionCommands::Flush { session_id } => {
            let resp = client
                .delete(format!("{}/sessions/{}", base, session_id))
                .send()
                .await
                .context("Cannot reach proxy — is it running?")?
                .json::<serde_json::Value>()
                .await?;

            if resp["flushed"].as_bool() == Some(true) {
                println!("Session {} flushed.", session_id);
            } else {
                println!("Session {} not found.", session_id);
            }
        }

        crate::SessionCommands::FlushAll => {
            let resp = client
                .delete(format!("{}/sessions", base))
                .send()
                .await
                .context("Cannot reach proxy — is it running?")?
                .json::<serde_json::Value>()
                .await?;

            println!("Flushed {} sessions.", resp["flushed"]);
        }
    }

    Ok(())
}
