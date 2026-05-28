//! CLI command implementations.

use crate::presets::{
    bundled_presets, install_bundled_presets, installed_preset_dir, resolve_installed_preset,
    ResolvedPreset,
};
use anyhow::{bail, Context, Result};
use cloakpipe_audit::AuditSink;
use cloakpipe_core::{
    config::{CloakPipeConfig, CustomPattern, DetectionConfig, NerBackend, ProxyMode},
    detector::Detector,
    paths,
    rehydrator::Rehydrator,
    replacer::Replacer,
    vault::Vault,
    MaskingStrategy,
};
use cloakpipe_proxy::{outbound_proxy, server, state::AppState, tls_mitm};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;

const GLINER_PIP_PACKAGE: &str = "gliner";
/// PyTorch's own wheel index. Needed on Python versions (e.g. 3.13) where
/// torch is not yet published to the main PyPI index.
const GLINER_TORCH_INDEX: &str = "https://download.pytorch.org/whl/cpu";
/// Python versions known to have PyTorch wheels, in order of preference.
/// Python 3.14+ does not have torch wheels yet.
const TORCH_COMPATIBLE_PYTHONS: &[&str] = &[
    "python3.12",
    "python3.11",
    "python3.10",
    "python3.9",
    "python3.13",
];
const GLINER_SIDECAR_URL: &str = "http://127.0.0.1:9111";
const GLINER_SERVER_SCRIPT: &str = "tools/gliner-pii-server.py";

const DISTILBERT_DOWNLOAD_SCRIPT: &str = "tools/download_model.sh";

enum ConfigSource {
    Existing(PathBuf),
    BundledPreset(ResolvedPreset),
    Missing(PathBuf),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConfigSourceKind {
    ExplicitPath,
    BundledPreset,
    Project,
    Global,
}

struct ResolvedConfig {
    config: CloakPipeConfig,
    path: PathBuf,
    base_dir: PathBuf,
    source: ConfigSourceKind,
    preset_name: Option<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EditablePolicySource {
    ExistingFile,
    BundledPreset,
    MissingFile,
}

struct EditablePolicy {
    path: PathBuf,
    config: CloakPipeConfig,
    source: EditablePolicySource,
    preset_name: Option<&'static str>,
}

#[derive(Clone, Copy)]
enum DetectionFamily {
    Secrets,
    Financial,
    Dates,
    Emails,
    PhoneNumbers,
    IpAddresses,
    InternalUrls,
}

impl DetectionFamily {
    fn enabled(self, config: &DetectionConfig) -> bool {
        match self {
            Self::Secrets => config.secrets,
            Self::Financial => config.financial,
            Self::Dates => config.dates,
            Self::Emails => config.emails,
            Self::PhoneNumbers => config.phone_numbers,
            Self::IpAddresses => config.ip_addresses,
            Self::InternalUrls => config.urls_internal,
        }
    }

    fn set(self, config: &mut DetectionConfig, enabled: bool) {
        match self {
            Self::Secrets => config.secrets = enabled,
            Self::Financial => config.financial = enabled,
            Self::Dates => config.dates = enabled,
            Self::Emails => config.emails = enabled,
            Self::PhoneNumbers => config.phone_numbers = enabled,
            Self::IpAddresses => config.ip_addresses = enabled,
            Self::InternalUrls => config.urls_internal = enabled,
        }
    }
}

struct DetectionToggle {
    family: DetectionFamily,
    label: &'static str,
}

const DETECTION_TOGGLES: &[DetectionToggle] = &[
    DetectionToggle {
        family: DetectionFamily::Secrets,
        label: "Secrets and API keys",
    },
    DetectionToggle {
        family: DetectionFamily::Financial,
        label: "Financial amounts and identifiers",
    },
    DetectionToggle {
        family: DetectionFamily::Dates,
        label: "Dates and fiscal periods",
    },
    DetectionToggle {
        family: DetectionFamily::Emails,
        label: "Email addresses",
    },
    DetectionToggle {
        family: DetectionFamily::PhoneNumbers,
        label: "Phone numbers",
    },
    DetectionToggle {
        family: DetectionFamily::IpAddresses,
        label: "IP addresses",
    },
    DetectionToggle {
        family: DetectionFamily::InternalUrls,
        label: "Internal URLs",
    },
];

#[derive(Clone, Copy)]
struct StrategyOption {
    label: &'static str,
    value: MaskingStrategy,
}

const STRATEGY_OPTIONS: &[StrategyOption] = &[
    StrategyOption {
        label: "similar",
        value: MaskingStrategy::Similar,
    },
    StrategyOption {
        label: "format-preserving",
        value: MaskingStrategy::FormatPreserving,
    },
    StrategyOption {
        label: "token",
        value: MaskingStrategy::Token,
    },
];

#[derive(Clone, Copy)]
struct NerBackendOption {
    label: &'static str,
    value: NerBackend,
}

const NER_BACKEND_OPTIONS: &[NerBackendOption] = &[
    NerBackendOption {
        label: "distilbert_pii",
        value: NerBackend::DistilBertPii,
    },
    NerBackendOption {
        label: "gliner_pii",
        value: NerBackend::GlinerPii,
    },
    NerBackendOption {
        label: "bert",
        value: NerBackend::Bert,
    },
    NerBackendOption {
        label: "gliner",
        value: NerBackend::Gliner,
    },
];

#[derive(Default)]
struct PolicyEditSummary {
    detection_rules: bool,
    replacement_strategy: bool,
    ner_settings: bool,
    preserve_list: bool,
    force_list: bool,
    custom_patterns: bool,
}

impl PolicyEditSummary {
    fn descriptions(&self) -> Vec<&'static str> {
        let mut descriptions = Vec::new();
        if self.detection_rules {
            descriptions.push("detection rule families");
        }
        if self.replacement_strategy {
            descriptions.push("replacement strategy");
        }
        if self.ner_settings {
            descriptions.push("NER settings");
        }
        if self.preserve_list {
            descriptions.push("preserve list");
        }
        if self.force_list {
            descriptions.push("force list");
        }
        if self.custom_patterns {
            descriptions.push("custom regex patterns");
        }
        descriptions
    }
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

fn load_resolved_config(config_path: Option<&str>) -> Result<ResolvedConfig> {
    match config_path.map(str::trim).filter(|value| !value.is_empty()) {
        Some(config_path) => load_explicit_config(config_path),
        None => load_discovered_config(),
    }
}

fn load_explicit_config(config_path: &str) -> Result<ResolvedConfig> {
    match resolve_config_path(config_path)? {
        ConfigSource::Existing(path) => {
            load_config_from_path(path, ConfigSourceKind::ExplicitPath, None)
        }
        ConfigSource::BundledPreset(preset) => load_config_from_path(
            preset.path,
            ConfigSourceKind::BundledPreset,
            Some(preset.name),
        ),
        ConfigSource::Missing(path) => {
            bail!(
                "Config file not found: {}. Omit --config to use project/global discovery, or pass a valid path/preset.",
                path.display()
            )
        }
    }
}

fn load_discovered_config() -> Result<ResolvedConfig> {
    if let Some(path) = find_project_config()? {
        return load_config_from_path(path, ConfigSourceKind::Project, None);
    }

    let global_config = ensure_global_layout()?;
    load_config_from_path(global_config, ConfigSourceKind::Global, None)
}

fn load_config_from_path(
    path: PathBuf,
    source: ConfigSourceKind,
    preset_name: Option<&'static str>,
) -> Result<ResolvedConfig> {
    let path = absolute_path(&path)?;
    let base_dir = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let mut config = load_config_file(&path)?;
    normalize_config_paths(&mut config, &base_dir)?;

    Ok(ResolvedConfig {
        config,
        path,
        base_dir,
        source,
        preset_name,
    })
}

fn absolute_path(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()
            .context("Cannot determine current directory")?
            .join(path))
    }
}

