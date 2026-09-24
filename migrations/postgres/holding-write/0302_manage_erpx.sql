INSERT INTO iam_permissions(id,definition) VALUES (
  'manage_erpx.access',
  '{"id":"manage_erpx.access","label":"Access Manage ERPX","module":"manage_erpx","roles":["holding_owner"],"conditions":["ERPX Holding and active Platform assignment"],"risk":"privileged","availability":"live","namespace":"customer","permittedScopes":["holding"]}'::jsonb
) ON CONFLICT(id) DO UPDATE SET definition=excluded.definition;

-- Published revisions are immutable. Extend only the built-in owner role;
-- the runtime additionally restricts this permission to the ERPX Holding.
WITH selected_role AS (
  SELECT r.company_id,r.id,r.published_revision,v.permissions_json,v.grantable_json,v.scopes_json
  FROM iam_roles r JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision
  WHERE r.company_id='*' AND r.id='holding_owner'
), next_revision AS (
  INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)
  SELECT company_id,id,published_revision+1,
    permissions_json || '["manage_erpx.access"]'::jsonb,
    grantable_json || '["manage_erpx.access"]'::jsonb,
    scopes_json,'system',floor(extract(epoch FROM clock_timestamp())*1000)::bigint
  FROM selected_role WHERE NOT permissions_json ? 'manage_erpx.access'
  RETURNING revision
)
UPDATE iam_roles SET published_revision=(SELECT revision FROM next_revision)
WHERE company_id='*' AND id='holding_owner' AND EXISTS(SELECT 1 FROM next_revision);
