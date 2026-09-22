-- Keep legacy revocations explicitly unknown rather than inventing a reason.
ALTER TABLE staff_task_delegations ADD COLUMN revoke_reason TEXT
  CHECK(revoke_reason IS NULL OR length(btrim(revoke_reason)) BETWEEN 3 AND 2000);

CREATE TRIGGER staff_task_revocation_immutable
  BEFORE UPDATE ON staff_task_delegations
  FOR EACH ROW WHEN (OLD.status = 'revoked') EXECUTE FUNCTION iam_immutable();