fn find_project_config() -> Result<Option<PathBuf>> {
    let current_dir = std::env::current_dir().context("Cannot determine current directory")?;
    for dir in current_dir.ancestors() {
        for file_name in paths::project_config_file_names() {
            let candidate = dir.join(file_name);
            if candidate.is_file() {
                return Ok(Some(candidate));
            }
        }
    }
    Ok(None)
}

fn ensure_global_layout() -> Result<PathBuf> {
    let home = paths::global_home()?;
    let models = paths::global_models_dir()?;
    let policies = paths::global_policies_dir()?;

    std::fs::create_dir_all(&models).with_context(|| {
        format!(
            "Cannot create global models directory: {}",
            models.display()
        )
    })?;
    std::fs::create_dir_all(&policies).with_context(|| {
        format!(
            "Cannot create global policies directory: {}",
            policies.display()
        )
    })?;

    for candidate in paths::global_config_candidates()? {
        if candidate.exists() {
            return Ok(candidate);
        }
    }

    std::fs::create_dir_all(&home)
        .with_context(|| format!("Cannot create global CloakPipe home: {}", home.display()))?;
    let config_path = paths::global_config_path()?;
    let toml_str = toml::to_string_pretty(&default_config())?;
    std::fs::write(&config_path, toml_str)
        .with_context(|| format!("Cannot write global config: {}", config_path.display()))?;
    Ok(config_path)
}

fn normalize_config_paths(config: &mut CloakPipeConfig, base_dir: &Path) -> Result<()> {
    config.vault.path = paths::resolve_config_relative_string(base_dir, &config.vault.path);
    config.audit.log_path = paths::resolve_config_relative_string(base_dir, &config.audit.log_path);
    config.tree.storage_path =
        paths::resolve_config_relative_string(base_dir, &config.tree.storage_path);

    if let Some(vector_db_path) = config.local.vector_db_path.as_mut() {
        *vector_db_path = paths::resolve_config_relative_string(base_dir, vector_db_path);
    }

    if let Some(model) = config.detection.ner.model.as_mut() {
        *model = paths::resolve_config_relative_string(base_dir, model);
    } else {
        config.detection.ner.model =
            Some(default_global_ner_model_path(config.detection.ner.backend)?);
    }

    normalize_optional_path(base_dir, &mut config.proxy.http_proxy.ca_cert_path);
    normalize_optional_path(base_dir, &mut config.proxy.http_proxy.ca_key_path);
    normalize_optional_path(base_dir, &mut config.proxy.http_proxy.cert_cache_dir);

    Ok(())
}

fn normalize_optional_path(base_dir: &Path, value: &mut Option<String>) {
    if let Some(path) = value.as_mut() {
        *path = paths::resolve_config_relative_string(base_dir, path);
    }
}

fn default_global_ner_model_path(backend: NerBackend) -> Result<String> {
    let path = match backend {
        NerBackend::Bert => paths::global_bert_ner_model_path()?,
        NerBackend::Gliner => paths::global_gliner_model_path()?,
        NerBackend::DistilBertPii => paths::global_distilbert_pii_model_path()?,
        NerBackend::GlinerPii => paths::global_gliner_model_path()?,
    };
    Ok(path.to_string_lossy().into_owned())
}

fn log_resolved_config(resolved: &ResolvedConfig) {
    match (resolved.source, resolved.preset_name) {
        (ConfigSourceKind::BundledPreset, Some(name)) => tracing::info!(
            preset = name,
            path = %resolved.path.display(),
            base_dir = %resolved.base_dir.display(),
            "Using bundled preset"
        ),
        (ConfigSourceKind::Project, _) => tracing::info!(
            path = %resolved.path.display(),
            base_dir = %resolved.base_dir.display(),
            "Using discovered project config"
        ),
        (ConfigSourceKind::Global, _) => tracing::info!(
            path = %resolved.path.display(),
            base_dir = %resolved.base_dir.display(),
            "Using global config"
        ),
        _ => tracing::info!(
            path = %resolved.path.display(),
            base_dir = %resolved.base_dir.display(),
            "Using explicit config"
        ),
    }
}

