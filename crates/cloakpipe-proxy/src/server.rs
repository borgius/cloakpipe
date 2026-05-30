//! HTTP server setup and router configuration.

use crate::{admin, handlers, http_proxy, llm_proxy, state::AppState, tree_handlers};
use axum::{
    routing::any,
    routing::{get, post},
    Router,
};
use cloakpipe_core::config::ProxyMode;
use hyper::{server::conn::http1, service::service_fn};
use hyper_util::rt::TokioIo;
use std::sync::Arc;
use tokio::net::TcpListener;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

fn build_server_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/health", get(handlers::health))
        // Direct privacy endpoints (same request/response shapes as the MCP tools)
        .route("/pseudonymize", post(handlers::api_pseudonymize))
        .route("/rehydrate", post(handlers::api_rehydrate))
        .route("/detect", post(handlers::api_detect))
        .route(
            "/vault_stats",
            get(handlers::api_vault_stats).post(handlers::api_vault_stats),
        )
        .route("/configure", post(handlers::api_configure))
        .route("/session_context", post(handlers::api_session_context))
        .route("/v1/pseudonymize", post(handlers::api_pseudonymize))
        .route("/v1/rehydrate", post(handlers::api_rehydrate))
        .route("/v1/detect", post(handlers::api_detect))
        .route(
            "/v1/vault_stats",
            get(handlers::api_vault_stats).post(handlers::api_vault_stats),
        )
        .route("/v1/configure", post(handlers::api_configure))
        .route("/v1/session_context", post(handlers::api_session_context))
        // CloakTree endpoints
        .route("/tree/index", post(tree_handlers::tree_index_text))
        .route("/tree/index/file", post(tree_handlers::tree_index_file))
        .route("/tree/list", get(tree_handlers::tree_list))
        .route("/tree/query", post(tree_handlers::tree_query))
        .route(
            "/tree/{id}",
            get(tree_handlers::tree_get).delete(tree_handlers::tree_delete),
        )
        .route("/tree/{id}/search", post(tree_handlers::tree_search))
        // Session management endpoints
        .route(
            "/sessions",
            get(handlers::sessions_list).delete(handlers::sessions_flush_all),
        )
        .route(
            "/sessions/{id}",
            get(handlers::session_inspect).delete(handlers::session_flush),
        )
        // Self-hosted admin API (server mode only)
        .nest("/admin/api", admin::router())
}

fn build_llm_proxy_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/health", get(handlers::health))
        .route("/", any(llm_proxy::proxy_request))
        .route("/*path", any(llm_proxy::proxy_request))
}

/// Build the axum router with mode-specific routes and middleware.
pub fn build_router(state: Arc<AppState>) -> Router {
    let router = match state.config.proxy.mode {
        ProxyMode::Server => build_server_router(),
        ProxyMode::LlmProxy => build_llm_proxy_router(),
        ProxyMode::HttpProxy => Router::new()
            .route("/", any(http_proxy::proxy_request))
            .route("/*path", any(http_proxy::proxy_request))
            .fallback(any(http_proxy::proxy_request)),
    };

    router
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// Start the proxy server.
pub async fn start(state: AppState) -> anyhow::Result<()> {
    let listen_addr = state.config.proxy.listen.clone();
    tracing::info!("CloakPipe proxy listening on {}", listen_addr);

    let listener = TcpListener::bind(&listen_addr).await?;
    serve_listener(listener, state).await?;

    Ok(())
}

/// Serve an already-bound listener. Tests use this to keep port allocation race-free.
pub async fn serve_listener(listener: TcpListener, state: AppState) -> anyhow::Result<()> {
    let state = Arc::new(state);

    match state.config.proxy.mode {
        ProxyMode::HttpProxy => serve_http_proxy_listener(listener, state).await,
        ProxyMode::Server | ProxyMode::LlmProxy => {
            let app = build_router(state);
            axum::serve(listener, app).await?;
            Ok(())
        }
    }
}

/// Serve the explicit forward proxy with HTTP/1 upgrades enabled for CONNECT.
pub async fn serve_http_proxy_listener(
    listener: TcpListener,
    state: Arc<AppState>,
) -> anyhow::Result<()> {
    loop {
        let (stream, peer_addr) = listener.accept().await?;
        let state = state.clone();

        tokio::spawn(async move {
            let service = service_fn(move |request| {
                let state = state.clone();
                http_proxy::proxy_hyper_request(state, request)
            });

            if let Err(error) = http1::Builder::new()
                .serve_connection(TokioIo::new(stream), service)
                .with_upgrades()
                .await
            {
                tracing::debug!(peer = %peer_addr, "http-proxy connection ended with error: {error}");
            }
        });
    }
}
