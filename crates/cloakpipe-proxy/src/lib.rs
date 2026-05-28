//! CloakPipe Proxy — OpenAI-compatible HTTP proxy with privacy middleware.
//!
//! Intercepts requests to LLM APIs, detects and pseudonymizes sensitive
//! entities in prompts, forwards the sanitized request, then rehydrates
//! the response before returning it to the caller.

pub mod handlers;
pub mod http_proxy;
pub mod json_filter;
pub mod llm_http;
pub mod outbound_proxy;
pub mod routing;
pub mod server;
pub mod state;
pub mod streaming;
pub mod tls_mitm;
pub mod tree_handlers;
