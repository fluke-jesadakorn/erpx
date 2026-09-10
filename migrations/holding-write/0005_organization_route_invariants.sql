CREATE TRIGGER IF NOT EXISTS organization_route_immutable
BEFORE UPDATE OF slug, parent_id, kind ON organization_nodes
WHEN NEW.slug <> OLD.slug
  OR NEW.kind <> OLD.kind
  OR NEW.parent_id IS NOT OLD.parent_id
BEGIN
  SELECT RAISE(ABORT, 'IMMUTABLE_ORGANIZATION_ROUTE');
END;

CREATE TRIGGER IF NOT EXISTS assignment_role_scope_insert
BEFORE INSERT ON assignments
WHEN NOT (
  (NEW.role = 'holding_owner' AND NEW.scope_kind = 'holding')
  OR (NEW.role IN ('employee','finance_preparer','people_manager')
      AND NEW.scope_kind IN ('holding','branch','department'))
)
BEGIN
  SELECT RAISE(ABORT, 'INVALID_ROLE_SCOPE');
END;

CREATE TRIGGER IF NOT EXISTS assignment_role_scope_update
BEFORE UPDATE OF role, scope_kind ON assignments
WHEN NOT (
  (NEW.role = 'holding_owner' AND NEW.scope_kind = 'holding')
  OR (NEW.role IN ('employee','finance_preparer','people_manager')
      AND NEW.scope_kind IN ('holding','branch','department'))
)
BEGIN
  SELECT RAISE(ABORT, 'INVALID_ROLE_SCOPE');
END;
