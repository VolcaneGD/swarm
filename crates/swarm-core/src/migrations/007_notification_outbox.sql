CREATE TABLE notification_selection (
    singleton INTEGER PRIMARY KEY CHECK(singleton=1),
    account_id TEXT NOT NULL REFERENCES accounts(id),
    token_hash TEXT NOT NULL CHECK(length(token_hash)=64),
    generation TEXT NOT NULL UNIQUE CHECK(length(generation) BETWEEN 1 AND 64)
);

CREATE TABLE notification_outbox (
    delivery_id TEXT PRIMARY KEY CHECK(length(delivery_id) BETWEEN 1 AND 128),
    generation TEXT NOT NULL CHECK(length(generation) BETWEEN 1 AND 64),
    account_id TEXT NOT NULL REFERENCES accounts(id),
    thread_id TEXT NOT NULL REFERENCES threads(id),
    post_id TEXT NOT NULL REFERENCES posts(id),
    kind TEXT NOT NULL CHECK(kind IN ('post','response')),
    created_at TEXT NOT NULL,
    UNIQUE(generation,account_id,post_id)
);

CREATE INDEX notification_outbox_pending
    ON notification_outbox(generation,account_id,created_at,delivery_id);
