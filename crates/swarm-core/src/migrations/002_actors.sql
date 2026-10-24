-- Version 1 -> 2. Run atomically with FK enforcement temporarily disabled, then check all foreign keys.
CREATE TABLE actors (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL CHECK(kind IN ('human','ai'))
);
INSERT INTO actors(id,kind) SELECT id,'human' FROM accounts;
CREATE TABLE display_ids (
    display_id TEXT PRIMARY KEY,
    actor_id TEXT NOT NULL REFERENCES actors(id)
);
INSERT INTO display_ids(display_id,actor_id) SELECT display_id,account_id FROM human_display_ids;
CREATE TABLE projects (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    canonical_root TEXT NOT NULL COLLATE NOCASE UNIQUE,
    created_at TEXT NOT NULL
);
CREATE TABLE project_paths (
    path TEXT PRIMARY KEY COLLATE NOCASE,
    project_id TEXT NOT NULL REFERENCES projects(id)
);
CREATE TABLE ai_identities (
    id TEXT PRIMARY KEY REFERENCES actors(id),
    project_id TEXT NOT NULL REFERENCES projects(id),
    model_name TEXT NOT NULL,
    model_source TEXT NOT NULL CHECK(model_source IN ('unknown','declared','automatic')),
    display_id TEXT NOT NULL UNIQUE REFERENCES display_ids(display_id),
    expires_at TEXT NOT NULL,
    resume_hash TEXT NOT NULL,
    created_at TEXT NOT NULL
);
CREATE TABLE ai_sessions (
    token_hash TEXT PRIMARY KEY,
    actor_id TEXT NOT NULL UNIQUE REFERENCES ai_identities(id),
    session_id TEXT NOT NULL UNIQUE
);
CREATE TABLE boards_new (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL CHECK(length(trim(name))>0),
    description TEXT NOT NULL,
    creator_id TEXT NOT NULL REFERENCES actors(id),
    revision INTEGER NOT NULL DEFAULT 1 CHECK(revision>0),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted_at TEXT
);
INSERT INTO boards_new SELECT * FROM boards;
CREATE TABLE threads_new (
    id TEXT PRIMARY KEY,
    board_id TEXT NOT NULL REFERENCES boards(id),
    name TEXT NOT NULL CHECK(length(trim(name))>0),
    description TEXT NOT NULL,
    creator_id TEXT NOT NULL REFERENCES actors(id),
    revision INTEGER NOT NULL DEFAULT 1 CHECK(revision>0),
    next_no INTEGER NOT NULL DEFAULT 1 CHECK(next_no>0),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted_at TEXT
);
INSERT INTO threads_new SELECT * FROM threads;
CREATE TABLE posts_new (
    id TEXT PRIMARY KEY,
    thread_id TEXT NOT NULL REFERENCES threads(id),
    no INTEGER NOT NULL CHECK(no>0),
    body TEXT NOT NULL,
    author_id TEXT NOT NULL REFERENCES actors(id),
    model_name TEXT NOT NULL,
    display_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    revision INTEGER NOT NULL DEFAULT 1 CHECK(revision>0),
    reply_to INTEGER,
    edited_by TEXT REFERENCES actors(id),
    editor_display_id TEXT,
    deleted_at TEXT,
    UNIQUE(thread_id,no),
    FOREIGN KEY(thread_id,reply_to) REFERENCES posts(thread_id,no)
);
INSERT INTO posts_new SELECT * FROM posts;
CREATE TABLE post_revisions_new (
    post_id TEXT NOT NULL REFERENCES posts(id),
    revision INTEGER NOT NULL,
    body TEXT NOT NULL,
    saved_at TEXT NOT NULL,
    editor_id TEXT NOT NULL REFERENCES actors(id),
    editor_display_id TEXT NOT NULL,
    PRIMARY KEY(post_id,revision)
);
INSERT INTO post_revisions_new SELECT * FROM post_revisions;
CREATE TABLE changes_new (
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    operation TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    actor_id TEXT NOT NULL REFERENCES actors(id),
    created_at TEXT NOT NULL
);
INSERT INTO changes_new SELECT * FROM changes;
CREATE TABLE create_requests_new (
    actor_id TEXT NOT NULL REFERENCES actors(id),
    request_id TEXT NOT NULL,
    fingerprint TEXT NOT NULL,
    operation TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY(actor_id,request_id)
);
INSERT INTO create_requests_new SELECT * FROM create_requests;
UPDATE sqlite_sequence SET seq=MAX(seq,COALESCE((SELECT seq FROM sqlite_sequence WHERE name='changes'),0)) WHERE name='changes_new';
DROP TABLE create_requests;
DROP TABLE changes;
DROP TABLE post_revisions;
DROP TABLE posts;
DROP TABLE threads;
DROP TABLE boards;
ALTER TABLE boards_new RENAME TO boards;
ALTER TABLE threads_new RENAME TO threads;
ALTER TABLE posts_new RENAME TO posts;
ALTER TABLE post_revisions_new RENAME TO post_revisions;
ALTER TABLE changes_new RENAME TO changes;
ALTER TABLE create_requests_new RENAME TO create_requests;
CREATE INDEX posts_replies ON posts(thread_id,reply_to,deleted_at);
CREATE TRIGGER posts_immutable_identity BEFORE UPDATE OF id,thread_id,no,author_id,model_name,display_id,created_at,reply_to ON posts
BEGIN SELECT RAISE(ABORT,'Post identity and reply target are immutable'); END;
CREATE TRIGGER history_immutable_update BEFORE UPDATE ON post_revisions
BEGIN SELECT RAISE(ABORT,'Revision history is immutable'); END;
