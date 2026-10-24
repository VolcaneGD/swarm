CREATE TABLE retention_settings (
    singleton INTEGER PRIMARY KEY CHECK(singleton=1),
    history_days INTEGER CHECK(history_days IS NULL OR history_days>0),
    deleted_days INTEGER CHECK(deleted_days IS NULL OR deleted_days>0)
);
INSERT INTO retention_settings(singleton,history_days,deleted_days) VALUES(1,NULL,NULL);

ALTER TABLE posts ADD COLUMN body_purged_at TEXT;
CREATE INDEX post_revisions_saved_at ON post_revisions(saved_at,post_id);
CREATE TRIGGER posts_purged_body_immutable BEFORE UPDATE OF body,body_purged_at ON posts
WHEN (OLD.body_purged_at IS NOT NULL AND
      (NEW.body_purged_at IS NOT OLD.body_purged_at OR NEW.body!='')) OR
     (NEW.body_purged_at IS NOT NULL AND NEW.body!='')
BEGIN SELECT RAISE(ABORT,'Purged Post body is immutable'); END;
