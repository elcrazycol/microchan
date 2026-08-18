mod config;
mod db;
mod error;
mod state;

use std::net::SocketAddr;

use axum::routing::get;
use axum::Router;
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

use crate::state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,microchan=debug,tower_http=debug".into()),
        )
        .init();

    let config = config::Config::load()?;
    tracing::info!(
        "loaded config: {} board(s), data dir {:?}",
        config.boards.len(),
        config.server.data_dir
    );

    let pool = db::connect_and_migrate(&config).await?;
    let state = AppState::new(config.clone(), pool);

    let app = Router::new()
        .route("/health", get(handler_health))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", config.server.host, config.server.port).parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("listening on http://{addr}");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn handler_health() -> &'static str {
    "ok"
}
