//! Profile endpoints — built-in industry presets plus persisted, user-defined
//! custom profiles.
//!
//! Built-in profiles (general, legal, healthcare, fintech) ship with CloakPipe
//! and are read-only. Custom profiles are stored as JSON files in the configured
//! profiles directory and can be created, updated, deleted and activated through
//! the admin API. A custom profile is simply a named [`DetectionConfig`] with an
//! optional description.

use super::{apply_detection_config, AdminError};
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    Json,
};
use cloakpipe_core::config::DetectionConfig;
use cloakpipe_core::detector::Detector;
use cloakpipe_core::profiles::IndustryProfile;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
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
    pub description: String,
    pub detection: DetectionConfig,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

/// Request body for creating/updating a custom profile.
#[derive(Deserialize)]
pub struct CustomProfileInput {
    pub name: Option<String>,
    #[serde(default)]
    pub description: String,
    pub detection: DetectionConfig,
}

/// On-disk representation of a persisted custom profile.
#[derive(Serialize, Deserialize)]
struct StoredProfile {
    name: String,
    #[serde(default)]
    description: String,
    detection: DetectionConfig,
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

/// Resolve the profiles directory or return a clear unsupported-state error.
fn profiles_dir(state: &AppState) -> Result<PathBuf, AdminError> {
    let admin = state
        .admin
        .read()
        .map_err(|_| AdminError::internal("admin context lock poisoned"))?;
    admin.profiles_dir.clone().ok_or_else(|| {
        AdminError::new(
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "profiles_unsupported",
            "No custom profiles directory is configured for this server instance.",
        )
    })
}

/// Convert a requested custom-profile name into a safe `<dir>/<name>.json` path,
/// rejecting path traversal and names that collide with a built-in profile.
fn custom_profile_path(dir: &std::path::Path, name: &str) -> Result<PathBuf, AdminError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(AdminError::bad_request("Profile name cannot be empty"));
    }
    let stem = trimmed.strip_suffix(".json").unwrap_or(trimmed);
    if stem.is_empty()
        || stem.contains('/')
        || stem.contains('\\')
        || stem.contains("..")
        || stem.contains('\0')
    {
        return Err(AdminError::bad_request(format!(
            "Invalid profile name '{name}'"
        )));
    }
    if IndustryProfile::from_name(stem).is_some() {
        return Err(AdminError::bad_request(format!(
            "'{stem}' is a reserved built-in profile name"
        )));
    }
    Ok(dir.join(format!("{stem}.json")))
}

/// Read a stored custom profile from disk, if it exists and parses.
fn read_custom(dir: &std::path::Path, name: &str) -> Result<Option<StoredProfile>, AdminError> {
    let path = custom_profile_path(dir, name)?;
    match std::fs::read_to_string(&path) {
        Ok(content) => {
            let stored: StoredProfile = serde_json::from_str(&content).map_err(|e| {
                AdminError::internal(format!("Custom profile '{name}' is corrupt: {e}"))
            })?;
            Ok(Some(stored))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(AdminError::internal(format!(
            "Cannot read custom profile '{name}': {e}"
        ))),
    }
}

/// List all custom profiles stored on disk (sorted by name).
fn list_custom(dir: &std::path::Path) -> Vec<StoredProfile> {
    let mut profiles = Vec::new();
    if let Ok(read_dir) = std::fs::read_dir(dir) {
        for entry in read_dir.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(mut stored) = serde_json::from_str::<StoredProfile>(&content) {
                    // Trust the filename as the canonical name to avoid mismatches.
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        stored.name = stem.to_string();
                    }
                    profiles.push(stored);
                }
            }
        }
    }
    profiles.sort_by(|a, b| a.name.cmp(&b.name));
    profiles
}

/// Validate that a detection config compiles into a working detector.
fn validate_detection(detection: &DetectionConfig) -> Result<(), AdminError> {
    Detector::from_config(detection)
        .map(|_| ())
        .map_err(|e| AdminError::unprocessable(format!("Invalid detection config: {e}")))
}

