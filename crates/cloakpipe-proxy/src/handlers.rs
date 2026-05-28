//! HTTP request handlers for the proxy.

use crate::state::AppState;
use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use cloakpipe_audit::{AuditContext, AuditEvent};
use cloakpipe_core::{
    config::DetectionConfig, detector::Detector, profiles::IndustryProfile, rehydrator::Rehydrator,
    replacer::Replacer, session::GLOBAL_SESSION_ID, PseudoToken,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct PseudonymizeRequest {
    pub text: String,
}

#[derive(Debug, Deserialize)]
pub struct RehydrateRequest {
    pub text: String,
}

#[derive(Debug, Deserialize)]
pub struct DetectRequest {
    pub text: String,
}

#[derive(Debug, Deserialize)]
pub struct ConfigureRequest {
    pub profile: Option<String>,
    pub enable: Option<Vec<String>>,
    pub disable: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct SessionContextRequest {
    pub session_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct PseudonymizeResponse {
    text: String,
    entities_detected: usize,
    categories: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct RehydrateResponse {
    text: String,
    tokens_rehydrated: usize,
}

#[derive(Debug, Serialize, Deserialize)]
struct DetectResponse {
    entities: Vec<EntityInfo>,
}

#[derive(Debug, Serialize, Deserialize)]
struct EntityInfo {
    original: String,
    category: String,
    confidence: f64,
    source: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct VaultStatsResponse {
    total_mappings: usize,
    categories: HashMap<String, u32>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ConfigureResponse {
    active_profile: Option<String>,
    secrets: bool,
    financial: bool,
    dates: bool,
    emails: bool,
    phone_numbers: bool,
    ip_addresses: bool,
}

/// Health check endpoint.
pub async fn health() -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "ok",
        "service": "cloakpipe"
    }))
}

/// Direct privacy endpoint: pseudonymize raw text.
pub async fn api_pseudonymize(
    State(state): State<Arc<AppState>>,
    Json(params): Json<PseudonymizeRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let request_id = Uuid::new_v4().to_string();
    let entities = detect_entities(&state, &params.text).await.map_err(|e| {
        tracing::error!(request_id = %request_id, "Detection failed: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Error: Detection failed: {}", e),
        )
    })?;

    let response = {
        let mut vault = state.vault.lock().await;
        let result = Replacer::pseudonymize(&params.text, &entities, &mut vault).map_err(|e| {
            tracing::error!(request_id = %request_id, "Pseudonymize failed: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Error: Pseudonymize failed: {}", e),
            )
        })?;
        let tokens: Vec<PseudoToken> = entities
            .iter()
            .map(|entity| vault.get_or_create(&entity.original, &entity.category))
            .collect();
        state.sessions.with_session(GLOBAL_SESSION_ID, |ctx| {
            ctx.record_entities(&entities, &tokens);
        });

        let categories = entity_categories(&entities);
        let _ = state.audit.log_pseudonymize(
            AuditContext::new("api", &request_id).with_session(Some(GLOBAL_SESSION_ID)),
            entities.len(),
            entities.len(),
            categories.clone(),
        );

        PseudonymizeResponse {
            text: result.text,
            entities_detected: entities.len(),
            categories,
        }
    };

    Ok(Json(response))
}

/// Direct privacy endpoint: rehydrate CloakPipe tokens.
pub async fn api_rehydrate(
    State(state): State<Arc<AppState>>,
    Json(params): Json<RehydrateRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let request_id = Uuid::new_v4().to_string();
    let response = {
        let vault = state.vault.lock().await;
        let result = Rehydrator::rehydrate(&params.text, &vault).map_err(|e| {
            tracing::error!(request_id = %request_id, "Rehydrate failed: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Error: Rehydrate failed: {}", e),
            )
        })?;

        let _ = state.audit.log_rehydrate(
            AuditContext::new("api", &request_id).with_session(Some(GLOBAL_SESSION_ID)),
            result.rehydrated_count,
        );

        RehydrateResponse {
            text: result.text,
            tokens_rehydrated: result.rehydrated_count,
        }
    };

    Ok(Json(response))
}

/// Direct privacy endpoint: dry-run detection without replacement.
pub async fn api_detect(
    State(state): State<Arc<AppState>>,
    Json(params): Json<DetectRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let request_id = Uuid::new_v4().to_string();
    let entities = detect_entities(&state, &params.text).await.map_err(|e| {
        tracing::error!(request_id = %request_id, "Detection failed: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Error: Detection failed: {}", e),
        )
    })?;
    let categories = entity_categories(&entities);
    let _ = state.audit.log_metadata(
        AuditContext::new("api", &request_id).with_session(Some(GLOBAL_SESSION_ID)),
        AuditEvent::Detect,
        Some(entities.len()),
        categories,
    );

    Ok(Json(DetectResponse {
        entities: entities
            .into_iter()
            .map(|entity| EntityInfo {
                original: entity.original,
                category: format!("{:?}", entity.category),
                confidence: entity.confidence,
                source: format!("{:?}", entity.source),
            })
            .collect(),
    }))
}

