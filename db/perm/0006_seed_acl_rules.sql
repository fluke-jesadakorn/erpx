-- Folio — seed perm.acl_rules.
-- Object-level rules: a user with a given permission may additionally act
-- on rows where owner_field = user_id or where dept_group_id matches.

BEGIN;

INSERT INTO perm.acl_rules (permission_id, subject_type, owner_field, can_assign_to_self, can_assign_to_group) VALUES
  ('finance:expense:update',    'Expense',          'submitter_id',         true,  true),
  ('finance:expense:delete',    'Expense',          'submitter_id',         true,  false),
  ('finance:pr:update',         'PR',               'requester_id',         true,  true),
  ('finance:pr:delete',         'PR',               'requester_id',         true,  false),
  ('finance:po:approve',        'PO',               'requester_id',         true,  false),
  ('finance:po:reject',         'PO',               'requester_id',         true,  false),
  ('policy:approval:edit',      'ApprovalPolicy',   'created_by',           true,  false),
  ('policy:approval:delete',    'ApprovalPolicy',   'created_by',           true,  false);

-- Slip is already covered by lib/perm/session logic via loaded 'me' ACL
-- ('hook:event:replay' uses dept_group_id of actors via rbac.groups).

COMMIT;
