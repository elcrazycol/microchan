//! Каталог доски и быстрый переход по номеру поста.

use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Redirect};
use axum::routing::get;
use axum::Router;
use serde::Deserialize;

use crate::error::AppError;
use crate::repo;
use crate::state::AppState;
use crate::views::{BoardNav, CatalogTemplate, CatalogThread};

#[derive(Debug, Deserialize)]
struct JumpQuery {
    n: Option<i64>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/{board}/catalog", get(catalog_page))
        .route("/{board}/catalog/", get(catalog_page))
        .route("/jump", get(jump_form))
        .route("/jump/{id}", get(jump_to_post))
        .route("/jump/{id}/", get(jump_to_post))
}

async fn catalog_page(
    State(state): State<AppState>,
    Path(board): Path<String>,
) -> Result<axum::response::Response, AppError> {
    let bcfg = state.config.board(&board).ok_or(AppError::NotFound)?;
    let items = repo::catalog(&state.pool, &board).await?;

    let threads: Vec<CatalogThread> = items
        .into_iter()
        .map(|i| CatalogThread {
            id: i.id,
            subject: i.subject.unwrap_or_default(),
            replies: i.reply_count,
            images: i.image_count,
            thumb_url: i
                .thumb_name
                .map(|t| format!("/files/{board}/thumb/{t}"))
                .unwrap_or_default(),
            sticky: i.sticky,
        })
        .collect();

    let template = CatalogTemplate {
        boards: BoardNav::all(&state.config),
        board: BoardNav::from_config(bcfg),
        threads,
    };

    Ok(template.into_response())
}

async fn jump_to_post(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Redirect, AppError> {
    redirect_to_post(&state, id).await
}

async fn jump_form(
    State(state): State<AppState>,
    Query(q): Query<JumpQuery>,
) -> Result<Redirect, AppError> {
    match q.n {
        Some(id) if id > 0 => redirect_to_post(&state, id).await,
        _ => Err(AppError::bad_request("Post number is required")),
    }
}

async fn redirect_to_post(state: &AppState, id: i64) -> Result<Redirect, AppError> {
    match repo::post_location(&state.pool, id).await? {
        Some((board, thread_id)) => {
            Ok(Redirect::to(&format!("/{board}/thread/{thread_id}#p{id}")))
        }
        None => Err(AppError::NotFound),
    }
}
