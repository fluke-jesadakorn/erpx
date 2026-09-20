CREATE TABLE ai_workspace_turns (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  user_id TEXT NOT NULL,
  assignment_id TEXT NOT NULL,
  scope_key TEXT NOT NULL,
  conversation_key TEXT NOT NULL,
  question TEXT NOT NULL,
  paths_json TEXT NOT NULL,
  sources_json TEXT,
  answer_json TEXT,
  state TEXT NOT NULL CHECK (state IN ('running','ready','failed')),
  error TEXT,
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL
);
CREATE INDEX ai_workspace_history ON ai_workspace_turns(company_id,user_id,assignment_id,scope_key,conversation_key,created_at DESC);
CREATE INDEX ai_workspace_quota ON ai_workspace_turns(company_id,created_at DESC);

-- Cross-page chat is an explicit permission, available for reviewed custom roles/policies.
-- Existing custom roles are never silently expanded.
INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)
SELECT r.company_id,r.id,r.published_revision+1,
  v.permissions_json || '["ai.context.cross"]'::jsonb,
  v.grantable_json || '["ai.context.cross"]'::jsonb,
  v.scopes_json,'system:workspace-assistant',floor(extract(epoch FROM clock_timestamp())*1000)::bigint
FROM iam_roles r JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision
WHERE r.company_id='*' AND r.id='holding_owner' AND NOT v.permissions_json @> '["ai.context.cross"]'::jsonb;
UPDATE iam_roles r SET published_revision=(SELECT max(revision) FROM iam_role_revisions v WHERE v.company_id=r.company_id AND v.role_id=r.id)
WHERE r.company_id='*' AND r.id='holding_owner';
