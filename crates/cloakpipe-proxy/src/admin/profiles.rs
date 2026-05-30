//! Profile endpoints — built-in industry presets.

use super::{apply_detection_config, AdminError};
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    Json,
};
use cloakpipe_core::config::DetectionConfig;
use cloakpipe_core::profiles::IndustryProfile;
use serde::Serialize;
use std::sync::Arc;

#[derive(Serialize)]
pub struct ProfileSummary {
    pub name: String,
    pub kind: &'static str,
    pub active: bool,
    pub description: String,
}

#[derive(Serialize)]
pub struct ProfileDetail {
    pub name: String,
    pub kind: &'static str,
    pub active: bool,
    pub detection: DetectionConfig,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

fn describe(profile: IndustryProfile) -> String {
    match profile {
        IndustryProfile::General => {
            "Balanced defaults for general PII (secrets, financial, contact, NER).".into()
        }
        IndustryProfile::Legal => {
            "Legal documents: case/docket/bar numbers, SSNs, court name preservation.".into()
        }
        IndustryProfile::Healthcare => {
            "Healthcare/HIPAA: MRN, NPI, DEA, ICD codes, regulator name preservation.".into()
        }
        IndustryProfile::Fintech => {
            "Financial services: SWIFT/ISIN/IBAN/routing numbers, exchange name preservation."
                .into()
        }
    }
}

/// `GET /admin/api/profiles` — list built-in industry profiles.
pub async fn list_profiles(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<ProfileSummary>>, AdminError> {
    let active = state.active_profile.read().await.clone();
    let profiles = IndustryProfile::all()
        .iter()
        .map(|p| ProfileSummary {
            name: p.name().to_string(),
            kind: "builtin",
            active: active.as_deref() == Some(p.name()),
            description: describe(*p),
        })
        .collect();
    Ok(Json(profiles))
}

/// `GET /admin/api/profiles/{name}` — detail of a built-in profile.
pub async fn get_profile(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<ProfileDetail>, AdminError> {
    let profile = IndustryProfile::from_name(&name)
        .ok_or_else(|| AdminError::not_found(format!("Unknown profile '{name}'")))?;
    let active = state.active_profile.read().await.clone();
    Ok(Json(ProfileDetail {
        name: profile.name().to_string(),
        kind: "builtin",
        active: active.as_deref() == Some(profile.name()),
        detection: profile.detection_config(),
        warnings: Vec::new(),
    }))
}

/// `POST /admin/api/profiles/{name}/activate` — switch the active profile.
pub async fn activate_profile(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<ProfileDetail>, AdminError> {
    let profile = IndustryProfile::from_name(&name).ok_or_else(|| {
        AdminError::bad_request(format!(
            "Unknown profile '{name}'. Use: general, legal, healthcare, fintech"
        ))
    })?;

    let detection = profile.detection_config();
    let warnings =
        apply_detection_config(&state, detection.clone(), Some(profile.name().to_string())).await?;

    let _ = state.audit.log_metadata(
        cloakpipe_audit::AuditContext::new("admin", "profile.activate")
            .with_session(Some(cloakpipe_core::session::GLOBAL_SESSION_ID)),
        cloakpipe_audit::AuditEvent::Configure,
        None,
        Vec::new(),
    );

    Ok(Json(ProfileDetail {
        name: profile.name().to_string(),
        kind: "builtin",
        active: true,
        detection,
        warnings,
    }))
}
