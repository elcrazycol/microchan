use std::collections::HashMap;

use anyhow::{Context, Result};
use sqlx::PgPool;

use crate::models::{FileRow, PostRow, ThreadRow};

/// Тред для отображения на странице доски.
pub struct ThreadSummary {
    pub op: PostRow,
    /// Последние ответы (без ОПа), по возрастанию id.
    pub replies: Vec<PostRow>,
    /// Файлы постов: post_id -> файлы.
    pub files: HashMap<i64, Vec<FileRow>>,
    /// Общее число картинок в треде.
    pub image_count: i64,
    pub sticky: bool,
    pub locked: bool,
    pub post_count: i32,
}

const REPLIES_ON_BOARD: i64 = 3;

/// Возвращает страницу тредов доски.
pub async fn board_threads(
    pool: &PgPool,
    board: &str,
    page: i64,
    per_page: i64,
) -> Result<(Vec<ThreadSummary>, i64)> {
    let total: i64 = query_scalar_count_threads(pool, board).await?;
    let total_pages = if total == 0 { 1 } else { (total + per_page - 1) / per_page };
    let page = page.clamp(0, total_pages - 1);

    let threads: Vec<ThreadRow> = sqlx::query_as!(
        ThreadRow,
        "SELECT * FROM threads
         WHERE board = $1 AND NOT deleted
         ORDER BY sticky DESC, last_bump DESC
         LIMIT $2 OFFSET $3",
        board,
        per_page,
        page * per_page
    )
    .fetch_all(pool)
    .await
    .context("query threads")?;

    let thread_ids: Vec<i64> = threads.iter().map(|t| t.id).collect();
    if thread_ids.is_empty() {
        return Ok((Vec::new(), total_pages));
    }

    // ОП-посты.
    let ops: Vec<PostRow> = sqlx::query_as!(
        PostRow,
        "SELECT * FROM posts WHERE id = ANY($1)",
        &thread_ids
    )
    .fetch_all(pool)
    .await
    .context("query op posts")?;

    // Последние ответы (до 3) по каждому треду.
    let replies: Vec<PostRow> = sqlx::query_as!(
        PostRow,
        "SELECT p.id, p.thread_id, p.board, p.is_op, p.name, p.tripcode, p.email,
                p.subject, p.body, p.ip_hash, p.created_at, p.deleted, p.delete_reason
         FROM (
            SELECT p.*, ROW_NUMBER() OVER (PARTITION BY thread_id ORDER BY id DESC) AS rn
            FROM posts p
            WHERE thread_id = ANY($1) AND NOT deleted AND NOT is_op
         ) p WHERE p.rn <= $2 ORDER BY p.thread_id, p.id",
        &thread_ids,
        REPLIES_ON_BOARD
    )
    .fetch_all(pool)
    .await
    .context("query replies")?;

    // Файлы всех показанных постов.
    let mut shown_posts: Vec<i64> = ops.iter().map(|p| p.id).collect();
    shown_posts.extend(replies.iter().map(|p| p.id));
    let files: Vec<FileRow> = if shown_posts.is_empty() {
        Vec::new()
    } else {
        sqlx::query_as!(
            FileRow,
            "SELECT * FROM files WHERE post_id = ANY($1) AND NOT deleted",
            &shown_posts
        )
        .fetch_all(pool)
        .await
        .context("query files")?
    };

    let mut files_by_post: HashMap<i64, Vec<FileRow>> = HashMap::new();
    for f in files {
        files_by_post.entry(f.post_id).or_default().push(f);
    }

    // Число картинок по тредам.
    let img_counts: HashMap<i64, i64> = query_image_counts(pool, &thread_ids).await?;

    let mut summaries = Vec::with_capacity(threads.len());
    for t in &threads {
        let op = ops
            .iter()
            .find(|p| p.id == t.id)
            .cloned()
            .unwrap_or_else(|| PostRow {
                id: t.id,
                thread_id: t.id,
                board: t.board.clone(),
                is_op: true,
                name: None,
                tripcode: None,
                email: None,
                subject: None,
                body: String::new(),
                ip_hash: String::new(),
                created_at: t.created_at,
                deleted: true,
                delete_reason: None,
            });
        let thread_replies: Vec<PostRow> = replies
            .iter()
            .filter(|p| p.thread_id == t.id)
            .cloned()
            .collect();
        let image_count = img_counts.get(&t.id).copied().unwrap_or(0);
        summaries.push(ThreadSummary {
            op,
            replies: thread_replies,
            files: files_by_post.clone(),
            image_count,
            sticky: t.sticky,
            locked: t.locked,
            post_count: t.post_count,
        });
    }

    Ok((summaries, total_pages))
}

async fn query_scalar_count_threads(pool: &PgPool, board: &str) -> Result<i64> {
    let n: Option<i64> = sqlx::query_scalar!(
        "SELECT count(*) FROM threads WHERE board = $1 AND NOT deleted",
        board
    )
    .fetch_one(pool)
    .await
    .context("count threads")?;
    Ok(n.unwrap_or(0))
}

async fn query_image_counts(pool: &PgPool, thread_ids: &[i64]) -> Result<HashMap<i64, i64>> {
    let rows = sqlx::query!(
        "SELECT p.thread_id, count(*) AS cnt
         FROM files f JOIN posts p ON p.id = f.post_id
         WHERE p.thread_id = ANY($1) AND NOT f.deleted
         GROUP BY p.thread_id",
        thread_ids
    )
    .fetch_all(pool)
    .await
    .context("count images per thread")?;
    Ok(rows.into_iter().map(|r| (r.thread_id, r.cnt.unwrap_or(0))).collect())
}
