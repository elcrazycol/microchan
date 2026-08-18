use askama::Template;
use askama_web::WebTemplate;

use crate::config::{BoardConfig, Config};
use crate::markup;
use crate::models::{FileRow, PostRow};

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

/// Файл для отображения.
#[derive(Debug, Clone)]
pub struct FileView {
    pub stored_name: String,
    pub original_name: String,
    pub mime: String,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub size: i64,
    pub spoiler: bool,
    pub is_video: bool,
}

impl FileView {
    pub fn from_row(f: &FileRow) -> Self {
        let is_video = matches!(f.mime.as_str(), "video/webm" | "video/mp4");
        Self {
            stored_name: f.stored_name.clone(),
            original_name: f.original_name.clone(),
            mime: f.mime.clone(),
            width: f.width,
            height: f.height,
            size: f.size,
            spoiler: f.spoiler,
            is_video,
        }
    }
}

/// Пост для отображения.
///
/// Поля-строки (subject, tripcode, counts) — пустые строки вместо None,
/// т.к. askama 0.16 не умеет интерполировать Option<String>.
#[derive(Debug, Clone)]
pub struct PostView {
    pub id: i64,
    pub board: String,
    pub is_op: bool,
    pub name: String,
    pub tripcode: String,
    pub subject: String,
    pub body_html: String,
    pub time: String,
    pub files: Vec<FileView>,
    pub deleted: bool,
    pub delete_reason: String,
    pub sage: bool,
    pub sticky: bool,
    pub locked: bool,
    /// Строка вида "5 replies, 2 images" для ОПа (пустая для ответов).
    pub counts: String,
}

impl PostView {
    pub fn from_row(
        p: &PostRow,
        files: Vec<FileView>,
        sticky: bool,
        locked: bool,
        counts: Option<String>,
    ) -> Self {
        let sage = p.email.as_deref() == Some("sage");
        let name = p.name.clone().unwrap_or_else(|| "Anonymous".into());
        let time = p.created_at.format("%d/%m/%y(%a)%H:%M").to_string();
        Self {
            id: p.id,
            board: p.board.clone(),
            is_op: p.is_op,
            name,
            tripcode: p.tripcode.clone().unwrap_or_default(),
            subject: p.subject.clone().unwrap_or_default(),
            body_html: markup::render_body(&p.body),
            time,
            files,
            deleted: p.deleted,
            delete_reason: p.delete_reason.clone().unwrap_or_default(),
            sage,
            sticky,
            locked,
            counts: counts.unwrap_or_default(),
        }
    }
}

/// Тред на странице доски.
#[derive(Debug, Clone)]
pub struct ThreadView {
    pub op: PostView,
    pub replies: Vec<PostView>,
    pub omitted: i64,
}

#[derive(Template, WebTemplate)]
#[template(path = "board.html")]
pub struct BoardTemplate {
    pub boards: Vec<BoardNav>,
    pub board: BoardNav,
    pub threads: Vec<ThreadView>,
    pub page: i64,
    pub has_next: bool,
    pub post_url: String,
}

#[derive(Template, WebTemplate)]
#[template(path = "thread.html")]
pub struct ThreadPageTemplate {
    pub boards: Vec<BoardNav>,
    pub board: BoardNav,
    pub op: PostView,
    pub posts: Vec<PostView>,
    pub thread_id: i64,
    pub reply_url: String,
}
