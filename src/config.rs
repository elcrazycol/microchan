use anyhow::{bail, Context, Result};
use ipnet::IpNet;
use serde::Deserialize;
use std::path::PathBuf;

/// Корневая конфигурация приложения.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub security: SecurityConfig,
    pub moderation: ModerationConfig,
    pub media: MediaConfig,
    pub boards: Vec<BoardConfig>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            database: DatabaseConfig::default(),
            security: SecurityConfig::default(),
            moderation: ModerationConfig::default(),
            media: MediaConfig::default(),
            boards: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    /// Публичный базовый URL (для генерации ссылок), например https://chan.example.com
    pub base_url: String,
    /// Каталог для хранения загруженных файлов.
    pub data_dir: PathBuf,
    /// Доверенные прокси (IP или CIDR). Если запрос пришёл от них,
    /// реальный IP берётся из X-Forwarded-For.
    pub trusted_proxies: Vec<IpNet>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".into(),
            port: 8080,
            base_url: "http://localhost:8080".into(),
            data_dir: PathBuf::from("data"),
            trusted_proxies: vec!["127.0.0.1/32".parse().unwrap()],
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct DatabaseConfig {
    pub url: String,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: "postgres://localhost/microchan".into(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SecurityConfig {
    /// Секрет для HMAC-хэширования IP. Никогда не хранить IP открыто.
    pub secret: String,
    /// Отдельная соль для secure-трипкодов. Если не задана — используется `secret`.
    pub secure_trip_salt: Option<String>,
    /// Старые секреты для ротации: по ним ещё проверяются баны.
    pub old_secrets: Vec<String>,
    /// Ограничение постов с одного IP-хэша в минуту.
    pub max_posts_per_minute: u32,
    /// Включать HSTS (только при работе по HTTPS).
    pub hsts: bool,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            secret: String::new(),
            secure_trip_salt: None,
            old_secrets: Vec::new(),
            max_posts_per_minute: 10,
            hsts: false,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ModerationConfig {
    /// Пароль для входа в модераторскую.
    pub admin_password: String,
    /// Если задан — модераторская доступна только по /mod/<secret>/.
    pub mod_secret_url: Option<String>,
    /// Срок жизни сессии модератора, часы.
    pub session_hours: i64,
}

impl Default for ModerationConfig {
    fn default() -> Self {
        Self {
            admin_password: String::new(),
            mod_secret_url: None,
            session_hours: 12,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct MediaConfig {
    pub thumb_width: u32,
    pub thumb_height: u32,
    pub thumb_quality: u8,
    /// Максимальная длительность видео (секунды), 0 = без ограничения.
    pub max_video_seconds: u32,
    /// Максимум пикселей картинки (ширина × высота), 0 = без ограничения.
    pub max_image_pixels: u64,
}

impl Default for MediaConfig {
    fn default() -> Self {
        Self {
            thumb_width: 200,
            thumb_height: 200,
            thumb_quality: 80,
            max_video_seconds: 0,
            max_image_pixels: 50_000_000,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct BoardConfig {
    /// Короткое имя доски, например "b".
    pub short: String,
    /// Название доски.
    pub title: String,
    pub description: String,
    pub nsfw: bool,
    /// Максимум тредов на доске (не считая sticky).
    pub thread_limit: usize,
    /// Максимум постов до прекращения бампа (0 = без лимита).
    pub bump_limit: usize,
    /// Максимум файлов на пост.
    pub max_images: usize,
    /// Максимальный размер файла в байтах.
    pub max_file_size: u64,
    /// Разрешённые расширения (без точки, нижний регистр).
    pub allowed_extensions: Vec<String>,
    /// Если задано — треды, не бампавшиеся дольше N дней, прунятся.
    pub max_thread_age_days: Option<i64>,
}

impl SecurityConfig {
    /// Соль для secure-трипкодов (fallback на `secret`).
    pub fn secure_trip_salt(&self) -> &str {
        self.secure_trip_salt.as_deref().unwrap_or(&self.secret)
    }
}

impl Config {
    /// Загружает конфиг из файла (путь из env MICROCHAN_CONFIG или config.toml)
    /// с переопределениями из переменных окружения.
    pub fn load() -> Result<Self> {
        let path = std::env::var("MICROCHAN_CONFIG").unwrap_or_else(|_| "config.toml".into());
        let raw = std::fs::read_to_string(&path)
            .with_context(|| format!("cannot read config file {path}"))?;
        let mut cfg: Config = toml::from_str(&raw).context("invalid config file")?;

        // Пустой secret-url = без secret-url.
        if cfg.moderation.mod_secret_url.as_deref() == Some("") {
            cfg.moderation.mod_secret_url = None;
        }

        // Переопределения из окружения.
        if let Ok(url) = std::env::var("DATABASE_URL") {
            cfg.database.url = url;
        }
        if let Ok(secret) = std::env::var("MICROCHAN_SECRET") {
            cfg.security.secret = secret;
        }

        cfg.validate()?;
        Ok(cfg)
    }

    fn validate(&self) -> Result<()> {
        if self.security.secret.is_empty() {
            bail!("security.secret is required (set in config or MICROCHAN_SECRET)");
        }
        let mut seen = std::collections::HashSet::new();
        for b in &self.boards {
            if b.short.is_empty() {
                bail!("board short name cannot be empty");
            }
            if b.allowed_extensions.is_empty() {
                bail!("board {} has no allowed_extensions", b.short);
            }
            if !seen.insert(b.short.clone()) {
                bail!("duplicate board short name: {}", b.short);
            }
        }
        if self.moderation.admin_password.is_empty() {
            bail!("moderation.admin_password is required");
        }
        Ok(())
    }

    pub fn board(&self, short: &str) -> Option<&BoardConfig> {
        self.boards.iter().find(|b| b.short == short)
    }
}
