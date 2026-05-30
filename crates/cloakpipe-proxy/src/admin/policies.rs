//! Policy endpoints — disk-backed local configs compatible with the CLI.
//!
//! A "policy" is a full `cloakpipe.toml`-compatible config file stored in the
//! configured policies directory. These endpoints let an operator list, read,
//! validate, create/update, delete and activate them.

use super::{apply_detection_config, AdminError};
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    Json,
};
use cloakpipe_core::config::CloakPipeConfig;
use cloakpipe_core::detector::Detector;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Serialize)]
pub struct PolicySummary {
    pub name: String,
    pub path: String,
    pub active: bool,
    pub size_bytes: u64,
}

#[derive(Serialize)]
pub struct PolicyDetail {
    pub name: String,
    pub path: String,
    pub active: bool,
    pub content: String,
    pub valid: bool,
    pub validation: ValidationReport,
}

#[derive(Serialize, Default)]
pub struct ValidationReport {
    pub valid: bool,
    pub errors: Vec<String>,
    pub profile: Option<String>,
    pub mode: Option<String>,
}

#[derive(Deserialize)]
pub struct PolicyContent {
    pub content: String,
}

/// Resolve the policies directory or return a clear unsupported-state error.
fn policies_dir(state: &AppState) -> Result<PathBuf, AdminError> {
    let admin = state
        .admin
        .read()
        .map_err(|_| AdminError::internal("admin context lock poisoned"))?;
    admin.policies_dir.clone().ok_or_else(|| {
        AdminError::new(
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "policies_unsupported",
            "No policies directory is configured for this server instance.",
        )
    })
}

fn active_config_path(state: &AppState) -> Option<PathBuf> {
    state.admin.read().ok().and_then(|a| a.config_path.clone())
}

/// Convert a requested policy name into a safe `<dir>/<name>.toml` path,
/// rejecting any path traversal attempts.
fn policy_path(dir: &std::path::Path, name: &str) -> Result<PathBuf, AdminError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(AdminError::bad_request("Policy name cannot be empty"));
    }
    let stem = trimmed.strip_suffix(".toml").unwrap_or(trimmed);
    if stem.is_empty()
        || stem.contains('/')
        || stem.contains('\\')
        || stem.contains("..")
        || stem.contains('\0')
    {
        return Err(AdminError::bad_request(format!(
            "Invalid policy name '{name}'"
        )));
    }
    Ok(dir.join(format!("{stem}.toml")))
}

/// Validate raw TOML content as a CloakPipe config and (if it parses) compile
/// its detector to surface regex errors.
fn validate_content(content: &str) -> ValidationReport {
    let mut report = ValidationReport::default();
    match toml::from_str::<CloakPipeConfig>(content) {
        Ok(config) => {
            report.profile = config.profile.clone();
            report.mode = serde_json::to_value(config.proxy.mode)
                .ok()
                .and_then(|v| v.as_str().map(str::to_string));
            if let Err(e) = Detector::from_config(&config.detection) {
                report.errors.push(format!("Detector build failed: {e}"));
            }
            report.valid = report.errors.is_empty();
        }
        Err(e) => {
            report.errors.push(format!("TOML parse error: {e}"));
            report.valid = false;
        }
    }
    report
}

/// `GET /admin/api/policies` — list policy files on disk.
pub async fn list_policies(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<PolicySummary>>, AdminError> {
    let dir = policies_dir(&state)?;
    let active = active_config_path(&state);

    let mut policies = Vec::new();
    if let Ok(read_dir) = std::fs::read_dir(&dir) {
        for entry in read_dir.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("toml") {
                continue;
            }
            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_string();
            let size_bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);
            let active = active
                .as_ref()
                .map(|a| same_file(a, &path))
                .unwrap_or(false);
            policies.push(PolicySummary {
                name,
                path: path.display().to_string(),
                active,
                size_bytes,
            });
        }
    }
    policies.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(Json(policies))
}

