//! Audit query / summary / export endpoints.
//!
//! Both SQLite and JSONL backends are supported. When auditing is disabled the
//! endpoints return a clear `backend: "disabled"` empty state so the UI can
//! render an unsupported/empty message instead of failing.

use super::AdminError;
use crate::state::AppState;
use axum::{
    extract::{Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use cloakpipe_audit::{AuditBackend, AuditQuery};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Deserialize)]
pub struct AuditQueryParams {
    pub event: Option<String>,
    pub surface: Option<String>,
    pub session_id: Option<String>,
    pub since: Option<String>,
    pub until: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

fn backend_label(backend: AuditBackend) -> &'static str {
    match backend {
        AuditBackend::Disabled => "disabled",
        AuditBackend::Jsonl => "jsonl",
        AuditBackend::Sqlite => "sqlite",
    }
}

impl AuditQueryParams {
    fn into_query(self) -> AuditQuery {
        AuditQuery {
            event: self.event,
            surface: self.surface,
            session_id: self.session_id,
            since: self.since,
            until: self.until,
            limit: self.limit.unwrap_or(100).min(1000),
            offset: self.offset.unwrap_or(0),
        }
    }
}

#[derive(Serialize)]
pub struct AuditEventsResponse {
    pub backend: &'static str,
    pub supported: bool,
    pub total_matched: usize,
    pub limit: usize,
    pub offset: usize,
    pub events: Vec<cloakpipe_audit::AuditEntry>,
}

/// `GET /admin/api/audit/events` — query/filter audit events.
pub async fn query_events(
    State(state): State<Arc<AppState>>,
    Query(params): Query<AuditQueryParams>,
) -> Result<Json<AuditEventsResponse>, AdminError> {
    let query = params.into_query();
    let limit = query.limit;
    let offset = query.offset;
    let result = state
        .audit
        .query(&query)
        .map_err(|e| AdminError::internal(format!("Audit query failed: {e}")))?;
    Ok(Json(AuditEventsResponse {
        backend: backend_label(result.backend),
        supported: result.backend != AuditBackend::Disabled,
        total_matched: result.total_matched,
        limit,
        offset,
        events: result.entries,
    }))
}

#[derive(Serialize)]
pub struct AuditSummaryResponse {
    pub backend: &'static str,
    pub supported: bool,
    pub total: usize,
    pub counts: Vec<EventCount>,
}

#[derive(Serialize)]
pub struct EventCount {
    pub event: String,
    pub count: usize,
}

/// `GET /admin/api/audit/summary` — aggregate event counts.
pub async fn summary(
    State(state): State<Arc<AppState>>,
) -> Result<Json<AuditSummaryResponse>, AdminError> {
    let backend = state.audit.backend();
    let counts = state
        .audit
        .summary()
        .map_err(|e| AdminError::internal(format!("Audit summary failed: {e}")))?;
    let total = counts.iter().map(|(_, c)| c).sum();
    Ok(Json(AuditSummaryResponse {
        backend: backend_label(backend),
        supported: backend != AuditBackend::Disabled,
        total,
        counts: counts
            .into_iter()
            .map(|(event, count)| EventCount { event, count })
            .collect(),
    }))
}

/// `GET /admin/api/audit/export` — export filtered events as CSV.
pub async fn export(
    State(state): State<Arc<AppState>>,
    Query(params): Query<AuditQueryParams>,
) -> Result<Response, AdminError> {
    let mut query = params.into_query();
    query.limit = 0; // export everything that matches
    query.offset = 0;
    let result = state
        .audit
        .query(&query)
        .map_err(|e| AdminError::internal(format!("Audit export failed: {e}")))?;

    let mut csv = String::from(
        "id,timestamp,event,surface,request_id,user_id,session_id,entities_detected,entities_replaced,tokens_rehydrated,categories,error\n",
    );
    for e in &result.entries {
        let categories = e
            .categories
            .as_ref()
            .map(|c| c.join("|"))
            .unwrap_or_default();
        csv.push_str(&format!(
            "{},{},{},{},{},{},{},{},{},{},{},{}\n",
            csv_field(&e.id),
            csv_field(&e.timestamp),
            csv_field(e.event.name()),
            csv_field(&e.surface),
            csv_field(e.request_id.as_deref().unwrap_or("")),
            csv_field(e.user_id.as_deref().unwrap_or("")),
            csv_field(e.session_id.as_deref().unwrap_or("")),
            e.entities_detected
                .map(|n| n.to_string())
                .unwrap_or_default(),
            e.entities_replaced
                .map(|n| n.to_string())
                .unwrap_or_default(),
            e.tokens_rehydrated
                .map(|n| n.to_string())
                .unwrap_or_default(),
            csv_field(&categories),
            csv_field(e.error.as_deref().unwrap_or("")),
        ));
    }

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "text/csv; charset=utf-8"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=\"cloakpipe-audit.csv\"",
            ),
        ],
        csv,
    )
        .into_response())
}

/// Escape a CSV field (RFC 4180): wrap in quotes if it contains a comma, quote,
/// or newline, and double any embedded quotes.
fn csv_field(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}
