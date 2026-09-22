CREATE TABLE staff_task_delegations (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  task_id TEXT NOT NULL,
  delegator_user_id TEXT NOT NULL,
  delegate_user_id TEXT NOT NULL,
  starts_at BIGINT NOT NULL,
  expires_at BIGINT NOT NULL CHECK(expires_at > starts_at),
  reason TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active','revoked')),
  revoked_by TEXT,
  revoked_at BIGINT,
  version BIGINT NOT NULL DEFAULT 1 CHECK(version > 0),
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL
);
CREATE INDEX staff_task_delegations_task ON staff_task_delegations(company_id,task_id,status,expires_at);
CREATE INDEX staff_task_delegations_delegate ON staff_task_delegations(company_id,delegate_user_id,status,expires_at);
CREATE TRIGGER staff_task_delegations_immutable
  BEFORE DELETE ON staff_task_delegations
  FOR EACH ROW EXECUTE FUNCTION iam_immutable();
DO $$ BEGIN
  INSERT INTO work_cache_revisions(table_name) VALUES('staff_task_delegations') ON CONFLICT DO NOTHING;
  IF NOT EXISTS(SELECT 1 FROM pg_trigger WHERE tgrelid='staff_task_delegations'::regclass AND tgname='work_cache_change') THEN
    CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON staff_task_delegations FOR EACH ROW EXECUTE FUNCTION work_cache_changed();
  END IF;
  IF NOT EXISTS(SELECT 1 FROM pg_trigger WHERE tgrelid='staff_task_delegations'::regclass AND tgname='work_cache_truncate') THEN
    CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON staff_task_delegations FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed();
  END IF;
END $$;
