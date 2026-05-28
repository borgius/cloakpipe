//! Shared application state for the proxy server.

use cloakpipe_audit::AuditSink;
use cloakpipe_core::{
    config::{CloakPipeConfig, DetectionConfig, ProxyAuthMode},
    detector::Detector,
    session::{ensure_global_session, SessionManager},
    vault::Vault,
};
use std::sync::Arc;
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
    pub api_key: Option<String>,
    pub sessions: Arc<SessionManager>,
}

impl AppState {
    pub fn new(
        config: CloakPipeConfig,
        detector: Detector,
        vault: Vault,
        audit: AuditSink,
        api_key: Option<String>,
    ) -> Self {
        let detection_config = config.detection.clone();
        let active_profile = config.profile.clone();
        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(config.proxy.timeout_seconds))
            .build()
            .expect("Failed to build HTTP client");

        let sessions = Arc::new(SessionManager::new(config.session.clone()));
        ensure_global_session(&sessions);

        Self {
            config,
            detector: Arc::new(RwLock::new(detector)),
            detection_config: Arc::new(RwLock::new(detection_config)),
            active_profile: Arc::new(RwLock::new(active_profile)),
            vault: Arc::new(Mutex::new(vault)),
            audit,
            http_client,
            api_key,
            sessions,
        }
    }

    /// Return the configured upstream API key, if present.
    pub fn upstream_api_key(&self) -> Option<&str> {
        self.api_key.as_deref()
    }

    /// Return the standard missing-key message for upstream-backed routes.
    pub fn missing_api_key_message(&self) -> String {
        format!("Set {} with your API key", self.config.proxy.api_key_env)
    }

    /// Return whether llm-http mode should forward caller credentials directly.
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
