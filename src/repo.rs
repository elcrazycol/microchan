use std::collections::HashMap;

use anyhow::{Context, Result};
use sqlx::PgPool;

use crate::media::Stored;
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
                thread_id: None,
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
            .filter(|p| p.thread_id == Some(t.id))
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

/// Данные треда для страницы треда.
pub struct ThreadData {
    pub thread: ThreadRow,
    pub op: PostRow,
    /// Все ответы (без ОПа), по возрастанию id.
    pub posts: Vec<PostRow>,
    /// Файлы постов: post_id -> файлы.
    pub files: HashMap<i64, Vec<FileRow>>,
}

/// Полная страница треда; None если тред не существует/удалён.
pub async fn thread_data(
    pool: &PgPool,
    board: &str,
    thread_id: i64,
) -> Result<Option<ThreadData>> {
    let thread: Option<ThreadRow> = sqlx::query_as!(
        ThreadRow,
        "SELECT * FROM threads WHERE id = $1 AND board = $2 AND NOT deleted",
        thread_id,
        board
    )
    .fetch_optional(pool)
    .await
    .context("query thread")?;
    let Some(thread) = thread else {
        return Ok(None);
    };

    let op: Option<PostRow> = sqlx::query_as!(
        PostRow,
        "SELECT * FROM posts WHERE id = $1 AND is_op",
        thread.id
    )
    .fetch_optional(pool)
    .await
    .context("query op post")?;
    let op = op.unwrap_or_else(|| PostRow {
        id: thread.id,
        thread_id: None,
        board: thread.board.clone(),
        is_op: true,
        name: None,
        tripcode: None,
        email: None,
        subject: None,
        body: String::new(),
        ip_hash: String::new(),
        created_at: thread.created_at,
        deleted: true,
        delete_reason: None,
    });

    let posts: Vec<PostRow> = sqlx::query_as!(
        PostRow,
        "SELECT * FROM posts WHERE thread_id = $1 AND NOT deleted ORDER BY id",
        thread.id
    )
    .fetch_all(pool)
    .await
    .context("query thread posts")?;

    let mut post_ids: Vec<i64> = posts.iter().map(|p| p.id).collect();
    post_ids.push(op.id);
    let files: Vec<FileRow> = sqlx::query_as!(
        FileRow,
        "SELECT * FROM files WHERE post_id = ANY($1) AND NOT deleted",
        &post_ids
    )
    .fetch_all(pool)
    .await
    .context("query thread files")?;
    let mut files_by_post: HashMap<i64, Vec<FileRow>> = HashMap::new();
    for f in files {
        files_by_post.entry(f.post_id).or_default().push(f);
    }

    Ok(Some(ThreadData {
        thread,
        op,
        posts,
        files: files_by_post,
    }))
}

/// Создаёт тред (пост-ОП + запись треда + файлы). Возвращает id треда.
pub async fn create_thread(
    pool: &PgPool,
    board: &str,
    ip_hash: &str,
    name: Option<&str>,
    tripcode: Option<&str>,
    email: Option<&str>,
    subject: Option<&str>,
    body: &str,
    files: &[Stored],
) -> Result<i64> {
    let mut tx = pool.begin().await.context("begin tx")?;
    let post_id: i64 = sqlx::query_scalar!(
        "INSERT INTO posts (thread_id, board, is_op, name, tripcode, email, subject, body, ip_hash)
         VALUES (NULL, $1, true, $2, $3, $4, $5, $6, $7) RETURNING id",
        board,
        name,
        tripcode,
        email,
        subject,
        body,
        ip_hash
    )
    .fetch_one(&mut *tx)
    .await
    .context("insert op post")?;

    sqlx::query!(
        "INSERT INTO threads (id, board) VALUES ($1, $2)",
        post_id,
        board
    )
    .execute(&mut *tx)
    .await
    .context("insert thread")?;

    insert_files(&mut tx, post_id, files).await?;

    tx.commit().await.context("commit thread")?;
    Ok(post_id)
}

