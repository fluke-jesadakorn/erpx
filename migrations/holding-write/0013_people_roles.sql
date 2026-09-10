DROP TRIGGER IF EXISTS assignment_role_scope_insert;
DROP TRIGGER IF EXISTS assignment_role_scope_update;

CREATE TRIGGER assignment_role_scope_insert
BEFORE INSERT ON assignments
WHEN NOT (
  (NEW.role = 'holding_owner' AND NEW.scope_kind = 'holding')
  OR (NEW.role IN ('employee','finance_preparer','people_manager','hr_admin','payroll_preparer','payroll_approver','finance_approver','treasury','auditor')
      AND NEW.scope_kind IN ('holding','branch','department'))
)
BEGIN
  SELECT RAISE(ABORT, 'INVALID_ROLE_SCOPE');
END;

CREATE TRIGGER assignment_role_scope_update
BEFORE UPDATE OF role, scope_kind ON assignments
WHEN NOT (
  (NEW.role = 'holding_owner' AND NEW.scope_kind = 'holding')
  OR (NEW.role IN ('employee','finance_preparer','people_manager','hr_admin','payroll_preparer','payroll_approver','finance_approver','treasury','auditor')
      AND NEW.scope_kind IN ('holding','branch','department'))
)
BEGIN
  SELECT RAISE(ABORT, 'INVALID_ROLE_SCOPE');
END;
