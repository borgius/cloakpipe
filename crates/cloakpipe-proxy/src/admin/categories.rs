//! Category & custom-rule endpoints.
//!
//! "Categories" are the existing detector model: the built-in detection
//! families (secrets, financial, dates, …) plus user-defined custom regex
//! patterns. Custom-rule changes update the live detection config and rebuild
//! the detector in-process (they are not persisted to disk — save a policy to
//! persist).

use super::{apply_detection_config, AdminError};
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    Json,
};
use cloakpipe_core::config::CustomPattern;
use serde::Serialize;
use std::sync::Arc;

#[derive(Serialize)]
pub struct DetectionFamily {
    pub key: &'static str,
    pub label: &'static str,
    pub enabled: bool,
}

#[derive(Serialize)]
pub struct CategoriesResponse {
    pub families: Vec<DetectionFamily>,
    pub ner_entity_types: Vec<String>,
    pub custom_rules: Vec<CustomPattern>,
}

/// `GET /admin/api/categories` — detection families + custom regex rules.
pub async fn list_categories(
    State(state): State<Arc<AppState>>,
) -> Result<Json<CategoriesResponse>, AdminError> {
    let detection = state.detection_config.read().await.clone();
    let families = vec![
        DetectionFamily {
            key: "secrets",
            label: "Secrets (API keys, tokens)",
            enabled: detection.secrets,
        },
        DetectionFamily {
            key: "financial",
            label: "Financial amounts",
            enabled: detection.financial,
        },
        DetectionFamily {
            key: "dates",
            label: "Dates",
            enabled: detection.dates,
        },
        DetectionFamily {
            key: "emails",
            label: "Email addresses",
            enabled: detection.emails,
        },
        DetectionFamily {
            key: "phone_numbers",
            label: "Phone numbers",
            enabled: detection.phone_numbers,
        },
        DetectionFamily {
            key: "ip_addresses",
            label: "IP addresses",
            enabled: detection.ip_addresses,
        },
        DetectionFamily {
            key: "urls_internal",
            label: "Internal URLs",
            enabled: detection.urls_internal,
        },
    ];
    Ok(Json(CategoriesResponse {
        families,
        ner_entity_types: detection.ner.entity_types.clone(),
        custom_rules: detection.custom.patterns.clone(),
    }))
}

fn validate_rule(rule: &CustomPattern) -> Result<(), AdminError> {
    if rule.name.trim().is_empty() {
        return Err(AdminError::bad_request("Rule name cannot be empty"));
    }
    if rule.category.trim().is_empty() {
        return Err(AdminError::bad_request("Rule category cannot be empty"));
    }
    regex::Regex::new(&rule.regex)
        .map_err(|e| AdminError::unprocessable(format!("Invalid regex: {e}")))?;
    Ok(())
}

async fn rebuild_with_patterns(
    state: &AppState,
    patterns: Vec<CustomPattern>,
) -> Result<(), AdminError> {
    let mut next = state.detection_config.read().await.clone();
    next.custom.patterns = patterns;
    let profile = state.active_profile.read().await.clone();
    apply_detection_config(state, next, profile).await?;
    Ok(())
}

/// `POST /admin/api/categories/rules` — add a custom regex rule.
pub async fn create_rule(
    State(state): State<Arc<AppState>>,
    Json(rule): Json<CustomPattern>,
) -> Result<Json<Vec<CustomPattern>>, AdminError> {
    validate_rule(&rule)?;
    let mut patterns = state.detection_config.read().await.custom.patterns.clone();
    if patterns.iter().any(|p| p.name == rule.name) {
        return Err(AdminError::bad_request(format!(
            "A rule named '{}' already exists",
            rule.name
        )));
    }
    patterns.push(rule);
    rebuild_with_patterns(&state, patterns.clone()).await?;
    Ok(Json(patterns))
}

/// `PUT /admin/api/categories/rules/{name}` — update an existing custom rule.
pub async fn update_rule(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(rule): Json<CustomPattern>,
) -> Result<Json<Vec<CustomPattern>>, AdminError> {
    validate_rule(&rule)?;
    let mut patterns = state.detection_config.read().await.custom.patterns.clone();
    let idx = patterns
        .iter()
        .position(|p| p.name == name)
        .ok_or_else(|| AdminError::not_found(format!("Rule '{name}' not found")))?;
    // Prevent rename collisions with a different existing rule.
    if rule.name != name && patterns.iter().any(|p| p.name == rule.name) {
        return Err(AdminError::bad_request(format!(
            "A rule named '{}' already exists",
            rule.name
        )));
    }
    patterns[idx] = rule;
    rebuild_with_patterns(&state, patterns.clone()).await?;
    Ok(Json(patterns))
}

/// `DELETE /admin/api/categories/rules/{name}` — remove a custom rule.
pub async fn delete_rule(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<Vec<CustomPattern>>, AdminError> {
    let mut patterns = state.detection_config.read().await.custom.patterns.clone();
    let before = patterns.len();
    patterns.retain(|p| p.name != name);
    if patterns.len() == before {
        return Err(AdminError::not_found(format!("Rule '{name}' not found")));
    }
    rebuild_with_patterns(&state, patterns.clone()).await?;
    Ok(Json(patterns))
}
