-- v7 schema: new approval stages for CEO and Finance (Treasury).
-- Seeds the two new stage-* modules in rbac.modules and links them into the
-- grp-workflow-approval module group so the universal ApprovalQueue picks
-- them up. Per-stage RBAC is granted to the appropriate legacy role
-- (L4 = exec; L3 = finance for disbursement; L3 = cfo for ceo via
-- grp-cockpit module-group cascade).
--
-- Additive only. Safe to re-run.

BEGIN;

INSERT INTO rbac.modules (id, display_name, group_name, sort_order, allowed_actions)
VALUES
  ('stage-ceo-review',     'CEO Approval Stage',     'Workflow · Approval', 207, ARRAY['create','read','update','delete']::rbac.action[]),
  ('stage-finance-review', 'Finance Disbursement',   'Workflow · Approval', 208, ARRAY['create','read','update','delete']::rbac.action[])
ON CONFLICT (id) DO UPDATE SET
  display_name = EXCLUDED.display_name,
  group_name   = EXCLUDED.group_name,
  sort_order   = EXCLUDED.sort_order,
  allowed_actions = EXCLUDED.allowed_actions;

INSERT INTO rbac.module_groups (module_id, group_id)
SELECT m.id, g.id
FROM rbac.modules m
JOIN rbac.groups g ON g.kind = 'module-group'
WHERE m.id IN ('stage-ceo-review','stage-finance-review')
  AND g.id = 'grp-workflow-approval'
ON CONFLICT DO NOTHING;

INSERT INTO rbac.audit (kind, actor, target)
VALUES
  ('module.add_to_group', 'seed', jsonb_build_object('module','stage-ceo-review','group','grp-workflow-approval')),
  ('module.add_to_group', 'seed', jsonb_build_object('module','stage-finance-review','group','grp-workflow-approval'));

COMMIT;
