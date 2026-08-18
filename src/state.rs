use std::sync::Arc;

use sqlx::PgPool;

use crate::config::Config;
use crate::security::RateLimiter;

/// Глобальное состояние, доступное всем обработчикам.
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub pool: PgPool,
    pub rate_limiter: Arc<RateLimiter>,
}

impl AppState {
    pub fn new(config: Config, pool: PgPool) -> Self {
        Self {
            config: Arc::new(config),
            pool,
            rate_limiter: Arc::new(RateLimiter::new()),
        }
    }
}
