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
use cloakpipe_proxy::{server::serve_http_proxy_listener, state::AppState, tls_mitm};
use http_body_util::BodyExt;
use hyper::{server::conn::http1, service::service_fn};
use hyper_util::rt::TokioIo;
use rcgen::{
    BasicConstraints, CertificateParams, DistinguishedName, DnType, ExtendedKeyUsagePurpose, IsCa,
    Issuer, KeyPair, KeyUsagePurpose,
};
use rustls::{
    pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer},
    ServerConfig,
};
use std::{collections::HashMap, net::SocketAddr, sync::Arc};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::mpsc,
};
use tokio_rustls::TlsAcceptor;
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

fn http_proxy_test_state_value(http_proxy: HttpProxyConfig) -> AppState {
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
            admin_token_env: "CLOAKPIPE_ADMIN_TOKEN".into(),
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

    AppState::new(config, detector, vault, audit, None)
}

fn http_proxy_test_state(http_proxy: HttpProxyConfig) -> Arc<AppState> {
    Arc::new(http_proxy_test_state_value(http_proxy))
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

async fn spawn_forward_proxy() -> (
    SocketAddr,
    mpsc::UnboundedReceiver<String>,
    tokio::task::JoinHandle<()>,
) {
    let (capture_tx, capture_rx) = mpsc::unbounded_channel();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        loop {
            let Ok((mut inbound, _)) = listener.accept().await else {
                break;
            };
            let capture_tx = capture_tx.clone();

            tokio::spawn(async move {
                let Ok(request_head) = read_http_head(&mut inbound).await else {
                    return;
                };
                let request_head = String::from_utf8_lossy(&request_head);
                let Some(target) = request_head
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .map(ToString::to_string)
                else {
                    let _ = inbound.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n").await;
                    return;
                };
                let _ = capture_tx.send(target.clone());

                match TcpStream::connect(&target).await {
                    Ok(mut upstream) => {
                        let _ = inbound
                            .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
                            .await;
                        let _ = tokio::io::copy_bidirectional(&mut inbound, &mut upstream).await;
                    }
                    Err(_) => {
                        let _ = inbound.write_all(b"HTTP/1.1 502 Bad Gateway\r\n\r\n").await;
                    }
                }
            });
        }
    });

    (address, capture_rx, handle)
}

struct TestHttpsUpstream {
    base_url: String,
    root_certificate: reqwest::Certificate,
    captured_rx: mpsc::UnboundedReceiver<CapturedRequest>,
    handle: tokio::task::JoinHandle<()>,
}

async fn spawn_mock_https_upstream() -> TestHttpsUpstream {
    let (capture_tx, capture_rx) = mpsc::unbounded_channel();
    let (ca_cert, leaf_cert, leaf_key) = generate_test_server_cert();
    let root_certificate = reqwest::Certificate::from_pem(ca_cert.pem().as_bytes()).unwrap();
    let cert_chain = vec![
        CertificateDer::from(leaf_cert),
        CertificateDer::from(ca_cert),
    ];
    let key: PrivateKeyDer<'static> = PrivatePkcs8KeyDer::from(leaf_key.serialize_der()).into();
    let tls_config = Arc::new(
        ServerConfig::builder_with_provider(rustls::crypto::ring::default_provider().into())
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(cert_chain, key)
            .unwrap(),
    );

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                break;
            };
            let tls_config = Arc::clone(&tls_config);
            let capture_tx = capture_tx.clone();

            tokio::spawn(async move {
                let Ok(tls_stream) = TlsAcceptor::from(tls_config).accept(stream).await else {
                    return;
                };
                let service = service_fn(move |request| {
                    let capture_tx = capture_tx.clone();
                    async move {
                        Ok::<_, std::convert::Infallible>(
                            mock_tls_upstream_handler(capture_tx, request).await,
                        )
                    }
                });
                let _ = http1::Builder::new()
                    .serve_connection(TokioIo::new(tls_stream), service)
                    .await;
            });
        }
    });

    TestHttpsUpstream {
        base_url: format!("https://{address}"),
        root_certificate,
        captured_rx: capture_rx,
        handle,
    }
}

async fn mock_tls_upstream_handler(
    capture_tx: mpsc::UnboundedSender<CapturedRequest>,
    request: hyper::Request<hyper::body::Incoming>,
) -> Response {
    let (parts, body) = request.into_parts();
    let body = body.collect().await.unwrap().to_bytes().to_vec();
    let path_and_query = parts
        .uri
        .path_and_query()
        .map(|value| value.as_str().to_string())
        .unwrap_or_else(|| parts.uri.path().to_string());

    capture_tx
        .send(CapturedRequest {
            method: parts.method,
            path_and_query,
            headers: parts.headers.clone(),
            body: body.clone(),
        })
        .unwrap();

    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap()
}

