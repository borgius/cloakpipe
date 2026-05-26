//! HTTP request handlers for the proxy.

use crate::state::AppState;
use crate::streaming;
use axum::{
    body::Body,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use cloakpipe_audit::{AuditContext, AuditEvent};
use cloakpipe_core::{
    config::DetectionConfig, detector::Detector, profiles::IndustryProfile, rehydrator::Rehydrator,
    replacer::Replacer, session::GLOBAL_SESSION_ID, PseudoToken,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
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

/// Extract session ID from request headers based on config.
fn extract_session_id(headers: &HeaderMap, id_from: &str) -> Option<String> {
    if let Some(header_name) = id_from.strip_prefix("header:") {
        headers
            .get(header_name)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
    } else if id_from == "connection" {
        Some(Uuid::new_v4().to_string())
    } else {
        None
    }
}

fn require_upstream_api_key(state: &AppState) -> Result<String, (StatusCode, String)> {
    state.upstream_api_key().map(str::to_owned).ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            state.missing_api_key_message(),
        )
    })
}

/// Proxy handler for /v1/chat/completions.
/// Pseudonymizes the request, forwards to upstream, rehydrates the response.
pub async fn proxy_chat_completions(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(mut body): Json<Value>,
) -> Result<Response, (StatusCode, String)> {
    let request_id = Uuid::new_v4().to_string();
    let is_streaming = body
        .get("stream")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let api_key = require_upstream_api_key(&state)?;

    // Extract session ID if session tracking is enabled
    let session_id = if state.sessions.is_enabled() {
        let sid = extract_session_id(&headers, &state.config.session.id_from);
        if let Some(ref id) = sid {
            state.sessions.get_or_create(id);
        }
        sid
    } else {
        None
    };

    // Pseudonymize message contents (session-aware)
    let entities_count =
        pseudonymize_messages(&state, &mut body, &request_id, session_id.as_deref())
            .await
            .map_err(|e| {
                tracing::error!(request_id = %request_id, "Pseudonymization failed: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Pseudonymization failed: {}", e),
                )
            })?;

    tracing::info!(
        request_id = %request_id,
        entities = entities_count,
        streaming = is_streaming,
        session_id = ?session_id,
        "Forwarding pseudonymized request"
    );

    // Forward to upstream
    let upstream_url = format!(
        "{}/v1/chat/completions",
        state.config.proxy.upstream.trim_end_matches('/')
    );

    let mut req = state
        .http_client
        .post(&upstream_url)
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&body);

    if let Some(org) = headers.get("openai-organization") {
        req = req.header("OpenAI-Organization", org);
    }

    let upstream_resp = req.send().await.map_err(|e| {
        tracing::error!(request_id = %request_id, "Upstream request failed: {}", e);
        (
            StatusCode::BAD_GATEWAY,
            format!("Upstream request failed: {}", e),
        )
    })?;

    let status = upstream_resp.status();
    if !status.is_success() {
        let error_body = upstream_resp.text().await.unwrap_or_default();
        tracing::warn!(request_id = %request_id, status = %status, "Upstream error");
        return Ok(Response::builder()
            .status(StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY))
            .header("Content-Type", "application/json")
            .body(Body::from(error_body))
            .unwrap());
    }

    if is_streaming {
        let vault = state.vault.clone();
        let stream = streaming::rehydrate_stream(upstream_resp, vault, request_id.clone()).await;

        Ok(Response::builder()
            .status(200)
            .header("Content-Type", "text/event-stream")
            .header("Cache-Control", "no-cache")
            .header("X-CloakPipe-Request-Id", &request_id)
            .body(Body::from_stream(stream))
            .unwrap())
    } else {
        let resp_text = upstream_resp.text().await.map_err(|e| {
            (
                StatusCode::BAD_GATEWAY,
                format!("Failed to read upstream response: {}", e),
            )
        })?;

        let mut resp_json: Value = serde_json::from_str(&resp_text).map_err(|e| {
            (
                StatusCode::BAD_GATEWAY,
                format!("Invalid upstream JSON: {}", e),
            )
        })?;

        // --- Response output scanning ---
        // Detect PII in the raw LLM response. Any entity NOT already in our vault
        // is leaked/hallucinated PII — redact it before returning to the caller.
        let vault_read = state.vault.lock().await;
        let mut leaked_count = 0usize;
        if let Some(choices) = resp_json.get_mut("choices").and_then(|c| c.as_array_mut()) {
            for choice in choices.iter_mut() {
                if let Some(content) = choice
                    .get("message")
                    .and_then(|m| m.get("content"))
                    .and_then(|c| c.as_str())
                    .map(|s| s.to_string())
                {
                    if let Ok(mut scan_entities) = detect_entities(&state, &content).await {
                        // Remove entities that are already in the vault (expected tokens)
                        scan_entities.retain(|e| !vault_read.contains_original(&e.original));
                        if !scan_entities.is_empty() {
                            leaked_count += scan_entities.len();
                            tracing::warn!(
                                request_id = %request_id,
                                leaked = scan_entities.len(),
                                "PII leakage detected in LLM response — redacting"
                            );
                            // Redact leaked entities by replacing with [REDACTED]
                            let mut redacted = content.clone();
                            // Sort descending by start so replacements don't shift offsets
                            scan_entities.sort_by_key(|entity| std::cmp::Reverse(entity.start));
                            for entity in &scan_entities {
                                redacted.replace_range(entity.start..entity.end, "[REDACTED]");
                            }
                            choice["message"]["content"] = Value::String(redacted);
                        }
                    }
                }
            }
        }
        drop(vault_read);
        // --- End response scanning ---

        let vault = state.vault.lock().await;
        if let Some(choices) = resp_json.get_mut("choices").and_then(|c| c.as_array_mut()) {
            for choice in choices {
                if let Some(content) = choice
                    .get_mut("message")
                    .and_then(|m| m.get_mut("content"))
                    .and_then(|c| c.as_str())
                    .map(|s| s.to_string())
                {
                    if let Ok(rehydrated) = Rehydrator::rehydrate(&content, &vault) {
                        choice["message"]["content"] = Value::String(rehydrated.text);
                        let _ = state.audit.log_rehydrate(
                            AuditContext::new("proxy", &request_id)
                                .with_session(session_id.as_deref()),
                            rehydrated.rehydrated_count,
                        );
                    }
                }
            }
        }

        Ok(Response::builder()
            .status(200)
            .header("Content-Type", "application/json")
            .header("X-CloakPipe-Request-Id", &request_id)
            .header("X-CloakPipe-Leaked-Entities", leaked_count.to_string())
            .body(Body::from(serde_json::to_string(&resp_json).unwrap()))
            .unwrap())
    }
}

