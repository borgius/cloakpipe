//! Explicit forward proxy support for `HTTP_PROXY` and `HTTPS_PROXY` clients.
//!
//! Plaintext HTTP requests use absolute-form URIs and can be inspected/mutated.
//! HTTPS requests arrive as CONNECT tunnels. By default they are relayed
//! byte-for-byte; when `inspect_https = true` and the host is a known or
//! allowlisted LLM endpoint, CloakPipe terminates TLS with a local CA-issued
//! leaf certificate, mutates the decrypted HTTP/1.1 request, and re-encrypts the
//! response to the client.

use crate::{llm_proxy, outbound_proxy, state::AppState, tls_mitm};
use axum::{
    body::Body,
    extract::State,
    http::{Method, Request, StatusCode, Uri},
    response::Response,
};
use cloakpipe_audit::AuditContext;
use cloakpipe_core::config::HttpProxyConfig;
use http_body_util::BodyExt;
use hyper::{body::Incoming, server::conn::http1, service::service_fn};
use hyper_util::rt::TokioIo;
use std::convert::Infallible;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio_rustls::TlsAcceptor;
use uuid::Uuid;

const KNOWN_LLM_HOST_PATTERNS: &[&str] = &[
    "api.openai.com",
    "api.anthropic.com",
    "generativelanguage.googleapis.com",
    "api.mistral.ai",
    "api.groq.com",
    "api.cohere.com",
    "api.together.xyz",
    "openrouter.ai",
    "*.openai.azure.com",
];

#[derive(Debug)]
struct ConnectTarget {
    host: String,
    port: u16,
    address: String,
}

/// Catch-all handler used only when `proxy.mode = "http-proxy"`.
pub async fn proxy_request(
    State(state): State<Arc<AppState>>,
    request: Request<Body>,
) -> Result<Response, (StatusCode, String)> {
    handle_request(state, request).await
}

/// Hyper service entry point used by the raw `http-proxy` server path.
pub async fn proxy_hyper_request(
    state: Arc<AppState>,
    request: Request<Incoming>,
) -> Result<Response, Infallible> {
    let request = request.map(Body::new);
    let response = match handle_request(state, request).await {
        Ok(response) => response,
        Err((status, message)) => error_response(status, message),
    };

    Ok(response)
}

async fn handle_request(
    state: Arc<AppState>,
    request: Request<Body>,
) -> Result<Response, (StatusCode, String)> {
    if request.method() == Method::CONNECT {
        return handle_connect(state, request).await;
    }

    handle_plain_http(state, request).await
}

fn error_response(status: StatusCode, message: String) -> Response {
    Response::builder()
        .status(status)
        .header("content-type", "text/plain; charset=utf-8")
        .body(Body::from(message))
        .unwrap_or_else(|_| Response::new(Body::empty()))
}

async fn handle_plain_http(
    state: Arc<AppState>,
    request: Request<Body>,
) -> Result<Response, (StatusCode, String)> {
    let request_id = Uuid::new_v4().to_string();
    let (parts, body) = request.into_parts();

    if is_local_health_request(&parts.uri) {
        return health_response();
    }

    let target_url = absolute_http_target(&parts.uri)?;
    let host = parts
        .uri
        .authority()
        .map(|authority| authority.host())
        .unwrap_or_default();
    let inspect_host = should_inspect_plain_http_host(&state.config.proxy.http_proxy, host);

    let body_bytes = body
        .collect()
        .await
        .map_err(|error| {
            (
                StatusCode::BAD_REQUEST,
                format!("Failed to read proxied request body: {error}"),
            )
        })?
        .to_bytes();

    let session_id = llm_proxy::extract_session_id(&state, &parts.headers);
    let should_bypass = state.should_bypass_upstream(&target_url) || !inspect_host;
    let prepared = llm_proxy::prepare_request_body(
        &state,
        &parts.headers,
        &body_bytes,
        session_id.as_deref(),
        should_bypass,
        &request_id,
    )
    .await?;

    if prepared.entities_detected > 0 {
        let _ = state.audit.log_pseudonymize(
            AuditContext::new("proxy", &request_id).with_session(session_id.as_deref()),
            prepared.entities_detected,
            prepared.entities_detected,
            prepared.categories.clone(),
        );
    }

    let method =
        reqwest::Method::from_bytes(parts.method.as_str().as_bytes()).map_err(|error| {
            (
                StatusCode::BAD_REQUEST,
                format!("Unsupported HTTP method: {error}"),
            )
        })?;

    let request_builder = llm_proxy::build_upstream_request(
        &state,
        method,
        &target_url,
        &parts.headers,
        prepared.body,
    )?;

    let upstream_response = request_builder.send().await.map_err(|error| {
        let _ = state.audit.log_error(
            AuditContext::new("proxy", &request_id).with_session(session_id.as_deref()),
            &format!("Forward-proxy upstream request failed: {error}"),
        );
        (
            StatusCode::BAD_GATEWAY,
            format!("Forward-proxy upstream request failed: {error}"),
        )
    })?;

    let skip_rehydration = should_bypass || state.config.proxy.dry_run;
    llm_proxy::build_upstream_response(
        &state,
        upstream_response,
        skip_rehydration,
        &request_id,
        session_id.as_deref(),
    )
    .await
}

