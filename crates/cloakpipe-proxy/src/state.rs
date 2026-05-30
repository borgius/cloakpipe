//! Shared application state for the proxy server.

use crate::outbound_proxy;
use anyhow::Result;
use cloakpipe_audit::AuditSink;
use cloakpipe_core::{
    config::{CloakPipeConfig, DetectionConfig, ProxyAuthMode},
    detector::Detector,
    session::{ensure_global_session, SessionManager},
    vault::Vault,
};
use std::sync::Arc;
use std::sync::RwLock as StdRwLock;
use tokio::sync::{Mutex, RwLock};

/// Shared state accessible from all request handlers.
pub struct AppState {
    pub config: CloakPipeConfig,
    pub detector: Arc<RwLock<Detector>>,
    pub detection_config: Arc<RwLock<DetectionConfig>>,
    pub active_profile: Arc<RwLock<Option<String>>>,
    pub vault: Arc<Mutex<Vault>>,
    pub audit: AuditSink,
    pub http_client: reqwest::Client,
    pub direct_http_client: reqwest::Client,
    pub api_key: Option<String>,
    pub sessions: Arc<SessionManager>,
    /// Admin context: filesystem locations used by the admin API. Optional so
    /// existing constructors (and tests) keep working without admin wiring.
    pub admin: Arc<StdRwLock<AdminContext>>,
}

/// Filesystem context the admin API needs to read/write config and policies.
#[derive(Debug, Clone, Default)]
pub struct AdminContext {
    /// Path of the loaded config/policy file, if any.
    pub config_path: Option<std::path::PathBuf>,
    /// Directory where policy files are stored/managed.
    pub policies_dir: Option<std::path::PathBuf>,
    /// Directory where user-defined custom profiles are persisted.
    pub profiles_dir: Option<std::path::PathBuf>,
    /// ****** required for `/admin/api/*` requests. When `None` the admin
    /// API is unauthenticated (trusted/local use).
    pub admin_token: Option<String>,
}

impl AppState {
    pub fn new(
        config: CloakPipeConfig,
        detector: Detector,
        vault: Vault,
        audit: AuditSink,
        api_key: Option<String>,
    ) -> Self {
        Self::try_new(config, detector, vault, audit, api_key).expect("Failed to build app state")
    }

    pub fn try_new(
        config: CloakPipeConfig,
        detector: Detector,
        vault: Vault,
        audit: AuditSink,
        api_key: Option<String>,
    ) -> Result<Self> {
        let detection_config = config.detection.clone();
        let active_profile = config.profile.clone();
        let http_client = outbound_proxy::build_http_client(&config, true)?;
        let direct_http_client = outbound_proxy::build_http_client(&config, false)?;

        let sessions = Arc::new(SessionManager::new(config.session.clone()));
        ensure_global_session(&sessions);

        Ok(Self {
            config,
            detector: Arc::new(RwLock::new(detector)),
            detection_config: Arc::new(RwLock::new(detection_config)),
            active_profile: Arc::new(RwLock::new(active_profile)),
            vault: Arc::new(Mutex::new(vault)),
            audit,
            http_client,
            direct_http_client,
            api_key,
            sessions,
            admin: Arc::new(StdRwLock::new(AdminContext::default())),
        })
    }

    /// Attach admin filesystem context (loaded config path + policies dir).
    pub fn with_admin_context(
        self,
        config_path: Option<std::path::PathBuf>,
        policies_dir: Option<std::path::PathBuf>,
    ) -> Self {
        if let Ok(mut admin) = self.admin.write() {
            admin.config_path = config_path;
            admin.policies_dir = policies_dir;
        }
        self
    }

    /// Attach the directory used to persist user-defined custom profiles.
    pub fn with_profiles_dir(self, profiles_dir: Option<std::path::PathBuf>) -> Self {
        if let Ok(mut admin) = self.admin.write() {
            admin.profiles_dir = profiles_dir;
        }
        self
    }

    /// Attach the admin API bearer token. When `Some(non-empty)`, every
    /// `/admin/api/*` request must present a matching `Authorization: ******
    /// header. Empty/whitespace tokens are treated as no authentication.
    pub fn with_admin_token(self, token: Option<String>) -> Self {
        let token = token.and_then(|t| {
            let t = t.trim().to_string();
            if t.is_empty() {
                None
            } else {
                Some(t)
            }
        });
        if let Ok(mut admin) = self.admin.write() {
            admin.admin_token = token;
        }
        self
    }

    /// Return whether the admin API currently requires authentication.
    pub fn admin_auth_required(&self) -> bool {
        self.admin
            .read()
            .ok()
            .map(|a| a.admin_token.is_some())
            .unwrap_or(false)
    }

    /// Return the outbound HTTP client for a target URL, honoring forward_no_proxy.
    pub fn http_client_for_url(&self, target_url: &str) -> &reqwest::Client {
        if let Ok(url) = reqwest::Url::parse(target_url) {
            if let Some(host) = url.host_str() {
                if outbound_proxy::should_bypass_forward_proxy(&self.config.proxy.http_proxy, host)
                {
                    return &self.direct_http_client;
                }
            }
        }

        &self.http_client
    }

    /// Return the configured upstream API key, if present.
    pub fn upstream_api_key(&self) -> Option<&str> {
        self.api_key.as_deref()
    }

    /// Return the standard missing-key message for upstream-backed routes.
    pub fn missing_api_key_message(&self) -> String {
        format!("Set {} with your API key", self.config.proxy.api_key_env)
    }

    /// Return whether LLM proxy mode should forward caller credentials directly.
    pub fn use_passthrough_auth(&self) -> bool {
        matches!(self.config.proxy.auth_mode, ProxyAuthMode::PassThrough)
    }

    /// Return whether the given upstream should bypass request mutation and response rehydration.
    pub fn should_bypass_upstream(&self, upstream: &str) -> bool {
        let upstream = upstream.to_ascii_lowercase();
        self.config
            .proxy
            .bypass
            .iter()
            .map(|pattern| pattern.to_ascii_lowercase())
            .any(|pattern| upstream.contains(&pattern))
    }
}
