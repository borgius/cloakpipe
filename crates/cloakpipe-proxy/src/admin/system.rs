//! System / runtime status endpoints.

use super::AdminError;
use crate::state::AppState;
use axum::{extract::State, Json};
use cloakpipe_core::config::ProxyMode;
use serde::Serialize;
use std::sync::Arc;

#[derive(Serialize)]
pub struct SystemStatus {
    pub service: &'static str,
    pub version: &'static str,
    pub mode: String,
    pub listen: String,
    pub upstream: String,
    pub active_profile: Option<String>,
    pub config_path: Option<String>,
    pub policies_dir: Option<String>,
    pub masking_strategy: String,
    pub detection: DetectionSummary,
    pub ner: NerSummary,
    pub audit: AuditSummary,
    pub vault: VaultSummary,
}

#[derive(Serialize)]
pub struct DetectionSummary {
    pub secrets: bool,
    pub financial: bool,
    pub dates: bool,
    pub emails: bool,
    pub phone_numbers: bool,
    pub ip_addresses: bool,
    pub urls_internal: bool,
    pub custom_pattern_count: usize,
}

#[derive(Serialize)]
pub struct NerSummary {
    pub enabled: bool,
    pub backend: String,
    pub model: Option<String>,
    pub confidence_threshold: f64,
    pub sidecar_url: String,
    pub entity_types: Vec<String>,
}

#[derive(Serialize)]
pub struct AuditSummary {
    pub enabled: bool,
    pub backend: String,
    pub location: Option<String>,
    pub log_entities: bool,
    pub retention_days: u32,
}

#[derive(Serialize)]
pub struct VaultSummary {
    pub backend: String,
    pub path: Option<String>,
    pub persistent: bool,
    pub encryption: String,
    pub total_mappings: usize,
}

fn mode_label(mode: ProxyMode) -> &'static str {
    match mode {
        ProxyMode::Server => "server",
        ProxyMode::LlmProxy => "llm-proxy",
        ProxyMode::HttpProxy => "http-proxy",
    }
}

fn backend_label(backend: cloakpipe_audit::AuditBackend) -> &'static str {
    match backend {
        cloakpipe_audit::AuditBackend::Disabled => "disabled",
        cloakpipe_audit::AuditBackend::Jsonl => "jsonl",
        cloakpipe_audit::AuditBackend::Sqlite => "sqlite",
    }
}

/// `GET /admin/api/system` — current runtime + config state.
pub async fn get_system(
    State(state): State<Arc<AppState>>,
) -> Result<Json<SystemStatus>, AdminError> {
    let detection = state.detection_config.read().await.clone();
    let active_profile = state.active_profile.read().await.clone();

    let (config_path, policies_dir) = {
        let admin = state
            .admin
            .read()
            .map_err(|_| AdminError::internal("admin context lock poisoned"))?;
        (
            admin.config_path.as_ref().map(|p| p.display().to_string()),
            admin.policies_dir.as_ref().map(|p| p.display().to_string()),
        )
    };

    let masking_strategy = serde_json::to_value(state.config.proxy.masking_strategy)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_else(|| "similar".to_string());

    let ner_backend = serde_json::to_value(detection.ner.backend)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_else(|| "distilbert_pii".to_string());

    let total_mappings = {
        let vault = state.vault.lock().await;
        vault.stats().total_mappings
    };

    let status = SystemStatus {
        service: "cloakpipe",
        version: env!("CARGO_PKG_VERSION"),
        mode: mode_label(state.config.proxy.mode).to_string(),
        listen: state.config.proxy.listen.clone(),
        upstream: state.config.proxy.upstream.clone(),
        active_profile,
        config_path,
        policies_dir,
        masking_strategy,
        detection: DetectionSummary {
            secrets: detection.secrets,
            financial: detection.financial,
            dates: detection.dates,
            emails: detection.emails,
            phone_numbers: detection.phone_numbers,
            ip_addresses: detection.ip_addresses,
            urls_internal: detection.urls_internal,
            custom_pattern_count: detection.custom.patterns.len(),
        },
        ner: NerSummary {
            enabled: detection.ner.enabled,
            backend: ner_backend,
            model: detection.ner.model.clone(),
            confidence_threshold: detection.ner.confidence_threshold,
            sidecar_url: detection.ner.sidecar_url.clone(),
            entity_types: detection.ner.entity_types.clone(),
        },
        audit: AuditSummary {
            enabled: state.config.audit.enabled,
            backend: backend_label(state.audit.backend()).to_string(),
            location: state.audit.location(),
            log_entities: state.config.audit.log_entities,
            retention_days: state.config.audit.retention_days,
        },
        vault: VaultSummary {
            backend: state.config.vault.backend.clone(),
            path: {
                let vault = state.vault.lock().await;
                vault.path().map(str::to_string)
            },
            persistent: {
                let vault = state.vault.lock().await;
                vault.is_persistent()
            },
            encryption: state.config.vault.encryption.clone(),
            total_mappings,
        },
    };

    Ok(Json(status))
}

/// `GET /admin/api/sessions` — runtime session diagnostics.
pub async fn list_sessions(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let sessions = state.sessions.list_sessions();
    Json(serde_json::json!({
        "total": sessions.len(),
        "sessions": sessions,
    }))
}
