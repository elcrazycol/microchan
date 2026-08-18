use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Redirect};
use axum::routing::get;
use axum::Router;

use crate::error::AppError;
use crate::repo;
use crate::security;
use crate::state::AppState;
use crate::views::{BoardNav, BoardTemplate, FileView, PostView, ThreadView};

const PER_PAGE: i64 = 10;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/{board}", get(board_page))
        .route("/{board}/", get(board_page))
        .route("/{board}/{page}", get(board_page_paged))
        .route("/{board}/{page}/", get(board_page_paged))
}

async fn board_page(
    State(state): State<AppState>,
    Path(board): Path<String>,
    headers: HeaderMap,
) -> Result<axum::response::Response, AppError> {
    render_board(&state, &board, 0, &headers).await
}

async fn board_page_paged(
    State(state): State<AppState>,
    Path((board, page)): Path<(String, i64)>,
    headers: HeaderMap,
) -> Result<axum::response::Response, AppError> {
    if page == 0 {
        return Ok(Redirect::to(&format!("/{board}/")).into_response());
    }
    render_board(&state, &board, page, &headers).await
}

async fn render_board(
    state: &AppState,
    board: &str,
    page: i64,
    headers: &HeaderMap,
) -> Result<axum::response::Response, AppError> {
    let bcfg = state.config.board(board).ok_or(AppError::NotFound)?;

    let (summaries, total_pages) =
        repo::board_threads(&state.pool, board, page, PER_PAGE).await?;

    let threads: Vec<ThreadView> = summaries
        .into_iter()
        .map(|s| {
            let reply_count = (s.post_count - 1).max(0) as i64;
            let counts = format!(
                "{} repl{}, {} img{}",
                reply_count,
                plural(reply_count),
                s.image_count,
                plural(s.image_count)
            );
            let files_of = |post_id: i64| -> Vec<FileView> {
                s.files
                    .get(&post_id)
                    .map(|v| v.iter().map(FileView::from_row).collect())
                    .unwrap_or_default()
            };
            let op = PostView::from_row(&s.op, files_of(s.op.id), s.sticky, s.locked, Some(counts));
            let replies = s
                .replies
                .iter()
                .map(|r| PostView::from_row(r, files_of(r.id), false, false, None))
                .collect();
            let omitted = (reply_count - s.replies.len() as i64).max(0);
            ThreadView {
                op,
                replies,
                omitted,
            }
        })
        .collect();

    let (csrf, is_new) = security::csrf_for_request(headers);
    let template = BoardTemplate {
        boards: BoardNav::all(&state.config),
        board: BoardNav::from_config(bcfg),
        threads,
        page,
        has_next: page + 1 < total_pages,
        post_url: format!("/{board}/post"),
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
