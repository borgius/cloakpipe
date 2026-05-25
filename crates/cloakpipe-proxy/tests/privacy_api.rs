use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
};
use cloakpipe_audit::AuditLogger;
use cloakpipe_core::{
    config::{
        AuditConfig, CloakPipeConfig, DetectionConfig, LocalConfig, ProxyConfig, TreeConfig,
        VaultConfig, VectorConfig,
    },
    detector::Detector,
    session::SessionConfig,
    vault::Vault,
    DetectedEntity, DetectionSource, EntityCategory, MaskingStrategy, PseudoToken,
};
use cloakpipe_proxy::{server::build_router, state::AppState};
use http_body_util::BodyExt;
use tower::ServiceExt;
use uuid::Uuid;

fn test_config(audit_dir: &str) -> CloakPipeConfig {
    CloakPipeConfig {
        proxy: ProxyConfig {
            listen: "127.0.0.1:8900".into(),
            upstream: "https://api.openai.com".into(),
            api_key_env: "OPENAI_API_KEY".into(),
            timeout_seconds: 120,
            max_concurrent: 256,
            mode: "proxy".into(),
            masking_strategy: MaskingStrategy::Token,
        },
        vault: VaultConfig {
            path: "./vault.enc".into(),
            encryption: "aes-256-gcm".into(),
            key_env: Some("CLOAKPIPE_VAULT_KEY".into()),
            key_keyring: false,
            backend: "file".into(),
        },
        profile: None,
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

fn test_state_with_api_key(api_key: Option<&str>) -> std::sync::Arc<AppState> {
    let audit_dir = std::env::temp_dir()
        .join(format!("cloakpipe-proxy-test-{}", Uuid::new_v4()))
        .to_string_lossy()
        .to_string();
    let config = test_config(&audit_dir);
    let detector = Detector::from_config(&config.detection).unwrap();
    let vault = Vault::ephemeral();
    let audit = AuditLogger::new(&audit_dir, true).unwrap();

    std::sync::Arc::new(AppState::new(
        config,
        detector,
        vault,
        audit,
        api_key.map(str::to_owned),
    ))
}

async fn json_response(
    app: axum::Router,
    method: Method,
    uri: &str,
    body: serde_json::Value,
) -> (StatusCode, serde_json::Value) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json = serde_json::from_slice(&bytes).unwrap();
    (status, json)
}

async fn empty_json_response(
    app: axum::Router,
    method: Method,
    uri: &str,
) -> (StatusCode, serde_json::Value) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json = serde_json::from_slice(&bytes).unwrap();
    (status, json)
}

async fn text_response(
    app: axum::Router,
    method: Method,
    uri: &str,
    body: serde_json::Value,
) -> (StatusCode, String) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8(bytes.to_vec()).unwrap();
    (status, text)
}

