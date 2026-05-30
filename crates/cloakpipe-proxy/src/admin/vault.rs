//! Vault / secrets endpoints.
//!
//! Raw mappings are sensitive: the original values are **redacted by default**.
//! Revealing them requires an explicit `reveal=true` query parameter and every
//! reveal is written to the audit log.

use super::AdminError;
use crate::state::AppState;
use axum::{
    extract::{Query, State},
    Json,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Serialize)]
pub struct VaultStatsResponse {
    pub backend: String,
    pub path: Option<String>,
    pub persistent: bool,
    pub encryption: String,
    pub total_mappings: usize,
    pub categories: HashMap<String, u32>,
}

/// `GET /admin/api/vault/stats` — enhanced vault statistics.
pub async fn stats(State(state): State<Arc<AppState>>) -> Json<VaultStatsResponse> {
    let vault = state.vault.lock().await;
    let s = vault.stats();
    Json(VaultStatsResponse {
        backend: state.config.vault.backend.clone(),
        path: vault.path().map(str::to_string),
        persistent: vault.is_persistent(),
        encryption: state.config.vault.encryption.clone(),
        total_mappings: s.total_mappings,
        categories: s.categories,
    })
}

#[derive(Debug, Deserialize)]
pub struct MappingParams {
    pub search: Option<String>,
    pub category: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    #[serde(default)]
    pub reveal: bool,
}

#[derive(Serialize)]
pub struct MappingEntry {
    pub token: String,
    pub category: String,
    pub original: String,
    pub redacted: bool,
}

#[derive(Serialize)]
pub struct MappingsResponse {
    /// True if original values are redacted in this response.
    pub redacted: bool,
    pub total_matched: usize,
    pub limit: usize,
    pub offset: usize,
    pub mappings: Vec<MappingEntry>,
    /// Always present when `redacted` is false to remind operators of the risk.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

/// Redact a sensitive original value, keeping only a short hint of its shape.
fn redact(original: &str) -> String {
    let len = original.chars().count();
    if len <= 2 {
        "••".to_string()
    } else {
        let first = original.chars().next().unwrap_or('•');
        let last = original.chars().last().unwrap_or('•');
        format!("{first}••••{last} ({len} chars)")
    }
}

/// `GET /admin/api/vault/mappings` — inspect/search vault mappings.
pub async fn mappings(
    State(state): State<Arc<AppState>>,
    Query(params): Query<MappingParams>,
) -> Result<Json<MappingsResponse>, AdminError> {
    let limit = params.limit.unwrap_or(100).min(1000);
    let offset = params.offset.unwrap_or(0);
    let reveal = params.reveal;
    let search = params.search.unwrap_or_default().to_lowercase();
    let category_filter = params.category.unwrap_or_default().to_lowercase();

    let all = {
        let vault = state.vault.lock().await;
        vault.mappings()
    };

    let filtered: Vec<_> = all
        .into_iter()
        .filter(|m| {
            if !category_filter.is_empty() && m.category.to_lowercase() != category_filter {
                return false;
            }
            if !search.is_empty() {
                let hay_token = m.token.to_lowercase();
                let hay_cat = m.category.to_lowercase();
                // Searching on the original value only works when revealing,
                // otherwise we'd leak whether a value is present.
                let hay_orig = if reveal {
                    m.original.to_lowercase()
                } else {
                    String::new()
                };
                if !hay_token.contains(&search)
                    && !hay_cat.contains(&search)
                    && !hay_orig.contains(&search)
                {
                    return false;
                }
            }
            true
        })
        .collect();

    let total_matched = filtered.len();
    let page: Vec<MappingEntry> = filtered
        .into_iter()
        .skip(offset)
        .take(limit)
        .map(|m| MappingEntry {
            token: m.token,
            category: m.category,
            original: if reveal {
                m.original
            } else {
                redact(&m.original)
            },
            redacted: !reveal,
        })
        .collect();

    if reveal {
        let _ = state.audit.log_metadata(
            cloakpipe_audit::AuditContext::new("admin", "vault.mappings.reveal")
                .with_session(Some(cloakpipe_core::session::GLOBAL_SESSION_ID)),
            cloakpipe_audit::AuditEvent::VaultStats,
            Some(page.len()),
            Vec::new(),
        );
    }

    Ok(Json(MappingsResponse {
        redacted: !reveal,
        total_matched,
        limit,
        offset,
        mappings: page,
        warning: if reveal {
            Some(
                "Raw sensitive values revealed — this access was recorded in the audit log.".into(),
            )
        } else {
            None
        },
    }))
}