fn scan_output_rel_path(input_is_file: bool, input_path: &Path, file_path: &Path) -> PathBuf {
    if input_is_file {
        return file_path
            .file_name()
            .map(PathBuf::from)
            .unwrap_or_else(|| file_path.to_path_buf());
    }

    file_path
        .strip_prefix(input_path)
        .unwrap_or(file_path)
        .to_path_buf()
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

fn preflight_http_proxy_config(config: &CloakPipeConfig) -> Result<()> {
    if config.proxy.mode != ProxyMode::HttpProxy {
        return Ok(());
    }

    if let Some(forward_proxy) = config
        .proxy
        .http_proxy
        .forward_proxy
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        outbound_proxy::validate_forward_proxy_url(forward_proxy)?;
        tracing::info!(
            forward_proxy = %outbound_proxy::redact_proxy_url(forward_proxy),
            "http-proxy egress will use configured forwarding proxy"
        );
    }

    if !config.proxy.http_proxy.inspect_https {
        tracing::info!("HTTPS CONNECT traffic will be tunneled without inspection");
        return Ok(());
    }

    let status = tls_mitm::ca_status(&config.proxy.http_proxy)?;
    if !status.ca_cert_exists || !status.ca_key_exists {
        bail!(
            "HTTPS inspection is enabled, but the CloakPipe root CA is not initialized.\n\n{}\n\n{}",
            format_ca_paths(&status),
            render_ca_install_instructions(default_ca_install_platform(), &status.paths.ca_cert_path)
        );
    }

    match detect_ca_trust(&status.paths) {
        TrustStatus::Trusted => tracing::info!("CloakPipe root CA appears to be trusted"),
        TrustStatus::NotTrusted => bail!(
            "HTTPS inspection is enabled, but the CloakPipe root CA does not appear to be trusted.\n\n{}",
            render_ca_install_instructions(default_ca_install_platform(), &status.paths.ca_cert_path)
        ),
        TrustStatus::Unknown => tracing::warn!(
            "Could not verify whether the CloakPipe root CA is trusted on this platform. If clients reject TLS, install it with:\n{}",
            render_ca_install_instructions(default_ca_install_platform(), &status.paths.ca_cert_path)
        ),
    }

    Ok(())
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
pub async fn start(config_path: Option<&str>) -> Result<()> {
    let resolved = load_resolved_config(config_path)?;
    log_resolved_config(&resolved);
    let config = resolved.config;
    preflight_http_proxy_config(&config)?;
    let key = resolve_vault_key(&config)?;
    let detector = Detector::from_config(&config.detection)?;
    let vault = Vault::open(&config.vault.path, key)?;
    let audit = AuditSink::from_config(&config.audit)?;
    let api_key = resolve_proxy_api_key(&config);

    tracing::info!(
        listen = %config.proxy.listen,
        upstream = %config.proxy.upstream,
        "Starting CloakPipe proxy"
    );

    let state = AppState::try_new(config, detector, vault, audit, api_key)?;
    server::start(state).await
}

/// Test detection on sample text.
pub async fn test(
    config_path: Option<&str>,
    text: Option<String>,
    file: Option<String>,
) -> Result<()> {
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

    let config = load_resolved_config(config_path)?.config;

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
pub async fn stats(config_path: Option<&str>) -> Result<()> {
    let config = load_resolved_config(config_path)?.config;
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
    let global_config = ensure_global_layout()?;
    let path = "cloakpipe.toml";
    if std::path::Path::new(path).exists() {
        bail!("{} already exists", path);
    }

    let config = default_config();
    let toml_str = toml::to_string_pretty(&config)?;
    std::fs::write(path, toml_str)?;
    let preset_dir = install_bundled_presets()?;
    println!("Created {}", path);
    println!("Global config available at {}", global_config.display());
    println!("Bundled presets installed in {}", preset_dir.display());
    println!("\nNext steps:");
    println!("  1. Set OPENAI_API_KEY (or your upstream API key)");
    println!("  2. Set CLOAKPIPE_VAULT_KEY (64-char hex string for encryption)");
    println!("  3. Run: cloakpipe start");
    println!("  4. Or run a bundled preset directly: cloakpipe --config dpdp.toml start");

    Ok(())
}

/// Explicit HTTP proxy helper commands.
pub async fn http_proxy(config_path: Option<&str>, action: crate::HttpProxyCommands) -> Result<()> {
    match action {
        crate::HttpProxyCommands::Ca { action } => http_proxy_ca(config_path, action).await,
    }
}

async fn http_proxy_ca(
    config_path: Option<&str>,
    action: crate::HttpProxyCaCommands,
) -> Result<()> {
    let config = load_resolved_config(config_path)?.config;
    let http_proxy = config.proxy.http_proxy;

    match action {
        crate::HttpProxyCaCommands::Init { force, dry_run } => {
            let status = tls_mitm::ca_status(&http_proxy)?;
            if dry_run {
                println!("CloakPipe HTTP proxy CA init (dry run)");
                println!("{}", format_ca_paths(&status));
                println!("Would create missing CA files and cache directory.");
                return Ok(());
            }

            let status = tls_mitm::ensure_root_ca(&http_proxy, force)?;
            println!("CloakPipe HTTP proxy CA is ready.");
            println!("{}", format_ca_paths(&status));
            println!("\nNext step: trust the CA for clients that will use HTTPS inspection.");
            println!(
                "{}",
                render_ca_install_instructions(
                    default_ca_install_platform(),
                    &status.paths.ca_cert_path
                )
            );
        }
        crate::HttpProxyCaCommands::Status => {
            let status = tls_mitm::ca_status(&http_proxy)?;
            println!("CloakPipe HTTP proxy CA status");
            println!("{}", format_ca_paths(&status));
            println!("Trust: {}", detect_ca_trust(&status.paths));
        }
        crate::HttpProxyCaCommands::PrintPath => {
            let status = tls_mitm::ca_status(&http_proxy)?;
            println!("cert={}", status.paths.ca_cert_path.display());
            println!("key={}", status.paths.ca_key_path.display());
            println!("cache={}", status.paths.cert_cache_dir.display());
        }
        crate::HttpProxyCaCommands::Install { platform } => {
            let status = tls_mitm::ca_status(&http_proxy)?;
            println!(
                "{}",
                render_ca_install_instructions(
                    platform.unwrap_or_else(default_ca_install_platform),
                    &status.paths.ca_cert_path,
                )
            );
        }
        crate::HttpProxyCaCommands::Trust { yes } => {
            if !yes {
                bail!("Refusing to modify the trust store without --yes. Run `cloakpipe http-proxy ca install` to print manual instructions.");
            }

            let status = tls_mitm::ca_status(&http_proxy)?;
            if !status.ca_cert_exists {
                bail!("CA certificate does not exist. Run `cloakpipe http-proxy ca init` first.");
            }
            trust_ca_best_effort(&status.paths.ca_cert_path)?;
            println!(
                "Requested trust-store install for {}",
                status.paths.ca_cert_path.display()
            );
        }
        crate::HttpProxyCaCommands::Untrust { yes } => {
            if !yes {
                bail!("Refusing to modify the trust store without --yes.");
            }

            untrust_ca_best_effort()?;
            println!(
                "Requested trust-store removal for {}",
                tls_mitm::CA_COMMON_NAME
            );
        }
    }

    Ok(())
}

fn format_ca_paths(status: &tls_mitm::CaStatus) -> String {
    format!(
        "  cert:  {} ({})\n  key:   {} ({})\n  cache: {} ({})",
        status.paths.ca_cert_path.display(),
        if status.ca_cert_exists {
            "exists"
        } else {
            "missing"
        },
        status.paths.ca_key_path.display(),
        if status.ca_key_exists {
            "exists"
        } else {
            "missing"
        },
        status.paths.cert_cache_dir.display(),
        if status.cert_cache_dir_exists {
            "exists"
        } else {
            "missing"
        },
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TrustStatus {
    Trusted,
    NotTrusted,
    Unknown,
}

impl std::fmt::Display for TrustStatus {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Trusted => formatter.write_str("trusted"),
            Self::NotTrusted => formatter.write_str("not trusted"),
            Self::Unknown => formatter.write_str("unknown"),
        }
    }
}

fn detect_ca_trust(paths: &tls_mitm::MitmPaths) -> TrustStatus {
    if !paths.ca_cert_path.is_file() {
        return TrustStatus::NotTrusted;
    }

    #[cfg(target_os = "macos")]
    {
        detect_macos_ca_trust()
    }

    #[cfg(not(target_os = "macos"))]
    {
        TrustStatus::Unknown
    }
}

#[cfg(target_os = "macos")]
fn detect_macos_ca_trust() -> TrustStatus {
    let mut keychains = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        keychains.push(PathBuf::from(home).join("Library/Keychains/login.keychain-db"));
    }
    keychains.push(PathBuf::from("/Library/Keychains/System.keychain"));

    let mut command_failed = false;
    for keychain in keychains {
        match std::process::Command::new("security")
            .arg("find-certificate")
            .arg("-c")
            .arg(tls_mitm::CA_COMMON_NAME)
            .arg(&keychain)
            .output()
        {
            Ok(output) if output.status.success() => return TrustStatus::Trusted,
            Ok(_) => {}
            Err(_) => command_failed = true,
        }
    }

    if command_failed {
        TrustStatus::Unknown
    } else {
        TrustStatus::NotTrusted
    }
}

fn default_ca_install_platform() -> crate::CaInstallPlatform {
    if cfg!(target_os = "macos") {
        crate::CaInstallPlatform::Macos
    } else if cfg!(target_os = "windows") {
        crate::CaInstallPlatform::Windows
    } else {
        crate::CaInstallPlatform::Linux
    }
}

fn render_ca_install_instructions(platform: crate::CaInstallPlatform, cert_path: &Path) -> String {
    let cert = cert_path.display();
    match platform {
        crate::CaInstallPlatform::Macos => format!(
            "macOS trust install:\n  security add-trusted-cert -r trustRoot -k ~/Library/Keychains/login.keychain-db {cert}\n\nSystem-wide alternative (admin prompt):\n  sudo security add-trusted-cert -d -r trustRoot -k /Library/Keychains/System.keychain {cert}"
        ),
        crate::CaInstallPlatform::Linux => format!(
            "Linux trust install:\n  Debian/Ubuntu:\n    sudo cp {cert} /usr/local/share/ca-certificates/cloakpipe-ca.crt\n    sudo update-ca-certificates\n  Fedora/RHEL:\n    sudo cp {cert} /etc/pki/ca-trust/source/anchors/cloakpipe-ca.crt\n    sudo update-ca-trust\n\nRuntime-specific stores:\n  NODE_EXTRA_CA_CERTS={cert}\n  REQUESTS_CA_BUNDLE={cert}\n  SSL_CERT_FILE={cert}\n  CURL_CA_BUNDLE={cert}"
        ),
        crate::CaInstallPlatform::Windows => format!(
            "Windows trust install (PowerShell):\n  Import-Certificate -FilePath '{cert}' -CertStoreLocation Cert:\\CurrentUser\\Root\n\nFor machine-wide trust, use an elevated PowerShell and Cert:\\LocalMachine\\Root."
        ),
    }
}

fn trust_ca_best_effort(cert_path: &Path) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        let Some(home) = std::env::var_os("HOME") else {
            bail!("Cannot find HOME to locate the macOS login keychain");
        };
        let keychain = PathBuf::from(home).join("Library/Keychains/login.keychain-db");
        let status = std::process::Command::new("security")
            .arg("add-trusted-cert")
            .arg("-r")
            .arg("trustRoot")
            .arg("-k")
            .arg(&keychain)
            .arg(cert_path)
            .status()
            .context("Failed to launch macOS security tool")?;

        if !status.success() {
            bail!("macOS security tool failed to add the CloakPipe CA");
        }
        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    {
        bail!(
            "Automatic trust installation is only implemented for macOS in this build.\n{}",
            render_ca_install_instructions(default_ca_install_platform(), cert_path)
        )
    }
}

