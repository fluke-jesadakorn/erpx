CREATE TABLE IF NOT EXISTS navigation_preferences (
 company_id TEXT NOT NULL,
 user_id TEXT NOT NULL,
 workspace_base TEXT NOT NULL,
 pinned_ids_json TEXT,
 version INTEGER NOT NULL DEFAULT 1,
 updated_at BIGINT NOT NULL,
 PRIMARY KEY (company_id, user_id, workspace_base)
);
