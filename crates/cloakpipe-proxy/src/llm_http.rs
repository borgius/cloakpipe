//! Raw LLM HTTP proxy handler for opt-in multi-provider traffic.

use crate::{json_filter, routing, state::AppState, streaming};
use axum::{
    body::Body,
    extract::State,
    http::{HeaderMap, Method, StatusCode, Uri},
    response::Response,
};
use cloakpipe_audit::AuditContext;
use cloakpipe_core::{
    rehydrator::Rehydrator, replacer::Replacer, vault::Vault, DetectedEntity, EntityCategory,
    MaskingStrategy, PseudoToken,
};
use http_body_util::BodyExt;
use reqwest::header::{HeaderName as ReqwestHeaderName, HeaderValue as ReqwestHeaderValue};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use uuid::Uuid;

pub(crate) struct PreparedRequest {
    pub(crate) body: Vec<u8>,
    pub(crate) entities_detected: usize,
    pub(crate) categories: Vec<String>,
}

struct TextMutation {
    text: String,
    entities: Vec<DetectedEntity>,
    categories: Vec<String>,
    tokens: Vec<PseudoToken>,
}

/// Catch-all raw proxy handler used only when `proxy.mode = "llm-http"`.
pub async fn proxy_request(
    State(state): State<Arc<AppState>>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Body,
) -> Result<Response, (StatusCode, String)> {
    let request_id = Uuid::new_v4().to_string();
    let path_and_query = uri
        .path_and_query()
        .map(|path| path.as_str())
        .unwrap_or_else(|| uri.path());

    let Some(route) = routing::resolve_provider(path_and_query, &state.config.proxy) else {
        return Err((
            StatusCode::BAD_GATEWAY,
            format!(
                "No provider matched for path: {}. Use an explicit provider prefix such as /anthropic or an OpenAI-compatible route.",
                path_and_query
            ),
        ));
    };

    let body_bytes = body
        .collect()
        .await
        .map_err(|error| {
            (
                StatusCode::BAD_REQUEST,
                format!("Failed to read request body: {error}"),
            )
        })?
        .to_bytes();

    let session_id = extract_session_id(&state, &headers);
    let should_bypass = state.should_bypass_upstream(&route.upstream_base);
    let prepared = prepare_request_body(
        &state,
        &headers,
        &body_bytes,
        session_id.as_deref(),
        should_bypass,
        &request_id,
    )
    .await?;

    if prepared.entities_detected > 0 {
        let _ = state.audit.log_pseudonymize(
            AuditContext::new("proxy", &request_id).with_session(session_id.as_deref()),
            prepared.entities_detected,
            prepared.entities_detected,
            prepared.categories.clone(),
        );
    }

    let target_url = format!(
        "{}{}",
        route.upstream_base.trim_end_matches('/'),
        route.forward_path
    );

    let method = reqwest::Method::from_bytes(method.as_str().as_bytes()).map_err(|error| {
        (
            StatusCode::BAD_REQUEST,
            format!("Unsupported HTTP method: {error}"),
        )
    })?;

    let request_builder =
        build_upstream_request(&state, method, &target_url, &headers, prepared.body)?;

    let upstream_response = request_builder.send().await.map_err(|error| {
        let _ = state.audit.log_error(
            AuditContext::new("proxy", &request_id).with_session(session_id.as_deref()),
            &format!("Upstream request failed: {error}"),
        );
        (
            StatusCode::BAD_GATEWAY,
            format!("Upstream request failed: {error}"),
        )
    })?;

    let skip_rehydration = should_bypass || state.config.proxy.dry_run;
    build_upstream_response(
        &state,
        upstream_response,
        skip_rehydration,
        &request_id,
        session_id.as_deref(),
    )
    .await
}

