ALTER TABLE account_invitations DROP CONSTRAINT account_invitations_scope_kind_check;
ALTER TABLE account_invitations ADD CONSTRAINT account_invitations_scope_kind_check CHECK(scope_kind IN ('holding','branch','department'));
ALTER TABLE account_invitations ADD COLUMN department_id TEXT;
