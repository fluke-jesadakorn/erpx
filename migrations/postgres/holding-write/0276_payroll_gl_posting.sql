-- Finance approvers own the controlled GL posting step after Payroll approval.
UPDATE iam_permissions
SET definition=jsonb_set(definition,'{roles}',(definition->'roles')||'"finance_approver"'::jsonb)
WHERE id='payroll.read' AND NOT (definition->'roles' ? 'finance_approver');

UPDATE iam_permissions
SET definition=jsonb_set(definition,'{conditions}','["Approved payroll run posted to the Finance ledger"]'::jsonb)
WHERE id='payroll.payment.prepare';

INSERT INTO iam_role_revisions
SELECT r.company_id,r.id,r.published_revision+1,
  v.permissions_json||'["payroll.read","module.people_payroll.read"]'::jsonb,
  v.grantable_json,v.scopes_json,'system',floor(extract(epoch FROM clock_timestamp())*1000)::bigint
FROM iam_roles r
JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision
WHERE r.company_id='*' AND r.id='finance_approver'
  AND NOT (v.permissions_json ? 'payroll.read');

UPDATE iam_roles r
SET published_revision=(SELECT max(revision) FROM iam_role_revisions v WHERE v.company_id=r.company_id AND v.role_id=r.id)
WHERE r.company_id='*' AND r.id='finance_approver';
