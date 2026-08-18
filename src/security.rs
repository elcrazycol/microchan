//! Безопасность: определение IP клиента и его хэширование.
//!
//! IP никогда не хранится в открытом виде — только HMAC-SHA256(secret, ip).

use std::net::{IpAddr, SocketAddr};

use axum::http::HeaderMap;

use crate::tripcode;

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

/// Проверяет, является ли клиент доверенным прокси из конфига.
pub fn is_trusted_proxy(ip: &IpAddr, trusted: &[ipnet::IpNet]) -> bool {
    trusted.iter().any(|net| net.contains(ip))
}
