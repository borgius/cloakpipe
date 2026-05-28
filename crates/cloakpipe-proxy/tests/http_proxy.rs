use axum::{
    body::Body,
    extract::State as AxumState,
    http::{Method, StatusCode},
    response::Response,
    routing::any,
    Router,
};
use cloakpipe_audit::AuditSink;
use cloakpipe_core::{
    config::{
        AuditConfig, CloakPipeConfig, DetectionConfig, HttpProxyConfig, LocalConfig, ProxyAuthMode,
        ProxyConfig, ProxyMode, TreeConfig, VaultConfig, VectorConfig,
    },
    detector::Detector,
    session::SessionConfig,
    vault::Vault,
    MaskingStrategy,
};
use cloakpipe_proxy::{server::serve_http_proxy_listener, state::AppState};
use http_body_util::BodyExt;
use std::{collections::HashMap, net::SocketAddr, sync::Arc};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::mpsc,
};
use uuid::Uuid;

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

    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap()
}

async fn spawn_mock_upstream() -> (
    String,
    mpsc::UnboundedReceiver<CapturedRequest>,
    tokio::task::JoinHandle<()>,
) {
    let (capture_tx, capture_rx) = mpsc::unbounded_channel();
    let state = MockUpstreamState { capture_tx };

    let app = Router::new()
        .route("/", any(mock_upstream_handler))
        .route("/*path", any(mock_upstream_handler))
        .with_state(state);

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    (format!("http://{address}"), capture_rx, handle)
}

fn http_proxy_test_state(http_proxy: HttpProxyConfig) -> Arc<AppState> {
    let audit_dir = std::env::temp_dir()
        .join(format!("cloakpipe-http-proxy-test-{}", Uuid::new_v4()))
        .to_string_lossy()
        .to_string();

    let config = CloakPipeConfig {
        proxy: ProxyConfig {
            listen: "127.0.0.1:0".into(),
            upstream: "https://api.openai.com".into(),
            api_key_env: "OPENAI_API_KEY".into(),
            timeout_seconds: 120,
            max_concurrent: 256,
            mode: ProxyMode::HttpProxy,
            dry_run: false,
            bypass: Vec::new(),
            auth_mode: ProxyAuthMode::PassThrough,
            provider_routes: HashMap::new(),
            http_proxy,
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

    Arc::new(AppState::new(config, detector, vault, audit, None))
}

async fn spawn_proxy(state: Arc<AppState>) -> (SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        serve_http_proxy_listener(listener, state).await.unwrap();
    });

    (address, handle)
}

async fn spawn_echo_server() -> (SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };

            tokio::spawn(async move {
                let mut buffer = [0_u8; 1024];
                loop {
                    match stream.read(&mut buffer).await {
                        Ok(0) => break,
                        Ok(n) => {
                            if stream.write_all(&buffer[..n]).await.is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            });
        }
    });

    (address, handle)
}

async fn read_response_head(stream: &mut TcpStream) -> Vec<u8> {
    let mut response = Vec::new();
    let mut byte = [0_u8; 1];

    loop {
        stream.read_exact(&mut byte).await.unwrap();
        response.push(byte[0]);
        if response.ends_with(b"\r\n\r\n") {
            return response;
        }
        assert!(response.len() < 8192, "response head exceeded 8 KiB");
    }
}

#[tokio::test]
async fn http_proxy_plain_http_mutates_request_and_rehydrates_response() {
    let (upstream, mut captured_rx, upstream_handle) = spawn_mock_upstream().await;
    let state = http_proxy_test_state(HttpProxyConfig::default());
    let (proxy_addr, proxy_handle) = spawn_proxy(state).await;

    let client = reqwest::Client::builder()
        .proxy(
            reqwest::Proxy::all(format!("http://{proxy_addr}"))
                .unwrap()
                .basic_auth("proxy-user", "proxy-pass"),
        )
        .build()
        .unwrap();

    let request_body = serde_json::json!({
        "model": "gpt-4o",
        "messages": [
            {
                "role": "user",
                "content": "Email alice@example.com"
            }
        ]
    });

    let response = client
        .post(format!("{upstream}/v1/chat/completions"))
        .header("authorization", "Bearer caller-token")
        .header("x-session-id", "http-proxy-sess")
        .header("content-type", "application/json")
        .body(request_body.to_string())
        .send()
        .await
        .unwrap();

    let status = response.status();
    let response_body = response.text().await.unwrap();
    let captured = captured_rx.recv().await.unwrap();

    upstream_handle.abort();
    proxy_handle.abort();

    assert_eq!(status, reqwest::StatusCode::OK);
    assert_eq!(captured.method, Method::POST);
    assert_eq!(captured.path_and_query, "/v1/chat/completions");
    assert_eq!(
        captured.headers.get("authorization").unwrap(),
        "Bearer caller-token"
    );
    assert!(captured.headers.get("proxy-authorization").is_none());

    let forwarded_json: serde_json::Value = serde_json::from_slice(&captured.body).unwrap();
    assert!(!forwarded_json["messages"][0]["content"]
        .as_str()
        .unwrap()
        .contains("alice@example.com"));
    assert_eq!(forwarded_json["model"], "gpt-4o");
    assert!(response_body.contains("alice@example.com"));
}

#[tokio::test]
async fn http_proxy_plain_http_allowlist_bypasses_unknown_hosts() {
    let (upstream, mut captured_rx, upstream_handle) = spawn_mock_upstream().await;
    let state = http_proxy_test_state(HttpProxyConfig {
        allowed_hosts: vec!["api.openai.com".to_string()],
        ..HttpProxyConfig::default()
    });
    let (proxy_addr, proxy_handle) = spawn_proxy(state).await;

    let client = reqwest::Client::builder()
        .proxy(reqwest::Proxy::all(format!("http://{proxy_addr}")).unwrap())
        .build()
        .unwrap();

    let response = client
        .post(format!("{upstream}/v1/chat/completions"))
        .header("content-type", "text/plain")
        .body("alice@example.com")
        .send()
        .await
        .unwrap();

    let status = response.status();
    let response_body = response.text().await.unwrap();
    let captured = captured_rx.recv().await.unwrap();

    upstream_handle.abort();
    proxy_handle.abort();

    assert_eq!(status, reqwest::StatusCode::OK);
    assert_eq!(
        String::from_utf8(captured.body).unwrap(),
        "alice@example.com"
    );
    assert_eq!(response_body, "alice@example.com");
}

#[tokio::test]
async fn http_proxy_connect_tunnels_bytes_without_inspection() {
    let (echo_addr, echo_handle) = spawn_echo_server().await;
    let state = http_proxy_test_state(HttpProxyConfig::default());
    let (proxy_addr, proxy_handle) = spawn_proxy(state).await;

    let mut stream = TcpStream::connect(proxy_addr).await.unwrap();
    let connect_request = format!(
        "CONNECT {echo_addr} HTTP/1.1\r\nHost: {echo_addr}\r\nProxy-Connection: keep-alive\r\n\r\n"
    );
    stream.write_all(connect_request.as_bytes()).await.unwrap();

    let response_head = read_response_head(&mut stream).await;
    let response_head = String::from_utf8(response_head).unwrap();
    assert!(
        response_head.starts_with("HTTP/1.1 200"),
        "unexpected CONNECT response: {response_head}"
    );

    let payload = b"hello through the tunnel";
    stream.write_all(payload).await.unwrap();

    let mut echoed = vec![0_u8; payload.len()];
    stream.read_exact(&mut echoed).await.unwrap();

    echo_handle.abort();
    proxy_handle.abort();

    assert_eq!(echoed, payload);
}
