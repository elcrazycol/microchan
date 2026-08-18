//! Создание тредов и ответов (multipart: текст + файлы).

use axum::extract::{ConnectInfo, DefaultBodyLimit, Multipart, Path, State};
use axum::http::HeaderMap;
use axum::response::Redirect;
use axum::Router;

use crate::config::BoardConfig;
use crate::error::AppError;
use crate::media;
use crate::repo;
use crate::security;
use crate::state::AppState;
use crate::tripcode;

const MAX_BODY: usize = 8000;
const MAX_NAME: usize = 100;
const MAX_SUBJECT: usize = 100;
const MAX_EMAIL: usize = 100;
/// Общий лимит тела запроса (файлы + поля). Точный лимит файлов — per-board.
const MAX_UPLOAD: usize = 64 * 1024 * 1024;

#[derive(Default)]
struct Fields {
    name: String,
    email: String,
    subject: String,
    body: String,
    spoiler: bool,
    csrf: String,
}

struct Parsed {
    fields: Fields,
    /// (имя файла, байты)
    files: Vec<(String, Vec<u8>)>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/{board}/post", axum::routing::post(create_thread))
        .route(
            "/{board}/thread/{id}/reply",
            axum::routing::post(create_reply),
        )
        .layer(DefaultBodyLimit::max(MAX_UPLOAD))
}

async fn create_thread(
    State(state): State<AppState>,
    Path(board): Path<String>,
    ConnectInfo(connect): ConnectInfo<std::net::SocketAddr>,
    headers: HeaderMap,
    multipart: Multipart,
) -> Result<Redirect, AppError> {
    let bcfg = state.config.board(&board).ok_or(AppError::NotFound)?;
    let (parsed, ip_hash) = guard_and_parse(&state, &headers, connect, multipart).await?;
    if repo::is_ip_banned(&state.pool, &ip_hash).await? {
        return Err(AppError::Banned);
    }

    let (body, name, subject, email) = sanitize(&parsed.fields)?;
    if body.is_empty() && parsed.files.is_empty() {
        return Err(AppError::bad_request("Comment or file is required"));
    }

    let stored = store_files(&state, &board, bcfg, &parsed).await?;

    let (name, trips) = tripcode::split_name(&name);
    let tripcode = trips.render(state.config.security.secure_trip_salt());

    let result = repo::create_thread(
        &state.pool,
        &board,
        &ip_hash,
        opt_str(name).as_deref(),
        tripcode.as_deref(),
        opt_str(email).as_deref(),
        opt_str(subject).as_deref(),
        &body,
        &stored,
    )
    .await;

    match result {
        Ok(thread_id) => {
            let _ = repo::prune_board(
                &state.pool,
                &board,
                bcfg.thread_limit,
                bcfg.max_thread_age_days,
            )
            .await;
            Ok(Redirect::to(&format!("/{board}/thread/{thread_id}")))
        }
        Err(e) => {
            media::cleanup(&stored, &board, &state.config.server.data_dir).await;
            Err(AppError::Internal(e))
        }
    }
}

async fn create_reply(
    State(state): State<AppState>,
    Path((board, thread_id)): Path<(String, i64)>,
    ConnectInfo(connect): ConnectInfo<std::net::SocketAddr>,
    headers: HeaderMap,
    multipart: Multipart,
) -> Result<Redirect, AppError> {
    let bcfg = state.config.board(&board).ok_or(AppError::NotFound)?;
    let (parsed, ip_hash) = guard_and_parse(&state, &headers, connect, multipart).await?;
    if repo::is_ip_banned(&state.pool, &ip_hash).await? {
        return Err(AppError::Banned);
    }

    let (body, name, _subject, email) = sanitize(&parsed.fields)?;
    if body.is_empty() && parsed.files.is_empty() {
        return Err(AppError::bad_request("Comment or file is required"));
    }

    let stored = store_files(&state, &board, bcfg, &parsed).await?;

    let (name, trips) = tripcode::split_name(&name);
    let tripcode = trips.render(state.config.security.secure_trip_salt());

    let result = repo::create_reply(
        &state.pool,
        &board,
        thread_id,
        &ip_hash,
        opt_str(name).as_deref(),
        tripcode.as_deref(),
        opt_str(email).as_deref(),
        &body,
        bcfg.bump_limit,
        &stored,
    )
    .await;

    match result {
        Ok(post_id) => Ok(Redirect::to(&format!(
            "/{board}/thread/{thread_id}#p{post_id}"
        ))),
        Err(e) => {
            media::cleanup(&stored, &board, &state.config.server.data_dir).await;
            let msg = e.to_string();
            if msg.contains("locked") {
                Err(AppError::bad_request("Thread is locked"))
            } else if msg.contains("not found") {
                Err(AppError::NotFound)
            } else {
                Err(AppError::Internal(e))
            }
        }
    }
}

