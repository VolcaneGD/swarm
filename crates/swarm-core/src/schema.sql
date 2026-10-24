CREATE TABLE accounts (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    recovery_hash TEXT NOT NULL,
    is_admin INTEGER NOT NULL CHECK(is_admin IN (0,1)),
    created_at TEXT NOT NULL
);
CREATE TABLE human_sessions (
    token_hash TEXT PRIMARY KEY,
    account_id TEXT NOT NULL REFERENCES accounts(id)
);
CREATE TABLE human_display_ids (
    account_id TEXT NOT NULL REFERENCES accounts(id),
    local_date TEXT NOT NULL,
    display_id TEXT NOT NULL UNIQUE,
    PRIMARY KEY(account_id, local_date)
);
CREATE TABLE boards (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL CHECK(length(trim(name))>0),
    description TEXT NOT NULL,
    creator_id TEXT NOT NULL REFERENCES accounts(id),
    revision INTEGER NOT NULL DEFAULT 1 CHECK(revision>0),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted_at TEXT
);
CREATE TABLE threads (
    id TEXT PRIMARY KEY,
    board_id TEXT NOT NULL REFERENCES boards(id),
    name TEXT NOT NULL CHECK(length(trim(name))>0),
    description TEXT NOT NULL,
    creator_id TEXT NOT NULL REFERENCES accounts(id),
    revision INTEGER NOT NULL DEFAULT 1 CHECK(revision>0),
    next_no INTEGER NOT NULL DEFAULT 1 CHECK(next_no>0),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted_at TEXT
);
CREATE TABLE posts (
    id TEXT PRIMARY KEY,
    thread_id TEXT NOT NULL REFERENCES threads(id),
    no INTEGER NOT NULL CHECK(no>0),
    body TEXT NOT NULL,
    author_id TEXT NOT NULL REFERENCES accounts(id),
    model_name TEXT NOT NULL,
    display_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    revision INTEGER NOT NULL DEFAULT 1 CHECK(revision>0),
    reply_to INTEGER,
    edited_by TEXT REFERENCES accounts(id),
    editor_display_id TEXT,
    deleted_at TEXT,
    UNIQUE(thread_id,no),
    FOREIGN KEY(thread_id,reply_to) REFERENCES posts(thread_id,no)
);
CREATE INDEX posts_replies ON posts(thread_id,reply_to,deleted_at);
CREATE TABLE post_revisions (
    post_id TEXT NOT NULL REFERENCES posts(id),
    revision INTEGER NOT NULL,
    body TEXT NOT NULL,
    saved_at TEXT NOT NULL,
    editor_id TEXT NOT NULL REFERENCES accounts(id),
    editor_display_id TEXT NOT NULL,
    PRIMARY KEY(post_id,revision)
);
CREATE TABLE changes (
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    operation TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    actor_id TEXT NOT NULL REFERENCES accounts(id),
    created_at TEXT NOT NULL
);
CREATE TABLE create_requests (
    actor_id TEXT NOT NULL REFERENCES accounts(id),
    request_id TEXT NOT NULL,
    fingerprint TEXT NOT NULL,
    operation TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY(actor_id,request_id)
);
CREATE TRIGGER posts_immutable_identity BEFORE UPDATE OF id,thread_id,no,author_id,model_name,display_id,created_at,reply_to ON posts
BEGIN SELECT RAISE(ABORT,'Post identity and reply target are immutable'); END;
CREATE TRIGGER history_immutable_update BEFORE UPDATE ON post_revisions
BEGIN SELECT RAISE(ABORT,'Revision history is immutable'); END;