async fn handle_connect(
    state: Arc<AppState>,
    mut request: Request<Body>,
) -> Result<Response, (StatusCode, String)> {
    let target = connect_target_from_uri(request.uri())?;
    let should_mitm = should_mitm_connect_target(&state, &target);
    let host_allowed =
        should_mitm || matches_allowed_host(&state.config.proxy.http_proxy, &target.host);

    if should_mitm {
        return handle_connect_with_mitm(state, request, target).await;
    }

    if !host_allowed && !state.config.proxy.http_proxy.tunnel_unknown_hosts {
        return Err((
            StatusCode::FORBIDDEN,
            format!(
                "CONNECT target '{}' is not in proxy.http_proxy.allowed_hosts",
                target.host
            ),
        ));
    }

    let on_upgrade = hyper::upgrade::on(&mut request);
    let tunnel_state = Arc::clone(&state);
    tokio::spawn(async move {
        match on_upgrade.await {
            Ok(upgraded) => {
                let mut client = TokioIo::new(upgraded);
                match outbound_proxy::connect_target(
                    &tunnel_state.config.proxy.http_proxy,
                    &target.host,
                    &target.address,
                )
                .await
                {
                    Ok(mut upstream) => {
                        if let Err(error) =
                            tokio::io::copy_bidirectional(&mut client, &mut upstream).await
                        {
                            tracing::debug!(target = %target.address, "CONNECT tunnel closed with error: {error}");
                        }
                    }
                    Err(error) => {
                        tracing::warn!(target = %target.address, "Failed to connect CONNECT upstream: {error}");
                        let _ = client.shutdown().await;
                    }
                }
            }
            Err(error) => {
                tracing::warn!("Failed to upgrade CONNECT request: {error}");
            }
        }
    });

    Response::builder()
        .status(StatusCode::OK)
        .body(Body::empty())
        .map_err(|error| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to build CONNECT response: {error}"),
            )
        })
}

async fn handle_connect_with_mitm(
    state: Arc<AppState>,
    mut request: Request<Body>,
    target: ConnectTarget,
) -> Result<Response, (StatusCode, String)> {
    let tls_config = tls_mitm::server_config_for_host(&state.config.proxy.http_proxy, &target.host)
        .map_err(|error| {
            (
                StatusCode::BAD_GATEWAY,
                format!("HTTPS inspection is not ready for {}: {error}", target.host),
            )
        })?;

    let on_upgrade = hyper::upgrade::on(&mut request);
    let target_host = target.host.clone();
    let target_authority = target.url_authority();
    let tunnel_state = Arc::clone(&state);

    tokio::spawn(async move {
        match on_upgrade.await {
            Ok(upgraded) => {
                let client = TokioIo::new(upgraded);
                let acceptor = TlsAcceptor::from(tls_config);
                match acceptor.accept(client).await {
                    Ok(tls_stream) => {
                        let log_target_authority = target_authority.clone();
                        let service = service_fn(move |request| {
                            let state = Arc::clone(&tunnel_state);
                            let target_host = target_host.clone();
                            let target_authority = target_authority.clone();
                            async move {
                                let response = match handle_mitm_http_request(
                                    state,
                                    target_host,
                                    target_authority,
                                    request,
                                )
                                .await
                                {
                                    Ok(response) => response,
                                    Err((status, message)) => error_response(status, message),
                                };
                                Ok::<_, Infallible>(response)
                            }
                        });

                        if let Err(error) = http1::Builder::new()
                            .serve_connection(TokioIo::new(tls_stream), service)
                            .await
                        {
                            tracing::debug!(target = %log_target_authority, "MITM HTTP/1 connection ended with error: {error}");
                        }
                    }
                    Err(error) => {
                        tracing::warn!(target = %target_authority, "Failed to accept client TLS for MITM CONNECT: {error}");
                    }
                }
            }
            Err(error) => {
                tracing::warn!(target = %target_authority, "Failed to upgrade MITM CONNECT request: {error}");
            }
        }
    });

    Response::builder()
        .status(StatusCode::OK)
        .body(Body::empty())
        .map_err(|error| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to build CONNECT response: {error}"),
            )
        })
}

