//! Медиа: проверка загружаемых файлов (magic bytes, лимиты), хранение
//! оригинала и генерация превью (image / ffmpeg для видео).

use std::path::Path;

use anyhow::Context;
use image::codecs::jpeg::JpegEncoder;

use crate::config::{BoardConfig, MediaConfig};
use crate::error::AppError;
use crate::tripcode::file_hash;

/// Категория файла по magic bytes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    Image(&'static str),
    Video(&'static str),
}

impl Kind {
    /// Каноническое расширение (для проверки allowed_extensions).
    pub fn canonical_ext(self) -> &'static str {
        match self {
            Kind::Image("image/jpeg") => "jpg",
            Kind::Image("image/png") => "png",
            Kind::Image("image/webp") => "webp",
            Kind::Image("image/gif") => "gif",
            Kind::Image(_) => "img",
            Kind::Video("video/webm") => "webm",
            Kind::Video("video/mp4") => "mp4",
            Kind::Video(_) => "video",
        }
    }

    pub fn mime(self) -> &'static str {
        match self {
            Kind::Image(m) | Kind::Video(m) => m,
        }
    }

    pub fn is_video(self) -> bool {
        matches!(self, Kind::Video(_))
    }
}

/// Файл, прошедший первичную валидацию (magic bytes, расширение, размер).
pub struct Validated {
    pub data: Vec<u8>,
    pub original_name: String,
    pub kind: Kind,
    pub spoiler: bool,
}

/// Метаданные сохранённого файла (для записи в БД).
pub struct Stored {
    pub original_name: String,
    pub stored_name: String,
    pub thumb_name: String,
    pub mime: String,
    pub size: i64,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub sha256: String,
    pub spoiler: bool,
}

/// Определяет тип файла по magic bytes.
pub fn detect(data: &[u8]) -> Option<Kind> {
    let mime = infer::get(data)?.mime_type();
    let kind = match mime {
        "image/jpeg" => Kind::Image("image/jpeg"),
        "image/png" => Kind::Image("image/png"),
        "image/webp" => Kind::Image("image/webp"),
        "image/gif" => Kind::Image("image/gif"),
        "video/webm" => Kind::Video("video/webm"),
        "video/mp4" => Kind::Video("video/mp4"),
        _ => return None,
    };
    Some(kind)
}

/// Очищает имя файла: только basename, без управляющих символов, лимит длины.
pub fn sanitize_filename(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let cleaned: String = base
        .chars()
        .filter(|c| !c.is_control())
        .collect();
    let cleaned = cleaned.trim();
    if cleaned.len() > 200 {
        cleaned.chars().take(200).collect()
    } else {
        cleaned.to_string()
    }
}

/// Первичная валидация: magic bytes, разрешённое расширение, размер.
pub fn validate(
    data: Vec<u8>,
    filename: &str,
    board: &BoardConfig,
    spoiler: bool,
) -> Result<Validated, AppError> {
    let original_name = sanitize_filename(filename);
    if original_name.is_empty() {
        return Err(AppError::bad_request("File name is empty"));
    }

    let Some(kind) = detect(&data) else {
        return Err(AppError::bad_request("Unsupported or corrupt file type"));
    };

    if !board.allowed_extensions.contains(&kind.canonical_ext().to_string()) {
        return Err(AppError::bad_request(format!(
            "File type .{} is not allowed on this board",
            kind.canonical_ext()
        )));
    }

    if data.len() as u64 > board.max_file_size {
        return Err(AppError::bad_request("File is too large"));
    }

    if data.is_empty() {
        return Err(AppError::bad_request("File is empty"));
    }

    Ok(Validated {
        data,
        original_name,
        kind,
        spoiler,
    })
}

/// Сохраняет оригинал и превью на диск, возвращает метаданные.
/// При любой ошибке удаляет уже записанные файлы.
pub async fn store(
    v: &Validated,
    board: &str,
    data_dir: &Path,
    media: &MediaConfig,
) -> Result<Stored, AppError> {
    let stem = random_hex();
    let stored_name = format!("{stem}.{}", v.kind.canonical_ext());
    let thumb_name = format!("{stem}.jpg");

    let board_dir = data_dir.join(board);
    let src_dir = board_dir.join("src");
    let thumb_dir = board_dir.join("thumb");
    tokio::fs::create_dir_all(&src_dir).await.context("create src dir")?;
    tokio::fs::create_dir_all(&thumb_dir).await.context("create thumb dir")?;

    let src_path = src_dir.join(&stored_name);
    let thumb_path = thumb_dir.join(&thumb_name);

    // Оригинал.
    if let Err(e) = tokio::fs::write(&src_path, &v.data).await {
        return Err(AppError::Internal(
            anyhow::anyhow!("write original: {e}"),
        ));
    }

    // Превью + размеры; при ошибке подчищаем записанные файлы.
    let result = if v.kind.is_video() {
        video_info(&src_path, &thumb_path, media).await
    } else {
        image_thumb(&v.data, &thumb_path, media)
    };
    let (width, height) = match result {
        Ok(wh) => wh,
        Err(e) => {
            let _ = tokio::fs::remove_file(&src_path).await;
            let _ = tokio::fs::remove_file(&thumb_path).await;
            return Err(e);
        }
    };

    let sha256 = file_hash(&v.data);

    Ok(Stored {
        original_name: v.original_name.clone(),
        stored_name,
        thumb_name,
        mime: v.kind.mime().to_string(),
        size: v.data.len() as i64,
        width,
        height,
        sha256,
        spoiler: v.spoiler,
    })
}

