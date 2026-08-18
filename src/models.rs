use chrono::{DateTime, Utc};
use sqlx::FromRow;

/// Тред. id треда равен id поста-ОПа.
#[derive(Debug, Clone, FromRow)]
pub struct ThreadRow {
    pub id: i64,
    pub board: String,
    pub created_at: DateTime<Utc>,
    pub last_bump: DateTime<Utc>,
    pub sticky: bool,
    pub locked: bool,
    pub post_count: i32,
    pub deleted: bool,
}

/// Пост. Номер поста (id) глобальный, как в классических бордах.
/// Для ОПа thread_id = None (тред создаётся по id ОПа).
#[derive(Debug, Clone, FromRow)]
pub struct PostRow {
    pub id: i64,
    pub thread_id: Option<i64>,
    pub board: String,
    pub is_op: bool,
    pub name: Option<String>,
    pub tripcode: Option<String>,
    pub email: Option<String>,
    pub subject: Option<String>,
    pub body: String,
    pub ip_hash: String,
    pub created_at: DateTime<Utc>,
    pub deleted: bool,
    pub delete_reason: Option<String>,
}

/// Файл, прикреплённый к посту.
#[derive(Debug, Clone, FromRow)]
pub struct FileRow {
    pub id: i64,
    pub post_id: i64,
    pub original_name: String,
    pub stored_name: String,
    pub mime: String,
    pub size: i64,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub sha256: String,
    pub spoiler: bool,
    pub deleted: bool,
}

/// Бан по хэшу IP или файла.
#[derive(Debug, Clone, FromRow)]
pub struct BanRow {
    pub id: i64,
    pub ip_hash: Option<String>,
    pub file_hash: Option<String>,
    pub reason: String,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

/// Жалоба на пост.
#[derive(Debug, Clone, FromRow)]
pub struct ReportRow {
    pub id: i64,
    pub post_id: i64,
    pub reason: Option<String>,
    pub ip_hash: String,
    pub created_at: DateTime<Utc>,
    pub resolved: bool,
}

/// Запись в логе действий модератора.
#[derive(Debug, Clone, FromRow)]
pub struct ModLogRow {
    pub id: i64,
    pub moderator: String,
    pub action: String,
    pub target: Option<String>,
    pub created_at: DateTime<Utc>,
}