/// Proxy handler for /v1/embeddings.
pub async fn proxy_embeddings(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(mut body): Json<Value>,
) -> Result<Response, (StatusCode, String)> {
    let request_id = Uuid::new_v4().to_string();
    let api_key = require_upstream_api_key(&state)?;

    let session_id = if state.sessions.is_enabled() {
        let sid = extract_session_id(&headers, &state.config.session.id_from);
        if let Some(ref id) = sid {
            state.sessions.get_or_create(id);
        }
        sid
    } else {
        None
    };

    let entities_count =
        pseudonymize_embedding_input(&state, &mut body, &request_id, session_id.as_deref())
            .await
            .map_err(|e| {
                tracing::error!(request_id = %request_id, "Pseudonymization failed: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Pseudonymization failed: {}", e),
                )
            })?;

    tracing::info!(
        request_id = %request_id,
        entities = entities_count,
        "Forwarding pseudonymized embeddings request"
    );

    let upstream_url = format!(
        "{}/v1/embeddings",
        state.config.proxy.upstream.trim_end_matches('/')
    );

    let mut req = state
        .http_client
        .post(&upstream_url)
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&body);

    if let Some(org) = headers.get("openai-organization") {
        req = req.header("OpenAI-Organization", org);
    }

    let upstream_resp = req.send().await.map_err(|e| {
        tracing::error!(request_id = %request_id, "Upstream request failed: {}", e);
        (
            StatusCode::BAD_GATEWAY,
            format!("Upstream request failed: {}", e),
        )
    })?;

    let status = upstream_resp.status();
    let resp_body = upstream_resp.text().await.unwrap_or_default();

    if !status.is_success() {
        tracing::warn!(request_id = %request_id, status = %status, "Upstream error");
    }

    Ok(Response::builder()
        .status(StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::OK))
        .header("Content-Type", "application/json")
        .header("X-CloakPipe-Request-Id", &request_id)
        .body(Body::from(resp_body))
        .unwrap())
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

// --- Internal pseudonymization helpers ---

