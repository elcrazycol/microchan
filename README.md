# microchan

Минималистичный imageboard-движок на Rust. Монолит, server-side rendering,
никакого bloat: только то, что нужно для классического imageboard-опыта.

## Стек

- Rust (stable), Axum
- PostgreSQL, SQLx (compile-time checked)
- Askama (SSR-шаблоны)
- image/webp для превью, ffmpeg (опционально) для видео-превью
- tracing

## Требования

- Rust (stable)
- PostgreSQL 16+ (запущенный сервер с базой)
- ffmpeg (опционально, для превью видео)

## Быстрый старт

```bash
# 1. Создать базу данных
createdb microchan

# 2. Настроить конфиг (секреты обязательны)
cp config.example.toml config.toml
$EDITOR config.toml

# 3. Запустить (DATABASE_URL нужен и для сборки — compile-time checked SQLx)
export DATABASE_URL=postgres://localhost/microchan
cargo run
```

При первом запуске миграции применяются автоматически. Доски задаются только
через `config.toml`.

## Безопасность

- IP никогда не хранится в открытом виде — только HMAC-SHA256 с секретом из
  конфига; поддерживается ротация секрета (`security.old_secrets`).
- CSRF-защита на всех формах (double-submit cookie).
- Security-заголовки: CSP, X-Frame-Options, Referrer-Policy, X-Content-Type-Options,
  HSTS (при HTTPS).
- Проверка magic bytes загружаемых файлов, лимиты размера/разрешения.
- Rate-limit на постинг по хэшу IP.
- Поддержка работы за обратным прокси с передачей реального IP через
  `X-Forwarded-For` (проверка доверенных прокси из конфига).

## Конфигурация

Смотрите `config.example.toml`. Переменные окружения:

| Переменная | Назначение |
|---|---|
| `DATABASE_URL` | строка подключения к PostgreSQL |
| `MICROCHAN_SECRET` | секрет для HMAC-хэширования IP |
| `MICROCHAN_CONFIG` | путь к конфигу (по умолчанию `config.toml`) |
| `RUST_LOG` | уровень логирования |

## Лицензия

Apache-2.0