fn generate_test_server_cert() -> (rcgen::Certificate, rcgen::Certificate, KeyPair) {
    let ca_key = KeyPair::generate().unwrap();
    let mut ca_params = CertificateParams::default();
    ca_params.distinguished_name = DistinguishedName::new();
    ca_params
        .distinguished_name
        .push(DnType::CommonName, "CloakPipe Test Upstream CA");
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    ca_params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    let ca_cert = ca_params.self_signed(&ca_key).unwrap();

    let issuer = Issuer::from_params(&ca_params, &ca_key);
    let leaf_key = KeyPair::generate().unwrap();
    let mut leaf_params = CertificateParams::new(vec!["127.0.0.1".to_string()]).unwrap();
    leaf_params.distinguished_name = DistinguishedName::new();
    leaf_params
        .distinguished_name
        .push(DnType::CommonName, "127.0.0.1");
    leaf_params.is_ca = IsCa::NoCa;
    leaf_params.key_usages = vec![
        KeyUsagePurpose::DigitalSignature,
        KeyUsagePurpose::KeyEncipherment,
    ];
    leaf_params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    let leaf_cert = leaf_params.signed_by(&leaf_key, &issuer).unwrap();

    (ca_cert, leaf_cert, leaf_key)
}

async fn read_http_head(stream: &mut TcpStream) -> std::io::Result<Vec<u8>> {
    let mut response = Vec::new();
    let mut byte = [0_u8; 1];

    loop {
        stream.read_exact(&mut byte).await?;
        response.push(byte[0]);
        if response.ends_with(b"\r\n\r\n") {
            return Ok(response);
        }
        if response.len() > 8192 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "HTTP head exceeded 8 KiB",
            ));
        }
    }
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

#[tokio::test]
async fn http_proxy_connect_tunnels_through_forward_proxy() {
    let (echo_addr, echo_handle) = spawn_echo_server().await;
    let (forward_addr, mut forward_rx, forward_handle) = spawn_forward_proxy().await;
    let state = http_proxy_test_state(HttpProxyConfig {
        forward_proxy: Some(format!("http://{forward_addr}")),
        ..HttpProxyConfig::default()
    });
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

    let payload = b"hello through chained proxies";
    stream.write_all(payload).await.unwrap();

    let mut echoed = vec![0_u8; payload.len()];
    stream.read_exact(&mut echoed).await.unwrap();
    let captured_target = forward_rx.recv().await.unwrap();

    echo_handle.abort();
    forward_handle.abort();
    proxy_handle.abort();

    assert_eq!(captured_target, echo_addr.to_string());
    assert_eq!(echoed, payload);
}

#[tokio::test]
async fn http_proxy_https_mitm_mutates_request_and_rehydrates_response() {
    let temp_dir = tempfile::tempdir().unwrap();
    let http_proxy = HttpProxyConfig {
        inspect_https: true,
        allowed_hosts: vec!["127.0.0.1".to_string()],
        ca_cert_path: Some(
            temp_dir
                .path()
                .join("ca.crt")
                .to_string_lossy()
                .into_owned(),
        ),
        ca_key_path: Some(
            temp_dir
                .path()
                .join("ca.key")
                .to_string_lossy()
                .into_owned(),
        ),
        cert_cache_dir: Some(temp_dir.path().join("cache").to_string_lossy().into_owned()),
        ..HttpProxyConfig::default()
    };
    tls_mitm::ensure_root_ca(&http_proxy, false).unwrap();

    let mut upstream = spawn_mock_https_upstream().await;
    let mut state = http_proxy_test_state_value(http_proxy.clone());
    let upstream_client = reqwest::Client::builder()
        .add_root_certificate(upstream.root_certificate.clone())
        .build()
        .unwrap();
    state.http_client = upstream_client.clone();
    state.direct_http_client = upstream_client;
    let (proxy_addr, proxy_handle) = spawn_proxy(Arc::new(state)).await;

    let cloakpipe_ca = std::fs::read(http_proxy.ca_cert_path.as_ref().unwrap()).unwrap();
    let client = reqwest::Client::builder()
        .proxy(reqwest::Proxy::all(format!("http://{proxy_addr}")).unwrap())
        .add_root_certificate(reqwest::Certificate::from_pem(&cloakpipe_ca).unwrap())
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
        .post(format!("{}/v1/chat/completions", upstream.base_url))
        .header("authorization", "Bearer caller-token")
        .header("x-session-id", "https-mitm-sess")
        .header("content-type", "application/json")
        .body(request_body.to_string())
        .send()
        .await
        .unwrap();

    let status = response.status();
    let response_body = response.text().await.unwrap();
    let captured = upstream.captured_rx.recv().await.unwrap();

    upstream.handle.abort();
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