async fn handle_mitm_http_request(
    state: Arc<AppState>,
    target_host: String,
    target_authority: String,
    request: Request<Incoming>,
) -> Result<Response, (StatusCode, String)> {
    if request.method() == Method::CONNECT {
        return Err((
            StatusCode::BAD_REQUEST,
            "Nested CONNECT requests are not supported inside HTTPS inspection tunnels".into(),
        ));
    }

    let request_id = Uuid::new_v4().to_string();
    let (parts, body) = request.into_parts();
    let target_url = mitm_target_url(&target_authority, &parts.uri)?;
    let body_bytes = body
        .collect()
        .await
        .map_err(|error| {
            (
                StatusCode::BAD_REQUEST,
                format!("Failed to read MITM request body: {error}"),
            )
        })?
        .to_bytes();

    let session_id = llm_proxy::extract_session_id(&state, &parts.headers);
    let should_bypass = state.should_bypass_upstream(&target_url);
    let prepared = llm_proxy::prepare_request_body(
        &state,
        &parts.headers,
        &body_bytes,
        session_id.as_deref(),
        should_bypass,
        &request_id,
    )
    .await?;

    if prepared.entities_detected > 0 {
        let _ = state.audit.log_pseudonymize(
            AuditContext::new("proxy", &request_id).with_session(session_id.as_deref()),
            prepared.entities_detected,
            prepared.entities_detected,
            prepared.categories.clone(),
        );
    }

    let method =
        reqwest::Method::from_bytes(parts.method.as_str().as_bytes()).map_err(|error| {
            (
                StatusCode::BAD_REQUEST,
                format!("Unsupported HTTP method: {error}"),
            )
        })?;

    let request_builder = llm_proxy::build_upstream_request(
        &state,
        method,
        &target_url,
        &parts.headers,
        prepared.body,
    )?;

    let upstream_response = request_builder.send().await.map_err(|error| {
        let _ = state.audit.log_error(
            AuditContext::new("proxy", &request_id).with_session(session_id.as_deref()),
            &format!("MITM upstream request failed for {target_host}: {error}"),
        );
        (
            StatusCode::BAD_GATEWAY,
            format!("MITM upstream request failed for {target_host}: {error}"),
        )
    })?;

    let skip_rehydration = should_bypass || state.config.proxy.dry_run;
    llm_proxy::build_upstream_response(
        &state,
        upstream_response,
        skip_rehydration,
        &request_id,
        session_id.as_deref(),
    )
    .await
}

fn absolute_http_target(uri: &Uri) -> Result<String, (StatusCode, String)> {
    let Some(scheme) = uri.scheme_str() else {
        return Err((
            StatusCode::BAD_REQUEST,
            "Forward proxy requests must use absolute-form HTTP URIs or CONNECT".into(),
        ));
    };

    if scheme != "http" {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("Unsupported forward-proxy scheme '{scheme}'; use CONNECT for HTTPS"),
        ));
    }

    if uri.authority().is_none() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Forward proxy request is missing a target authority".into(),
        ));
    }

    Ok(uri.to_string())
}

fn connect_target_from_uri(uri: &Uri) -> Result<ConnectTarget, (StatusCode, String)> {
    if uri.scheme().is_some() {
        return Err((
            StatusCode::BAD_REQUEST,
            "CONNECT requests must use authority-form targets such as api.openai.com:443".into(),
        ));
    }

    let Some(authority) = uri.authority() else {
        return Err((
            StatusCode::BAD_REQUEST,
            "CONNECT request is missing a target authority".into(),
        ));
    };

    let host = authority
        .host()
        .trim_matches(['[', ']'])
        .to_ascii_lowercase();
    if host.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "CONNECT target host is empty".into(),
        ));
    }

    let port = authority.port_u16().unwrap_or(443);
    if port == 0 {
        return Err((
            StatusCode::BAD_REQUEST,
            "CONNECT target port must be greater than zero".into(),
        ));
    }

    let address = if host.contains(':') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    };

    Ok(ConnectTarget {
        host,
        port,
        address,
    })
}