/// Создаёт ответ. Управляет бампом (sage/locked/bump_limit).
/// Возвращает id поста.
pub async fn create_reply(
    pool: &PgPool,
    board: &str,
    thread_id: i64,
    ip_hash: &str,
    name: Option<&str>,
    tripcode: Option<&str>,
    email: Option<&str>,
    body: &str,
    bump_limit: usize,
    files: &[Stored],
) -> Result<i64> {
    let mut tx = pool.begin().await.context("begin tx")?;

    let thread: Option<ThreadRow> = sqlx::query_as!(
        ThreadRow,
        "SELECT * FROM threads WHERE id = $1 AND board = $2 AND NOT deleted FOR UPDATE",
        thread_id,
        board
    )
    .fetch_optional(&mut *tx)
    .await
    .context("lock thread")?;
    let Some(thread) = thread else {
        return Err(anyhow::anyhow!("thread not found"));
    };
    if thread.locked {
        return Err(anyhow::anyhow!("thread is locked"));
    }

    let post_id: i64 = sqlx::query_scalar!(
        "INSERT INTO posts (thread_id, board, is_op, name, tripcode, email, body, ip_hash)
         VALUES ($1, $2, false, $3, $4, $5, $6, $7) RETURNING id",
        thread_id,
        board,
        name,
        tripcode,
        email,
        body,
        ip_hash
    )
    .fetch_one(&mut *tx)
    .await
    .context("insert reply")?;

    let new_count = thread.post_count + 1;
    let sage = email == Some("sage");
    let under_limit = bump_limit == 0 || new_count as usize <= bump_limit;
    if !sage && under_limit {
        sqlx::query!(
            "UPDATE threads SET post_count = $1, last_bump = now() WHERE id = $2",
            new_count,
            thread_id
        )
        .execute(&mut *tx)
        .await
        .context("bump thread")?;
    } else {
        sqlx::query!(
            "UPDATE threads SET post_count = $1 WHERE id = $2",
            new_count,
            thread_id
        )
        .execute(&mut *tx)
        .await
        .context("update thread count")?;
    }

    insert_files(&mut tx, post_id, files).await?;

    tx.commit().await.context("commit reply")?;
    Ok(post_id)
}

/// Вставляет записи файлов поста в рамках транзакции.
async fn insert_files(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    post_id: i64,
    files: &[Stored],
) -> Result<()> {
    for f in files {
        sqlx::query!(
            "INSERT INTO files (post_id, original_name, stored_name, thumb_name, mime, size, width, height, sha256, spoiler)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
            post_id,
            f.original_name,
            f.stored_name,
            f.thumb_name,
            f.mime,
            f.size,
            f.width,
            f.height,
            f.sha256,
            f.spoiler,
        )
        .execute(&mut **tx)
        .await
        .context("insert file")?;
    }
    Ok(())
}

/// Элемент каталога: ОП треда с превью и счётчиками.
pub struct CatalogItem {
    pub id: i64,
    pub subject: Option<String>,
    pub reply_count: i64,
    pub image_count: i64,
    pub sticky: bool,
    /// Имя превью первого файла ОПа.
    pub thumb_name: Option<String>,
}

/// Все треды доски для каталога (без пагинации, только ОП и первое превью).
pub async fn catalog(pool: &PgPool, board: &str) -> Result<Vec<CatalogItem>> {
    let rows = sqlx::query!(
        "SELECT t.id, t.sticky, t.post_count,
                op.subject,
                (SELECT count(*)::bigint FROM files f
                  WHERE f.post_id = op.id AND NOT f.deleted) AS image_count,
                (SELECT f.thumb_name FROM files f
                  WHERE f.post_id = op.id AND NOT f.deleted ORDER BY f.id LIMIT 1) AS thumb_name
         FROM threads t
         JOIN posts op ON op.id = t.id
         WHERE t.board = $1 AND NOT t.deleted
         ORDER BY t.sticky DESC, t.last_bump DESC",
        board
    )
    .fetch_all(pool)
    .await
    .context("query catalog")?;

    Ok(rows
        .into_iter()
        .map(|r| CatalogItem {
            id: r.id,
            subject: r.subject,
            reply_count: (r.post_count - 1).max(0) as i64,
            image_count: r.image_count.unwrap_or(0),
            sticky: r.sticky,
            thumb_name: r.thumb_name,
        })
        .collect())
}

/// Находит доску и тред поста (для перехода по номеру поста).
pub async fn post_location(pool: &PgPool, post_id: i64) -> Result<Option<(String, i64)>> {
    let row = sqlx::query!(
        "SELECT board, COALESCE(thread_id, id) AS tid
         FROM posts WHERE id = $1 AND NOT deleted",
        post_id
    )
    .fetch_optional(pool)
    .await
    .context("query post location")?;
    Ok(row.and_then(|r| r.tid.map(|tid| (r.board, tid))))
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
    Ok(rows
        .into_iter()
        .filter_map(|r| r.thread_id.map(|tid| (tid, r.cnt.unwrap_or(0))))
        .collect())
}