/// `GET /admin/api/policies/{name}` — fetch one policy with validation report.
pub async fn get_policy(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<PolicyDetail>, AdminError> {
    let dir = policies_dir(&state)?;
    let path = policy_path(&dir, &name)?;
    let content = std::fs::read_to_string(&path)
        .map_err(|_| AdminError::not_found(format!("Policy '{name}' not found")))?;
    let validation = validate_content(&content);
    let active = active_config_path(&state)
        .map(|a| same_file(&a, &path))
        .unwrap_or(false);
    Ok(Json(PolicyDetail {
        name: path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(&name)
            .to_string(),
        path: path.display().to_string(),
        active,
        valid: validation.valid,
        validation,
        content,
    }))
}

/// `POST /admin/api/policy/validate` — validate arbitrary policy content.
pub async fn validate_policy(
    Json(payload): Json<PolicyContent>,
) -> Result<Json<ValidationReport>, AdminError> {
    Ok(Json(validate_content(&payload.content)))
}

/// `PUT /admin/api/policies/{name}` — create or update a policy file.
pub async fn put_policy(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(payload): Json<PolicyContent>,
) -> Result<Json<PolicyDetail>, AdminError> {
    let dir = policies_dir(&state)?;
    let path = policy_path(&dir, &name)?;

    let validation = validate_content(&payload.content);
    if !validation.valid {
        return Err(AdminError::unprocessable(format!(
            "Policy is invalid: {}",
            validation.errors.join("; ")
        )));
    }

    std::fs::create_dir_all(&dir)
        .map_err(|e| AdminError::internal(format!("Cannot create policies dir: {e}")))?;
    std::fs::write(&path, payload.content.as_bytes())
        .map_err(|e| AdminError::internal(format!("Cannot write policy: {e}")))?;

    let active = active_config_path(&state)
        .map(|a| same_file(&a, &path))
        .unwrap_or(false);
    Ok(Json(PolicyDetail {
        name: path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(&name)
            .to_string(),
        path: path.display().to_string(),
        active,
        valid: validation.valid,
        validation,
        content: payload.content,
    }))
}

/// `DELETE /admin/api/policies/{name}` — remove a policy file.
pub async fn delete_policy(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, AdminError> {
    let dir = policies_dir(&state)?;
    let path = policy_path(&dir, &name)?;

    if let Some(active) = active_config_path(&state) {
        if same_file(&active, &path) {
            return Err(AdminError::bad_request(
                "Cannot delete the currently active policy. Activate another policy first.",
            ));
        }
    }

    if !path.exists() {
        return Err(AdminError::not_found(format!("Policy '{name}' not found")));
    }
    std::fs::remove_file(&path)
        .map_err(|e| AdminError::internal(format!("Cannot delete policy: {e}")))?;
    Ok(Json(serde_json::json!({ "deleted": true, "name": name })))
}

/// `POST /admin/api/policies/{name}/activate` — load and apply a policy.
///
/// Applies the policy's detection settings and active profile to the running
/// detector. Listener/upstream/masking changes require a restart and are
/// reported back to the caller.
pub async fn activate_policy(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, AdminError> {
    let dir = policies_dir(&state)?;
    let path = policy_path(&dir, &name)?;
    let content = std::fs::read_to_string(&path)
        .map_err(|_| AdminError::not_found(format!("Policy '{name}' not found")))?;

    let config: CloakPipeConfig = toml::from_str(&content)
        .map_err(|e| AdminError::unprocessable(format!("Policy parse error: {e}")))?;

    let warnings =
        apply_detection_config(&state, config.detection.clone(), config.profile.clone()).await?;

    if let Ok(mut admin) = state.admin.write() {
        admin.config_path = Some(path.clone());
    }

    let restart_required = config.proxy.listen != state.config.proxy.listen
        || config.proxy.upstream != state.config.proxy.upstream
        || config.proxy.masking_strategy != state.config.proxy.masking_strategy;

    let _ = state.audit.log_metadata(
        cloakpipe_audit::AuditContext::new("admin", "policy.activate")
            .with_session(Some(cloakpipe_core::session::GLOBAL_SESSION_ID)),
        cloakpipe_audit::AuditEvent::Configure,
        None,
        Vec::new(),
    );

    Ok(Json(serde_json::json!({
        "activated": true,
        "name": name,
        "path": path.display().to_string(),
        "active_profile": config.profile,
        "restart_required": restart_required,
        "warnings": warnings,
        "note": if restart_required {
            "Detection settings applied live. Listener/upstream/masking changes take effect after restart."
        } else {
            "Detection settings applied live."
        },
    })))
}

/// Compare two paths by canonical form, falling back to literal comparison.
fn same_file(a: &std::path::Path, b: &std::path::Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}
