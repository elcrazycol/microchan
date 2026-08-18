use axum::extract::State;
use axum::routing::get;
use axum::Router;

use crate::state::AppState;
use crate::views::{BoardNav, IndexTemplate};

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(index))
}

async fn index(State(state): State<AppState>) -> IndexTemplate {
    IndexTemplate {
        boards: BoardNav::all(&state.config),
    }
}
