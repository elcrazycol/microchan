use askama::Template;
use askama_web::WebTemplate;

use crate::config::{BoardConfig, Config};

/// Лёгкое представление доски для навигации.
#[derive(Debug, Clone)]
pub struct BoardNav {
    pub short: String,
    pub title: String,
    pub description: String,
    pub nsfw: bool,
}

impl BoardNav {
    pub fn from_config(b: &BoardConfig) -> Self {
        Self {
            short: b.short.clone(),
            title: b.title.clone(),
            description: b.description.clone(),
            nsfw: b.nsfw,
        }
    }

    pub fn all(cfg: &Config) -> Vec<Self> {
        cfg.boards.iter().map(Self::from_config).collect()
    }
}

#[derive(Template, WebTemplate)]
#[template(path = "index.html")]
pub struct IndexTemplate {
    pub boards: Vec<BoardNav>,
}
