# microchan

Минималистичный imageboard-движок на Rust: монолит, server-side rendering,
анонимность по умолчанию, без bloat. Классический imageboard-опыт, сделанный хорошо.

**Read in English: [README.md](README.md).**

## Требования

- Rust (stable)
- PostgreSQL 16+
- `ffmpeg` (опционально, только для превью видео)

## Быстрый старт

```bash
createdb microchan
cp config.example.toml config.toml
$EDITOR config.toml   # database.url, security.secret, moderation.admin_password
cargo run --release
```

Миграции применяются автоматически при первом запуске. Доски управляются только
через `config.toml`.

Укажите имя пользователя PostgreSQL в `database.url` (обычно это пользователь ОС,
`whoami`):

```toml
[database]
url = "postgres://yourname@localhost/microchan"
```

Для сборки база данных не нужна — в репозитории закоммичен offline-кэш SQLx
(`.sqlx/`). После изменения SQL-запросов обновите его:

```bash
export DATABASE_URL=postgres://yourname@localhost/microchan
cargo sqlx prepare
```

## Возможности

- **Доски** — управляются конфигом: название, описание, NSFW, лимит тредов,
  лимит файлов на пост, разрешённые расширения, максимальный размер файла.
- **Треды и посты** — sage, bump-limit, sticky/lock, цитаты `>>123`, greentext,
  спойлеры `[spoiler]…[/spoiler]`, классические (`#`) и secure (`##`) трипкоды,
  тема, e-mail.
- **Медиа** — 1..N файлов на пост (JPEG/PNG/WebP/GIF/WebM/MP4), проверка
  magic bytes, превью, спойлер-картинки.
- **Каталог и навигация** — индекс доски с пагинацией, сетка превью, переход
  по номеру поста.
- **Модерация** — удаление поста/треда, баны по IP/файлу, жалобы, лог действий,
  простая мод-панель.
- **Прунинг** — автоудаление тредов старше `max_thread_age_days` / превышающих
  `thread_limit`.

## Безопасность

- IP хранится только как HMAC-SHA256; ротация через `security.old_secrets`.
- CSRF-защита и security-заголовки (CSP, X-Frame-Options, Referrer-Policy, HSTS).
- Rate-limit на постинг, проверка magic bytes, лимиты размера и разрешения.
- Реальный IP из `X-Forwarded-For` только от доверенных прокси.

## Конфигурация

Смотрите `config.example.toml`. Переменные окружения:

| Переменная | Назначение |
|---|---|
| `DATABASE_URL` | переопределяет `database.url` |
| `MICROCHAN_SECRET` | секрет HMAC для хэширования IP |
| `MICROCHAN_CONFIG` | путь к конфигу (по умолчанию `config.toml`) |
| `RUST_LOG` | уровень логирования |

## Развёртывание

Запускайте за reverse-прокси (nginx/caddy) с HTTPS. Включите `security.hsts = true`
при работе по HTTPS.

## Лицензия

Apache-2.0