fn untrust_ca_best_effort() -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        let Some(home) = std::env::var_os("HOME") else {
            bail!("Cannot find HOME to locate the macOS login keychain");
        };
        let keychain = PathBuf::from(home).join("Library/Keychains/login.keychain-db");
        let status = std::process::Command::new("security")
            .arg("delete-certificate")
            .arg("-c")
            .arg(tls_mitm::CA_COMMON_NAME)
            .arg(&keychain)
            .status()
            .context("Failed to launch macOS security tool")?;

        if !status.success() {
            bail!("macOS security tool failed to remove the CloakPipe CA from the login keychain");
        }
        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    {
        bail!(
            "Automatic trust removal is only implemented for macOS in this build. Remove `{}` from your trust store manually.",
            tls_mitm::CA_COMMON_NAME
        )
    }
}

/// Bundled preset management commands.
pub async fn presets(action: crate::PresetCommands) -> Result<()> {
    match action {
        crate::PresetCommands::Install => {
            let global_config = ensure_global_layout()?;
            let preset_dir = install_bundled_presets()?;
            println!("Global config available at {}", global_config.display());
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

/// Policy editing commands.
pub async fn policy(config_path: Option<&str>, action: crate::PolicyCommands) -> Result<()> {
    match action {
        crate::PolicyCommands::Edit => edit_policy(config_path).await,
    }
}

async fn edit_policy(config_path: Option<&str>) -> Result<()> {
    let target = resolve_editable_policy(config_path)?;
    run_policy_editor(target)
}

fn resolve_editable_policy(config_path: Option<&str>) -> Result<EditablePolicy> {
    let Some(config_path) = config_path.map(str::trim).filter(|value| !value.is_empty()) else {
        let resolved = load_discovered_config()?;
        return Ok(EditablePolicy {
            config: load_config_file(&resolved.path)?,
            path: resolved.path,
            source: EditablePolicySource::ExistingFile,
            preset_name: resolved.preset_name,
        });
    };

    match resolve_config_path(config_path)? {
        ConfigSource::Existing(path) => Ok(EditablePolicy {
            config: load_config_file(&path)?,
            path,
            source: EditablePolicySource::ExistingFile,
            preset_name: None,
        }),
        ConfigSource::BundledPreset(preset) => Ok(EditablePolicy {
            config: load_config_file(&preset.path)?,
            path: preset.path,
            source: EditablePolicySource::BundledPreset,
            preset_name: Some(preset.name),
        }),
        ConfigSource::Missing(path) => Ok(EditablePolicy {
            path,
            config: default_config(),
            source: EditablePolicySource::MissingFile,
            preset_name: None,
        }),
    }
}

fn run_policy_editor(mut target: EditablePolicy) -> Result<()> {
    use dialoguer::{Confirm, Select};

    println!("CloakPipe Policy Editor\n");
    print_policy_destination(&target);
    if matches!(target.source, EditablePolicySource::MissingFile) {
        println!("No policy exists at this path; editing starts from default settings.");
    }

    let mut summary = PolicyEditSummary::default();
    let actions = [
        "Detection rule families",
        "Replacement strategy",
        "NER settings",
        "Preserve list",
        "Force list",
        "Custom regex patterns",
        "Show destination path",
        "Save policy",
        "Exit without saving",
    ];

    loop {
        let action_idx = Select::new()
            .with_prompt("Choose a policy section")
            .items(&actions)
            .default(0)
            .interact()?;

        match action_idx {
            0 => summary.detection_rules |= edit_detection_rules(&mut target.config)?,
            1 => summary.replacement_strategy |= edit_replacement_strategy(&mut target.config)?,
            2 => summary.ner_settings |= edit_ner_settings(&mut target.config)?,
            3 => {
                summary.preserve_list |= edit_string_list(
                    &mut target.config.detection.overrides.preserve,
                    "preserve list",
                )?;
            }
            4 => {
                summary.force_list |=
                    edit_string_list(&mut target.config.detection.overrides.force, "force list")?;
            }
            5 => {
                summary.custom_patterns |=
                    edit_custom_patterns(&mut target.config.detection.custom.patterns)?;
            }
            6 => print_policy_destination(&target),
            7 => {
                let prompt = save_confirmation_prompt(&target);
                if !Confirm::new()
                    .with_prompt(prompt)
                    .default(true)
                    .interact()?
                {
                    println!("Save cancelled.");
                    continue;
                }

                save_policy(&target.path, &target.config)?;
                println!("Saved policy to {}", target.path.display());
                print_policy_change_summary(&summary);
                return Ok(());
            }
            8 => {
                if Confirm::new()
                    .with_prompt("Exit without saving?")
                    .default(false)
                    .interact()?
                {
                    println!("Policy edit cancelled. No changes written.");
                    return Ok(());
                }
            }
            _ => unreachable!(),
        }
    }
}

fn print_policy_destination(target: &EditablePolicy) {
    match (target.source, target.preset_name) {
        (EditablePolicySource::BundledPreset, Some(name)) => {
            println!(
                "Destination: {} (installed '{}' preset copy)",
                target.path.display(),
                name
            );
        }
        (EditablePolicySource::MissingFile, _) => {
            println!("Destination: {} (new policy)", target.path.display());
        }
        _ => println!("Destination: {}", target.path.display()),
    }
}

fn save_confirmation_prompt(target: &EditablePolicy) -> String {
    match (target.source, target.preset_name) {
        (EditablePolicySource::MissingFile, _) => {
            format!("Create policy at {}?", target.path.display())
        }
        (EditablePolicySource::BundledPreset, Some(name)) => format!(
            "Overwrite installed '{}' preset copy at {}?",
            name,
            target.path.display()
        ),
        _ => format!("Overwrite policy at {}?", target.path.display()),
    }
}

fn print_policy_change_summary(summary: &PolicyEditSummary) {
    let descriptions = summary.descriptions();
    if descriptions.is_empty() {
        println!("Updated: no policy fields changed.");
    } else {
        println!("Updated: {}.", descriptions.join(", "));
    }
}

fn edit_detection_rules(config: &mut CloakPipeConfig) -> Result<bool> {
    use dialoguer::MultiSelect;

    let items: Vec<&str> = DETECTION_TOGGLES
        .iter()
        .map(|toggle| toggle.label)
        .collect();
    let defaults: Vec<bool> = DETECTION_TOGGLES
        .iter()
        .map(|toggle| toggle.family.enabled(&config.detection))
        .collect();

    let selected = MultiSelect::new()
        .with_prompt("Select detection rule families to enable")
        .items(&items)
        .defaults(&defaults)
        .interact()?;

    Ok(apply_detection_selection(&mut config.detection, &selected))
}

fn apply_detection_selection(config: &mut DetectionConfig, selected_indices: &[usize]) -> bool {
    let mut changed = false;
    for (index, toggle) in DETECTION_TOGGLES.iter().enumerate() {
        let selected = selected_indices.contains(&index);
        if toggle.family.enabled(config) != selected {
            toggle.family.set(config, selected);
            changed = true;
        }
    }
    changed
}

fn edit_replacement_strategy(config: &mut CloakPipeConfig) -> Result<bool> {
    use dialoguer::Select;

    let items: Vec<&str> = STRATEGY_OPTIONS.iter().map(|option| option.label).collect();
    let selected = Select::new()
        .with_prompt("Default replacement strategy")
        .items(&items)
        .default(strategy_index(config.proxy.masking_strategy))
        .interact()?;
    let strategy = STRATEGY_OPTIONS[selected].value;
    let changed = config.proxy.masking_strategy != strategy;
    config.proxy.masking_strategy = strategy;
    Ok(changed)
}

fn strategy_index(strategy: MaskingStrategy) -> usize {
    STRATEGY_OPTIONS
        .iter()
        .position(|option| option.value == strategy)
        .unwrap_or(0)
}

fn edit_ner_settings(config: &mut CloakPipeConfig) -> Result<bool> {
    use dialoguer::{Confirm, Input, Select};

    let ner = &mut config.detection.ner;
    let original_enabled = ner.enabled;
    let original_backend = ner.backend;
    let original_confidence_threshold = ner.confidence_threshold;
    let original_entity_types = ner.entity_types.clone();
    let original_sidecar_url = ner.sidecar_url.clone();
    let original_model = ner.model.clone();

    ner.enabled = Confirm::new()
        .with_prompt("Enable NER detection?")
        .default(ner.enabled)
        .interact()?;

    let backend_items: Vec<&str> = NER_BACKEND_OPTIONS
        .iter()
        .map(|option| option.label)
        .collect();
    let backend_idx = Select::new()
        .with_prompt("NER backend")
        .items(&backend_items)
        .default(ner_backend_index(ner.backend))
        .interact()?;
    ner.backend = NER_BACKEND_OPTIONS[backend_idx].value;

    ner.confidence_threshold = prompt_confidence_threshold(ner.confidence_threshold)?;
    ner.entity_types = prompt_comma_separated_list(
        "NER entity types (comma-separated, blank for all)",
        &ner.entity_types,
    )?;

    let sidecar_url: String = Input::new()
        .with_prompt("GLiNER-PII sidecar URL")
        .with_initial_text(ner.sidecar_url.clone())
        .interact_text()?;
    ner.sidecar_url = sidecar_url.trim().to_string();
    ner.model = prompt_optional_text("NER model path", ner.model.as_deref())?;

    Ok(ner.enabled != original_enabled
        || ner.backend != original_backend
        || ner.confidence_threshold != original_confidence_threshold
        || ner.entity_types != original_entity_types
        || ner.sidecar_url != original_sidecar_url
        || ner.model != original_model)
}

fn ner_backend_index(backend: NerBackend) -> usize {
    NER_BACKEND_OPTIONS
        .iter()
        .position(|option| option.value == backend)
        .unwrap_or(0)
}

fn prompt_confidence_threshold(current: f64) -> Result<f64> {
    use dialoguer::Input;

    loop {
        let threshold: f64 = Input::new()
            .with_prompt("NER confidence threshold (0.0-1.0)")
            .default(current)
            .interact_text()?;
        if (0.0..=1.0).contains(&threshold) {
            return Ok(threshold);
        }
        println!("Enter a value between 0.0 and 1.0.");
    }
}

fn prompt_comma_separated_list(prompt: &str, current: &[String]) -> Result<Vec<String>> {
    use dialoguer::Input;

    let input: String = Input::new()
        .with_prompt(prompt)
        .with_initial_text(current.join(", "))
        .allow_empty(true)
        .interact_text()?;
    Ok(parse_comma_separated(&input))
}

fn parse_comma_separated(input: &str) -> Vec<String> {
    input
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn prompt_optional_text(prompt: &str, current: Option<&str>) -> Result<Option<String>> {
    use dialoguer::Input;

    let current = current.unwrap_or_default();
    let input: String = Input::new()
        .with_prompt(format!("{} (blank for default, '-' to clear)", prompt))
        .with_initial_text(current.to_string())
        .allow_empty(true)
        .interact_text()?;
    let trimmed = input.trim();
    if trimmed.is_empty() || trimmed == "-" {
        Ok(None)
    } else {
        Ok(Some(trimmed.to_string()))
    }
}

fn edit_string_list(values: &mut Vec<String>, label: &str) -> Result<bool> {
    use dialoguer::{Input, MultiSelect, Select};

    let mut changed = false;
    let actions = ["Add value", "Remove selected values", "Done"];

    loop {
        print_string_list(label, values);
        let action_idx = Select::new()
            .with_prompt(format!("Manage {}", label))
            .items(&actions)
            .default(0)
            .interact()?;

        match action_idx {
            0 => {
                let value: String = Input::new()
                    .with_prompt(format!("Value to add to {}", label))
                    .allow_empty(true)
                    .interact_text()?;
                if add_unique_value(values, &value) {
                    changed = true;
                }
            }
            1 => {
                if values.is_empty() {
                    println!("No values to remove.");
                    continue;
                }
                let selected = MultiSelect::new()
                    .with_prompt(format!("Select values to remove from {}", label))
                    .items(values.as_slice())
                    .interact()?;
                if remove_selected_indices(values, &selected) {
                    changed = true;
                }
            }
            2 => return Ok(changed),
            _ => unreachable!(),
        }
    }
}

fn print_string_list(label: &str, values: &[String]) {
    if values.is_empty() {
        println!("{} is empty.", label);
    } else {
        println!("{}:", label);
        for value in values {
            println!("  {}", value);
        }
    }
}

fn add_unique_value(values: &mut Vec<String>, value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() || values.iter().any(|existing| existing == trimmed) {
        return false;
    }
    values.push(trimmed.to_string());
    true
}

fn edit_custom_patterns(patterns: &mut Vec<CustomPattern>) -> Result<bool> {
    use dialoguer::{MultiSelect, Select};

    let mut changed = false;
    let actions = [
        "Add pattern",
        "Edit pattern",
        "Remove selected patterns",
        "Done",
    ];

    loop {
        print_custom_patterns(patterns);
        let action_idx = Select::new()
            .with_prompt("Manage custom regex patterns")
            .items(&actions)
            .default(0)
            .interact()?;

        match action_idx {
            0 => {
                patterns.push(prompt_custom_pattern(None)?);
                changed = true;
            }
            1 => {
                if patterns.is_empty() {
                    println!("No custom patterns to edit.");
                    continue;
                }
                let labels = custom_pattern_labels(patterns);
                let pattern_idx = Select::new()
                    .with_prompt("Select a custom pattern to edit")
                    .items(&labels)
                    .default(0)
                    .interact()?;
                let updated = prompt_custom_pattern(Some(&patterns[pattern_idx]))?;
                if custom_pattern_changed(&patterns[pattern_idx], &updated) {
                    patterns[pattern_idx] = updated;
                    changed = true;
                }
            }
            2 => {
                if patterns.is_empty() {
                    println!("No custom patterns to remove.");
                    continue;
                }
                let labels = custom_pattern_labels(patterns);
                let selected = MultiSelect::new()
                    .with_prompt("Select custom patterns to remove")
                    .items(&labels)
                    .interact()?;
                if remove_selected_indices(patterns, &selected) {
                    changed = true;
                }
            }
            3 => return Ok(changed),
            _ => unreachable!(),
        }
    }
}

fn print_custom_patterns(patterns: &[CustomPattern]) {
    if patterns.is_empty() {
        println!("No custom regex patterns configured.");
    } else {
        println!("Custom regex patterns:");
        for label in custom_pattern_labels(patterns) {
            println!("  {}", label);
        }
    }
}

fn custom_pattern_labels(patterns: &[CustomPattern]) -> Vec<String> {
    patterns
        .iter()
        .map(|pattern| {
            let value_group = pattern
                .value_group
                .map(|group| format!(" value_group={group}"))
                .unwrap_or_default();
            format!(
                "{} [{}{}] {}",
                pattern.name, pattern.category, value_group, pattern.regex
            )
        })
        .collect()
}

fn prompt_custom_pattern(existing: Option<&CustomPattern>) -> Result<CustomPattern> {
    let name = prompt_non_empty_text(
        "Pattern name",
        existing
            .map(|pattern| pattern.name.as_str())
            .unwrap_or_default(),
    )?;
    let regex = prompt_non_empty_text(
        "Regex",
        existing
            .map(|pattern| pattern.regex.as_str())
            .unwrap_or_default(),
    )?;
    let category = prompt_non_empty_text(
        "Category",
        existing
            .map(|pattern| pattern.category.as_str())
            .unwrap_or_default(),
    )?;
    let value_group = prompt_optional_usize(
        "Value capture group (blank for full match)",
        existing.and_then(|pattern| pattern.value_group),
    )?;

    Ok(CustomPattern {
        name,
        regex,
        category,
        value_group,
    })
}

fn prompt_optional_usize(prompt: &str, current: Option<usize>) -> Result<Option<usize>> {
    use dialoguer::Input;

    loop {
        let input: String = Input::new()
            .with_prompt(prompt)
            .with_initial_text(current.map(|value| value.to_string()).unwrap_or_default())
            .allow_empty(true)
            .interact_text()?;
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }
        match trimmed.parse::<usize>() {
            Ok(value) => return Ok(Some(value)),
            Err(_) => println!("Enter a non-negative integer, or leave blank for full match."),
        }
    }
}

fn prompt_non_empty_text(prompt: &str, initial: &str) -> Result<String> {
    use dialoguer::Input;

    loop {
        let input: String = Input::new()
            .with_prompt(prompt)
            .with_initial_text(initial.to_string())
            .allow_empty(true)
            .interact_text()?;
        let trimmed = input.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
        println!("Value cannot be empty.");
    }
}

fn custom_pattern_changed(current: &CustomPattern, updated: &CustomPattern) -> bool {
    current.name != updated.name
        || current.regex != updated.regex
        || current.category != updated.category
        || current.value_group != updated.value_group
}

fn remove_selected_indices<T>(values: &mut Vec<T>, selected_indices: &[usize]) -> bool {
    if selected_indices.is_empty() {
        return false;
    }

    let mut indices = selected_indices.to_vec();
    indices.sort_unstable();
    indices.dedup();
    let original_len = values.len();

    for index in indices.into_iter().rev() {
        if index < values.len() {
            values.remove(index);
        }
    }

    values.len() != original_len
}

fn validate_policy_config(config: &CloakPipeConfig) -> Result<String> {
    let toml_str = toml::to_string_pretty(config).context("Cannot serialize edited policy")?;
    let roundtrip: CloakPipeConfig =
        toml::from_str(&toml_str).context("Edited policy did not round-trip through TOML")?;
    Detector::from_config(&roundtrip.detection).context(
        "Edited detection policy is invalid; check custom regex patterns and NER settings",
    )?;
    Ok(toml_str)
}

fn save_policy(path: &Path, config: &CloakPipeConfig) -> Result<()> {
    let toml_str = validate_policy_config(config)?;
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Cannot create policy directory: {}", parent.display()))?;
    }
    std::fs::write(path, toml_str)
        .with_context(|| format!("Cannot write policy: {}", path.display()))
}

/// Interactive guided setup.
pub async fn setup() -> Result<()> {
    use cloakpipe_core::profiles::IndustryProfile;
    use dialoguer::{Confirm, Select};

    let global_config = ensure_global_layout()?;

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
    println!("Global config available at {}", global_config.display());
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
        crate::NerCommands::Download { backend, force } => match backend {
            crate::NerDownloadBackend::DistilbertPii => download_distilbert_pii(force).await,
        },
    }
}

/// Download the DistilBERT-PII quantized ONNX model.
/// Delegates to tools/download_model.sh which tries GitHub LFS first,
/// then falls back to HuggingFace download + ONNX conversion.
async fn download_distilbert_pii(force: bool) -> Result<()> {
    ensure_global_layout()?;
    let current_dir = std::env::current_dir().context("Cannot determine current directory")?;
    let project_root = find_gliner_project_root(&current_dir)?;
    let script = project_root.join(DISTILBERT_DOWNLOAD_SCRIPT);
    let target_dir = paths::global_distilbert_pii_dir()?;
    if !script.exists() {
        bail!(
            "Download script not found: {}\nRun from the CloakPipe project root.",
            script.display()
        );
    }

    let mut cmd = std::process::Command::new("bash");
    cmd.arg(&script);
    cmd.arg("--target-dir").arg(&target_dir);
    if force {
        cmd.arg("--force");
    }

    let status = cmd
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("Failed to run {}", script.display()))?;

    if !status.success() {
        bail!("Model download failed.");
    }
    Ok(())
}