#[tokio::test]
async fn test_pseudonymize_and_rehydrate_endpoints_roundtrip_without_upstream_api_key() {
    let state = test_state_with_api_key(None);
    let app = build_router(state.clone());

    let (status, pseudonymized) = json_response(
        app,
        Method::POST,
        "/v1/pseudonymize",
        serde_json::json!({
            "text": "Send $500 to alice@example.com before June 1, 2026."
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert!(pseudonymized["entities_detected"].as_u64().unwrap() >= 2);
    assert!(pseudonymized["text"].as_str().unwrap().contains("EMAIL_1"));

    let app = build_router(state);
    let (status, rehydrated) = json_response(
        app,
        Method::POST,
        "/rehydrate",
        serde_json::json!({
            "text": pseudonymized["text"].as_str().unwrap()
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let text = rehydrated["text"].as_str().unwrap();
    assert!(text.contains("alice@example.com"));
    assert!(text.contains("$500"));
    assert!(rehydrated["tokens_rehydrated"].as_u64().unwrap() >= 2);
}

#[tokio::test]
async fn test_detect_and_vault_stats_endpoints() {
    let state = test_state_with_api_key(None);
    let app = build_router(state.clone());

    let (status, detected) = json_response(
        app,
        Method::POST,
        "/detect",
        serde_json::json!({
            "text": "Contact bob@example.com about $900."
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let entities = detected["entities"].as_array().unwrap();
    assert!(entities
        .iter()
        .any(|entity| entity["original"] == "bob@example.com"));

    let app = build_router(state.clone());
    let (status, _) = json_response(
        app,
        Method::POST,
        "/pseudonymize",
        serde_json::json!({
            "text": "Contact bob@example.com about $900."
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let app = build_router(state);
    let (status, stats) = empty_json_response(app, Method::GET, "/v1/vault_stats").await;
    assert_eq!(status, StatusCode::OK);
    assert!(stats["total_mappings"].as_u64().unwrap() >= 2);
    assert!(stats["categories"].get("EMAIL").is_some());
}

#[tokio::test]
async fn test_configure_endpoint_updates_detector_state() {
    let state = test_state_with_api_key(None);
    let app = build_router(state.clone());

    let (status, configured) = json_response(
        app,
        Method::POST,
        "/v1/configure",
        serde_json::json!({
            "disable": ["emails"]
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(configured["emails"], false);

    let app = build_router(state);
    let (status, detected) = json_response(
        app,
        Method::POST,
        "/v1/detect",
        serde_json::json!({
            "text": "alice@example.com"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(detected["entities"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn test_session_context_endpoint_matches_mcp_shape() {
    let state = test_state_with_api_key(None);
    state.sessions.get_or_create("session-123");
    state.sessions.with_session("session-123", |ctx| {
        let entity = DetectedEntity {
            original: "Alice Example".into(),
            start: 0,
            end: 13,
            category: EntityCategory::Person,
            confidence: 1.0,
            source: DetectionSource::Pattern,
        };
        let token = PseudoToken {
            token: "PERSON_1".into(),
            category: EntityCategory::Person,
            id: 1,
        };
        ctx.record_entities(&[entity], &[token]);
    });

    let app = build_router(state.clone());
    let (status, listed) = json_response(
        app,
        Method::POST,
        "/session_context",
        serde_json::json!({
            "session_id": "list"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(listed["total"], 1);
    assert_eq!(
        listed["sessions"].as_array().unwrap()[0]["session_id"],
        "session-123"
    );

    let app = build_router(state.clone());
    let (status, inspected) = json_response(
        app,
        Method::POST,
        "/v1/session_context",
        serde_json::json!({
            "session_id": "session-123"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(inspected["session_id"], "session-123");
    assert_eq!(inspected["entity_count"], 1);

    let app = build_router(state);
    let (status, missing) = json_response(
        app,
        Method::POST,
        "/v1/session_context",
        serde_json::json!({
            "session_id": "missing-session"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(missing["error"], "Session 'missing-session' not found");
}

#[tokio::test]
async fn test_chat_and_embeddings_return_service_unavailable_without_upstream_api_key() {
    let state = test_state_with_api_key(None);

    let app = build_router(state.clone());
    let (status, body) = text_response(
        app,
        Method::POST,
        "/v1/chat/completions",
        serde_json::json!({
            "model": "gpt-4o",
            "messages": [
                {
                    "role": "user",
                    "content": "Email alice@example.com"
                }
            ]
        }),
    )
    .await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body, "Set OPENAI_API_KEY with your API key");

    let app = build_router(state);
    let (status, body) = text_response(
        app,
        Method::POST,
        "/v1/embeddings",
        serde_json::json!({
            "model": "text-embedding-3-small",
            "input": "alice@example.com"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body, "Set OPENAI_API_KEY with your API key");
}

#[tokio::test]
async fn test_upstream_backed_tree_routes_return_service_unavailable_without_upstream_api_key() {
    let state = test_state_with_api_key(None);
    let app = build_router(state);

    let (status, body) = text_response(
        app,
        Method::POST,
        "/tree/index",
        serde_json::json!({
            "name": "doc.txt",
            "text": "This is a sample document."
        }),
    )
    .await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body, "Set OPENAI_API_KEY with your API key");
}