pub(crate) async fn prepare_request_body(
    state: &AppState,
    headers: &HeaderMap,
    body: &[u8],
    session_id: Option<&str>,
    should_bypass: bool,
    request_id: &str,
) -> Result<PreparedRequest, (StatusCode, String)> {
    if body.is_empty() || should_bypass || has_non_identity_encoding(headers) {
        return Ok(PreparedRequest {
            body: body.to_vec(),
            entities_detected: 0,
            categories: Vec::new(),
        });
    }

    let content_type = header_value(headers, "content-type");
    if !json_filter::is_textual_content_type(&content_type) {
        return Ok(PreparedRequest {
            body: body.to_vec(),
            entities_detected: 0,
            categories: Vec::new(),
        });
    }

    if json_filter::is_json_content_type(&content_type) {
        if let Ok(mut json) = serde_json::from_slice::<Value>(body) {
            let mut detected_count = 0usize;
            let mut categories = HashSet::new();

            if state.config.proxy.dry_run {
                let detector = state.detector.read().await;
                let mut detect_only = |text: &str| -> anyhow::Result<Option<String>> {
                    let entities = detect_sorted_entities(&detector, text)?;
                    if entities.is_empty() || json_filter::should_skip_text_rewrite(text) {
                        return Ok(None);
                    }
                    detected_count += entities.len();
                    categories.extend(entity_categories(&entities));
                    Ok(None)
                };

                json_filter::mutate_json_text(&mut json, &mut detect_only).map_err(|error| {
                    (
                        StatusCode::BAD_REQUEST,
                        format!("Failed to inspect JSON request: {error}"),
                    )
                })?;

                return Ok(PreparedRequest {
                    body: body.to_vec(),
                    entities_detected: detected_count,
                    categories: sorted_categories(categories),
                });
            }

            let detector = state.detector.read().await;
            let mut vault = state.vault.lock().await;
            let mut mutate = |text: &str| -> anyhow::Result<Option<String>> {
                let Some(mutation) = pseudonymize_text(
                    text,
                    &detector,
                    &mut vault,
                    state.config.proxy.masking_strategy,
                    state,
                    session_id,
                )?
                else {
                    return Ok(None);
                };

                detected_count += mutation.entities.len();
                categories.extend(mutation.categories.iter().cloned());
                let _ = &mutation.tokens;
                Ok(Some(mutation.text))
            };

            json_filter::mutate_json_text(&mut json, &mut mutate).map_err(|error| {
                (
                    StatusCode::BAD_REQUEST,
                    format!("Failed to mutate JSON request: {error}"),
                )
            })?;

            let mutated = serde_json::to_vec(&json).map_err(|error| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to serialize mutated JSON request: {error}"),
                )
            })?;

            return Ok(PreparedRequest {
                body: mutated,
                entities_detected: detected_count,
                categories: sorted_categories(categories),
            });
        }
    }

    let text = match std::str::from_utf8(body) {
        Ok(text) => text,
        Err(_) => {
            return Ok(PreparedRequest {
                body: body.to_vec(),
                entities_detected: 0,
                categories: Vec::new(),
            });
        }
    };

    if json_filter::should_skip_text_rewrite(text) {
        return Ok(PreparedRequest {
            body: body.to_vec(),
            entities_detected: 0,
            categories: Vec::new(),
        });
    }

    if state.config.proxy.dry_run {
        let detector = state.detector.read().await;
        let entities = detect_sorted_entities(&detector, text).map_err(|error| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Detection failed: {error}"),
            )
        })?;

        return Ok(PreparedRequest {
            body: body.to_vec(),
            entities_detected: entities.len(),
            categories: entity_categories(&entities),
        });
    }

    let detector = state.detector.read().await;
    let mut vault = state.vault.lock().await;
    let Some(mutation) = pseudonymize_text(
        text,
        &detector,
        &mut vault,
        state.config.proxy.masking_strategy,
        state,
        session_id,
    )
    .map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Pseudonymization failed: {error}"),
        )
    })?
    else {
        return Ok(PreparedRequest {
            body: body.to_vec(),
            entities_detected: 0,
            categories: Vec::new(),
        });
    };

    let _ = request_id;
    Ok(PreparedRequest {
        body: mutation.text.into_bytes(),
        entities_detected: mutation.entities.len(),
        categories: mutation.categories,
    })
}

