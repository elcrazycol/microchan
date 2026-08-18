//! Создание тредов и ответов.

use axum::extract::{ConnectInfo, Path, State};
use axum::response::Redirect;
use axum::Form;
use axum::Router;
use serde::Deserialize;

use crate::error::AppError;
use crate::repo;
use crate::security;
use crate::state::AppState;
use crate::tripcode;

const MAX_BODY: usize = 8000;
const MAX_NAME: usize = 100;
const MAX_SUBJECT: usize = 100;
const MAX_EMAIL: usize = 100;

#[derive(Debug, Deserialize)]
pub struct PostForm {
    pub name: Option<String>,
    pub email: Option<String>,
    pub subject: Option<String>,
    pub body: Option<String>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/{board}/post", axum::routing::post(create_thread))
        .route(
            "/{board}/thread/{id}/reply",
            axum::routing::post(create_reply),
        )
}

async fn create_thread(
    State(state): State<AppState>,
    Path(board): Path<String>,
    ConnectInfo(connect): ConnectInfo<std::net::SocketAddr>,
    headers: axum::http::HeaderMap,
    Form(form): Form<PostForm>,
) -> Result<Redirect, AppError> {
    state.config.board(&board).ok_or(AppError::NotFound)?;

    let ip = security::client_ip(
        &headers,
        connect,
        &state.config.server.trusted_proxies,
    );
    let ip_hash = security::ip_hash(&ip, &state.config.security.secret);

    let (body, name, subject, email) = sanitize(&form)?;
    if body.is_empty() {
        return Err(AppError::bad_request("Comment is required"));
    }

    let (name, trips) = tripcode::split_name(&name);
    let tripcode = trips.render(state.config.security.secure_trip_salt());

    let thread_id = repo::create_thread(
        &state.pool,
        &board,
        &ip_hash,
        opt_str(name).as_deref(),
        tripcode.as_deref(),
        opt_str(email).as_deref(),
        opt_str(subject).as_deref(),
        &body,
    )
    .await?;

    Ok(Redirect::to(&format!("/{board}/thread/{thread_id}")))
}

async fn create_reply(
    State(state): State<AppState>,
    Path((board, thread_id)): Path<(String, i64)>,
    ConnectInfo(connect): ConnectInfo<std::net::SocketAddr>,
    headers: axum::http::HeaderMap,
    Form(form): Form<PostForm>,
) -> Result<Redirect, AppError> {
    let bcfg = state.config.board(&board).ok_or(AppError::NotFound)?;

    let ip = security::client_ip(
        &headers,
        connect,
        &state.config.server.trusted_proxies,
    );
    let ip_hash = security::ip_hash(&ip, &state.config.security.secret);

    let (body, name, _subject, email) = sanitize(&form)?;
    if body.is_empty() {
        return Err(AppError::bad_request("Comment is required"));
    }

    let (name, trips) = tripcode::split_name(&name);
    let tripcode = trips.render(state.config.security.secure_trip_salt());

    let post_id = repo::create_reply(
        &state.pool,
        &board,
        thread_id,
        &ip_hash,
        opt_str(name).as_deref(),
        tripcode.as_deref(),
        opt_str(email).as_deref(),
        &body,
        bcfg.bump_limit,
    )
    .await
    .map_err(|e| {
        let msg = e.to_string();
        if msg.contains("locked") {
            AppError::bad_request("Thread is locked")
        } else if msg.contains("not found") {
            AppError::NotFound
        } else {
            AppError::Internal(e)
        }
    })?;

    Ok(Redirect::to(&format!(
        "/{board}/thread/{thread_id}#p{post_id}"
    )))
}

/// Обрезает и нормализует поля формы.
fn sanitize(form: &PostForm) -> Result<(String, String, String, String), AppError> {
    let body = form
        .body
        .clone()
        .unwrap_or_default()
        .trim()
        .to_string();
    if body.len() > MAX_BODY {
        return Err(AppError::bad_request("Comment is too long"));
    }
    let name = truncate(form.name.as_deref(), MAX_NAME);
    let subject = truncate(form.subject.as_deref(), MAX_SUBJECT);
    let email = truncate(form.email.as_deref(), MAX_EMAIL);
    Ok((body, name, subject, email))
}

fn truncate(s: Option<&str>, max: usize) -> String {
    let mut s = s.unwrap_or("").trim().to_string();
    if s.chars().count() > max {
        s = s.chars().take(max).collect();
    }
    s
}

fn opt_str(s: String) -> Option<String> {
    if s.is_empty() { None } else { Some(s) }
}
