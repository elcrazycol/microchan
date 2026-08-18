//! Публичная подача жалобы на пост.

use axum::extract::{ConnectInfo, Path, State};
use axum::http::HeaderMap;
use axum::response::Redirect;
use axum::Form;
use axum::Router;
use serde::Deserialize;

use crate::error::AppError;
use crate::repo;
use crate::security;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct ReportForm {
    pub reason: Option<String>,
    pub csrf: String,
}

pub fn router() -> Router<AppState> {
    Router::new().route(
        "/{board}/report/{post_id}",
        axum::routing::post(submit_report),
    )
}

async fn submit_report(
    State(state): State<AppState>,
    Path((board, post_id)): Path<(String, i64)>,
    ConnectInfo(connect): ConnectInfo<std::net::SocketAddr>,
    headers: HeaderMap,
    Form(form): Form<ReportForm>,
) -> Result<Redirect, AppError> {
    state.config.board(&board).ok_or(AppError::NotFound)?;

    if !security::csrf_valid(&headers, &form.csrf) {
        return Err(AppError::Forbidden);
    }

    let Some((post_board, thread_id)) = repo::post_location(&state.pool, post_id).await? else {
        return Err(AppError::NotFound);
    };
    if post_board != board {
        return Err(AppError::NotFound);
    }

    let ip = security::client_ip(&headers, connect, &state.config.server.trusted_proxies);
    let ip_hash = security::ip_hash(&ip, &state.config.security.secret);
    let reason = form
        .reason
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    repo::create_report(&state.pool, post_id, reason.as_deref(), &ip_hash).await?;

    Ok(Redirect::to(&format!("/{board}/thread/{thread_id}#p{post_id}")))
}
