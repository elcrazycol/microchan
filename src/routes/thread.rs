use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Redirect};
use axum::routing::get;
use axum::Router;

use crate::error::AppError;
use crate::repo;
use crate::security;
use crate::state::AppState;
use crate::views::{BoardNav, FileView, PostView, ThreadPageTemplate};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/{board}/thread/{id}", get(thread_page))
        .route("/{board}/thread/{id}/", get(thread_page))
}

async fn thread_page(
    State(state): State<AppState>,
    Path((board, id)): Path<(String, i64)>,
    headers: HeaderMap,
) -> Result<axum::response::Response, AppError> {
    let bcfg = state.config.board(&board).ok_or(AppError::NotFound)?;

    let Some(data) = repo::thread_data(&state.pool, &board, id).await? else {
        return Ok(Redirect::to(&format!("/{board}/")).into_response());
    };

    let files_of = |post_id: i64| -> Vec<FileView> {
        data.files
            .get(&post_id)
            .map(|v| v.iter().map(FileView::from_row).collect())
            .unwrap_or_default()
    };

    let reply_count = (data.thread.post_count - 1).max(0) as i64;
    let image_count: usize = data.files.values().map(|v| v.len()).sum();
    let counts = format!(
        "{} repl{}, {} img{}",
        reply_count,
        plural(reply_count),
        image_count,
        plural(image_count as i64)
    );

    let op = PostView::from_row(
        &data.op,
        files_of(data.op.id),
        data.thread.sticky,
        data.thread.locked,
        Some(counts),
    );
    let posts = data
        .posts
        .iter()
        .map(|p| PostView::from_row(p, files_of(p.id), false, false, None))
        .collect();

    let (csrf, is_new) = security::csrf_for_request(&headers);
    let template = ThreadPageTemplate {
        boards: BoardNav::all(&state.config),
        board: BoardNav::from_config(bcfg),
        op,
        posts,
        thread_id: data.thread.id,
        reply_url: format!("/{board}/thread/{}/reply", data.thread.id),
        csrf: csrf.clone(),
    };

    let mut resp = template.into_response();
    if is_new {
        resp.headers_mut()
            .insert("set-cookie", security::csrf_set_cookie(&csrf));
    }
    Ok(resp)
}

fn plural(n: i64) -> &'static str {
    if n == 1 { "" } else { "s" }
}
