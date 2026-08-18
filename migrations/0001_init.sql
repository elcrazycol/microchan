-- Доски задаются конфигом; в БД хранятся контент и служебные сущности.

CREATE TABLE threads (
    id BIGINT PRIMARY KEY,          -- id поста-ОПа
    board TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_bump TIMESTAMPTZ NOT NULL DEFAULT now(),
    sticky BOOLEAN NOT NULL DEFAULT FALSE,
    locked BOOLEAN NOT NULL DEFAULT FALSE,
    post_count INT NOT NULL DEFAULT 1,
    deleted BOOLEAN NOT NULL DEFAULT FALSE
);

CREATE INDEX idx_threads_board_bump ON threads (board, last_bump DESC) WHERE NOT deleted;

CREATE TABLE posts (
    id BIGSERIAL PRIMARY KEY,
    -- NULL для ОПа (тред создаётся по id поста-ОПа)
    thread_id BIGINT REFERENCES threads(id) ON DELETE CASCADE,
    board TEXT NOT NULL,
    is_op BOOLEAN NOT NULL DEFAULT FALSE,
    name TEXT,
    tripcode TEXT,
    email TEXT,
    subject TEXT,
    body TEXT NOT NULL DEFAULT '',
    ip_hash TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted BOOLEAN NOT NULL DEFAULT FALSE,
    delete_reason TEXT
);

CREATE INDEX idx_posts_thread ON posts (thread_id, id);
CREATE INDEX idx_posts_board ON posts (board, id);

CREATE TABLE files (
    id BIGSERIAL PRIMARY KEY,
    post_id BIGINT NOT NULL REFERENCES posts(id) ON DELETE CASCADE,
    original_name TEXT NOT NULL,
    stored_name TEXT NOT NULL,
    mime TEXT NOT NULL,
    size BIGINT NOT NULL,
    width INT,
    height INT,
    sha256 TEXT NOT NULL,
    spoiler BOOLEAN NOT NULL DEFAULT FALSE,
    deleted BOOLEAN NOT NULL DEFAULT FALSE
);

CREATE INDEX idx_files_post ON files (post_id);

CREATE TABLE bans (
    id BIGSERIAL PRIMARY KEY,
    ip_hash TEXT,
    file_hash TEXT,
    reason TEXT NOT NULL,
    created_by TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ
);

CREATE INDEX idx_bans_ip ON bans (ip_hash);
CREATE INDEX idx_bans_file ON bans (file_hash);

CREATE TABLE reports (
    id BIGSERIAL PRIMARY KEY,
    post_id BIGINT NOT NULL REFERENCES posts(id) ON DELETE CASCADE,
    reason TEXT,
    ip_hash TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    resolved BOOLEAN NOT NULL DEFAULT FALSE
);

CREATE INDEX idx_reports_unresolved ON reports (resolved, created_at);

CREATE TABLE mod_logs (
    id BIGSERIAL PRIMARY KEY,
    moderator TEXT NOT NULL,
    action TEXT NOT NULL,
    target TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE mod_sessions (
    token TEXT PRIMARY KEY,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL
);
