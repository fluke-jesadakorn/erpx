-- Attachment policies for People, Payroll, Talent, Learning and Benefits must
-- never fall back to the broad records.* permissions.  Keep these grants
-- purpose-specific so the registry can fail closed for sensitive documents.
WITH permissions(id,label,roles,conditions,scopes) AS (VALUES
  ('people.documents.read','Read restricted employee documents','["holding_owner","hr_admin"]'::jsonb,'["Exact employee scope and purpose policy"]'::jsonb,'["holding","branch","department"]'::jsonb),
  ('people.documents.manage','Manage restricted employee documents','["holding_owner","hr_admin"]'::jsonb,'["Exact employee scope, purpose policy and current record state"]'::jsonb,'["holding","branch","department"]'::jsonb),
  ('payroll.documents.read','Read restricted payroll documents','["holding_owner","hr_admin","payroll_preparer","payroll_approver","auditor"]'::jsonb,'["Holding scope and exact payroll record"]'::jsonb,'["holding"]'::jsonb),
  ('payroll.documents.manage','Manage restricted payroll documents','["holding_owner","hr_admin","payroll_preparer"]'::jsonb,'["Holding scope and exact payroll record"]'::jsonb,'["holding"]'::jsonb),
  ('talent.documents.read','Read restricted talent documents','["holding_owner","hr_admin","people_manager"]'::jsonb,'["Assigned requisition or candidate scope and exact document purpose"]'::jsonb,'["holding","branch","department"]'::jsonb),
  ('talent.documents.manage','Manage restricted talent documents','["holding_owner","hr_admin"]'::jsonb,'["Assigned requisition or candidate scope and exact document purpose"]'::jsonb,'["holding","branch","department"]'::jsonb),
  ('learning.documents.read','Read learning documents','["holding_owner","hr_admin","people_manager"]'::jsonb,'["Exact course, enrollment or employee scope"]'::jsonb,'["holding","branch","department"]'::jsonb),
  ('learning.documents.manage','Manage learning documents','["holding_owner","hr_admin"]'::jsonb,'["Exact course, enrollment or employee scope"]'::jsonb,'["holding","branch","department"]'::jsonb),
  ('benefits.review','Review benefit records','["holding_owner","hr_admin","people_manager"]'::jsonb,'["Exact benefit record and independent workflow step"]'::jsonb,'["holding","branch","department"]'::jsonb),
  ('benefits.documents.read','Read restricted benefit documents','["holding_owner","hr_admin"]'::jsonb,'["Exact benefit record; employee ownership never grants access to internal HR documents"]'::jsonb,'["holding","branch","department"]'::jsonb),
  ('benefits.documents.manage','Manage restricted benefit documents','["holding_owner","hr_admin"]'::jsonb,'["Exact benefit record and purpose policy"]'::jsonb,'["holding","branch","department"]'::jsonb)
)
INSERT INTO iam_permissions(id,definition)
SELECT id,jsonb_build_object(
  'id',id,'label',label,'module','people_payroll','namespace','customer',
  'availability','live','risk','privileged','roles',roles,
  'conditions',conditions,'permittedScopes',scopes,'identityBased',false
)
FROM permissions
ON CONFLICT(id) DO UPDATE SET definition=excluded.definition;

WITH grants(role_id,permissions,grantable) AS (VALUES
  ('holding_owner','["people.documents.read","people.documents.manage","payroll.documents.read","payroll.documents.manage","talent.documents.read","talent.documents.manage","learning.documents.read","learning.documents.manage","benefits.review","benefits.documents.read","benefits.documents.manage"]'::jsonb,'["people.documents.read","people.documents.manage","payroll.documents.read","payroll.documents.manage","talent.documents.read","talent.documents.manage","learning.documents.read","learning.documents.manage","benefits.review","benefits.documents.read","benefits.documents.manage"]'::jsonb),
  ('hr_admin','["people.documents.read","people.documents.manage","payroll.documents.read","payroll.documents.manage","talent.documents.read","talent.documents.manage","learning.documents.read","learning.documents.manage","benefits.review","benefits.documents.read","benefits.documents.manage"]'::jsonb,'[]'::jsonb),
  ('people_manager','["talent.documents.read","learning.documents.read","benefits.review"]'::jsonb,'[]'::jsonb),
  ('payroll_preparer','["payroll.documents.read","payroll.documents.manage"]'::jsonb,'[]'::jsonb),
  ('payroll_approver','["payroll.documents.read"]'::jsonb,'[]'::jsonb),
  ('auditor','["payroll.documents.read"]'::jsonb,'[]'::jsonb)
), selected AS (
  SELECT r.company_id,r.id role_id,r.published_revision,
    v.permissions_json,v.grantable_json,v.scopes_json,g.permissions,g.grantable
  FROM iam_roles r
  JOIN iam_role_revisions v
    ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision
  JOIN grants g ON g.role_id=r.id
  WHERE r.company_id='*'
), next_revisions AS (
  INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)
  SELECT company_id,role_id,published_revision+1,
    COALESCE((SELECT jsonb_agg(value ORDER BY value) FROM (
      SELECT DISTINCT jsonb_array_elements_text(permissions_json || permissions) value
    ) merged_permissions),'[]'::jsonb),
    COALESCE((SELECT jsonb_agg(value ORDER BY value) FROM (
      SELECT DISTINCT jsonb_array_elements_text(grantable_json || grantable) value
    ) merged_grantable),'[]'::jsonb),
    scopes_json,'system:sensitive-documents',floor(extract(epoch FROM clock_timestamp())*1000)::bigint
  FROM selected
  WHERE NOT permissions_json @> permissions OR NOT grantable_json @> grantable
  RETURNING company_id,role_id,revision
)
UPDATE iam_roles AS role
SET published_revision=next_revisions.revision
FROM next_revisions
WHERE role.company_id=next_revisions.company_id AND role.id=next_revisions.role_id;