pub(crate) fn build_upstream_request(
    state: &AppState,
    method: reqwest::Method,
    target_url: &str,
    headers: &HeaderMap,
    body: Vec<u8>,
) -> Result<reqwest::RequestBuilder, (StatusCode, String)> {
    let mut builder = state.http_client.request(method, target_url);

    for (name, value) in headers {
        let header_name = name.as_str().to_ascii_lowercase();
        if should_drop_request_header(&header_name) {
            continue;
        }

        if !state.use_passthrough_auth() && is_auth_header(&header_name) {
            continue;
        }

        if let Ok(reqwest_name) = ReqwestHeaderName::from_bytes(name.as_str().as_bytes()) {
            if let Ok(reqwest_value) = ReqwestHeaderValue::from_bytes(value.as_bytes()) {
                builder = builder.header(reqwest_name, reqwest_value);
            }
        }
    }

    if state.use_passthrough_auth() {
        builder = builder.header("accept-encoding", "identity");
        return Ok(builder.body(body));
    }

    let api_key = state.upstream_api_key().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            state.missing_api_key_message(),
        )
    })?;

    Ok(builder
        .header("accept-encoding", "identity")
        .header("authorization", format!("Bearer {api_key}"))
        .body(body))
}

pub(crate) async fn build_upstream_response(
    state: &AppState,
    response: reqwest::Response,
    skip_rehydration: bool,
    request_id: &str,
    session_id: Option<&str>,
) -> Result<Response, (StatusCode, String)> {
    let status = response.status();
    let headers = response.headers().clone();
    let content_type = header_content_type(&headers);
    let mappings = if skip_rehydration {
        HashMap::new()
    } else {
        state.vault.lock().await.reverse_mappings()
    };

    if should_stream_response(&headers, &content_type) {
        let skip_stream_rehydration = skip_rehydration
            || !json_filter::is_textual_content_type(&content_type)
            || has_non_identity_response_encoding(&headers);
        let stream = streaming::rehydrate_text_stream(
            response,
            mappings,
            skip_stream_rehydration,
            request_id.to_string(),
        );

        return build_response(status, &headers, Body::from_stream(stream), request_id);
    }

    let body_bytes = response.bytes().await.map_err(|error| {
        (
            StatusCode::BAD_GATEWAY,
            format!("Failed to read upstream response: {error}"),
        )
    })?;

    if skip_rehydration
        || mappings.is_empty()
        || !json_filter::is_textual_content_type(&content_type)
        || has_non_identity_response_encoding(&headers)
    {
        return build_response(status, &headers, Body::from(body_bytes), request_id);
    }

    let text = match String::from_utf8(body_bytes.to_vec()) {
        Ok(text) => text,
        Err(error) => {
            return build_response(status, &headers, Body::from(error.into_bytes()), request_id);
        }
    };

    if json_filter::should_skip_text_rewrite(&text) {
        return build_response(status, &headers, Body::from(text), request_id);
    }

    let rehydrated = Rehydrator::rehydrate_from_mappings(&text, &mappings).map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to rehydrate upstream response: {error}"),
        )
    })?;

    if rehydrated.rehydrated_count > 0 {
        let _ = state.audit.log_rehydrate(
            AuditContext::new("proxy", request_id).with_session(session_id),
            rehydrated.rehydrated_count,
        );
    }

    build_response(status, &headers, Body::from(rehydrated.text), request_id)
}

fn build_response(
    status: reqwest::StatusCode,
    headers: &reqwest::header::HeaderMap,
    body: Body,
    request_id: &str,
) -> Result<Response, (StatusCode, String)> {
    let mut builder = Response::builder()
        .status(StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY))
        .header("X-CloakPipe-Request-Id", request_id);

    for (name, value) in headers {
        let header_name = name.as_str().to_ascii_lowercase();
        if matches!(header_name.as_str(), "content-length" | "transfer-encoding") {
            continue;
        }

        if let Ok(axum_name) = axum::http::HeaderName::from_bytes(name.as_str().as_bytes()) {
            if let Ok(axum_value) = axum::http::HeaderValue::from_bytes(value.as_bytes()) {
                builder = builder.header(axum_name, axum_value);
            }
        }
    }

    builder.body(body).map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to build upstream response: {error}"),
        )
    })
}