fn install_gliner_pii(python: Option<String>, dry_run: bool, no_verify: bool) -> Result<()> {
    ensure_global_layout()?;
    let user_specified_python = python.is_some();
    let python = PathBuf::from(match python {
        Some(python) => python,
        None if dry_run => "python3".to_string(),
        None => detect_python_interpreter()?,
    });
    let venv_dir = default_gliner_venv_dir()?;

    let install_args = [
        "-m",
        "pip",
        "install",
        "--prefer-binary",
        "--extra-index-url",
        GLINER_TORCH_INDEX,
        GLINER_PIP_PACKAGE,
        // torch 2.2.x (latest on x86 macOS) was compiled against NumPy 1.x.
        "numpy<2",
        // transformers 4.45+ requires torch>=2.4 (unavailable on x86 macOS)
        // and introduced a type-annotation bug (NameError: 'nn' not defined).
        "transformers<4.45",
    ];

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

        // When the user hasn't pinned a specific Python, prefer a version that
        // has PyTorch wheels (3.9–3.12). Python 3.14+ has no torch wheels yet.
        let venv_base_python = if !user_specified_python {
            let best = find_gliner_venv_python(&python);
            if best != python {
                println!(
                    "Selecting {} for the virtualenv (torch has no wheels for {}).",
                    best.display(),
                    python.display()
                );
            }
            best
        } else {
            python.clone()
        };

        println!(
            "Falling back to a local virtualenv at {}",
            venv_dir.display()
        );

        let venv_python = ensure_virtualenv(&venv_base_python, &venv_dir)?;
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
            let output_text = render_process_output(&venv_install_output);
            if is_torch_unavailable(&venv_install_output) {
                bail!(
                    "GLiNER install failed: torch has no wheel for this Python version.\n\
                     Tip: specify a compatible Python explicitly, e.g.:\n\
                     cloakpipe ner install --python python3.12\n\n{}",
                    output_text
                );
            }
            bail!(
                "GLiNER install failed in the local virtualenv.\n{}",
                output_text
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
    let python = resolve_gliner_start_python(python, dry_run)?;

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

/// Find the best Python for creating the GLiNER virtualenv.
///
/// PyTorch has no wheels for Python 3.14+. When the detected system Python is
/// too new, this returns a known-compatible version instead.
fn find_gliner_venv_python(default_python: &Path) -> PathBuf {
    for candidate in TORCH_COMPATIBLE_PYTHONS {
        if command_available(candidate) {
            return PathBuf::from(candidate);
        }
    }
    default_python.to_path_buf()
}

/// Returns true when the pip output signals that torch has no wheel for this
/// Python version/platform combination.
fn is_torch_unavailable(output: &std::process::Output) -> bool {
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
    .to_ascii_lowercase();
    combined.contains("no matching distribution") || combined.contains("resolutionimpossible")
}

fn command_available(command: &str) -> bool {
    std::process::Command::new(command)
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn default_gliner_venv_dir() -> Result<PathBuf> {
    paths::global_gliner_runtime_dir()
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

fn resolve_gliner_start_python(python: Option<String>, dry_run: bool) -> Result<PathBuf> {
    if let Some(python) = python {
        return Ok(PathBuf::from(python));
    }

    let managed_python = virtualenv_python_path(&default_gliner_venv_dir()?);
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

    // Upgrade pip in the new venv (best-effort) so the latest resolver is used.
    // Old pip bundled with venvs can struggle with complex dependency graphs.
    let _ = std::process::Command::new(&venv_python)
        .args(["-m", "pip", "install", "--upgrade", "pip", "--quiet"])
        .output();

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
#[expect(
    clippy::items_after_test_module,
    reason = "command tests are colocated with CLI helpers; moving this large module is separate cleanup"
)]
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
    fn default_gliner_venv_dir_is_global() {
        let config_home = tempfile::tempdir().unwrap();
        let _env = EnvVarGuard::set_path("CLOAKPIPE_HOME", config_home.path());

        assert_eq!(
            default_gliner_venv_dir().unwrap(),
            config_home.path().join("gliner-pii-venv")
        );
    }

    #[test]
    fn scan_output_rel_path_uses_file_name_for_single_file_inputs() {
        assert_eq!(
            scan_output_rel_path(
                true,
                Path::new("assets/example.md"),
                Path::new("assets/example.md")
            ),
            PathBuf::from("example.md")
        );
    }

    #[test]
    fn scan_output_rel_path_preserves_relative_path_for_directory_inputs() {
        assert_eq!(
            scan_output_rel_path(
                false,
                Path::new("assets"),
                Path::new("assets/examples/example.md")
            ),
            PathBuf::from("examples/example.md")
        );
    }

    #[test]
    fn apply_detection_selection_sets_supported_families() {
        let mut detection = default_config().detection;

        let changed = apply_detection_selection(&mut detection, &[0, 4, 6]);

        assert!(changed);
        assert!(detection.secrets);
        assert!(!detection.financial);
        assert!(!detection.dates);
        assert!(!detection.emails);
        assert!(detection.phone_numbers);
        assert!(!detection.ip_addresses);
        assert!(detection.urls_internal);
    }

    #[test]
    fn parse_comma_separated_trims_and_skips_empty_values() {
        assert_eq!(
            parse_comma_separated("PERSON, , ORG,LOCATION "),
            vec![
                "PERSON".to_string(),
                "ORG".to_string(),
                "LOCATION".to_string()
            ]
        );
    }

    #[test]
    fn remove_selected_indices_removes_sorted_unique_indices() {
        let mut values = vec!["a".to_string(), "b".to_string(), "c".to_string()];

        let changed = remove_selected_indices(&mut values, &[2, 0, 2]);

        assert!(changed);
        assert_eq!(values, vec!["b".to_string()]);
    }

    #[test]
    fn validate_policy_config_rejects_invalid_custom_regex() {
        let mut config = default_config();
        config.detection.custom.patterns.push(CustomPattern {
            name: "bad".to_string(),
            regex: "[".to_string(),
            category: "BAD".to_string(),
            value_group: None,
        });

        let err = validate_policy_config(&config).unwrap_err();

        assert!(err
            .to_string()
            .contains("Edited detection policy is invalid"));
    }

    #[test]
    fn save_policy_writes_roundtrippable_toml() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("nested").join("policy.toml");

        save_policy(&path, &default_config()).unwrap();

        let written = std::fs::read_to_string(&path).unwrap();
        let roundtrip: CloakPipeConfig = toml::from_str(&written).unwrap();
        assert!(roundtrip.detection.secrets);
    }

    #[test]
    fn save_policy_does_not_overwrite_when_validation_fails() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("policy.toml");
        std::fs::write(&path, "original").unwrap();

        let mut config = default_config();
        config.detection.custom.patterns.push(CustomPattern {
            name: "bad".to_string(),
            regex: "[".to_string(),
            category: "BAD".to_string(),
            value_group: None,
        });

        let result = save_policy(&path, &config);

        assert!(result.is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "original");
    }

    #[test]
    fn resolve_editable_policy_uses_defaults_for_missing_local_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("missing.toml");

        let target = resolve_editable_policy(path.to_str()).unwrap();

        assert_eq!(target.source, EditablePolicySource::MissingFile);
        assert_eq!(target.path, path);
        assert!(target.config.detection.secrets);
    }

    #[test]
    fn resolve_editable_policy_targets_installed_user_preset_copy() {
        let config_home = tempfile::tempdir().unwrap();
        let _env = EnvVarGuard::set_path("CLOAKPIPE_CONFIG_HOME", config_home.path());

        let target = resolve_editable_policy(Some("dpdp.toml")).unwrap();

        assert_eq!(target.source, EditablePolicySource::BundledPreset);
        assert_eq!(target.preset_name, Some("dpdp"));
        assert_eq!(target.path, config_home.path().join("policies/dpdp.toml"));
        assert!(target.path.exists());
    }

    #[test]
    fn ensure_global_layout_creates_home_config_models_and_policies() {
        let config_home = tempfile::tempdir().unwrap();
        let _env = EnvVarGuard::set_path("CLOAKPIPE_HOME", config_home.path());

        let config_path = ensure_global_layout().unwrap();

        assert_eq!(config_path, config_home.path().join("cloakpipe.toml"));
        assert!(config_path.exists());
        assert!(config_home.path().join("models").is_dir());
        assert!(config_home.path().join("policies").is_dir());
    }

    #[test]
    fn omitted_config_discovers_nearest_project_alias_before_global() {
        let temp_dir = tempfile::tempdir().unwrap();
        let global_home = tempfile::tempdir().unwrap();
        let _env = EnvVarGuard::set_path("CLOAKPIPE_HOME", global_home.path());

        let project = temp_dir.path().join("project");
        let nested = project.join("nested");
        std::fs::create_dir_all(&nested).unwrap();

        let mut root_config = default_config();
        root_config.vault.path = "root-vault.enc".into();
        std::fs::write(
            project.join("cloakpipe.toml"),
            toml::to_string_pretty(&root_config).unwrap(),
        )
        .unwrap();

        let mut nested_config = default_config();
        nested_config.vault.path = "nested-vault.enc".into();
        nested_config.audit.log_path = "audit".into();
        std::fs::write(
            nested.join("cloackpipe.toml"),
            toml::to_string_pretty(&nested_config).unwrap(),
        )
        .unwrap();

        let _cwd = CwdGuard::chdir(&nested);
        let resolved = load_resolved_config(None).unwrap();

        assert_eq!(resolved.source, ConfigSourceKind::Project);
        let nested = nested.canonicalize().unwrap();
        assert_eq!(resolved.path, nested.join("cloackpipe.toml"));
        assert_eq!(
            resolved.config.vault.path,
            path_arg(&nested.join("nested-vault.enc"))
        );
        assert_eq!(
            resolved.config.audit.log_path,
            path_arg(&nested.join("audit"))
        );
    }

    #[test]
    fn omitted_config_bootstraps_global_when_no_project_config_exists() {
        let temp_dir = tempfile::tempdir().unwrap();
        let global_home = tempfile::tempdir().unwrap();
        let _env = EnvVarGuard::set_path("CLOAKPIPE_HOME", global_home.path());
        let _cwd = CwdGuard::chdir(temp_dir.path());

        let resolved = load_resolved_config(None).unwrap();

        assert_eq!(resolved.source, ConfigSourceKind::Global);
        assert_eq!(resolved.path, global_home.path().join("cloakpipe.toml"));
        assert!(global_home.path().join("models").is_dir());
        assert!(global_home.path().join("policies").is_dir());
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

    struct EnvVarGuard {
        key: &'static str,
        previous: Option<std::ffi::OsString>,
        _guard: std::sync::MutexGuard<'static, ()>,
    }

    impl EnvVarGuard {
        fn set_path(key: &'static str, value: &Path) -> Self {
            static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

            let guard = ENV_LOCK.lock().unwrap();
            let previous = std::env::var_os(key);
            std::env::set_var(key, value);
            Self {
                key,
                previous,
                _guard: guard,
            }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            if let Some(previous) = &self.previous {
                std::env::set_var(self.key, previous);
            } else {
                std::env::remove_var(self.key);
            }
        }
    }

    struct CwdGuard {
        previous: PathBuf,
    }

    impl CwdGuard {
        fn chdir(path: &Path) -> Self {
            let previous = std::env::current_dir().unwrap();
            std::env::set_current_dir(path).unwrap();
            Self { previous }
        }
    }

    impl Drop for CwdGuard {
        fn drop(&mut self) {
            std::env::set_current_dir(&self.previous).unwrap();
        }
    }
}

