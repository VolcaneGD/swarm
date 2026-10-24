CREATE TABLE app_metadata (key TEXT PRIMARY KEY, value TEXT NOT NULL);
ALTER TABLE posts ADD COLUMN editor_model_name TEXT;
UPDATE posts SET editor_model_name='USER' WHERE edited_by IS NOT NULL;
ALTER TABLE post_revisions ADD COLUMN editor_model_name TEXT NOT NULL DEFAULT 'USER';
