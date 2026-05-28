//! Outbound proxy chaining for CloakPipe egress.
//!
//! This module intentionally reads only `proxy.http_proxy.forward_proxy` from
//! CloakPipe config. It must not inherit the process `HTTP_PROXY`/`HTTPS_PROXY`
//! variables, because those usually point the application at CloakPipe itself.

use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine};
use cloakpipe_core::config::HttpProxyConfig;
use reqwest::Url;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

const MAX_PROXY_RESPONSE_HEAD: usize = 16 * 1024;

#[derive(Debug, Clone)]
struct ParsedForwardProxy {
    address: String,
    authorization: Option<String>,
}

pub(crate) fn build_http_client(
    config: &cloakpipe_core::config::CloakPipeConfig,
    use_forward_proxy: bool,
) -> Result<reqwest::Client> {
    let mut builder = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(config.proxy.timeout_seconds));

    if use_forward_proxy {
        if let Some(proxy_url) =
            normalized_forward_proxy(config.proxy.http_proxy.forward_proxy.as_deref())
        {
            validate_forward_proxy_url(proxy_url)?;
            let proxy = reqwest::Proxy::all(proxy_url).with_context(|| {
                format!(
                    "Invalid proxy.http_proxy.forward_proxy: {}",
                    redact_proxy_url(proxy_url)
                )
            })?;
            builder = builder.proxy(proxy);
        }
    }

    builder.build().context("Failed to build HTTP client")
}

pub(crate) async fn connect_target(
    config: &HttpProxyConfig,
    target_host: &str,
    target_address: &str,
) -> Result<TcpStream> {
    let Some(proxy_url) = normalized_forward_proxy(config.forward_proxy.as_deref()) else {
        return TcpStream::connect(target_address)
            .await
            .with_context(|| format!("Failed to connect to {target_address}"));
    };

    if should_bypass_forward_proxy(config, target_host) {
        return TcpStream::connect(target_address)
            .await
            .with_context(|| format!("Failed to connect to {target_address}"));
    }

    let proxy = parse_forward_proxy(proxy_url)?;
    let mut stream = TcpStream::connect(&proxy.address).await.with_context(|| {
        format!(
            "Failed to connect to forward proxy {}",
            redact_proxy_url(proxy_url)
        )
    })?;

    let mut request = format!(
        "CONNECT {target_address} HTTP/1.1\r\nHost: {target_address}\r\nProxy-Connection: keep-alive\r\n"
    );
    if let Some(authorization) = proxy.authorization {
        request.push_str("Proxy-Authorization: ");
        request.push_str(&authorization);
        request.push_str("\r\n");
    }
    request.push_str("\r\n");

    stream
        .write_all(request.as_bytes())
        .await
        .with_context(|| {
            format!(
                "Failed to write CONNECT request to forward proxy {}",
                redact_proxy_url(proxy_url)
            )
        })?;

    let response_head = read_response_head(&mut stream).await.with_context(|| {
        format!(
            "Failed to read CONNECT response from forward proxy {}",
            redact_proxy_url(proxy_url)
        )
    })?;
    let response_head = String::from_utf8_lossy(&response_head);
    let status_line = response_head.lines().next().unwrap_or_default();
    if !is_successful_connect_status(status_line) {
        bail!(
            "Forward proxy {} rejected CONNECT to {}: {}",
            redact_proxy_url(proxy_url),
            target_address,
            status_line
        );
    }

    Ok(stream)
}

pub(crate) fn should_bypass_forward_proxy(config: &HttpProxyConfig, host: &str) -> bool {
    let host = normalize_host(host);
    config
        .forward_no_proxy
        .iter()
        .any(|pattern| host_matches_pattern(&host, pattern))
}

pub fn redact_proxy_url(proxy_url: &str) -> String {
    let Ok(url) = Url::parse(proxy_url) else {
        return proxy_url.to_string();
    };

    if url.username().is_empty() && url.password().is_none() {
        return proxy_url.to_string();
    }

    let host = url.host_str().unwrap_or_default();
    let port = url
        .port()
        .map(|port| format!(":{port}"))
        .unwrap_or_default();
    format!("{}://***@{}{}", url.scheme(), host, port)
}