/// Start as MCP server (stdio transport).
pub async fn mcp(config_path: Option<&str>) -> Result<()> {
    let config = load_resolved_config(config_path)?.config;

    let key = resolve_vault_key(&config)?;
    let vault = cloakpipe_core::vault::Vault::open(&config.vault.path, key)?;
    let detector = cloakpipe_core::detector::Detector::from_config(&config.detection)?;
    let audit = AuditSink::from_config(&config.audit)?;

    cloakpipe_mcp::serve_stdio(config, detector, vault, audit).await
}

/// CloakTree commands — vectorless document retrieval.
pub async fn tree(config_path: Option<&str>, action: crate::TreeCommands) -> Result<()> {
    let config = load_resolved_config(config_path)?.config;

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
            mode: cloakpipe_core::config::ProxyMode::Proxy,
            dry_run: false,
            bypass: Vec::new(),
            auth_mode: cloakpipe_core::config::ProxyAuthMode::PassThrough,
            provider_routes: HashMap::new(),
            http_proxy: Default::default(),
            masking_strategy: cloakpipe_core::MaskingStrategy::default(),
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
    config_path: Option<&str>,
    input: String,
    output: Option<String>,
    strategy: String,
    detect_only: bool,
    min_confidence: f64,
    no_ner: bool,
) -> Result<()> {
    let mut config = load_resolved_config(config_path)?.config;
    if no_ner {
        config.detection.ner.enabled = false;
    }
    let ner_enabled = config.detection.ner.enabled;

    let detector = Detector::from_config(&config.detection)?;
    let masking_strategy = match strategy.as_str() {
        "similar" | "similar-values" | "sv" => cloakpipe_core::MaskingStrategy::Similar,
        "format-preserving" | "fp" => cloakpipe_core::MaskingStrategy::FormatPreserving,
        "token" => cloakpipe_core::MaskingStrategy::Token,
        _ => cloakpipe_core::MaskingStrategy::default(),
    };

    let input_path = std::path::Path::new(&input);
    let input_is_file = input_path.is_file();
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

        let rel_path = scan_output_rel_path(input_is_file, input_path, file_path);
        let rel_path_display = rel_path.to_string_lossy().to_string();

        if entity_count > 0 {
            total_files += 1;
            file_results.push((rel_path_display.clone(), entity_count));

            if detect_only {
                println!("  {} — {} entities", rel_path_display, entity_count);
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
    println!(
        "  NER:            {}",
        if ner_enabled {
            "enabled (pass --no-ner to disable)"
        } else {
            "disabled"
        }
    );

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

/// Restore masked files using the `vault-mappings.json` produced by `scan`.
pub async fn restore(
    input: String,
    output: Option<String>,
    mappings: Option<String>,
) -> Result<()> {
    let input_path = Path::new(&input);
    if !input_path.exists() {
        bail!("Path does not exist: {}", input_path.display());
    }

    let mappings_path = mappings
        .map(PathBuf::from)
        .unwrap_or_else(|| default_restore_mappings_path(input_path));
    let mappings = load_restore_mappings(&mappings_path)?;

    if input_path.is_file() {
        let content = std::fs::read_to_string(input_path)
            .with_context(|| format!("Cannot read: {}", input_path.display()))?;
        let restored = Rehydrator::rehydrate_from_mappings(&content, &mappings)?.text;
        if let Some(output) = output {
            std::fs::write(&output, restored).with_context(|| format!("Cannot write: {output}"))?;
            println!("Restored 1 file to {output}");
        } else {
            print!("{restored}");
        }
        return Ok(());
    }

    let output_dir = output.unwrap_or_else(|| format!("{}-restored", input.trim_end_matches('/')));
    std::fs::create_dir_all(&output_dir)
        .with_context(|| format!("Cannot create output dir: {output_dir}"))?;

    let files = collect_scannable_files(input_path)?;
    let mut restored_count = 0usize;
    for file_path in files {
        if file_path
            .file_name()
            .is_some_and(|name| name == "vault-mappings.json")
        {
            continue;
        }

        let content = std::fs::read_to_string(&file_path)
            .with_context(|| format!("Cannot read: {}", file_path.display()))?;
        let restored = Rehydrator::rehydrate_from_mappings(&content, &mappings)?.text;
        let rel_path = file_path.strip_prefix(input_path).unwrap_or(&file_path);
        let out_path = Path::new(&output_dir).join(rel_path);
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&out_path, restored)?;
        restored_count += 1;
    }

    println!("Restored {restored_count} files to {output_dir}");
    Ok(())
}

fn default_restore_mappings_path(input_path: &Path) -> PathBuf {
    if input_path.is_file() {
        input_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("vault-mappings.json")
    } else {
        input_path.join("vault-mappings.json")
    }
}

fn load_restore_mappings(path: &Path) -> Result<HashMap<String, String>> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Cannot read mappings: {}", path.display()))?;
    serde_json::from_str(&content)
        .with_context(|| format!("Invalid mappings JSON: {}", path.display()))
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
pub async fn sessions(config_path: Option<&str>, action: crate::SessionCommands) -> Result<()> {
    let config = load_resolved_config(config_path)?.config;

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
