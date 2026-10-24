CREATE TABLE thread_subscriptions (
    account_id TEXT NOT NULL REFERENCES accounts(id),
    thread_id TEXT NOT NULL REFERENCES threads(id),
    created_at TEXT NOT NULL,
    PRIMARY KEY (account_id, thread_id)
);

CREATE TABLE notification_post_reads (
    account_id TEXT NOT NULL REFERENCES accounts(id),
    post_id TEXT NOT NULL REFERENCES posts(id),
    read_at TEXT NOT NULL,
    PRIMARY KEY (account_id, post_id)
);

CREATE TABLE notification_settings (
    account_id TEXT PRIMARY KEY REFERENCES accounts(id),
    enabled INTEGER NOT NULL CHECK(enabled IN (0,1)),
    include_excerpt INTEGER NOT NULL CHECK(include_excerpt IN (0,1))
);
