CREATE TABLE beta_policy (
  company_id TEXT PRIMARY KEY, member_limit INTEGER NOT NULL, branch_limit INTEGER NOT NULL,
  ai_monthly_limit INTEGER NOT NULL, storage_bytes INTEGER NOT NULL,
  created_at INTEGER NOT NULL
) STRICT;
CREATE TABLE social_post_drafts (
  id TEXT PRIMARY KEY, author_id TEXT NOT NULL, company_id TEXT NOT NULL, branch_id TEXT,
  body TEXT NOT NULL DEFAULT '' CHECK(length(body)<=3000), version INTEGER NOT NULL DEFAULT 1,
  published_post_id TEXT UNIQUE REFERENCES social_posts(id), created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL
) STRICT;
ALTER TABLE social_comments ADD COLUMN version INTEGER NOT NULL DEFAULT 1;
CREATE TABLE social_task_origins (
  post_id TEXT PRIMARY KEY REFERENCES social_posts(id), task_id TEXT NOT NULL UNIQUE REFERENCES staff_tasks(id),
  source_version INTEGER NOT NULL, source_hash TEXT NOT NULL, ai_draft_id TEXT,
  created_by TEXT NOT NULL, created_at INTEGER NOT NULL
) STRICT;
CREATE TABLE social_ai_drafts (
  id TEXT PRIMARY KEY, post_id TEXT NOT NULL REFERENCES social_posts(id), actor_id TEXT NOT NULL,
  kind TEXT NOT NULL CHECK(kind IN ('summary','task')), source_version INTEGER NOT NULL,
  source_hash TEXT NOT NULL, comment_ids_json TEXT NOT NULL CHECK(json_valid(comment_ids_json)),
  sources_json TEXT NOT NULL CHECK(json_valid(sources_json)), result_json TEXT CHECK(result_json IS NULL OR json_valid(result_json)),
  state TEXT NOT NULL CHECK(state IN ('pending','ready','failed')), model TEXT NOT NULL,
  created_at INTEGER NOT NULL, expires_at INTEGER NOT NULL
) STRICT;
CREATE INDEX social_ai_actor ON social_ai_drafts(actor_id,created_at);
CREATE TABLE social_ai_usage (
  id TEXT PRIMARY KEY REFERENCES social_ai_drafts(id), company_id TEXT NOT NULL, actor_id TEXT NOT NULL,
  month TEXT NOT NULL, state TEXT NOT NULL CHECK(state IN ('reserved','succeeded','failed')),
  input_tokens INTEGER NOT NULL DEFAULT 0, output_tokens INTEGER NOT NULL DEFAULT 0,
  lease_until INTEGER NOT NULL, created_at INTEGER NOT NULL
) STRICT;
CREATE INDEX social_ai_quota ON social_ai_usage(company_id,month);
CREATE TABLE social_reports (
  id TEXT PRIMARY KEY, post_id TEXT NOT NULL REFERENCES social_posts(id), reporter_id TEXT NOT NULL,
  reason TEXT NOT NULL CHECK(length(reason) BETWEEN 1 AND 2000), created_at INTEGER NOT NULL,
  UNIQUE(post_id,reporter_id)
) STRICT;
CREATE TABLE beta_feedback (
  id TEXT PRIMARY KEY, company_id TEXT NOT NULL, actor_id TEXT NOT NULL,
  body TEXT NOT NULL CHECK(length(body) BETWEEN 1 AND 3000), created_at INTEGER NOT NULL
) STRICT;
CREATE TABLE beta_members (
  user_id TEXT PRIMARY KEY, invitation_id TEXT UNIQUE, joined_at INTEGER NOT NULL
) STRICT;
