CREATE TABLE reports (
  id TEXT PRIMARY KEY,
  schema_version INTEGER NOT NULL CHECK(schema_version=1),
  app_version TEXT NOT NULL,
  category TEXT NOT NULL CHECK(category IN ('permissions','accounts','gui','cli','runtime','data','backup_restore','notifications','updates','installation','ai_integration','security','other')),
  severity TEXT NOT NULL CHECK(severity IN ('critical','high','medium','low')),
  reported_severity TEXT NOT NULL,
  summary TEXT NOT NULL,
  payload TEXT NOT NULL,
  payload_digest TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'new' CHECK(status IN ('new','investigating','resolved')),
  revision INTEGER NOT NULL DEFAULT 1,
  created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  resolved_at TEXT,
  resolution_note TEXT NOT NULL DEFAULT ''
);
CREATE INDEX reports_triage ON reports(status,severity,category,created_at);
CREATE TRIGGER report_capacity BEFORE INSERT ON reports
WHEN (SELECT COUNT(*) FROM reports)>=10000
BEGIN SELECT RAISE(ABORT,'REPORT_CAPACITY'); END;
