# microchan

A minimal imageboard engine in Rust: a monolith, server-side rendered,
anonymous by default, no bloat. Just the classic imageboard experience, done well.

**Читать на русском: [README_ru.md](README_ru.md).**

## Requirements

- Rust (stable)
- PostgreSQL 16+
- `ffmpeg` (optional, for video thumbnails only)

## Quick start

```bash
createdb microchan
cp config.example.toml config.toml
$EDITOR config.toml   # database.url, security.secret, moderation.admin_password
cargo run --release
```

Migrations run automatically on first start. Boards are managed only via `config.toml`.

Set your PostgreSQL username in `database.url` (usually your OS user, `whoami`):

```toml
[database]
url = "postgres://yourname@localhost/microchan"
```

Building needs no database — the SQLx offline cache (`.sqlx/`) is committed. After
changing SQL queries, refresh it:

```bash
export DATABASE_URL=postgres://yourname@localhost/microchan
cargo sqlx prepare
```

## Features

- **Boards** — config-managed: title, description, NSFW, thread limit,
  files-per-post limit, allowed extensions, max file size.
- **Threads & posts** — sage, bump limit, sticky/lock, `>>123` quotes,
  greentext, `[spoiler]…[/spoiler]`, classic (`#`) and secure (`##`) tripcodes,
  subject, email.
- **Media** — 1..N files per post (JPEG/PNG/WebP/GIF/WebM/MP4), magic-byte
  validation, thumbnails, image spoilers.
- **Catalog & navigation** — paginated board index, thumbnail grid, jump-to-post.
- **Moderation** — delete post/thread, IP/file-hash bans, reports, action log,
  simple mod panel.
- **Pruning** — auto-removal of threads over `max_thread_age_days` / `thread_limit`.

## Security

- IPs stored only as HMAC-SHA256; rotation via `security.old_secrets`.
- CSRF protection and security headers (CSP, X-Frame-Options, Referrer-Policy, HSTS).
- Posting rate limit, magic-byte checks, size/resolution limits.
- Real IP from `X-Forwarded-For` only when the proxy is trusted.

## Configuration

See `config.example.toml`. Environment variables:

| Variable | Purpose |
|---|---|
| `DATABASE_URL` | overrides `database.url` |
| `MICROCHAN_SECRET` | HMAC secret for IP hashing |
| `MICROCHAN_CONFIG` | config path (default `config.toml`) |
| `RUST_LOG` | log level |

## Deployment

Run behind a reverse proxy (nginx/caddy) with HTTPS. Enable `security.hsts = true`
when serving over HTTPS.

## License

Apache-2.0