/// Pseudonymize all message contents in the request body.
/// When session tracking is enabled, also resolves coreferences and checks sensitivity.
async fn pseudonymize_messages(
    state: &AppState,
    body: &mut Value,
    request_id: &str,
    session_id: Option<&str>,
) -> anyhow::Result<usize> {
    let mut total_entities = 0;

    if let Some(messages) = body.get_mut("messages").and_then(|m| m.as_array_mut()) {
        for msg in messages {
            if let Some(content) = msg
                .get_mut("content")
                .and_then(|c| c.as_str())
                .map(|s| s.to_string())
            {
                // Check sensitivity escalation before detection
                if let Some(sid) = session_id {
                    state.sessions.with_session(sid, |ctx| {
                        if ctx.check_sensitivity(&content) {
                            tracing::info!(
                                session_id = sid,
                                request_id = request_id,
                                sensitivity = ?ctx.sensitivity,
                                keywords = ?ctx.escalation_keywords,
                                "Sensitivity escalated to elevated"
                            );
                        }
                    });
                }

                // Standard detection
                let mut entities = detect_entities(state, &content).await?;

                // Resolve coreferences from session context
                let mut coref_tokens: Vec<(usize, PseudoToken)> = Vec::new();
                if let Some(sid) = session_id {
                    if let Some(coref_results) = state
                        .sessions
                        .with_session_ref(sid, |ctx| ctx.resolve_coreferences(&content))
                    {
                        for (coref_entity, coref_token) in coref_results {
                            let overlaps = entities
                                .iter()
                                .any(|e| coref_entity.start < e.end && coref_entity.end > e.start);
                            if !overlaps {
                                let idx = entities.len();
                                entities.push(coref_entity);
                                coref_tokens.push((idx, coref_token));
                            }
                        }
                    }
                }

                if !entities.is_empty() {
                    entities.sort_by_key(|e| e.start);

                    let (pseudonymized_text, tokens) = {
                        let mut vault = state.vault.lock().await;
                        let result = Replacer::pseudonymize(&content, &entities, &mut vault)?;

                        let mut tokens: Vec<PseudoToken> = Vec::new();
                        for (i, e) in entities.iter().enumerate() {
                            if let Some((_, token)) = coref_tokens.iter().find(|(idx, _)| *idx == i)
                            {
                                tokens.push(token.clone());
                            } else {
                                tokens.push(vault.get_or_create(&e.original, &e.category));
                            }
                        }

                        (result.text, tokens)
                    };

                    msg["content"] = Value::String(pseudonymized_text);

                    // Record in session context
                    if let Some(sid) = session_id {
                        state.sessions.with_session(sid, |ctx| {
                            ctx.record_entities(&entities, &tokens);
                        });
                    }

                    let categories = entity_categories(&entities);

                    let _ = state.audit.log_pseudonymize(
                        AuditContext::new("proxy", request_id).with_session(session_id),
                        entities.len(),
                        entities.len(),
                        categories,
                    );

                    total_entities += entities.len();
                }
            }
        }
    }

    Ok(total_entities)
}

/// Pseudonymize embedding input (string or array of strings).
async fn pseudonymize_embedding_input(
    state: &AppState,
    body: &mut Value,
    request_id: &str,
    session_id: Option<&str>,
) -> anyhow::Result<usize> {
    let mut total_entities = 0;

    if let Some(input) = body.get_mut("input") {
        match input {
            Value::String(text) => {
                let original = text.clone();
                let entities = detect_entities(state, &original).await?;
                if !entities.is_empty() {
                    let result = {
                        let mut vault = state.vault.lock().await;
                        Replacer::pseudonymize(&original, &entities, &mut vault)?
                    };
                    *input = Value::String(result.text);

                    let categories = entity_categories(&entities);
                    let _ = state.audit.log_pseudonymize(
                        AuditContext::new("proxy", request_id).with_session(session_id),
                        entities.len(),
                        entities.len(),
                        categories,
                    );
                    total_entities += entities.len();
                }
            }
            Value::Array(items) => {
                for item in items.iter_mut() {
                    if let Some(text) = item.as_str().map(|s| s.to_string()) {
                        let entities = detect_entities(state, &text).await?;
                        if !entities.is_empty() {
                            let result = {
                                let mut vault = state.vault.lock().await;
                                Replacer::pseudonymize(&text, &entities, &mut vault)?
                            };
                            *item = Value::String(result.text);

                            let categories = entity_categories(&entities);
                            let _ = state.audit.log_pseudonymize(
                                AuditContext::new("proxy", request_id).with_session(session_id),
                                entities.len(),
                                entities.len(),
                                categories,
                            );
                            total_entities += entities.len();
                        }
                    }
                }
            }
            _ => {}
        }
    }

    Ok(total_entities)
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
