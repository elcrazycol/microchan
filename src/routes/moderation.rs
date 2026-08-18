//! Модераторская панель: логин по паролю, жалобы, баны, удаление, лог.

use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue};
use axum::response::{IntoResponse, Redirect};
use axum::routing::get;
use axum::{Form, Router};
use chrono::{Duration, Utc};
use serde::Deserialize;

use crate::config::Config;
use crate::error::AppError;
use crate::repo;
use crate::security;
use crate::state::AppState;
use crate::views::{
    BoardNav, ModBan, ModLog, ModLoginTemplate, ModPanelTemplate, ModReport,
};

const SESSION_COOKIE: &str = "mod_session";

/// Роуты мод-панели по полным путям (base = `/mod` или `/mod/<secret>`).
pub fn router(base: &str) -> Router<AppState> {
    Router::new()
        .route(base, get(panel))
        .route(&format!("{base}/"), get(panel))
        .route(&format!("{base}/login"), get(login_page).post(login_submit))
        .route(&format!("{base}/logout"), axum::routing::post(logout))
        .route(&format!("{base}/delete-post"), axum::routing::post(delete_post))
        .route(&format!("{base}/delete-thread"), axum::routing::post(delete_thread))
        .route(&format!("{base}/ban"), axum::routing::post(ban))
        .route(&format!("{base}/resolve"), axum::routing::post(resolve_report))
}

/// Базовый путь мод-панели (`/mod` или `/mod/<secret>`).
fn mod_base(config: &Config) -> String {
    match &config.moderation.mod_secret_url {
        Some(s) => format!("/mod/{s}"),
        None => "/mod".to_string(),
    }
}

