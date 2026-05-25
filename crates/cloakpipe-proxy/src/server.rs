//! HTTP server setup and router configuration.

use crate::{handlers, state::AppState, tree_handlers};
use axum::{
    routing::{get, post},
    Router,
};
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

/// Build the axum router with all routes and middleware.
pub fn build_router(state: Arc<AppState>) -> Router {
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
        .route(
            "/v1/chat/completions",
            post(handlers::proxy_chat_completions),
        )
        .route("/v1/embeddings", post(handlers::proxy_embeddings))
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
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// Start the proxy server.
pub async fn start(state: AppState) -> anyhow::Result<()> {
    let listen_addr = state.config.proxy.listen.clone();
    let state = Arc::new(state);

    let app = build_router(state);

    tracing::info!("CloakPipe proxy listening on {}", listen_addr);

    let listener = tokio::net::TcpListener::bind(&listen_addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
