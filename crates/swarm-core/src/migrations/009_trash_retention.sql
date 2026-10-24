ALTER TABLE retention_settings ADD COLUMN deleted_board_days INTEGER
    CHECK(deleted_board_days IS NULL OR deleted_board_days>0);
ALTER TABLE retention_settings ADD COLUMN deleted_thread_days INTEGER
    CHECK(deleted_thread_days IS NULL OR deleted_thread_days>0);
ALTER TABLE retention_settings ADD COLUMN archived_thread_days INTEGER
    CHECK(archived_thread_days IS NULL OR archived_thread_days>0);

ALTER TABLE threads ADD COLUMN archived_at TEXT;
UPDATE threads
SET archived_at=COALESCE(
    (SELECT p.created_at FROM posts p WHERE p.thread_id=threads.id AND p.no=1000),
    (SELECT MAX(p.created_at) FROM posts p WHERE p.thread_id=threads.id)
)
WHERE next_no>1000;
