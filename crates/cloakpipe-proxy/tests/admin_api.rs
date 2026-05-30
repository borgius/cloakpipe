//! Integration tests for the self-hosted admin API (`/admin/api/*`).

use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
    Router,
};
use cloakpipe_audit::AuditSink;
use cloakpipe_core::{
    config::{
        AuditConfig, CloakPipeConfig, DetectionConfig, LocalConfig, ProxyAuthMode, ProxyConfig,
        ProxyMode, TreeConfig, VaultConfig, VectorConfig,
    },
    detector::Detector,
    session::SessionConfig,
    vault::Vault,
    MaskingStrategy,
};
use cloakpipe_proxy::{server::build_router, state::AppState};
use http_body_util::BodyExt;
use serde_json::Value;
use std::path::PathBuf;
use tower::ServiceExt;

struct Harness {
    router: Router,
    _tmp: tempfile::TempDir,
    policies_dir: PathBuf,
    profiles_dir: PathBuf,
}

fn base_config(audit_dir: &str, audit_backend: &str) -> CloakPipeConfig {
    CloakPipeConfig {
        proxy: ProxyConfig {
            listen: "127.0.0.1:8400".into(),
            upstream: "https://api.openai.com".into(),
            api_key_env: "OPENAI_API_KEY".into(),
            timeout_seconds: 120,
            max_concurrent: 256,
            mode: ProxyMode::Server,
            dry_run: false,
            bypass: Vec::new(),
            auth_mode: ProxyAuthMode::ServerKey,
            provider_routes: std::collections::HashMap::new(),
            http_proxy: Default::default(),
            masking_strategy: MaskingStrategy::Token,
            admin_token_env: "CLOAKPIPE_ADMIN_TOKEN".into(),
        },
        vault: VaultConfig {
            path: "./vault.enc".into(),
            encryption: "aes-256-gcm".into(),
            key_env: Some("CLOAKPIPE_VAULT_KEY".into()),
            key_keyring: false,
            backend: "file".into(),
        },
        profile: Some("general".into()),
        detection: DetectionConfig {
            secrets: true,
            financial: true,
            dates: true,
            emails: true,
            phone_numbers: false,
            ip_addresses: false,
            urls_internal: false,
            ner: Default::default(),
            custom: Default::default(),
            overrides: Default::default(),
            resolver: Default::default(),
        },
        tree: TreeConfig::default(),
        vectors: VectorConfig::default(),
        local: LocalConfig::default(),
        audit: AuditConfig {
            log_path: audit_dir.into(),
            backend: audit_backend.into(),
            ..AuditConfig::default()
        },
        session: SessionConfig {
            enabled: true,
            id_from: "header:x-session-id".into(),
            ttl_seconds: 1800,
            coreference: true,
            sensitivity_escalation: true,
            session_threshold: 0.8,
        },
    }
}

fn harness(audit_backend: &str) -> Harness {
    harness_with_token(audit_backend, None)
}

fn harness_with_token(audit_backend: &str, admin_token: Option<&str>) -> Harness {
    let tmp = tempfile::tempdir().unwrap();
    let policies_dir = tmp.path().join("policies");
    std::fs::create_dir_all(&policies_dir).unwrap();
    let profiles_dir = tmp.path().join("profiles");
    std::fs::create_dir_all(&profiles_dir).unwrap();
    let audit_dir = tmp.path().join("audit");
    std::fs::create_dir_all(&audit_dir).unwrap();
    let config_path = tmp.path().join("cloakpipe.toml");
    std::fs::write(&config_path, "# active config\n").unwrap();

    let config = base_config(&audit_dir.to_string_lossy(), audit_backend);
    let detector = Detector::from_config(&config.detection).unwrap();
    let vault = Vault::ephemeral();
    let audit = AuditSink::from_config(&config.audit).unwrap();
    let state = std::sync::Arc::new(
        AppState::new(config, detector, vault, audit, None)
            .with_admin_context(Some(config_path), Some(policies_dir.clone()))
            .with_profiles_dir(Some(profiles_dir.clone()))
            .with_admin_token(admin_token.map(str::to_string)),
    );
    Harness {
        router: build_router_from_state(state),
        _tmp: tmp,
        policies_dir,
        profiles_dir,
    }
}

fn build_router_from_state(state: std::sync::Arc<AppState>) -> Router {
    // build_router consumes Arc<AppState>; reuse the public helper.
    build_router(state)
}

async fn request(
    router: &Router,
    method: Method,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let body = match body {
        Some(v) => Body::from(serde_json::to_vec(&v).unwrap()),
        None => Body::empty(),
    };
    let req = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(body)
        .unwrap();
    let resp = router.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let value: Value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, value)
}

/// Issue a request with an optional `Authorization` header value.
async fn request_with_auth(
    router: &Router,
    method: Method,
    uri: &str,
    authorization: Option<&str>,
) -> StatusCode {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(value) = authorization {
        builder = builder.header("authorization", value);
    }
    let req = builder.body(Body::empty()).unwrap();
    router.clone().oneshot(req).await.unwrap().status()
}

