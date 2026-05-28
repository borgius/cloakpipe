//! Configuration types parsed from cloakpipe.toml.

use crate::resolver::ResolverConfig;
use crate::session::SessionConfig;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CloakPipeConfig {
    pub proxy: ProxyConfig,
    pub vault: VaultConfig,
    /// Industry profile for smart detection defaults (general, legal, healthcare, fintech).
    #[serde(default)]
    pub profile: Option<String>,
    pub detection: DetectionConfig,
    #[serde(default)]
    pub tree: TreeConfig,
    #[serde(default)]
    pub vectors: VectorConfig,
    #[serde(default)]
    pub local: LocalConfig,
    #[serde(default)]
    pub audit: AuditConfig,
    #[serde(default)]
    pub session: SessionConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProxyConfig {
    pub listen: String,
    pub upstream: String,
    #[serde(default = "default_api_key_env")]
    pub api_key_env: String,
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent: usize,
    #[serde(default = "default_mode")]
    pub mode: ProxyMode,
    #[serde(default)]
    pub dry_run: bool,
    #[serde(default)]
    pub bypass: Vec<String>,
    #[serde(default = "default_auth_mode")]
    pub auth_mode: ProxyAuthMode,
    #[serde(default)]
    pub provider_routes: HashMap<String, String>,
    #[serde(default)]
    pub http_proxy: HttpProxyConfig,
    #[serde(default)]
    pub masking_strategy: crate::MaskingStrategy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ProxyMode {
    #[default]
    #[serde(alias = "cloaktree")]
    Proxy,
    #[serde(alias = "llm_http")]
    LlmHttp,
    #[serde(alias = "http_proxy")]
    HttpProxy,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HttpProxyConfig {
    #[serde(default)]
    pub inspect_https: bool,
    #[serde(default)]
    pub allowed_hosts: Vec<String>,
    #[serde(default = "default_tunnel_unknown_hosts")]
    pub tunnel_unknown_hosts: bool,
    pub ca_cert_path: Option<String>,
    pub ca_key_path: Option<String>,
    pub cert_cache_dir: Option<String>,
    #[serde(default)]
    pub http2_mitm: bool,
    pub max_connect_tunnels: Option<usize>,
}

impl Default for HttpProxyConfig {
    fn default() -> Self {
        Self {
            inspect_https: false,
            allowed_hosts: Vec::new(),
            tunnel_unknown_hosts: true,
            ca_cert_path: None,
            ca_key_path: None,
            cert_cache_dir: None,
            http2_mitm: false,
            max_connect_tunnels: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ProxyAuthMode {
    #[default]
    PassThrough,
    #[serde(alias = "server_key")]
    ServerKey,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct VaultConfig {
    pub path: String,
    #[serde(default = "default_encryption")]
    pub encryption: String,
    pub key_env: Option<String>,
    #[serde(default)]
    pub key_keyring: bool,
    #[serde(default = "default_vault_backend")]
    pub backend: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DetectionConfig {
    #[serde(default = "default_true")]
    pub secrets: bool,
    #[serde(default = "default_true")]
    pub financial: bool,
    #[serde(default = "default_true")]
    pub dates: bool,
    #[serde(default = "default_true")]
    pub emails: bool,
    #[serde(default)]
    pub phone_numbers: bool,
    #[serde(default)]
    pub ip_addresses: bool,
    #[serde(default)]
    pub urls_internal: bool,
    #[serde(default)]
    pub ner: NerConfig,
    #[serde(default)]
    pub custom: CustomConfig,
    #[serde(default)]
    pub overrides: OverrideConfig,
    #[serde(default)]
    pub resolver: ResolverConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum NerBackend {
    Bert,
    Gliner,
    /// nvidia/gliner-PII via Python sidecar (no ONNX deps required).
    #[serde(rename = "gliner_pii", alias = "gliner-pii", alias = "glinerpii")]
    GlinerPii,
    /// DistilBERT PII — 63MB ONNX model, 33 entity types, runs on any CPU.
    #[default]
    #[serde(
        rename = "distilbert_pii",
        alias = "distilbert-pii",
        alias = "distilbertpii"
    )]
    DistilBertPii,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct NerConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub backend: NerBackend,
    pub model: Option<String>,
    #[serde(default = "default_confidence")]
    pub confidence_threshold: f64,
    #[serde(default)]
    pub entity_types: Vec<String>,
    /// URL of the GLiNER-PII sidecar server (for GlinerPii backend).
    #[serde(default = "default_sidecar_url")]
    pub sidecar_url: String,
}

fn default_sidecar_url() -> String {
    "http://127.0.0.1:9111".into()
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct CustomConfig {
    #[serde(default)]
    pub patterns: Vec<CustomPattern>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CustomPattern {
    pub name: String,
    pub regex: String,
    pub category: String,
    #[serde(default)]
    pub value_group: Option<usize>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct OverrideConfig {
    #[serde(default)]
    pub preserve: Vec<String>,
    #[serde(default)]
    pub force: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TreeConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_tree_path")]
    pub storage_path: String,
    #[serde(default = "default_tree_model")]
    pub index_model: String,
    #[serde(default = "default_tree_model")]
    pub search_model: String,
    #[serde(default = "default_max_pages")]
    pub max_pages_per_node: usize,
    #[serde(default = "default_max_tokens")]
    pub max_tokens_per_node: usize,
    #[serde(default = "default_true")]
    pub add_node_summaries: bool,
    #[serde(default = "default_true")]
    pub pseudonymize_summaries: bool,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct VectorConfig {
    #[serde(default)]
    pub encrypt: bool,
    #[serde(default = "default_adcpe")]
    pub algorithm: String,
    pub key_env: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct LocalConfig {
    pub embeddings_model: Option<String>,
    #[serde(default = "default_vector_db")]
    pub vector_db: String,
    pub vector_db_path: Option<String>,
    pub llm_model: Option<String>,
    pub llm_backend: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AuditConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_audit_path")]
    pub log_path: String,
    #[serde(default = "default_jsonl")]
    pub format: String,
    #[serde(default = "default_retention")]
    pub retention_days: u32,
    #[serde(default = "default_true")]
    pub log_entities: bool,
    #[serde(default)]
    pub log_mappings: bool,
    #[serde(default = "default_audit_backend")]
    pub backend: String,
}

// Default value functions
fn default_true() -> bool {
    true
}
fn default_api_key_env() -> String {
    "OPENAI_API_KEY".into()
}
fn default_timeout() -> u64 {
    120
}
fn default_max_concurrent() -> usize {
    256
}
fn default_mode() -> ProxyMode {
    ProxyMode::Proxy
}
fn default_auth_mode() -> ProxyAuthMode {
    ProxyAuthMode::PassThrough
}
fn default_tunnel_unknown_hosts() -> bool {
    true
}
fn default_encryption() -> String {
    "aes-256-gcm".into()
}
fn default_vault_backend() -> String {
    "file".into()
}
fn default_audit_backend() -> String {
    "jsonl".into()
}
fn default_confidence() -> f64 {
    0.85
}
fn default_tree_path() -> String {
    "./trees/".into()
}
fn default_tree_model() -> String {
    "gpt-4o".into()
}
fn default_max_pages() -> usize {
    10
}
fn default_max_tokens() -> usize {
    20000
}
fn default_adcpe() -> String {
    "adcpe".into()
}
fn default_vector_db() -> String {
    "lancedb".into()
}
fn default_audit_path() -> String {
    "./audit/".into()
}
fn default_jsonl() -> String {
    "jsonl".into()
}
fn default_retention() -> u32 {
    90
}

impl Default for TreeConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            storage_path: default_tree_path(),
            index_model: default_tree_model(),
            search_model: default_tree_model(),
            max_pages_per_node: default_max_pages(),
            max_tokens_per_node: default_max_tokens(),
            add_node_summaries: true,
            pseudonymize_summaries: true,
        }
    }
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            log_path: default_audit_path(),
            format: default_jsonl(),
            retention_days: default_retention(),
            log_entities: true,
            log_mappings: false,
            backend: default_audit_backend(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{HttpProxyConfig, NerBackend, NerConfig, ProxyAuthMode, ProxyMode};
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Deserialize, Serialize)]
    struct NerBackendDoc {
        backend: NerBackend,
    }

    #[derive(Debug, Deserialize, Serialize)]
    struct ProxyModeDoc {
        mode: ProxyMode,
    }

    #[derive(Debug, Deserialize, Serialize)]
    struct ProxyAuthModeDoc {
        auth_mode: ProxyAuthMode,
    }

    #[test]
    fn ner_config_defaults_to_distilbert_pii_backend() {
        assert!(matches!(
            NerConfig::default().backend,
            NerBackend::DistilBertPii
        ));
    }

    #[test]
    fn ner_backend_serializes_documented_distilbert_pii_name() {
        assert_eq!(
            toml::to_string(&NerBackendDoc {
                backend: NerBackend::DistilBertPii,
            })
            .unwrap(),
            "backend = \"distilbert_pii\"\n"
        );
    }

    #[test]
    fn ner_backend_accepts_previous_lowercase_serialization() {
        let doc: NerBackendDoc = toml::from_str("backend = \"distilbertpii\"").unwrap();

        assert!(matches!(doc.backend, NerBackend::DistilBertPii));
    }

    #[test]
    fn proxy_mode_defaults_to_proxy() {
        assert!(matches!(super::default_mode(), ProxyMode::Proxy));
    }

    #[test]
    fn proxy_mode_serializes_documented_llm_http_name() {
        assert_eq!(
            toml::to_string(&ProxyModeDoc {
                mode: ProxyMode::LlmHttp,
            })
            .unwrap(),
            "mode = \"llm-http\"\n"
        );
    }

    #[test]
    fn proxy_mode_accepts_snake_case_alias() {
        let doc: ProxyModeDoc = toml::from_str("mode = \"llm_http\"").unwrap();

        assert!(matches!(doc.mode, ProxyMode::LlmHttp));
    }

    #[test]
    fn proxy_mode_accepts_legacy_cloaktree_alias() {
        let doc: ProxyModeDoc = toml::from_str("mode = \"cloaktree\"").unwrap();

        assert!(matches!(doc.mode, ProxyMode::Proxy));
    }

    #[test]
    fn proxy_mode_serializes_documented_http_proxy_name() {
        assert_eq!(
            toml::to_string(&ProxyModeDoc {
                mode: ProxyMode::HttpProxy,
            })
            .unwrap(),
            "mode = \"http-proxy\"\n"
        );
    }

    #[test]
    fn proxy_mode_accepts_http_proxy_snake_case_alias() {
        let doc: ProxyModeDoc = toml::from_str("mode = \"http_proxy\"").unwrap();

        assert!(matches!(doc.mode, ProxyMode::HttpProxy));
    }

    #[test]
    fn http_proxy_config_defaults_are_safe() {
        let config = HttpProxyConfig::default();

        assert!(!config.inspect_https);
        assert!(config.allowed_hosts.is_empty());
        assert!(config.tunnel_unknown_hosts);
        assert!(!config.http2_mitm);
        assert!(config.max_connect_tunnels.is_none());
    }

    #[test]
    fn proxy_auth_mode_serializes_documented_pass_through_name() {
        assert_eq!(
            toml::to_string(&ProxyAuthModeDoc {
                auth_mode: ProxyAuthMode::PassThrough,
            })
            .unwrap(),
            "auth_mode = \"pass-through\"\n"
        );
    }
}
