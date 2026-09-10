ALTER TABLE social_ai_drafts ADD COLUMN provider_json TEXT CHECK(provider_json IS NULL OR json_valid(provider_json));
CREATE TABLE expense_ai_usage (
 id TEXT PRIMARY KEY REFERENCES expense_ai_proposals(id), company_id TEXT NOT NULL, actor_id TEXT NOT NULL,
 month TEXT NOT NULL, state TEXT NOT NULL CHECK(state IN ('reserved','succeeded','failed')),
 lease_until INTEGER NOT NULL, input_tokens INTEGER NOT NULL DEFAULT 0,
 output_tokens INTEGER NOT NULL DEFAULT 0, created_at INTEGER NOT NULL
) STRICT;
CREATE INDEX expense_ai_quota ON expense_ai_usage(company_id,month);
