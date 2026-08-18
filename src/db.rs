use anyhow::{Context, Result};
use sqlx::postgres::{PgPool, PgPoolOptions};

use crate::config::Config;

/// Создаёт пул подключений и применяет миграции.
pub async fn connect_and_migrate(cfg: &Config) -> Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&cfg.database.url)
        .await
        .context("cannot connect to database")?;

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .context("cannot run migrations")?;

    Ok(pool)
}
