//! Self-hosted admin API.
//!
//! These routes are mounted **only** in `server` mode (see
//! [`crate::server::build_router`]) under the `/admin/api` namespace. They power
//! the local CloakPipe admin UI: profiles, policies, categories/rules, audit
//! logs and vault inspection.
//!
//! Security note: there is no built-in authentication. The admin API is intended
//! for trusted/local operators or to be fronted by an external auth proxy. Some
//! routes expose sensitive data (raw vault mappings) and are redacted by default.

mod audit;
mod categories;
mod policies;
mod profiles;
mod system;
mod vault;

use crate::state::AppState;
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
};
use cloakpipe_core::{config::DetectionConfig, detector::Detector};
use serde::Serialize;
use std::sync::Arc;

/// Build the `/admin/api` sub-router.
pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        // System / runtime status
        .route("/system", get(system::get_system))
        // Profiles
        .route("/profiles", get(profiles::list_profiles))
        .route("/profiles/:name", get(profiles::get_profile))
        .route("/profiles/:name/activate", post(profiles::activate_profile))
        // Policies (disk-backed local configs / presets)
        .route("/policies", get(policies::list_policies))
        .route(
            "/policies/:name",
            get(policies::get_policy)
                .put(policies::put_policy)
                .delete(policies::delete_policy),
        )
        .route("/policies/:name/activate", post(policies::activate_policy))
        .route("/policy/validate", post(policies::validate_policy))
        // Categories & custom rules
        .route("/categories", get(categories::list_categories))
        .route("/categories/rules", post(categories::create_rule))
        .route(
            "/categories/rules/:name",
            put(categories::update_rule).delete(categories::delete_rule),
        )
        // Audit
        .route("/audit/events", get(audit::query_events))
        .route("/audit/summary", get(audit::summary))
        .route("/audit/export", get(audit::export))
        // Vault / secrets
        .route("/vault/stats", get(vault::stats))
        .route("/vault/mappings", get(vault::mappings))
        // Runtime diagnostics
        .route("/sessions", get(system::list_sessions))
}

/// Structured admin API error returned as JSON.
#[derive(Debug)]
pub struct AdminError {
    status: StatusCode,
    code: &'static str,
    message: String,
}

impl AdminError {
    pub fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
        }
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "bad_request", message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, "not_found", message)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", message)
    }

    pub fn unprocessable(message: impl Into<String>) -> Self {
        Self::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "validation_failed",
            message,
        )
    }
}

#[derive(Serialize)]
struct AdminErrorBody {
    error: AdminErrorDetail,
}

#[derive(Serialize)]
struct AdminErrorDetail {
    code: &'static str,
    message: String,
}

impl IntoResponse for AdminError {
    fn into_response(self) -> Response {
        let body = Json(AdminErrorBody {
            error: AdminErrorDetail {
                code: self.code,
                message: self.message,
            },
        });
        (self.status, body).into_response()
    }
}

/// Rebuild the live detector from a new detection config and atomically swap it
/// in, mirroring the behaviour of the direct `/v1/configure` endpoint.
///
/// If the configuration enables an NER backend whose model cannot be loaded
/// (for example the local ONNX model has not been downloaded yet), the NER
/// layer is disabled and a warning is returned instead of failing the whole
/// activation. This mirrors the graceful-degradation behaviour already used for
/// the GLiNER-PII sidecar backend and keeps the admin UI usable on machines
/// where the optional model assets are not present.
///
/// Returns any non-fatal warnings that the caller should surface to the operator.
pub(crate) async fn apply_detection_config(
    state: &AppState,
    mut next_config: DetectionConfig,
    next_profile: Option<String>,
) -> Result<Vec<String>, AdminError> {
    let mut warnings = Vec::new();

    let new_detector = match Detector::from_config(&next_config) {
        Ok(detector) => detector,
        Err(err) if next_config.ner.enabled => {
            warnings.push(format!(
                "NER backend could not be initialised ({err}); NER detection \
                 has been disabled. Install the model (e.g. `cloakpipe ner download`) \
                 and re-activate to enable it."
            ));
            next_config.ner.enabled = false;
            Detector::from_config(&next_config)
                .map_err(|e| AdminError::internal(format!("Failed to rebuild detector: {e}")))?
        }
        Err(err) => {
            return Err(AdminError::internal(format!(
                "Failed to rebuild detector: {err}"
            )));
        }
    };

    {
        let mut config = state.detection_config.write().await;
        *config = next_config;
    }
    {
        let mut detector = state.detector.write().await;
        *detector = new_detector;
    }
    {
        let mut active_profile = state.active_profile.write().await;
        *active_profile = next_profile;
    }
    Ok(warnings)
}