fn is_local_health_request(uri: &Uri) -> bool {
    uri.scheme().is_none() && uri.authority().is_none() && uri.path() == "/health"
}

fn health_response() -> Result<Response, (StatusCode, String)> {
    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/json")
        .body(Body::from(r#"{"status":"ok","service":"cloakpipe"}"#))
        .map_err(|error| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to build health response: {error}"),
            )
        })
}

fn should_inspect_plain_http_host(config: &HttpProxyConfig, host: &str) -> bool {
    config.allowed_hosts.is_empty() || matches_allowed_host(config, host)
}

fn should_mitm_connect_target(state: &AppState, target: &ConnectTarget) -> bool {
    let config = &state.config.proxy.http_proxy;
    if !config.inspect_https {
        return false;
    }

    if state.should_bypass_upstream(&target.host) || state.should_bypass_upstream(&target.address) {
        return false;
    }

    matches_known_llm_host(&target.host) || matches_allowed_host(config, &target.host)
}

fn matches_known_llm_host(host: &str) -> bool {
    let host = host.trim_matches(['[', ']']).to_ascii_lowercase();
    KNOWN_LLM_HOST_PATTERNS
        .iter()
        .any(|pattern| host_matches_pattern(&host, pattern))
}

fn matches_allowed_host(config: &HttpProxyConfig, host: &str) -> bool {
    let host = host.trim_matches(['[', ']']).to_ascii_lowercase();
    config
        .allowed_hosts
        .iter()
        .any(|pattern| host_matches_pattern(&host, pattern))
}

fn host_matches_pattern(host: &str, pattern: &str) -> bool {
    let pattern = pattern.trim().trim_matches(['[', ']']).to_ascii_lowercase();
    if pattern.is_empty() {
        return false;
    }

    if pattern == "*" {
        return true;
    }

    let suffix_pattern = pattern
        .strip_prefix("*.")
        .or_else(|| pattern.strip_prefix('.'));

    if let Some(domain) = suffix_pattern {
        return host == domain || host.ends_with(&format!(".{domain}"));
    }

    host == pattern
}

fn mitm_target_url(authority: &str, uri: &Uri) -> Result<String, (StatusCode, String)> {
    if let Some(scheme) = uri.scheme_str() {
        if scheme != "https" {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("Unsupported MITM request scheme '{scheme}'; expected https"),
            ));
        }
        if uri.authority().is_none() {
            return Err((
                StatusCode::BAD_REQUEST,
                "Absolute-form HTTPS request is missing an authority".into(),
            ));
        }
        let uri_authority = uri.authority().expect("checked absolute URI authority");
        if !https_authorities_equivalent(authority, uri_authority.as_str()) {
            return Err((
                StatusCode::BAD_REQUEST,
                format!(
                    "MITM request authority '{}' does not match CONNECT target '{}'",
                    uri_authority, authority
                ),
            ));
        }
        return Ok(uri.to_string());
    }

    let path_and_query = uri
        .path_and_query()
        .map(|value| value.as_str())
        .unwrap_or_else(|| uri.path());
    Ok(format!("https://{authority}{path_and_query}"))
}

fn https_authorities_equivalent(expected: &str, actual: &str) -> bool {
    let Ok(expected_uri) = format!("https://{expected}/").parse::<Uri>() else {
        return false;
    };
    let Ok(actual_uri) = format!("https://{actual}/").parse::<Uri>() else {
        return false;
    };

    let Some(expected_authority) = expected_uri.authority() else {
        return false;
    };
    let Some(actual_authority) = actual_uri.authority() else {
        return false;
    };

    expected_authority
        .host()
        .trim_matches(['[', ']'])
        .eq_ignore_ascii_case(actual_authority.host().trim_matches(['[', ']']))
        && expected_authority.port_u16().unwrap_or(443)
            == actual_authority.port_u16().unwrap_or(443)
}