/// Rate-limit + CSRF-проверка + разбор multipart. Возвращает (parsed, ip_hash).
async fn guard_and_parse(
    state: &AppState,
    headers: &HeaderMap,
    connect: std::net::SocketAddr,
    multipart: Multipart,
) -> Result<(Parsed, String), AppError> {
    let ip = security::client_ip(headers, connect, &state.config.server.trusted_proxies);
    let ip_hash = security::ip_hash(&ip, &state.config.security.secret);

    if !state
        .rate_limiter
        .allow(&ip_hash, state.config.security.max_posts_per_minute)
    {
        return Err(AppError::RateLimited);
    }

    if security::csrf_from_cookie(headers).is_none() {
        return Err(AppError::Forbidden);
    }

    let parsed = parse_multipart(multipart).await?;

    if !security::csrf_valid(headers, &parsed.fields.csrf) {
        return Err(AppError::Forbidden);
    }

    Ok((parsed, ip_hash))
}

async fn parse_multipart(multipart: Multipart) -> Result<Parsed, AppError> {
    let mut fields = Fields::default();
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();

    let mut mp = multipart;
    while let Some(field) = mp
        .next_field()
        .await
        .map_err(|e| AppError::bad_request(format!("Bad upload: {e}")))?
    {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "name" => fields.name = read_text(field).await?,
            "email" => fields.email = read_text(field).await?,
            "subject" => fields.subject = read_text(field).await?,
            "body" => fields.body = read_text(field).await?,
            "csrf" => fields.csrf = read_text(field).await?,
            "spoiler" => {
                let v = read_text(field).await?;
                if matches!(v.as_str(), "on" | "1" | "true" | "yes") {
                    fields.spoiler = true;
                }
            }
            "file" => {
                let filename = field.file_name().unwrap_or("").to_string();
                if !filename.is_empty() {
                    let data = field
                        .bytes()
                        .await
                        .map_err(|e| AppError::bad_request(format!("Bad upload: {e}")))?
                        .to_vec();
                    files.push((filename, data));
                }
            }
            _ => {}
        }
    }

    Ok(Parsed { fields, files })
}

/// Валидирует и сохраняет файлы поста на диск.
async fn store_files(
    state: &AppState,
    board: &str,
    bcfg: &BoardConfig,
    parsed: &Parsed,
) -> Result<Vec<media::Stored>, AppError> {
    if parsed.files.len() > bcfg.max_images {
        return Err(AppError::bad_request(format!(
            "Too many files (max {})",
            bcfg.max_images
        )));
    }

    let mut stored = Vec::with_capacity(parsed.files.len());
    for (filename, data) in &parsed.files {
        let validated = media::validate(data.clone(), filename, bcfg, parsed.fields.spoiler)?;
        if repo::is_file_banned(&state.pool, &validated.sha256).await? {
            media::cleanup(&stored, board, &state.config.server.data_dir).await;
            return Err(AppError::Banned);
        }
        let s = media::store(
            &validated,
            board,
            &state.config.server.data_dir,
            &state.config.media,
        )
        .await?;
        stored.push(s);
    }

    Ok(stored)
}

async fn read_text(field: axum::extract::multipart::Field<'_>) -> Result<String, AppError> {
    field
        .text()
        .await
        .map_err(|e| AppError::bad_request(format!("Bad field: {e}")))
}

/// Обрезает и нормализует поля формы.
fn sanitize(fields: &Fields) -> Result<(String, String, String, String), AppError> {
    let body = fields.body.trim().to_string();
    if body.len() > MAX_BODY {
        return Err(AppError::bad_request("Comment is too long"));
    }
    Ok((
        body,
        truncate(&fields.name, MAX_NAME),
        truncate(&fields.subject, MAX_SUBJECT),
        truncate(&fields.email, MAX_EMAIL),
    ))
}

fn truncate(s: &str, max: usize) -> String {
    let s = s.trim();
    if s.chars().count() > max {
        s.chars().take(max).collect()
    } else {
        s.to_string()
    }
}

fn opt_str(s: String) -> Option<String> {
    if s.is_empty() { None } else { Some(s) }
}