pub(crate) fn extract_session_id(state: &AppState, headers: &HeaderMap) -> Option<String> {
    if !state.sessions.is_enabled() {
        return None;
    }

    let session_id = if let Some(header_name) = state.config.session.id_from.strip_prefix("header:")
    {
        headers
            .get(header_name)
            .and_then(|value| value.to_str().ok())
            .map(ToString::to_string)
    } else if state.config.session.id_from == "connection" {
        Some(Uuid::new_v4().to_string())
    } else {
        None
    };

    if let Some(ref session_id) = session_id {
        state.sessions.get_or_create(session_id);
    }

    session_id
}

fn pseudonymize_text(
    text: &str,
    detector: &cloakpipe_core::detector::Detector,
    vault: &mut Vault,
    strategy: MaskingStrategy,
    state: &AppState,
    session_id: Option<&str>,
) -> anyhow::Result<Option<TextMutation>> {
    if json_filter::should_skip_text_rewrite(text) {
        return Ok(None);
    }

    let entities = detect_sorted_entities(detector, text)?;
    if entities.is_empty() {
        return Ok(None);
    }

    let result = Replacer::pseudonymize_with_strategy(text, &entities, vault, strategy)?;
    let tokens = entities
        .iter()
        .map(|entity| token_for_strategy(vault, &entity.original, &entity.category, strategy))
        .collect::<Vec<_>>();

    if let Some(session_id) = session_id {
        state.sessions.with_session(session_id, |ctx| {
            ctx.record_entities(&entities, &tokens);
        });
    }

    Ok(Some(TextMutation {
        text: result.text,
        entities: entities.clone(),
        categories: entity_categories(&entities),
        tokens,
    }))
}

fn detect_sorted_entities(
    detector: &cloakpipe_core::detector::Detector,
    text: &str,
) -> anyhow::Result<Vec<DetectedEntity>> {
    let mut entities = detector.detect(text)?;
    entities.sort_by_key(|entity| entity.start);
    Ok(entities)
}

fn token_for_strategy(
    vault: &mut Vault,
    original: &str,
    category: &EntityCategory,
    strategy: MaskingStrategy,
) -> PseudoToken {
    match strategy {
        MaskingStrategy::Similar => vault.get_or_create_similar(original, category),
        MaskingStrategy::Token => vault.get_or_create(original, category),
        MaskingStrategy::FormatPreserving => vault.get_or_create_fp(original, category),
    }
}

fn entity_categories(entities: &[DetectedEntity]) -> Vec<String> {
    let mut categories: Vec<String> = entities
        .iter()
        .map(|entity| match &entity.category {
            EntityCategory::Custom(name) => name.clone(),
            other => format!("{other:?}"),
        })
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    categories.sort();
    categories
}

fn sorted_categories(categories: HashSet<String>) -> Vec<String> {
    let mut categories: Vec<String> = categories.into_iter().collect();
    categories.sort();
    categories
}

fn header_value(headers: &HeaderMap, name: &str) -> String {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase()
}

fn header_content_type(headers: &reqwest::header::HeaderMap) -> String {
    headers
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase()
}

fn should_stream_response(headers: &reqwest::header::HeaderMap, content_type: &str) -> bool {
    content_type.contains("text/event-stream")
        || headers
            .get("transfer-encoding")
            .and_then(|value| value.to_str().ok())
            .map(|value| value.to_ascii_lowercase().contains("chunked"))
            .unwrap_or(false)
}

fn has_non_identity_encoding(headers: &HeaderMap) -> bool {
    let encoding = header_value(headers, "content-encoding");
    !encoding.is_empty() && encoding != "identity"
}

fn has_non_identity_response_encoding(headers: &reqwest::header::HeaderMap) -> bool {
    headers
        .get("content-encoding")
        .and_then(|value| value.to_str().ok())
        .map(|value| {
            let value = value.to_ascii_lowercase();
            !value.is_empty() && value != "identity"
        })
        .unwrap_or(false)
}

fn should_drop_request_header(name: &str) -> bool {
    matches!(
        name,
        "host"
            | "connection"
            | "keep-alive"
            | "proxy-connection"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
            | "content-length"
            | "accept-encoding"
    )
}

fn is_auth_header(name: &str) -> bool {
    matches!(name, "authorization" | "x-api-key" | "api-key")
}
