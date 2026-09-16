CREATE TABLE account_workspace_preferences (
 user_id TEXT PRIMARY KEY,
 workspace_path TEXT,
 platform_assignment_id TEXT,
 updated_at BIGINT NOT NULL
);
CREATE TABLE account_company_preferences (
 user_id TEXT NOT NULL,
 holding_id TEXT NOT NULL,
 company_id TEXT NOT NULL,
 updated_at BIGINT NOT NULL,
 PRIMARY KEY(user_id,holding_id)
);
