//! Безопасность: определение IP клиента, CSRF, security-заголовки, rate-limit.
//!
//! IP никогда не хранится в открытом виде — только HMAC-SHA256(secret, ip).

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Mutex;
use std::time::Instant;

use axum::extract::Request;
use axum::http::HeaderMap;
use axum::middleware::Next;
use axum::response::Response;

use crate::tripcode;

/// Content-Security-Policy: только собственные ресурсы, без inline-кода.
const CSP: &str = "default-src 'none'; img-src 'self'; media-src 'self'; \
     style-src 'self'; script-src 'self'; connect-src 'self'; \
     form-action 'self'; base-uri 'self'; frame-ancestors 'none'";

/// Определяет реальный IP клиента.
///
/// Если запрос пришёл от доверенного прокси — берётся первый IP из
/// X-Forwarded-For; иначе — адрес сокета.
pub fn client_ip(headers: &HeaderMap, connect: SocketAddr, trusted: &[ipnet::IpNet]) -> IpAddr {
    let direct = connect.ip();
    if !trusted.iter().any(|net| net.contains(&direct)) {
        return direct;
    }
    if let Some(xff) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
        if let Some(first) = xff.split(',').next().map(str::trim) {
            if let Ok(ip) = first.parse::<IpAddr>() {
                return ip;
            }
        }
    }
    direct
}

/// Хэш IP для хранения и банов.
pub fn ip_hash(ip: &IpAddr, secret: &str) -> String {
    tripcode::hash_ip(&ip.to_string(), secret)
}

// ---------------------------------------------------------------- CSRF

/// Генерирует CSRF-токен (double-submit cookie).
pub fn csrf_token() -> String {
    let bytes: [u8; 32] = rand::random();
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Читает значение cookie по имени.
pub fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    let cookie = headers.get("cookie")?.to_str().ok()?;
    cookie.split(';').map(str::trim).find_map(|kv| {
        let (k, v) = kv.split_once('=')?;
        (k == name).then(|| v.to_string())
    })
}

/// Читает CSRF-токен из cookie-заголовка.
pub fn csrf_from_cookie(headers: &HeaderMap) -> Option<String> {
    cookie_value(headers, "csrf")
}

/// Проверяет double-submit: токен формы должен совпадать с токеном cookie.
pub fn csrf_valid(headers: &HeaderMap, form_token: &str) -> bool {
    match csrf_from_cookie(headers) {
        Some(cookie) if !cookie.is_empty() => cookie == form_token,
        _ => false,
    }
}

/// Достаёт CSRF-токен из cookie или создаёт новый. Возвращает (токен, новый?).
pub fn csrf_for_request(headers: &HeaderMap) -> (String, bool) {
    match csrf_from_cookie(headers) {
        Some(t) if !t.is_empty() => (t, false),
        _ => (csrf_token(), true),
    }
}

/// Значение заголовка Set-Cookie для CSRF-токена.
pub fn csrf_set_cookie(token: &str) -> axum::http::HeaderValue {
    format!("csrf={token}; Path=/; SameSite=Lax; HttpOnly")
        .parse()
        .expect("valid cookie header")
}

// ---------------------------------------------------------------- Headers

/// Middleware: security-заголовки на каждый ответ.
pub async fn security_headers(req: Request, next: Next, hsts: bool) -> Response {
    let mut resp = next.run(req).await;
    let h = resp.headers_mut();
    h.insert("x-content-type-options", "nosniff".parse().unwrap());
    h.insert("x-frame-options", "DENY".parse().unwrap());
    h.insert("referrer-policy", "same-origin".parse().unwrap());
    h.insert("content-security-policy", CSP.parse().unwrap());
    if hsts {
        h.insert(
            "strict-transport-security",
            "max-age=31536000; includeSubDomains".parse().unwrap(),
        );
    }
    resp
}

// ---------------------------------------------------------------- Rate limit

/// Простой in-memory rate limiter (скользящее окно 60 секунд).
pub struct RateLimiter {
    windows: Mutex<HashMap<String, Vec<Instant>>>,
}

impl RateLimiter {
    pub fn new() -> Self {
        Self {
            windows: Mutex::new(HashMap::new()),
        }
    }

    /// Разрешает запрос, если не превышен лимит за последнюю минуту.
    /// `limit == 0` отключает ограничение.
    pub fn allow(&self, key: &str, limit: u32) -> bool {
        if limit == 0 {
            return true;
        }
        let now = Instant::now();
        let mut map = self.windows.lock().unwrap();
        let times = map.entry(key.to_string()).or_default();
        times.retain(|t| now.duration_since(*t).as_secs() < 60);
        if times.len() >= limit as usize {
            return false;
        }
        times.push(now);
        true
    }
}
