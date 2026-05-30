use axum::{
    body::Body,
    extract::State as AxumState,
    http::{Method, Request, StatusCode},
    response::Response,
    routing::any,
    Router,
};
use bytes::Bytes;
use cloakpipe_audit::AuditSink;
use cloakpipe_core::{
    config::{
        AuditConfig, CloakPipeConfig, DetectionConfig, LocalConfig, ProxyAuthMode, ProxyConfig,
        ProxyMode, TreeConfig, VaultConfig, VectorConfig,
    },
    detector::Detector,
    session::SessionConfig,
    vault::Vault,
    DetectedEntity, DetectionSource, EntityCategory, MaskingStrategy, PseudoToken,
};
use cloakpipe_proxy::{server::build_router, state::AppState};
use http_body_util::BodyExt;
use std::collections::HashMap;
use tokio::sync::mpsc;
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
            mode: ProxyMode::Server,
            dry_run: false,
            bypass: Vec::new(),
            auth_mode: ProxyAuthMode::ServerKey,
            provider_routes: std::collections::HashMap::new(),
            http_proxy: Default::default(),
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

fn test_state_for_mode_with_api_key(
    mode: ProxyMode,
    api_key: Option<&str>,
) -> std::sync::Arc<AppState> {
    let audit_dir = std::env::temp_dir()
        .join(format!("cloakpipe-proxy-test-{}", Uuid::new_v4()))
        .to_string_lossy()
        .to_string();
    let mut config = test_config(&audit_dir);
    config.proxy.mode = mode;
    let detector = Detector::from_config(&config.detection).unwrap();
    let vault = Vault::ephemeral();
    let audit = AuditSink::from_config(&config.audit).unwrap();

    std::sync::Arc::new(AppState::new(
        config,
        detector,
        vault,
        audit,
        api_key.map(str::to_owned),
    ))
}

fn test_state_with_api_key(api_key: Option<&str>) -> std::sync::Arc<AppState> {
    test_state_for_mode_with_api_key(ProxyMode::Server, api_key)
}

#[derive(Clone)]
enum MockBehavior {
    Echo {
        content_type: &'static str,
    },
    StreamEcho {
        content_type: &'static str,
    },
    Static {
        content_type: &'static str,
        body: &'static str,
    },
}

#[derive(Debug)]
struct CapturedRequest {
    method: Method,
    path_and_query: String,
    headers: axum::http::HeaderMap,
    body: Vec<u8>,
}

#[derive(Clone)]
struct MockUpstreamState {
    capture_tx: mpsc::UnboundedSender<CapturedRequest>,
    behavior: MockBehavior,
}

async fn mock_upstream_handler(
    AxumState(state): AxumState<MockUpstreamState>,
    method: Method,
    uri: axum::http::Uri,
    headers: axum::http::HeaderMap,
    body: Body,
) -> Response {
    let body = body.collect().await.unwrap().to_bytes().to_vec();
    let path_and_query = uri
        .path_and_query()
        .map(|value| value.as_str().to_string())
        .unwrap_or_else(|| uri.path().to_string());

    state
        .capture_tx
        .send(CapturedRequest {
            method,
            path_and_query,
            headers: headers.clone(),
            body: body.clone(),
        })
        .unwrap();

    match state.behavior {
        MockBehavior::Echo { content_type } => Response::builder()
            .status(StatusCode::OK)
            .header("content-type", content_type)
            .body(Body::from(body))
            .unwrap(),
        MockBehavior::StreamEcho { content_type } => {
            let text = String::from_utf8(body).unwrap();
            let split_at = previous_char_boundary(&text, text.len() / 2);
            let (first, second) = text.split_at(split_at);
            let stream = futures::stream::iter(vec![
                Ok::<_, std::io::Error>(Bytes::from(first.to_string())),
                Ok::<_, std::io::Error>(Bytes::from(second.to_string())),
            ]);

            Response::builder()
                .status(StatusCode::OK)
                .header("content-type", content_type)
                .body(Body::from_stream(stream))
                .unwrap()
        }
        MockBehavior::Static { content_type, body } => Response::builder()
            .status(StatusCode::OK)
            .header("content-type", content_type)
            .body(Body::from(body.to_string()))
            .unwrap(),
    }
}

