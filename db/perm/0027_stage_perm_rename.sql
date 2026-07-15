-- Folio — Migrate stage:* permission IDs from legacy to canonical.
--
-- The waybill foundation migration renamed expense stage VALUES (column data)
-- but did not update the stage:*:act:all permission IDs themselves.
-- This migration adds the canonical permission IDs and propagates the
-- existing grants from the legacy IDs. Legacy IDs are kept for backward
-- compatibility (no DELETE) so any client still reading them keeps working.

BEGIN;

-- Canonical :act:all
INSERT INTO perm.permissions (domain, subject, verb, scope, description) VALUES
  ('stage', 'dept_verification',          'act', 'all',  'Approve dept verification stage'),
  ('stage', 'dept_authorization',         'act', 'all',  'Authorize at dept level'),
  ('stage', 'accounting_verification',    'act', 'all',  'Accounting officer line-item check'),
  ('stage', 'accounting_supervision',     'act', 'all',  'Accounting supervisor quality control'),
  ('stage', 'accounting_authorization',   'act', 'all',  'Accounting manager recognizes expense'),
  ('stage', 'disbursement_authorization', 'act', 'all',  'Finance releases funds'),
  ('stage', 'cfo_authorization',          'act', 'all',  'CFO fiscal sign-off'),
  ('stage', 'ceo_authorization',          'act', 'all',  'CEO fiscal sign-off'),
  ('stage', 'submission',                 'act', 'all',  'Submit slip / OCR auto-extracted')
ON CONFLICT DO NOTHING;

-- Canonical :act:dept
INSERT INTO perm.permissions (domain, subject, verb, scope, description) VALUES
  ('stage', 'dept_verification',          'act', 'dept', 'Approve dept verification stage (own dept)'),
  ('stage', 'dept_authorization',         'act', 'dept', 'Authorize at dept level (own dept)'),
  ('stage', 'accounting_verification',    'act', 'dept', 'Accounting officer line-item check (own dept)'),
  ('stage', 'accounting_supervision',     'act', 'dept', 'Accounting supervisor quality control (own dept)'),
  ('stage', 'accounting_authorization',   'act', 'dept', 'Accounting manager recognizes expense (own dept)'),
  ('stage', 'disbursement_authorization', 'act', 'dept', 'Finance releases funds (own dept)'),
  ('stage', 'cfo_authorization',          'act', 'dept', 'CFO fiscal sign-off (own dept)'),
  ('stage', 'ceo_authorization',          'act', 'dept', 'CEO fiscal sign-off (own dept)'),
  ('stage', 'submission',                 'act', 'dept', 'Submit slip (own dept)')
ON CONFLICT DO NOTHING;

-- Propagate grants :act:all
INSERT INTO perm.role_permissions (role_id, permission_id, effect, granted_by)
SELECT rp.role_id,
       CASE rp.permission_id
         WHEN 'stage:supervisor_review:act:all'          THEN 'stage:dept_verification:act:all'
         WHEN 'stage:manager_review:act:all'             THEN 'stage:dept_authorization:act:all'
         WHEN 'stage:account_officer_review:act:all'     THEN 'stage:accounting_verification:act:all'
         WHEN 'stage:account_supervisor_review:act:all'  THEN 'stage:accounting_supervision:act:all'
         WHEN 'stage:accounting_review:act:all'          THEN 'stage:accounting_authorization:act:all'
         WHEN 'stage:finance_review:act:all'             THEN 'stage:disbursement_authorization:act:all'
         WHEN 'stage:cfo_review:act:all'                 THEN 'stage:cfo_authorization:act:all'
         WHEN 'stage:ceo_review:act:all'                 THEN 'stage:ceo_authorization:act:all'
         WHEN 'stage:ocr_extracted:act:all'              THEN 'stage:submission:act:all'
       END,
       rp.effect,
       'migration:0027-stage-perm-rename'
  FROM perm.role_permissions rp
 WHERE rp.permission_id IN (
   'stage:supervisor_review:act:all',
   'stage:manager_review:act:all',
   'stage:account_officer_review:act:all',
   'stage:account_supervisor_review:act:all',
   'stage:accounting_review:act:all',
   'stage:finance_review:act:all',
   'stage:cfo_review:act:all',
   'stage:ceo_review:act:all',
   'stage:ocr_extracted:act:all'
 )
ON CONFLICT (role_id, permission_id) DO NOTHING;

-- Propagate grants :act:dept
INSERT INTO perm.role_permissions (role_id, permission_id, effect, granted_by)
SELECT rp.role_id,
       CASE rp.permission_id
         WHEN 'stage:supervisor_review:act:dept'          THEN 'stage:dept_verification:act:dept'
         WHEN 'stage:manager_review:act:dept'             THEN 'stage:dept_authorization:act:dept'
         WHEN 'stage:account_officer_review:act:dept'     THEN 'stage:accounting_verification:act:dept'
         WHEN 'stage:account_supervisor_review:act:dept'  THEN 'stage:accounting_supervision:act:dept'
         WHEN 'stage:accounting_review:act:dept'          THEN 'stage:accounting_authorization:act:dept'
         WHEN 'stage:finance_review:act:dept'             THEN 'stage:disbursement_authorization:act:dept'
         WHEN 'stage:cfo_review:act:dept'                 THEN 'stage:cfo_authorization:act:dept'
         WHEN 'stage:ceo_review:act:dept'                 THEN 'stage:ceo_authorization:act:dept'
         WHEN 'stage:ocr_extracted:act:dept'              THEN 'stage:submission:act:dept'
       END,
       rp.effect,
       'migration:0027-stage-perm-rename'
  FROM perm.role_permissions rp
 WHERE rp.permission_id IN (
   'stage:supervisor_review:act:dept',
   'stage:manager_review:act:dept',
   'stage:account_officer_review:act:dept',
   'stage:account_supervisor_review:act:dept',
   'stage:accounting_review:act:dept',
   'stage:finance_review:act:dept',
   'stage:cfo_review:act:dept',
   'stage:ceo_review:act:dept',
   'stage:ocr_extracted:act:dept'
 )
ON CONFLICT (role_id, permission_id) DO NOTHING;

COMMIT;