/// Генерирует превью картинки (JPEG) и возвращает (width, height).
fn image_thumb(
    data: &[u8],
    thumb_path: &Path,
    media: &MediaConfig,
) -> Result<(Option<i32>, Option<i32>), AppError> {
    let img = image::load_from_memory(data)
        .map_err(|_| AppError::bad_request("Corrupt image"))?;
    let (w, h) = (img.width(), img.height());

    if media.max_image_pixels > 0 && (w as u64) * (h as u64) > media.max_image_pixels {
        return Err(AppError::bad_request("Image resolution is too large"));
    }

    let thumb = img.thumbnail(media.thumb_width, media.thumb_height);
    let mut buf = Vec::new();
    let encoder = JpegEncoder::new_with_quality(&mut buf, media.thumb_quality);
    thumb
        .write_with_encoder(encoder)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("encode thumb: {e}")))?;
    std::fs::write(thumb_path, &buf).map_err(|e| {
        AppError::Internal(anyhow::anyhow!("write thumb: {e}"))
    })?;

    Ok((Some(w as i32), Some(h as i32)))
}

/// Для видео: проверка длительности (если задана) и кадр-превью через ffmpeg.
async fn video_info(
    src_path: &Path,
    thumb_path: &Path,
    media: &MediaConfig,
) -> Result<(Option<i32>, Option<i32>), AppError> {
    if media.max_video_seconds > 0 {
        if let Ok(Some(secs)) = ffprobe_duration(src_path).await {
            if secs > media.max_video_seconds as f64 {
                return Err(AppError::bad_request("Video is too long"));
            }
        }
    }

    ffmpeg_thumb(src_path, thumb_path, media).await?;
    Ok((None, None))
}

async fn ffprobe_duration(path: &Path) -> Result<Option<f64>, AppError> {
    let out = tokio::process::Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
        ])
        .arg(path)
        .output()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("ffprobe: {e}")))?;
    let s = String::from_utf8_lossy(&out.stdout);
    Ok(s.trim().parse::<f64>().ok())
}

async fn ffmpeg_thumb(
    src_path: &Path,
    thumb_path: &Path,
    media: &MediaConfig,
) -> Result<(), AppError> {
    let scale = format!(
        "scale={}:{}:force_original_aspect_ratio=decrease",
        media.thumb_width, media.thumb_height
    );
    let status = tokio::process::Command::new("ffmpeg")
        .args(["-y", "-i"])
        .arg(src_path)
        .args(["-frames:v", "1", "-vf"])
        .arg(scale)
        .arg(thumb_path)
        .status()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("ffmpeg: {e}")))?;
    if !status.success() {
        return Err(AppError::bad_request("Cannot process video"));
    }
    Ok(())
}

/// Удаляет сохранённые файлы (для отката при ошибке БД).
pub async fn cleanup(files: &[Stored], board: &str, data_dir: &Path) {
    for f in files {
        let _ = tokio::fs::remove_file(data_dir.join(board).join("src").join(&f.stored_name)).await;
        let _ = tokio::fs::remove_file(data_dir.join(board).join("thumb").join(&f.thumb_name)).await;
    }
}

/// Случайный hex-идентификатор файла (без расширения).
fn random_hex() -> String {
    let bytes: [u8; 16] = rand::random();
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_strips_path() {
        assert_eq!(sanitize_filename("../../etc/passwd"), "passwd");
        assert_eq!(sanitize_filename("C:\\windows\\evil.png"), "evil.png");
        assert_eq!(sanitize_filename("normal.png"), "normal.png");
        assert_eq!(sanitize_filename("no_ext"), "no_ext");
    }

    #[test]
    fn detect_png_and_reject_text() {
        let png = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        assert!(matches!(detect(&png), Some(Kind::Image("image/png"))));
        assert!(detect(b"hello world").is_none());
    }
}
