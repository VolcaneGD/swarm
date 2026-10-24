CREATE TABLE thread_pins (
    account_id TEXT NOT NULL REFERENCES accounts(id),
    thread_id TEXT NOT NULL REFERENCES threads(id),
    created_at TEXT NOT NULL,
    PRIMARY KEY (account_id, thread_id)
);
