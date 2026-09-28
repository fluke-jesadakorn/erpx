-- Keep the source of AI usage and the workspace attribution fixed when the
-- idempotent usage row is first written. Existing rows deliberately remain
-- unassigned; a missing connection alone cannot prove who paid the provider.
ALTER TABLE ai_provider_usage
  ADD COLUMN branch_id TEXT,
  ADD COLUMN funding_source TEXT NOT NULL DEFAULT 'unknown'
    CHECK (funding_source IN ('erpx','byok','simulation','unknown'));

CREATE INDEX ai_provider_usage_month_scope
  ON ai_provider_usage(created_at,company_id,branch_id);

-- A connection is a customer-managed provider. Legacy deployment rows are
-- still unknown because their deployment snapshot was not stored on the row.
UPDATE ai_provider_usage SET funding_source='byok'
  WHERE connection_id IS NOT NULL;

INSERT INTO iam_permissions(id,definition) VALUES(
  'billing.usage.read',
  '{"id":"billing.usage.read","label":"View measured usage","module":"billing","roles":["holding_owner","operations_manager"],"conditions":["Holding owners see their Holding; branch operations managers see only their selected Branch"],"risk":"normal","availability":"live","namespace":"customer","permittedScopes":["holding","branch"]}'::jsonb
) ON CONFLICT(id) DO UPDATE SET definition=excluded.definition;

INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)
SELECT r.company_id,r.id,r.published_revision+1,
  v.permissions_json || '["billing.usage.read"]'::jsonb,
  v.grantable_json,v.scopes_json,'system:usage-meter',floor(extract(epoch FROM clock_timestamp())*1000)::bigint
FROM iam_roles r JOIN iam_role_revisions v
  ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision
WHERE r.company_id='*' AND r.built_in AND r.id IN ('holding_owner','operations_manager')
  AND NOT v.permissions_json @> '["billing.usage.read"]'::jsonb;
UPDATE iam_roles r SET published_revision=(
  SELECT max(revision) FROM iam_role_revisions v WHERE v.company_id=r.company_id AND v.role_id=r.id
) WHERE r.company_id='*' AND r.id IN ('holding_owner','operations_manager');
