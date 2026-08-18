use std::sync::Arc;

use sqlx::PgPool;

use crate::config::Config;

/// Глобальное состояние, доступное всем обработчикам.
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub pool: PgPool,
}

impl AppState {
    pub fn new(config: Config, pool: PgPool) -> Self {
        Self {
            config: Arc::new(config),
            pool,
        }
    }
}