async fn spawn_mock_upstream(
    behavior: MockBehavior,
) -> (
    String,
    mpsc::UnboundedReceiver<CapturedRequest>,
    tokio::task::JoinHandle<()>,
) {
    let (capture_tx, capture_rx) = mpsc::unbounded_channel();
    let state = MockUpstreamState {
        capture_tx,
        behavior,
    };

    let app = Router::new()
        .route("/", any(mock_upstream_handler))
        .route("/*path", any(mock_upstream_handler))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    (format!("http://{}", address), capture_rx, handle)
}

fn llm_proxy_test_state(
    upstream: &str,
    dry_run: bool,
    bypass: Vec<String>,
    auth_mode: ProxyAuthMode,
) -> std::sync::Arc<AppState> {
    let audit_dir = std::env::temp_dir()
        .join(format!("cloakpipe-proxy-llm-proxy-test-{}", Uuid::new_v4()))
        .to_string_lossy()
        .to_string();

    let config = CloakPipeConfig {
        proxy: ProxyConfig {
            listen: "127.0.0.1:8900".into(),
            upstream: upstream.into(),
            api_key_env: "OPENAI_API_KEY".into(),
            timeout_seconds: 120,
            max_concurrent: 256,
            mode: ProxyMode::LlmProxy,
            dry_run,
            bypass,
            auth_mode,
            provider_routes: HashMap::from([
                ("openai".to_string(), upstream.to_string()),
                ("anthropic".to_string(), upstream.to_string()),
            ]),
            http_proxy: Default::default(),
            masking_strategy: MaskingStrategy::Similar,
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
            log_path: audit_dir,
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
    };

    let detector = Detector::from_config(&config.detection).unwrap();
    let vault = Vault::ephemeral();
    let audit = AuditSink::from_config(&config.audit).unwrap();

    std::sync::Arc::new(AppState::new(
        config,
        detector,
        vault,
        audit,
        Some("server-side-key".to_string()),
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

async fn raw_response(
    app: axum::Router,
    method: Method,
    uri: &str,
    content_type: &str,
    extra_headers: &[(&str, &str)],
    body: impl Into<Body>,
) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
    let mut request = Request::builder().method(method).uri(uri);
    if !content_type.is_empty() {
        request = request.header("content-type", content_type);
    }
    for (name, value) in extra_headers {
        request = request.header(*name, *value);
    }

    let request = request.body(body.into()).unwrap();
    let response = app.oneshot(request).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = response
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes()
        .to_vec();
    (status, headers, bytes)
}

fn previous_char_boundary(text: &str, mut index: usize) -> usize {
    index = index.min(text.len());
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
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
    assert_eq!(listed["total"], 2);
    let sessions = listed["sessions"].as_array().unwrap();
    assert!(sessions
        .iter()
        .any(|session| session["session_id"] == "global"));
    assert!(sessions
        .iter()
        .any(|session| session["session_id"] == "session-123"));

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
async fn test_server_mode_does_not_expose_llm_proxy_routes() {
    let state = test_state_with_api_key(None);
    let app = build_router(state);

    let (status, _, _) = raw_response(
        app,
        Method::POST,
        "/v1/chat/completions",
        "application/json",
        &[],
        Body::from(
            serde_json::json!({
                "model": "gpt-4o",
                "messages": [{"role": "user", "content": "hello"}]
            })
            .to_string(),
        ),
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_llm_proxy_mode_does_not_expose_direct_api_endpoints() {
    let (upstream, mut captured_rx, server_handle) = spawn_mock_upstream(MockBehavior::Echo {
        content_type: "application/json",
    })
    .await;
    let state = llm_proxy_test_state(&upstream, false, Vec::new(), ProxyAuthMode::PassThrough);
    let app = build_router(state);

    let (status, _, _) = raw_response(
        app,
        Method::POST,
        "/v1/pseudonymize",
        "application/json",
        &[("authorization", "Bearer caller-token")],
        Body::from(serde_json::json!({ "text": "alice@example.com" }).to_string()),
    )
    .await;

    let captured = captured_rx.recv().await.unwrap();
    server_handle.abort();

    assert_eq!(status, StatusCode::OK);
    assert_eq!(captured.path_and_query, "/v1/pseudonymize");
}

#[tokio::test]
async fn test_chat_and_embeddings_return_service_unavailable_without_upstream_api_key() {
    let state = test_state_for_mode_with_api_key(ProxyMode::LlmProxy, None);

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

#[tokio::test]
async fn test_llm_proxy_routes_openai_requests_and_mutates_only_content_fields() {
    let (upstream, mut captured_rx, server_handle) = spawn_mock_upstream(MockBehavior::Echo {
        content_type: "application/json",
    })
    .await;
    let state = llm_proxy_test_state(&upstream, false, Vec::new(), ProxyAuthMode::PassThrough);
    let app = build_router(state);

    let request_body = serde_json::json!({
        "model": "gpt-4o",
        "messages": [
            {
                "role": "user",
                "content": "Email alice@example.com"
            }
        ],
        "metadata": {
            "content": "alice@example.com"
        }
    });

    let (status, response_headers, response_body) = raw_response(
        app,
        Method::POST,
        "/chat/completions",
        "application/json",
        &[
            ("authorization", "Bearer caller-token"),
            ("x-session-id", "sess-1"),
        ],
        Body::from(request_body.to_string()),
    )
    .await;

    let captured = captured_rx.recv().await.unwrap();
    server_handle.abort();

    assert_eq!(status, StatusCode::OK);
    assert_eq!(captured.method, Method::POST);
    assert_eq!(captured.path_and_query, "/v1/chat/completions");
    assert_eq!(
        captured.headers.get("authorization").unwrap(),
        "Bearer caller-token"
    );
    assert_eq!(captured.headers.get("accept-encoding").unwrap(), "identity");

    let forwarded_json: serde_json::Value = serde_json::from_slice(&captured.body).unwrap();
    assert!(!forwarded_json["messages"][0]["content"]
        .as_str()
        .unwrap()
        .contains("alice@example.com"));
    assert_eq!(forwarded_json["metadata"]["content"], "alice@example.com");
    assert_eq!(forwarded_json["model"], "gpt-4o");

    let response_json: serde_json::Value = serde_json::from_slice(&response_body).unwrap();
    assert!(response_json.to_string().contains("alice@example.com"));
    assert_eq!(
        response_headers.get("x-cloakpipe-leaked-entities").unwrap(),
        "0"
    );
}

#[tokio::test]
async fn test_llm_proxy_redacts_unexpected_pii_in_json_responses() {
    let (upstream, mut captured_rx, server_handle) = spawn_mock_upstream(MockBehavior::Static {
        content_type: "application/json",
        body: r#"{"choices":[{"message":{"content":"Contact leaked@example.com immediately."}}]}"#,
    })
    .await;
    let state = llm_proxy_test_state(&upstream, false, Vec::new(), ProxyAuthMode::PassThrough);
    let app = build_router(state);

    let (status, response_headers, response_body) = raw_response(
        app,
        Method::POST,
        "/v1/chat/completions",
        "application/json",
        &[("authorization", "Bearer caller-token")],
        Body::from(
            serde_json::json!({
                "model": "gpt-4o",
                "messages": [{"role": "user", "content": "hello"}]
            })
            .to_string(),
        ),
    )
    .await;

    let _captured = captured_rx.recv().await.unwrap();
    server_handle.abort();

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        response_headers.get("x-cloakpipe-leaked-entities").unwrap(),
        "1"
    );

    let response_json: serde_json::Value = serde_json::from_slice(&response_body).unwrap();
    assert_eq!(
        response_json["choices"][0]["message"]["content"],
        "Contact [REDACTED] immediately."
    );
}

#[tokio::test]
async fn test_llm_proxy_reuses_session_coreference_tokens() {
    let (upstream, mut captured_rx, server_handle) = spawn_mock_upstream(MockBehavior::Echo {
        content_type: "application/json",
    })
    .await;
    let state = llm_proxy_test_state(&upstream, false, Vec::new(), ProxyAuthMode::PassThrough);

    let app = build_router(state.clone());
    let (first_status, _, _) = raw_response(
        app,
        Method::POST,
        "/v1/chat/completions",
        "application/json",
        &[
            ("authorization", "Bearer caller-token"),
            ("x-session-id", "session-coref"),
        ],
        Body::from(
            serde_json::json!({
                "model": "gpt-4o",
                "messages": [{"role": "user", "content": "$500 approved the request."}]
            })
            .to_string(),
        ),
    )
    .await;

    let app = build_router(state);
    let (second_status, _, _) = raw_response(
        app,
        Method::POST,
        "/v1/chat/completions",
        "application/json",
        &[
            ("authorization", "Bearer caller-token"),
            ("x-session-id", "session-coref"),
        ],
        Body::from(
            serde_json::json!({
                "model": "gpt-4o",
                "messages": [{"role": "user", "content": "the amount approved the request."}]
            })
            .to_string(),
        ),
    )
    .await;

    let first_captured = captured_rx.recv().await.unwrap();
    let second_captured = captured_rx.recv().await.unwrap();
    server_handle.abort();

    assert_eq!(first_status, StatusCode::OK);
    assert_eq!(second_status, StatusCode::OK);

    let first_json: serde_json::Value = serde_json::from_slice(&first_captured.body).unwrap();
    let second_json: serde_json::Value = serde_json::from_slice(&second_captured.body).unwrap();

    let first_content = first_json["messages"][0]["content"].as_str().unwrap();
    let second_content = second_json["messages"][0]["content"].as_str().unwrap();

    assert_ne!(first_content, "$500 approved the request.");
    assert_eq!(first_content, second_content);
    assert!(!second_content.contains("the amount approved the request."));
}

#[tokio::test]
async fn test_llm_proxy_routes_anthropic_prefix_and_forwards_pass_through_auth() {
    let (upstream, mut captured_rx, server_handle) = spawn_mock_upstream(MockBehavior::Echo {
        content_type: "application/json",
    })
    .await;
    let state = llm_proxy_test_state(&upstream, false, Vec::new(), ProxyAuthMode::PassThrough);
    let app = build_router(state);

    let request_body = serde_json::json!({
        "messages": [
            {
                "role": "user",
                "content": "Email alice@example.com"
            }
        ]
    });

    let (status, _, _) = raw_response(
        app,
        Method::POST,
        "/anthropic/v1/messages",
        "application/json",
        &[
            ("x-api-key", "anthropic-test-key"),
            ("anthropic-version", "2023-06-01"),
            ("x-session-id", "anthropic-sess"),
        ],
        Body::from(request_body.to_string()),
    )
    .await;

    let captured = captured_rx.recv().await.unwrap();
    server_handle.abort();

    assert_eq!(status, StatusCode::OK);
    assert_eq!(captured.path_and_query, "/v1/messages");
    assert_eq!(
        captured.headers.get("x-api-key").unwrap(),
        "anthropic-test-key"
    );
    assert_eq!(
        captured.headers.get("anthropic-version").unwrap(),
        "2023-06-01"
    );

    let forwarded_json: serde_json::Value = serde_json::from_slice(&captured.body).unwrap();
    assert!(!forwarded_json["messages"][0]["content"]
        .as_str()
        .unwrap()
        .contains("alice@example.com"));
}

#[tokio::test]
async fn test_llm_proxy_dry_run_forwards_original_body() {
    let (upstream, mut captured_rx, server_handle) = spawn_mock_upstream(MockBehavior::Echo {
        content_type: "text/plain",
    })
    .await;
    let state = llm_proxy_test_state(&upstream, true, Vec::new(), ProxyAuthMode::PassThrough);
    let app = build_router(state);

    let (status, _, response_body) = raw_response(
        app,
        Method::POST,
        "/responses",
        "text/plain",
        &[("authorization", "Bearer caller-token")],
        Body::from("alice@example.com"),
    )
    .await;

    let captured = captured_rx.recv().await.unwrap();
    server_handle.abort();

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        String::from_utf8(captured.body).unwrap(),
        "alice@example.com"
    );
    assert_eq!(
        String::from_utf8(response_body).unwrap(),
        "alice@example.com"
    );
}

#[tokio::test]
async fn test_llm_proxy_bypass_forwards_original_body_and_skips_rehydration() {
    let (upstream, mut captured_rx, server_handle) = spawn_mock_upstream(MockBehavior::Echo {
        content_type: "text/plain",
    })
    .await;
    let state = llm_proxy_test_state(
        &upstream,
        false,
        vec!["127.0.0.1".to_string()],
        ProxyAuthMode::PassThrough,
    );
    let app = build_router(state);

    let (status, _, response_body) = raw_response(
        app,
        Method::POST,
        "/responses",
        "text/plain",
        &[("authorization", "Bearer caller-token")],
        Body::from("alice@example.com"),
    )
    .await;

    let captured = captured_rx.recv().await.unwrap();
    server_handle.abort();

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        String::from_utf8(captured.body).unwrap(),
        "alice@example.com"
    );
    assert_eq!(
        String::from_utf8(response_body).unwrap(),
        "alice@example.com"
    );
}

#[tokio::test]
async fn test_llm_proxy_rehydrates_full_text_responses() {
    let (upstream, mut captured_rx, server_handle) = spawn_mock_upstream(MockBehavior::Echo {
        content_type: "text/plain",
    })
    .await;
    let state = llm_proxy_test_state(&upstream, false, Vec::new(), ProxyAuthMode::PassThrough);
    let app = build_router(state);

    let (status, response_headers, response_body) = raw_response(
        app,
        Method::POST,
        "/responses",
        "text/plain",
        &[("authorization", "Bearer caller-token")],
        Body::from("alice@example.com"),
    )
    .await;

    let captured = captured_rx.recv().await.unwrap();
    server_handle.abort();

    let forwarded_text = String::from_utf8(captured.body).unwrap();
    assert_ne!(forwarded_text, "alice@example.com");
    assert_eq!(status, StatusCode::OK);
    assert!(response_headers.get("x-cloakpipe-request-id").is_some());
    assert_eq!(
        String::from_utf8(response_body).unwrap(),
        "alice@example.com"
    );
}

#[tokio::test]
async fn test_llm_proxy_restores_streamed_fake_across_chunk_boundary() {
    let (upstream, mut captured_rx, server_handle) =
        spawn_mock_upstream(MockBehavior::StreamEcho {
            content_type: "text/event-stream",
        })
        .await;
    let state = llm_proxy_test_state(&upstream, false, Vec::new(), ProxyAuthMode::PassThrough);
    let app = build_router(state);

    let (status, _, response_body) = raw_response(
        app,
        Method::POST,
        "/responses",
        "text/plain",
        &[("authorization", "Bearer caller-token")],
        Body::from("alice@example.com"),
    )
    .await;

    let captured = captured_rx.recv().await.unwrap();
    server_handle.abort();

    let forwarded_text = String::from_utf8(captured.body).unwrap();
    assert_ne!(forwarded_text, "alice@example.com");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        String::from_utf8(response_body).unwrap(),
        "alice@example.com"
    );
}
