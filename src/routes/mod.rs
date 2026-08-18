pub mod board;
pub mod index;
pub mod post;
pub mod thread;

use axum::Router;

use crate::state::AppState;

/// Все роуты приложения.
pub fn router() -> Router<AppState> {
    Router::new()
        .merge(index::router())
        .merge(board::router())
        .merge(thread::router())
        .merge(post::router())
}
