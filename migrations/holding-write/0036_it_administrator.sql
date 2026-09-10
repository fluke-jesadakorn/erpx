DROP TRIGGER assignment_role_scope_insert;
DROP TRIGGER assignment_role_scope_update;
CREATE TRIGGER assignment_role_scope_insert BEFORE INSERT ON assignments
WHEN NOT ((NEW.role IN ('holding_owner','it_admin') AND NEW.scope_kind='holding') OR
 (NEW.role IN ('warehouse_manager','operations_manager') AND NEW.scope_kind IN ('holding','branch')) OR
 (NEW.role IN ('employee','finance_preparer','people_manager','hr_admin','payroll_preparer','payroll_approver','finance_approver','treasury','auditor','sales_manager','sales_operator','purchasing_manager','purchasing_operator') AND NEW.scope_kind IN ('holding','branch','department')))
BEGIN SELECT RAISE(ABORT,'INVALID_ROLE_SCOPE'); END;
CREATE TRIGGER assignment_role_scope_update BEFORE UPDATE OF role,scope_kind ON assignments
WHEN NOT ((NEW.role IN ('holding_owner','it_admin') AND NEW.scope_kind='holding') OR
 (NEW.role IN ('warehouse_manager','operations_manager') AND NEW.scope_kind IN ('holding','branch')) OR
 (NEW.role IN ('employee','finance_preparer','people_manager','hr_admin','payroll_preparer','payroll_approver','finance_approver','treasury','auditor','sales_manager','sales_operator','purchasing_manager','purchasing_operator') AND NEW.scope_kind IN ('holding','branch','department')))
BEGIN SELECT RAISE(ABORT,'INVALID_ROLE_SCOPE'); END;
ALTER TABLE beta_team_invitations ADD COLUMN role TEXT NOT NULL DEFAULT 'employee' CHECK(role IN ('employee','it_admin'));
