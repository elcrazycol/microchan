pub mod index;

use axum::Router;

use crate::state::AppState;

/// Все роуты приложения.
pub fn router() -> Router<AppState> {
    Router::new().merge(index::router())
}