/// Direct privacy endpoint: safe aggregate vault stats.
pub async fn api_vault_stats(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let request_id = Uuid::new_v4().to_string();
    let response = {
        let vault = state.vault.lock().await;
        let stats = vault.stats();
        VaultStatsResponse {
            total_mappings: stats.total_mappings,
            categories: stats.categories,
        }
    };
    let _ = state.audit.log_metadata(
        AuditContext::new("api", &request_id).with_session(Some(GLOBAL_SESSION_ID)),
        AuditEvent::VaultStats,
        None,
        Vec::new(),
    );

    Json(response)
}

/// Direct privacy endpoint: switch profiles or toggle detection categories.
pub async fn api_configure(
    State(state): State<Arc<AppState>>,
    Json(params): Json<ConfigureRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let request_id = Uuid::new_v4().to_string();
    let mut next_config = state.detection_config.read().await.clone();
    let mut next_active_profile = state.active_profile.read().await.clone();

    if let Some(ref profile_name) = params.profile {
        if let Some(profile) = IndustryProfile::from_name(profile_name) {
            next_config = profile.detection_config();
            next_active_profile = Some(profile.name().to_string());
        } else {
            return Err((
                StatusCode::BAD_REQUEST,
                format!(
                    "Error: Unknown profile '{}'. Use: general, legal, healthcare, fintech",
                    profile_name
                ),
            ));
        }
    }

    if let Some(ref enable) = params.enable {
        apply_toggles(&mut next_config, enable, true);
    }
    if let Some(ref disable) = params.disable {
        apply_toggles(&mut next_config, disable, false);
    }

    let new_detector = Detector::from_config(&next_config).map_err(|e| {
        tracing::error!("Failed to rebuild detector: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Error: Failed to rebuild detector: {}", e),
        )
    })?;

    {
        let mut config = state.detection_config.write().await;
        *config = next_config.clone();
    }
    {
        let mut detector = state.detector.write().await;
        *detector = new_detector;
    }
    {
        let mut active_profile = state.active_profile.write().await;
        *active_profile = next_active_profile.clone();
    }
    let _ = state.audit.log_metadata(
        AuditContext::new("api", &request_id).with_session(Some(GLOBAL_SESSION_ID)),
        AuditEvent::Configure,
        None,
        Vec::new(),
    );

    Ok(Json(ConfigureResponse {
        active_profile: next_active_profile,
        secrets: next_config.secrets,
        financial: next_config.financial,
        dates: next_config.dates,
        emails: next_config.emails,
        phone_numbers: next_config.phone_numbers,
        ip_addresses: next_config.ip_addresses,
    }))
}

/// Direct privacy endpoint: inspect session stats in the MCP tool shape.
pub async fn api_session_context(
    State(state): State<Arc<AppState>>,
    Json(params): Json<SessionContextRequest>,
) -> impl IntoResponse {
    let request_id = Uuid::new_v4().to_string();
    let body = if params.session_id == "list" {
        let sessions = state.sessions.list_sessions();
        if sessions.is_empty() {
            serde_json::json!({
                "sessions": [],
                "note": "No active sessions. Sessions are created when requests include x-session-id header."
            })
        } else {
            serde_json::json!({
                "sessions": sessions,
                "total": sessions.len(),
            })
        }
    } else if let Some(stats) = state.sessions.inspect(&params.session_id) {
        serde_json::json!(stats)
    } else {
        serde_json::json!({
            "error": format!("Session '{}' not found", params.session_id)
        })
    };
    let _ = state.audit.log_metadata(
        AuditContext::new("api", &request_id).with_session(Some(GLOBAL_SESSION_ID)),
        AuditEvent::SessionContext,
        None,
        Vec::new(),
    );

    Json(body)
}

// --- Session management endpoints ---

pub async fn sessions_list(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    Json(state.sessions.list_sessions())
}

pub async fn session_inspect(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(session_id): axum::extract::Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    state.sessions.inspect(&session_id).map(Json).ok_or((
        StatusCode::NOT_FOUND,
        format!("Session {} not found", session_id),
    ))
}

pub async fn session_flush(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(session_id): axum::extract::Path<String>,
) -> impl IntoResponse {
    let flushed = state.sessions.flush_session(&session_id);
    Json(serde_json::json!({ "flushed": flushed, "session_id": session_id }))
}

pub async fn sessions_flush_all(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let count = state.sessions.flush_all();
    Json(serde_json::json!({ "flushed": count }))
}

async fn detect_entities(
    state: &AppState,
    text: &str,
) -> anyhow::Result<Vec<cloakpipe_core::DetectedEntity>> {
    let detector = state.detector.read().await;
    detector.detect(text)
}

fn entity_categories(entities: &[cloakpipe_core::DetectedEntity]) -> Vec<String> {
    let mut categories: Vec<String> = entities
        .iter()
        .map(|entity| format!("{:?}", entity.category))
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    categories.sort();
    categories
}

fn apply_toggles(config: &mut DetectionConfig, categories: &[String], value: bool) {
    for category in categories {
        match category.to_lowercase().as_str() {
            "secrets" => config.secrets = value,
            "financial" => config.financial = value,
            "dates" => config.dates = value,
            "emails" => config.emails = value,
            "phone_numbers" | "phone" => config.phone_numbers = value,
            "ip_addresses" | "ip" => config.ip_addresses = value,
            "urls_internal" | "urls" => config.urls_internal = value,
            _ => {}
        }
    }
}
