//! Provider route resolution for opt-in raw llm-http proxying.

use cloakpipe_core::config::ProxyConfig;

const OPENAI_PROVIDER: &str = "openai";
const ANTHROPIC_PROVIDER: &str = "anthropic";
const ANTHROPIC_UPSTREAM: &str = "https://api.anthropic.com";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderKind {
    OpenAi,
    Anthropic,
}

impl ProviderKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OpenAi => OPENAI_PROVIDER,
            Self::Anthropic => ANTHROPIC_PROVIDER,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedRoute {
    pub provider: ProviderKind,
    pub upstream_base: String,
    pub forward_path: String,
}

pub fn resolve_provider(path_and_query: &str, config: &ProxyConfig) -> Option<ResolvedRoute> {
    if let Some(remaining) = strip_prefix(path_and_query, "/anthropic") {
        return Some(ResolvedRoute {
            provider: ProviderKind::Anthropic,
            upstream_base: anthropic_upstream(config),
            forward_path: normalize_remaining_path(remaining),
        });
    }

    if let Some(remaining) = strip_prefix(path_and_query, "/openai") {
        return Some(ResolvedRoute {
            provider: ProviderKind::OpenAi,
            upstream_base: openai_upstream(config),
            forward_path: normalize_remaining_path(remaining),
        });
    }

    let openai_path =
        if path_and_query.starts_with("/v1/") || path_and_query.starts_with("/responses") {
            Some(path_and_query.to_string())
        } else if path_and_query.starts_with("/chat/completions")
            || path_and_query.starts_with("/completions")
            || path_and_query.starts_with("/embeddings")
            || path_and_query.starts_with("/models")
        {
            Some(format!("/v1{}", path_and_query))
        } else {
            None
        };

    openai_path.map(|forward_path| ResolvedRoute {
        provider: ProviderKind::OpenAi,
        upstream_base: openai_upstream(config),
        forward_path,
    })
}

fn provider_upstream(config: &ProxyConfig, provider: &str, default_upstream: &str) -> String {
    config
        .provider_routes
        .get(provider)
        .cloned()
        .unwrap_or_else(|| default_upstream.to_string())
}

fn openai_upstream(config: &ProxyConfig) -> String {
    config
        .provider_routes
        .get(OPENAI_PROVIDER)
        .cloned()
        .unwrap_or_else(|| config.upstream.clone())
}

fn anthropic_upstream(config: &ProxyConfig) -> String {
    provider_upstream(config, ANTHROPIC_PROVIDER, ANTHROPIC_UPSTREAM)
}

fn strip_prefix<'a>(path_and_query: &'a str, prefix: &str) -> Option<&'a str> {
    path_and_query.strip_prefix(prefix)
}

fn normalize_remaining_path(remaining: &str) -> String {
    if remaining.is_empty() {
        "/".to_string()
    } else {
        remaining.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{resolve_provider, ProviderKind};
    use cloakpipe_core::{
        config::{ProxyAuthMode, ProxyConfig, ProxyMode},
        MaskingStrategy,
    };
    use std::collections::HashMap;

    fn proxy_config() -> ProxyConfig {
        ProxyConfig {
            listen: "127.0.0.1:8900".into(),
            upstream: "https://api.openai.com".into(),
            api_key_env: "OPENAI_API_KEY".into(),
            timeout_seconds: 120,
            max_concurrent: 256,
            mode: ProxyMode::LlmHttp,
            dry_run: false,
            bypass: Vec::new(),
            auth_mode: ProxyAuthMode::PassThrough,
            provider_routes: HashMap::from([
                ("openai".to_string(), "http://openai.local".to_string()),
                (
                    "anthropic".to_string(),
                    "http://anthropic.local".to_string(),
                ),
            ]),
            http_proxy: Default::default(),
            masking_strategy: MaskingStrategy::Similar,
        }
    }

    #[test]
    fn resolves_anthropic_prefix_and_preserves_query() {
        let route = resolve_provider("/anthropic/v1/messages?beta=true", &proxy_config()).unwrap();

        assert_eq!(route.provider, ProviderKind::Anthropic);
        assert_eq!(route.upstream_base, "http://anthropic.local");
        assert_eq!(route.forward_path, "/v1/messages?beta=true");
    }

    #[test]
    fn resolves_openai_compatible_v1_path_without_prefix() {
        let route = resolve_provider("/v1/chat/completions", &proxy_config()).unwrap();

        assert_eq!(route.provider, ProviderKind::OpenAi);
        assert_eq!(route.upstream_base, "http://openai.local");
        assert_eq!(route.forward_path, "/v1/chat/completions");
    }

    #[test]
    fn resolves_openai_path_without_v1_prefix() {
        let route = resolve_provider("/chat/completions?stream=true", &proxy_config()).unwrap();

        assert_eq!(route.provider, ProviderKind::OpenAi);
        assert_eq!(route.forward_path, "/v1/chat/completions?stream=true");
    }

    #[test]
    fn rejects_unknown_paths() {
        assert!(resolve_provider("/unknown/provider/path", &proxy_config()).is_none());
    }
}