impl ConnectTarget {
    fn url_authority(&self) -> String {
        let host = if self.host.contains(':') {
            format!("[{}]", self.host)
        } else {
            self.host.clone()
        };

        if self.port == 443 {
            host
        } else {
            format!("{host}:{}", self.port)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absolute_http_target_accepts_absolute_http_uri() {
        let uri: Uri = "http://api.openai.test/v1/chat/completions?x=1"
            .parse()
            .unwrap();

        assert_eq!(
            absolute_http_target(&uri).unwrap(),
            "http://api.openai.test/v1/chat/completions?x=1"
        );
    }

    #[test]
    fn absolute_http_target_rejects_origin_form_uri() {
        let uri: Uri = "/v1/chat/completions".parse().unwrap();

        assert_eq!(
            absolute_http_target(&uri).unwrap_err().0,
            StatusCode::BAD_REQUEST
        );
    }

    #[test]
    fn absolute_http_target_rejects_https_without_connect() {
        let uri: Uri = "https://api.openai.test/v1/chat/completions"
            .parse()
            .unwrap();

        assert_eq!(
            absolute_http_target(&uri).unwrap_err().0,
            StatusCode::BAD_REQUEST
        );
    }

    #[test]
    fn connect_target_defaults_to_https_port() {
        let uri: Uri = "api.openai.test".parse().unwrap();
        let target = connect_target_from_uri(&uri).unwrap();

        assert_eq!(target.host, "api.openai.test");
        assert_eq!(target.address, "api.openai.test:443");
    }

    #[test]
    fn connect_target_accepts_explicit_port() {
        let uri: Uri = "127.0.0.1:9443".parse().unwrap();
        let target = connect_target_from_uri(&uri).unwrap();

        assert_eq!(target.host, "127.0.0.1");
        assert_eq!(target.address, "127.0.0.1:9443");
    }

    #[test]
    fn connect_target_rejects_absolute_uri() {
        let uri: Uri = "https://api.openai.test:443".parse().unwrap();

        assert_eq!(
            connect_target_from_uri(&uri).unwrap_err().0,
            StatusCode::BAD_REQUEST
        );
    }

    #[test]
    fn host_matching_supports_exact_and_suffix_patterns() {
        assert!(host_matches_pattern("api.openai.com", "api.openai.com"));
        assert!(host_matches_pattern("api.openai.com", "*.openai.com"));
        assert!(host_matches_pattern("api.openai.com", ".openai.com"));
        assert!(!host_matches_pattern("evilopenai.com", "*.openai.com"));
    }

    #[test]
    fn plain_http_inspection_allows_all_hosts_when_allowlist_is_empty() {
        let config = HttpProxyConfig::default();

        assert!(should_inspect_plain_http_host(&config, "example.com"));
    }

    #[test]
    fn plain_http_inspection_honors_allowlist_when_configured() {
        let config = HttpProxyConfig {
            allowed_hosts: vec!["*.openai.com".to_string()],
            ..HttpProxyConfig::default()
        };

        assert!(should_inspect_plain_http_host(&config, "api.openai.com"));
        assert!(!should_inspect_plain_http_host(&config, "example.com"));
    }

    #[test]
    fn known_llm_host_matching_supports_builtins() {
        assert!(matches_known_llm_host("api.openai.com"));
        assert!(matches_known_llm_host("tenant.openai.azure.com"));
        assert!(!matches_known_llm_host("example.com"));
    }

    #[test]
    fn mitm_target_url_builds_https_url_from_origin_form() {
        let uri: Uri = "/v1/chat/completions?x=1".parse().unwrap();

        assert_eq!(
            mitm_target_url("api.openai.test", &uri).unwrap(),
            "https://api.openai.test/v1/chat/completions?x=1"
        );
    }

    #[test]
    fn mitm_target_url_accepts_absolute_https_uri() {
        let uri: Uri = "https://api.openai.test:443/v1/chat/completions"
            .parse()
            .unwrap();

        assert_eq!(
            mitm_target_url("api.openai.test", &uri).unwrap(),
            "https://api.openai.test:443/v1/chat/completions"
        );
    }

    #[test]
    fn mitm_target_url_rejects_mismatched_absolute_authority() {
        let uri: Uri = "https://example.com/v1/chat/completions".parse().unwrap();

        assert_eq!(
            mitm_target_url("api.openai.test", &uri).unwrap_err().0,
            StatusCode::BAD_REQUEST
        );
    }
}
