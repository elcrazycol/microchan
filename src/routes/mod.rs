pub mod board;
pub mod catalog;
pub mod index;
pub mod moderation;
pub mod post;
pub mod report;
pub mod thread;

use axum::Router;

use crate::config::Config;
use crate::state::AppState;

/// Все роуты приложения.
pub fn router(config: &Config) -> Router<AppState> {
    let mod_base = match &config.moderation.mod_secret_url {
        Some(s) => format!("/mod/{s}"),
        None => "/mod".to_string(),
    };

    Router::new()
        .merge(index::router())
        .merge(board::router())
        .merge(thread::router())
        .merge(post::router())
        .merge(catalog::router())
        .merge(report::router())
        .merge(moderation::router(&mod_base))
}
