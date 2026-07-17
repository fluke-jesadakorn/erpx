-- 9004-seed-policies.sql
-- Seed the default policy ASTs that authorize() consults for workflow
-- decisions. These replace the hardcoded STAGE_TO_ROLE / OVERRIDE_ROLE_NAMES
-- / 200k threshold logic that previously lived in lib/perm/chain.ts.

BEGIN;

UPDATE perm.policies
   SET ast = '{"rules":[
     {"allow":"stage:submission:act::allow"}
   ]}'::jsonb,
       description = 'Submission stage permission'
 WHERE id = 'canActOnWaybillStage';

UPDATE perm.policies
   SET ast = '{"rules":[
     {"when":"actor.id == resource.submitterId && resource.currentStage != ''disbursed''",
      "allow":"finance:waybill:recall::allow"},
     {"when":"actor.level <= 2",
      "allow":"admin:system:bypass::allow"}
   ]}'::jsonb,
       description = 'Recall a waybill: submitter (not yet disbursed) or CEO/admin'
 WHERE id = 'recallWaybill';

INSERT INTO perm.policies (id, name, ast, description, enabled) VALUES
  ('waybill.dept_verification',
   'Waybill dept verification',
   '{"rules":[
     {"when":"actor.dept_id == resource.submitterDeptId",
      "allow":"stage:dept_verification:act::allow"}
   ]}'::jsonb,
   'Actor can act on dept_verification only when in submitter dept',
   true),
  ('waybill.accounting_verification',
   'Waybill accounting verification',
   '{"rules":[
     {"allow":"stage:accounting_verification:act::allow"}
   ]}'::jsonb,
   'Accounting officer verification',
   true),
  ('waybill.accounting_review',
   'Waybill accounting review',
   '{"rules":[
     {"allow":"stage:accounting_review:act::allow"}
   ]}'::jsonb,
   'Combined accounting supervisor + manager review',
   true),
  ('waybill.disbursement_authorization',
   'Waybill disbursement authorization',
   '{"rules":[
     {"allow":"stage:disbursement_authorization:act::allow"}
   ]}'::jsonb,
   'Finance disbursement lead',
   true),
  ('waybill.cfo_authorization',
   'Waybill CFO authorization',
   '{"rules":[
     {"when":"(total_amount_thb == null || total_amount_thb < 200000)",
      "deny":"stage:cfo_authorization:act::allow",
      "reason":"CFO authorization skipped: amount under 200k THB"},
     {"allow":"stage:cfo_authorization:act::allow"}
   ]}'::jsonb,
   'CFO authorization required when total >= 200,000 THB',
   true),
  ('waybill.ceo_authorization',
   'Waybill CEO authorization',
   '{"rules":[
     {"when":"(total_amount_thb == null || total_amount_thb < 200000)",
      "deny":"stage:ceo_authorization:act::allow",
      "reason":"CEO authorization not required: amount under 200k THB"},
     {"allow":"stage:ceo_authorization:act::allow"}
   ]}'::jsonb,
   'CEO authorization only required when total >= 200,000 THB',
   true),
  ('waybill.po_cfo',
   'PO CFO authorization',
   '{"rules":[
     {"allow":"stage:po_cfo:act::allow"}
   ]}'::jsonb,
   'CFO sign-off on PO',
   true),
  ('waybill.po_pending',
   'PO pending approval',
   '{"rules":[
     {"allow":"stage:po_pending:act::allow"}
   ]}'::jsonb,
   'Manager approves PO',
   true),
  ('hr.leave.request',
   'HR leave request',
   '{"rules":[
     {"allow":"hr:leave:submit::allow"}
   ]}'::jsonb,
   'Submit own leave request',
   true),
  ('hr.leave.approve',
   'HR leave approval',
   '{"rules":[
     {"when":"actor.id == resource.submitterId",
      "allow":"hr:leave:decide_self::allow"},
     {"when":"actor.dept_id == resource.submitterDeptId",
      "allow":"hr:leave:approve::allow"},
     {"allow":"hr:leave:approve::allow"}
   ]}'::jsonb,
   'Self approve own OR dept manager approves OR HR',
   true),
  ('sales.draft.discard',
   'Sales draft discard',
   '{"rules":[
     {"when":"actor.id == resource.submitterId",
      "allow":"sales:draft:discard::allow"},
     {"allow":"sales:draft:discard::allow"}
   ]}'::jsonb,
   'Submitter can discard their own draft; supervisor can discard any',
   true)
ON CONFLICT (id) DO UPDATE SET
  ast = EXCLUDED.ast,
  description = EXCLUDED.description,
  enabled = EXCLUDED.enabled;

COMMIT;