async fn login_page(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<axum::response::Response, AppError> {
    let (csrf, is_new) = security::csrf_for_request(&headers);
    let template = ModLoginTemplate {
        boards: BoardNav::all(&state.config),
        action: format!("{}/login", mod_base(&state.config)),
        csrf: csrf.clone(),
    };
    let mut resp = template.into_response();
    if is_new {
        resp.headers_mut()
            .insert("set-cookie", security::csrf_set_cookie(&csrf));
    }
    Ok(resp)
}

#[derive(Debug, Deserialize)]
struct LoginForm {
    password: String,
    csrf: String,
}

async fn login_submit(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<LoginForm>,
) -> Result<axum::response::Response, AppError> {
    if !security::csrf_valid(&headers, &form.csrf) {
        return Err(AppError::Forbidden);
    }
    if form.password != state.config.moderation.admin_password {
        return Err(AppError::bad_request("Wrong password"));
    }

    let token = security::csrf_token();
    let expires = Utc::now() + Duration::hours(state.config.moderation.session_hours);
    repo::create_session(&state.pool, &token, expires).await?;

    let mut resp = Redirect::to(&format!("{}/", mod_base(&state.config))).into_response();
    resp.headers_mut()
        .insert("set-cookie", session_cookie(&token));
    Ok(resp)
}

async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Redirect {
    if let Some(token) = security::cookie_value(&headers, SESSION_COOKIE) {
        let _ = repo::delete_session(&state.pool, &token).await;
    }
    Redirect::to("/")
}

async fn panel(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<axum::response::Response, AppError> {
    require_session(&state, &headers).await?;

    let reports = repo::list_reports(&state.pool, false).await?;
    let bans = repo::list_bans(&state.pool).await?;
    let logs = repo::list_mod_logs(&state.pool, 50).await?;

    let reports: Vec<ModReport> = reports
        .into_iter()
        .map(|r| ModReport {
            id: r.id,
            post_id: r.post_id,
            board: r.board,
            reason: r.reason.unwrap_or_default(),
            created_at: r.created_at.format("%d/%m/%y %H:%M").to_string(),
            ip_hash: r.ip_hash,
        })
        .collect();

    let bans: Vec<ModBan> = bans
        .into_iter()
        .map(|b| ModBan {
            id: b.id,
            ip_hash: b.ip_hash.unwrap_or_default(),
            file_hash: b.file_hash.unwrap_or_default(),
            reason: b.reason,
            created_by: b.created_by,
            created_at: b.created_at.format("%d/%m/%y %H:%M").to_string(),
            expires_at: b
                .expires_at
                .map(|t| t.format("%d/%m/%y %H:%M").to_string())
                .unwrap_or_else(|| "permanent".into()),
        })
        .collect();

    let logs: Vec<ModLog> = logs
        .into_iter()
        .map(|l| ModLog {
            moderator: l.moderator,
            action: l.action,
            target: l.target.unwrap_or_default(),
            created_at: l.created_at.format("%d/%m/%y %H:%M").to_string(),
        })
        .collect();

    let (csrf, is_new) = security::csrf_for_request(&headers);
    let template = ModPanelTemplate {
        boards: BoardNav::all(&state.config),
        base: mod_base(&state.config),
        reports,
        bans,
        logs,
        csrf: csrf.clone(),
    };
    let mut resp = template.into_response();
    if is_new {
        resp.headers_mut()
            .insert("set-cookie", security::csrf_set_cookie(&csrf));
    }
    Ok(resp)
}

#[derive(Debug, Deserialize)]
struct IdForm {
    id: i64,
    csrf: String,
}

async fn delete_post(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<IdForm>,
) -> Result<Redirect, AppError> {
    require_session(&state, &headers).await?;
    check_csrf(&headers, &form.csrf)?;
    repo::delete_post(&state.pool, form.id).await?;
    repo::log_action(&state.pool, "admin", "delete-post", Some(&form.id.to_string())).await?;
    Ok(redirect_home(&state))
}

async fn delete_thread(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<IdForm>,
) -> Result<Redirect, AppError> {
    require_session(&state, &headers).await?;
    check_csrf(&headers, &form.csrf)?;
    repo::delete_thread(&state.pool, form.id).await?;
    repo::log_action(&state.pool, "admin", "delete-thread", Some(&form.id.to_string())).await?;
    Ok(redirect_home(&state))
}

#[derive(Debug, Deserialize)]
struct BanForm {
    ip_hash: Option<String>,
    file_hash: Option<String>,
    reason: String,
    hours: Option<i64>,
    csrf: String,
}

async fn ban(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<BanForm>,
) -> Result<Redirect, AppError> {
    require_session(&state, &headers).await?;
    check_csrf(&headers, &form.csrf)?;

    let reason = form.reason.trim().to_string();
    if reason.is_empty() {
        return Err(AppError::bad_request("Reason is required"));
    }
    let ip_hash = form.ip_hash.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    let file_hash = form.file_hash.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    if ip_hash.is_none() && file_hash.is_none() {
        return Err(AppError::bad_request("IP hash or file hash is required"));
    }

    let expires_at = form
        .hours
        .filter(|h| *h > 0)
        .map(|h| Utc::now() + Duration::hours(h));

    repo::create_ban(
        &state.pool,
        ip_hash.as_deref(),
        file_hash.as_deref(),
        &reason,
        "admin",
        expires_at,
    )
    .await?;

    let target = ip_hash.as_deref().or(file_hash.as_deref()).unwrap_or("");
    repo::log_action(&state.pool, "admin", "ban", Some(target)).await?;
    Ok(redirect_home(&state))
}

#[derive(Debug, Deserialize)]
struct ResolveForm {
    id: i64,
    csrf: String,
}

async fn resolve_report(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<ResolveForm>,
) -> Result<Redirect, AppError> {
    require_session(&state, &headers).await?;
    check_csrf(&headers, &form.csrf)?;
    repo::resolve_report(&state.pool, form.id).await?;
    repo::log_action(&state.pool, "admin", "resolve-report", Some(&form.id.to_string())).await?;
    Ok(redirect_home(&state))
}

// ------------------------------------------------------------------ Helpers

async fn require_session(state: &AppState, headers: &HeaderMap) -> Result<(), AppError> {
    let Some(token) = security::cookie_value(headers, SESSION_COOKIE) else {
        return Err(AppError::Forbidden);
    };
    if !repo::session_valid(&state.pool, &token).await? {
        return Err(AppError::Forbidden);
    }
    Ok(())
}

fn check_csrf(headers: &HeaderMap, form_token: &str) -> Result<(), AppError> {
    if security::csrf_valid(headers, form_token) {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}

fn redirect_home(state: &AppState) -> Redirect {
    Redirect::to(&format!("{}/", mod_base(&state.config)))
}

fn session_cookie(token: &str) -> HeaderValue {
    format!("{SESSION_COOKIE}={token}; Path=/; SameSite=Lax; HttpOnly")
        .parse()
        .expect("valid cookie header")
}