pub fn validate_forward_proxy_url(proxy_url: &str) -> Result<()> {
    let url = Url::parse(proxy_url)
        .with_context(|| format!("Invalid forward proxy URL: {}", redact_proxy_url(proxy_url)))?;

    if url.scheme() != "http" {
        bail!(
            "Unsupported forward proxy scheme '{}'. Only http:// proxies are supported for chaining.",
            url.scheme()
        );
    }

    if url.host_str().is_none() {
        bail!("Forward proxy URL is missing a host");
    }

    Ok(())
}

fn normalized_forward_proxy(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn parse_forward_proxy(proxy_url: &str) -> Result<ParsedForwardProxy> {
    validate_forward_proxy_url(proxy_url)?;
    let url = Url::parse(proxy_url)?;
    let host = url.host_str().expect("validated forward proxy host");
    let port = url.port_or_known_default().unwrap_or(80);
    let address = if host.contains(':') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    };

    let authorization = if url.username().is_empty() && url.password().is_none() {
        None
    } else {
        let password = url.password().unwrap_or_default();
        let credentials = format!("{}:{password}", url.username());
        Some(format!("Basic {}", BASE64_STANDARD.encode(credentials)))
    };

    Ok(ParsedForwardProxy {
        address,
        authorization,
    })
}

async fn read_response_head(stream: &mut TcpStream) -> Result<Vec<u8>> {
    let mut response = Vec::new();
    let mut byte = [0_u8; 1];

    loop {
        let n = stream.read(&mut byte).await?;
        if n == 0 {
            bail!("forward proxy closed the connection before sending a response");
        }
        response.push(byte[0]);
        if response.ends_with(b"\r\n\r\n") {
            return Ok(response);
        }
        if response.len() > MAX_PROXY_RESPONSE_HEAD {
            bail!("forward proxy response head exceeded {MAX_PROXY_RESPONSE_HEAD} bytes");
        }
    }
}

fn is_successful_connect_status(status_line: &str) -> bool {
    let mut parts = status_line.split_whitespace();
    let Some(version) = parts.next() else {
        return false;
    };
    let Some(status) = parts.next() else {
        return false;
    };

    version.starts_with("HTTP/") && status.starts_with('2')
}

fn normalize_host(host: &str) -> String {
    host.trim().trim_matches(['[', ']']).to_ascii_lowercase()
}

fn host_matches_pattern(host: &str, pattern: &str) -> bool {
    let pattern = normalize_host(pattern);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_forward_proxy_credentials() {
        assert_eq!(
            redact_proxy_url("http://user:secret@corp.example.com:8080"),
            "http://***@corp.example.com:8080"
        );
    }

    #[test]
    fn validates_only_http_forward_proxy_urls() {
        assert!(validate_forward_proxy_url("http://corp.example.com:8080").is_ok());
        assert!(validate_forward_proxy_url("https://corp.example.com:8443").is_err());
        assert!(validate_forward_proxy_url("not a url").is_err());
    }

    #[test]
    fn parses_forward_proxy_address_and_auth_header() {
        let parsed = parse_forward_proxy("http://user:pass@127.0.0.1:8080").unwrap();

        assert_eq!(parsed.address, "127.0.0.1:8080");
        assert_eq!(parsed.authorization.as_deref(), Some("Basic dXNlcjpwYXNz"));
    }

    #[test]
    fn forward_no_proxy_matches_exact_and_suffix_patterns() {
        let config = HttpProxyConfig {
            forward_no_proxy: vec!["localhost".into(), "*.internal.example".into()],
            ..HttpProxyConfig::default()
        };

        assert!(should_bypass_forward_proxy(&config, "localhost"));
        assert!(should_bypass_forward_proxy(&config, "api.internal.example"));
        assert!(!should_bypass_forward_proxy(&config, "api.openai.com"));
    }
}