/// `GET /admin/api/profiles` — list built-in and custom profiles.
pub async fn list_profiles(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<ProfileSummary>>, AdminError> {
    let active = state.active_profile.read().await.clone();
    let mut profiles: Vec<ProfileSummary> = IndustryProfile::all()
        .iter()
        .map(|p| ProfileSummary {
            name: p.name().to_string(),
            kind: "builtin",
            active: active.as_deref() == Some(p.name()),
            description: describe(*p),
        })
        .collect();

    // Custom profiles are optional: only listed when a profiles dir is configured.
    if let Ok(dir) = profiles_dir(&state) {
        for stored in list_custom(&dir) {
            profiles.push(ProfileSummary {
                active: active.as_deref() == Some(stored.name.as_str()),
                name: stored.name,
                kind: "custom",
                description: stored.description,
            });
        }
    }

    Ok(Json(profiles))
}

/// `GET /admin/api/profiles/{name}` — detail of a built-in or custom profile.
pub async fn get_profile(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<ProfileDetail>, AdminError> {
    let active = state.active_profile.read().await.clone();

    if let Some(profile) = IndustryProfile::from_name(&name) {
        return Ok(Json(ProfileDetail {
            name: profile.name().to_string(),
            kind: "builtin",
            active: active.as_deref() == Some(profile.name()),
            description: describe(profile),
            detection: profile.detection_config(),
            warnings: Vec::new(),
        }));
    }

    let dir = profiles_dir(&state)?;
    let stored = read_custom(&dir, &name)?
        .ok_or_else(|| AdminError::not_found(format!("Unknown profile '{name}'")))?;
    Ok(Json(ProfileDetail {
        active: active.as_deref() == Some(stored.name.as_str()),
        name: stored.name,
        kind: "custom",
        description: stored.description,
        detection: stored.detection,
        warnings: Vec::new(),
    }))
}

/// `POST /admin/api/profiles` — create a new custom profile.
pub async fn create_profile(
    State(state): State<Arc<AppState>>,
    Json(input): Json<CustomProfileInput>,
) -> Result<Json<ProfileDetail>, AdminError> {
    let dir = profiles_dir(&state)?;
    let name = input
        .name
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .ok_or_else(|| AdminError::bad_request("Profile name is required"))?
        .to_string();
    let path = custom_profile_path(&dir, &name)?;

    if read_custom(&dir, &name)?.is_some() {
        return Err(AdminError::bad_request(format!(
            "A custom profile named '{name}' already exists"
        )));
    }

    validate_detection(&input.detection)?;

    let stored = StoredProfile {
        name: path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(&name)
            .to_string(),
        description: input.description,
        detection: input.detection,
    };
    write_custom(&dir, &path, &stored)?;

    Ok(Json(ProfileDetail {
        name: stored.name,
        kind: "custom",
        active: false,
        description: stored.description,
        detection: stored.detection,
        warnings: Vec::new(),
    }))
}

/// `PUT /admin/api/profiles/{name}` — update an existing custom profile.
pub async fn update_profile(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(input): Json<CustomProfileInput>,
) -> Result<Json<ProfileDetail>, AdminError> {
    let dir = profiles_dir(&state)?;
    let path = custom_profile_path(&dir, &name)?;

    if read_custom(&dir, &name)?.is_none() {
        return Err(AdminError::not_found(format!(
            "Custom profile '{name}' not found"
        )));
    }

    validate_detection(&input.detection)?;

    let stored = StoredProfile {
        name: path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(&name)
            .to_string(),
        description: input.description,
        detection: input.detection,
    };
    write_custom(&dir, &path, &stored)?;

    let active = state.active_profile.read().await.clone();
    // If the active profile was just edited, apply the new detection live.
    let mut warnings = Vec::new();
    if active.as_deref() == Some(stored.name.as_str()) {
        warnings =
            apply_detection_config(&state, stored.detection.clone(), Some(stored.name.clone()))
                .await?;
    }

    Ok(Json(ProfileDetail {
        active: active.as_deref() == Some(stored.name.as_str()),
        name: stored.name,
        kind: "custom",
        description: stored.description,
        detection: stored.detection,
        warnings,
    }))
}

/// `DELETE /admin/api/profiles/{name}` — remove a custom profile.
pub async fn delete_profile(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, AdminError> {
    let dir = profiles_dir(&state)?;
    let path = custom_profile_path(&dir, &name)?;

    let stored = read_custom(&dir, &name)?
        .ok_or_else(|| AdminError::not_found(format!("Custom profile '{name}' not found")))?;

    let active = state.active_profile.read().await.clone();
    if active.as_deref() == Some(stored.name.as_str()) {
        return Err(AdminError::bad_request(
            "Cannot delete the currently active profile. Activate another profile first.",
        ));
    }

    std::fs::remove_file(&path)
        .map_err(|e| AdminError::internal(format!("Cannot delete profile: {e}")))?;
    Ok(Json(
        serde_json::json!({ "deleted": true, "name": stored.name }),
    ))
}

/// `POST /admin/api/profiles/{name}/activate` — switch the active profile.
///
/// Works for both built-in industry presets and persisted custom profiles.
pub async fn activate_profile(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<ProfileDetail>, AdminError> {
    let (canonical_name, kind, description, detection) =
        if let Some(profile) = IndustryProfile::from_name(&name) {
            (
                profile.name().to_string(),
                "builtin",
                describe(profile),
                profile.detection_config(),
            )
        } else {
            let dir = profiles_dir(&state).map_err(|_| {
                AdminError::bad_request(format!(
                    "Unknown profile '{name}'. Use a built-in (general, legal, healthcare, \
                     fintech) or a configured custom profile."
                ))
            })?;
            let stored = read_custom(&dir, &name)?.ok_or_else(|| {
                AdminError::bad_request(format!(
                    "Unknown profile '{name}'. Use a built-in (general, legal, healthcare, \
                     fintech) or a configured custom profile."
                ))
            })?;
            (stored.name, "custom", stored.description, stored.detection)
        };

    let warnings =
        apply_detection_config(&state, detection.clone(), Some(canonical_name.clone())).await?;

    let _ = state.audit.log_metadata(
        cloakpipe_audit::AuditContext::new("admin", "profile.activate")
            .with_session(Some(cloakpipe_core::session::GLOBAL_SESSION_ID)),
        cloakpipe_audit::AuditEvent::Configure,
        None,
        Vec::new(),
    );

    Ok(Json(ProfileDetail {
        name: canonical_name,
        kind,
        active: true,
        description,
        detection,
        warnings,
    }))
}

/// Persist a custom profile to disk as pretty JSON, creating the directory.
fn write_custom(
    dir: &std::path::Path,
    path: &std::path::Path,
    stored: &StoredProfile,
) -> Result<(), AdminError> {
    std::fs::create_dir_all(dir)
        .map_err(|e| AdminError::internal(format!("Cannot create profiles dir: {e}")))?;
    let json = serde_json::to_string_pretty(stored)
        .map_err(|e| AdminError::internal(format!("Cannot serialize profile: {e}")))?;
    std::fs::write(path, json.as_bytes())
        .map_err(|e| AdminError::internal(format!("Cannot write profile: {e}")))?;
    Ok(())
}