#[tokio::test]
async fn system_status_reports_runtime_state() {
    let h = harness("jsonl");
    let (status, body) = request(&h.router, Method::GET, "/admin/api/system", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["service"], "cloakpipe");
    assert_eq!(body["mode"], "server");
    assert_eq!(body["active_profile"], "general");
    assert_eq!(body["audit"]["backend"], "jsonl");
    assert!(body["config_path"].is_string());
    assert!(body["policies_dir"].is_string());
    assert!(body["profiles_dir"].is_string());
    assert_eq!(body["auth_required"], false);
}

#[tokio::test]
async fn lists_builtin_profiles_and_activates() {
    let h = harness("jsonl");
    let (status, body) = request(&h.router, Method::GET, "/admin/api/profiles", None).await;
    assert_eq!(status, StatusCode::OK);
    let names: Vec<&str> = body
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"legal"));

    let (status, body) = request(
        &h.router,
        Method::POST,
        "/admin/api/profiles/legal/activate",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["active"], true);
    assert_eq!(body["name"], "legal");

    let (status, body) = request(
        &h.router,
        Method::POST,
        "/admin/api/profiles/nope/activate",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "bad_request");
}

#[tokio::test]
async fn policy_crud_and_validation() {
    let h = harness("jsonl");

    // Validate good + bad content.
    let good = "[proxy]\nlisten=\"127.0.0.1:8400\"\nupstream=\"https://api.openai.com\"\n[vault]\npath=\"./vault.enc\"\n[detection]\n";
    let (status, body) = request(
        &h.router,
        Method::POST,
        "/admin/api/policy/validate",
        Some(serde_json::json!({ "content": good })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["valid"], true);

    let (status, body) = request(
        &h.router,
        Method::POST,
        "/admin/api/policy/validate",
        Some(serde_json::json!({ "content": "not valid toml = " })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["valid"], false);

    // Create via PUT.
    let (status, _) = request(
        &h.router,
        Method::PUT,
        "/admin/api/policies/my-policy",
        Some(serde_json::json!({ "content": good })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(h.policies_dir.join("my-policy.toml").exists());

    // List should include it.
    let (status, body) = request(&h.router, Method::GET, "/admin/api/policies", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p["name"] == "my-policy"));

    // Reject invalid content on save.
    let (status, _) = request(
        &h.router,
        Method::PUT,
        "/admin/api/policies/bad",
        Some(serde_json::json!({ "content": "garbage = " })),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    // Path traversal is rejected.
    let (status, _) = request(
        &h.router,
        Method::PUT,
        "/admin/api/policies/..%2f..%2fetc%2fpasswd",
        Some(serde_json::json!({ "content": good })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Delete.
    let (status, _) = request(
        &h.router,
        Method::DELETE,
        "/admin/api/policies/my-policy",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(!h.policies_dir.join("my-policy.toml").exists());
}

#[tokio::test]
async fn custom_rule_lifecycle() {
    let h = harness("jsonl");

    // Create.
    let rule = serde_json::json!({
        "name": "employee_id",
        "regex": r"EMP-\d{4}",
        "category": "EMPLOYEE_ID"
    });
    let (status, body) = request(
        &h.router,
        Method::POST,
        "/admin/api/categories/rules",
        Some(rule),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.as_array().unwrap().len(), 1);

    // Invalid regex rejected.
    let bad = serde_json::json!({ "name": "x", "regex": "(", "category": "X" });
    let (status, _) = request(
        &h.router,
        Method::POST,
        "/admin/api/categories/rules",
        Some(bad),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    // Categories listing includes the rule + families.
    let (status, body) = request(&h.router, Method::GET, "/admin/api/categories", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(!body["families"].as_array().unwrap().is_empty());
    assert_eq!(body["custom_rules"].as_array().unwrap().len(), 1);

    // Update.
    let updated = serde_json::json!({
        "name": "employee_id",
        "regex": r"EMP-\d{6}",
        "category": "EMPLOYEE_ID"
    });
    let (status, _) = request(
        &h.router,
        Method::PUT,
        "/admin/api/categories/rules/employee_id",
        Some(updated),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Delete.
    let (status, body) = request(
        &h.router,
        Method::DELETE,
        "/admin/api/categories/rules/employee_id",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.as_array().unwrap().is_empty());

    // Deleting a missing rule 404s.
    let (status, _) = request(
        &h.router,
        Method::DELETE,
        "/admin/api/categories/rules/missing",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn audit_endpoints_sqlite() {
    let h = harness("sqlite");
    // Generate an audit entry by activating a profile.
    let _ = request(
        &h.router,
        Method::POST,
        "/admin/api/profiles/legal/activate",
        None,
    )
    .await;

    let (status, body) = request(&h.router, Method::GET, "/admin/api/audit/summary", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["backend"], "sqlite");
    assert_eq!(body["supported"], true);

    let (status, body) = request(
        &h.router,
        Method::GET,
        "/admin/api/audit/events?limit=10",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["backend"], "sqlite");
    assert!(body["events"].is_array());
}

#[tokio::test]
async fn vault_mappings_redacted_by_default() {
    let h = harness("jsonl");
    let (status, body) = request(&h.router, Method::GET, "/admin/api/vault/stats", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total_mappings"], 0);

    let (status, body) = request(&h.router, Method::GET, "/admin/api/vault/mappings", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["redacted"], true);
    assert!(body["mappings"].is_array());
}

#[tokio::test]
async fn custom_profile_lifecycle() {
    let h = harness("jsonl");

    // Create a custom profile.
    let create = serde_json::json!({
        "name": "acme",
        "description": "ACME corp defaults",
        "detection": {
            "secrets": true,
            "financial": false,
            "dates": false,
            "emails": true,
            "phone_numbers": false,
            "ip_addresses": false,
            "urls_internal": false,
            "ner": { "enabled": false }
        }
    });
    let (status, body) = request(
        &h.router,
        Method::POST,
        "/admin/api/profiles",
        Some(create.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
    assert_eq!(body["kind"], "custom");
    assert_eq!(body["name"], "acme");
    assert!(h.profiles_dir.join("acme.json").exists());

    // It now shows up in the listing alongside built-ins.
    let (status, body) = request(&h.router, Method::GET, "/admin/api/profiles", None).await;
    assert_eq!(status, StatusCode::OK);
    let custom = body
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "acme")
        .expect("custom profile listed");
    assert_eq!(custom["kind"], "custom");

    // Reserved built-in names are rejected.
    let (status, _) = request(
        &h.router,
        Method::POST,
        "/admin/api/profiles",
        Some(serde_json::json!({ "name": "legal", "detection": create["detection"] })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Duplicate create is rejected.
    let (status, _) = request(
        &h.router,
        Method::POST,
        "/admin/api/profiles",
        Some(create.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Activate the custom profile.
    let (status, body) = request(
        &h.router,
        Method::POST,
        "/admin/api/profiles/acme/activate",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["active"], true);
    assert_eq!(body["kind"], "custom");

    // Cannot delete the active profile.
    let (status, _) = request(&h.router, Method::DELETE, "/admin/api/profiles/acme", None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Switch away, then delete succeeds.
    let _ = request(
        &h.router,
        Method::POST,
        "/admin/api/profiles/general/activate",
        None,
    )
    .await;
    let (status, body) = request(&h.router, Method::DELETE, "/admin/api/profiles/acme", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["deleted"], true);
    assert!(!h.profiles_dir.join("acme.json").exists());

    // Updating a missing profile 404s.
    let (status, _) = request(
        &h.router,
        Method::PUT,
        "/admin/api/profiles/ghost",
        Some(create),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn audit_jsonl_query_supported() {
    let h = harness("jsonl");
    // Activating profiles writes JSONL audit metadata entries.
    let _ = request(
        &h.router,
        Method::POST,
        "/admin/api/profiles/legal/activate",
        None,
    )
    .await;
    let _ = request(
        &h.router,
        Method::POST,
        "/admin/api/profiles/healthcare/activate",
        None,
    )
    .await;

    let (status, body) = request(
        &h.router,
        Method::GET,
        "/admin/api/audit/events?limit=10",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["backend"], "jsonl");
    assert_eq!(body["supported"], true);
    assert!(body["total_matched"].as_u64().unwrap() >= 2);
    assert!(!body["events"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn admin_auth_enforced_when_token_set() {
    let h = harness_with_token("jsonl", Some("s3cret"));

    // Correct bearer token is accepted.
    let status = request_with_auth(
        &h.router,
        Method::GET,
        "/admin/api/system",
        Some(concat!("Bearer ", "s3cret")),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Missing token is rejected.
    let status = request_with_auth(&h.router, Method::GET, "/admin/api/system", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // Wrong token is rejected.
    let status = request_with_auth(
        &h.router,
        Method::GET,
        "/admin/api/system",
        Some(concat!("Bearer ", "nope")),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn admin_open_when_no_token() {
    let h = harness("jsonl");
    let status = request_with_auth(&h.router, Method::GET, "/admin/api/system", None).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn parameterized_routes_capture_path_segments() {
    // Regression test for the `:id`/`:name` route syntax (axum 0.7 / matchit
    // 0.7): a path parameter must be *captured*, not matched literally. With the
    // wrong `{id}` syntax the segment is treated as a literal and the handler
    // never sees the real id.
    let h = harness("jsonl");

    // DELETE /sessions/<id> must reach the handler and echo the captured id.
    let (status, body) = request(&h.router, Method::DELETE, "/sessions/sess-123", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["session_id"], "sess-123");

    // Admin parameterized route reaches its handler too.
    let (status, body) = request(&h.router, Method::GET, "/admin/api/profiles/legal", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["name"], "legal");
}
