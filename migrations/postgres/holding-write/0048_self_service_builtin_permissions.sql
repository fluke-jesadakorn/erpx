-- Repair the earlier 0041 variant without changing Company-defined custom roles.
-- Existing built-in revisions remain immutable; publish a revision only when needed.
INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)
 SELECT r.company_id,r.id,r.published_revision+1,
 v.permissions_json || CASE WHEN v.permissions_json @> '["leave.self"]'::jsonb THEN '[]'::jsonb ELSE '["leave.self"]'::jsonb END ||
 CASE WHEN r.id IN ('holding_owner','hr_admin') AND NOT v.permissions_json @> '["leave.policy.manage"]'::jsonb THEN '["leave.policy.manage"]'::jsonb ELSE '[]'::jsonb END,
 v.grantable_json || CASE WHEN r.id IN ('holding_owner','hr_admin') THEN
  CASE WHEN v.grantable_json @> '["leave.self"]'::jsonb THEN '[]'::jsonb ELSE '["leave.self"]'::jsonb END ||
  CASE WHEN v.grantable_json @> '["leave.policy.manage"]'::jsonb THEN '[]'::jsonb ELSE '["leave.policy.manage"]'::jsonb END
 ELSE '[]'::jsonb END,
 v.scopes_json,'system:self-service-repair',floor(extract(epoch FROM clock_timestamp())*1000)::bigint
 FROM iam_roles r JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision
 WHERE r.company_id='*' AND (NOT v.permissions_json @> '["leave.self"]'::jsonb OR
 (r.id IN ('holding_owner','hr_admin') AND NOT v.permissions_json @> '["leave.policy.manage"]'::jsonb));
UPDATE iam_roles r SET published_revision=(SELECT max(revision) FROM iam_role_revisions v WHERE v.company_id=r.company_id AND v.role_id=r.id)
 WHERE r.company_id='*';
