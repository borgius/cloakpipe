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
        })
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
