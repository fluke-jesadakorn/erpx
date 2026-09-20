ALTER TABLE pos_payments ADD COLUMN provider_received_at bigint;
ALTER TABLE pos_sales ADD COLUMN issuer_json text NOT NULL DEFAULT '{}';
INSERT INTO iam_role_revisions
 SELECT r.company_id,r.id,r.published_revision+1,v.permissions_json||'["pos.approve"]'::jsonb,v.grantable_json,v.scopes_json,'system',floor(extract(epoch FROM clock_timestamp())*1000)::bigint
 FROM iam_roles r JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision
 WHERE r.company_id='*' AND r.id='finance_approver';
UPDATE iam_roles r SET published_revision=(SELECT MAX(revision) FROM iam_role_revisions v WHERE v.company_id=r.company_id AND v.role_id=r.id) WHERE r.company_id='*' AND r.id='finance_approver';
UPDATE iam_permissions SET definition=jsonb_set(definition,'{roles}',(definition->'roles')||'"finance_approver"'::jsonb) WHERE id='pos.approve';
