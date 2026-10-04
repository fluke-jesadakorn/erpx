-- Cancelled ordinary drafts retain their original source and cease reserving a
-- work period. Posted runs and employee-issued copies cannot enter this path.
DO $$
DECLARE old_constraint text;
BEGIN
 SELECT conname INTO old_constraint FROM pg_constraint
 WHERE conrelid='hr_payroll_runs'::regclass AND contype='u'
   AND pg_get_constraintdef(oid)='UNIQUE (company_id, pay_group_id, period_start, period_end, revision)';
 IF old_constraint IS NULL THEN
  RAISE EXCEPTION 'Payroll period source uniqueness constraint is missing';
 END IF;
 EXECUTE format('ALTER TABLE hr_payroll_runs DROP CONSTRAINT %I',old_constraint);
END $$;
CREATE UNIQUE INDEX hr_payroll_runs_active_period_revision
 ON hr_payroll_runs(company_id,pay_group_id,period_start,period_end,revision) WHERE status!='cancelled';
CREATE UNIQUE INDEX hr_payroll_runs_company_id ON hr_payroll_runs(company_id,id);

CREATE TABLE hr_payroll_run_cancellations (
 id text PRIMARY KEY,
 company_id text NOT NULL,
 run_id text NOT NULL UNIQUE,
 command_id text NOT NULL UNIQUE REFERENCES command_receipts(command_id),
 cancelled_by text NOT NULL,
 reason text NOT NULL CHECK(length(btrim(reason)) BETWEEN 8 AND 1000),
 prior_status text NOT NULL CHECK(prior_status IN ('draft','calculating','failed','preview')),
 prior_version bigint NOT NULL CHECK(prior_version>0),
 run_revision bigint NOT NULL CHECK(run_revision>0),
 source_snapshot_json text NOT NULL CHECK(erpx_json_valid(source_snapshot_json)),
 created_at bigint NOT NULL,
 FOREIGN KEY(company_id,run_id) REFERENCES hr_payroll_runs(company_id,id)
);

CREATE FUNCTION hr_payroll_draft_cancellation_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF OLD.status='cancelled' THEN
  RAISE EXCEPTION 'Cancelled payroll source is retained and terminal' USING ERRCODE='23514';
 END IF;
 IF TG_OP='DELETE' THEN RETURN OLD; END IF;
 IF NEW.status='cancelled' THEN
  IF OLD.status NOT IN ('draft','calculating','failed','preview') OR OLD.mode='final_pay'
    OR OLD.submitted_by IS NOT NULL OR OLD.approved_by IS NOT NULL OR OLD.posted_by IS NOT NULL
    OR NEW.version!=OLD.version+1
    OR (to_jsonb(NEW)-ARRAY['status','version','updated_at']) IS DISTINCT FROM (to_jsonb(OLD)-ARRAY['status','version','updated_at'])
    OR EXISTS(SELECT 1 FROM hr_payroll_postings p WHERE p.run_id=OLD.id)
    OR EXISTS(SELECT 1 FROM hr_issued_payslips p WHERE p.run_id=OLD.id)
    OR EXISTS(SELECT 1 FROM hr_payment_batches p WHERE p.company_id=OLD.company_id AND p.source_type='payroll' AND p.source_id=OLD.id)
    OR EXISTS(SELECT 1 FROM finance_journals j WHERE j.company_id=OLD.company_id AND j.source_type='payroll' AND j.source_id=OLD.id) THEN
   RAISE EXCEPTION 'Only an unsubmitted ordinary payroll source can be cancelled without changing its evidence' USING ERRCODE='23514';
  END IF;
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER hr_payroll_draft_cancellation_guard BEFORE UPDATE OR DELETE ON hr_payroll_runs
 FOR EACH ROW EXECUTE FUNCTION hr_payroll_draft_cancellation_guard();

CREATE FUNCTION hr_payroll_run_cancellation_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 RAISE EXCEPTION 'Payroll cancellation receipt and original source are immutable' USING ERRCODE='23514';
END $$;
CREATE TRIGGER hr_payroll_run_cancellation_immutable BEFORE UPDATE OR DELETE ON hr_payroll_run_cancellations
 FOR EACH ROW EXECUTE FUNCTION hr_payroll_run_cancellation_immutable();

CREATE FUNCTION hr_payroll_run_cancellation_source_guard() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE retained jsonb;
BEGIN
 retained:=NEW.source_snapshot_json::jsonb;
 IF NOT EXISTS(SELECT 1 FROM hr_payroll_runs r JOIN command_receipts c ON c.command_id=NEW.command_id
   WHERE r.id=NEW.run_id AND r.company_id=NEW.company_id AND r.status='cancelled'
    AND r.version=NEW.prior_version+1 AND r.revision=NEW.run_revision
    AND retained->>'id'=r.id AND retained->>'company_id'=r.company_id
    AND retained->>'status'=NEW.prior_status AND (retained->>'version')::bigint=NEW.prior_version
    AND (retained-ARRAY['status','version','updated_at'])=(to_jsonb(r)-ARRAY['status','version','updated_at'])
    AND c.operation='people.payroll.cancel' AND c.record_id=r.id AND c.actor_id=NEW.cancelled_by) THEN
  RAISE EXCEPTION 'Payroll cancellation requires the exact original source and owner command receipt' USING ERRCODE='23514';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER hr_payroll_run_cancellation_source_guard BEFORE INSERT ON hr_payroll_run_cancellations
 FOR EACH ROW EXECUTE FUNCTION hr_payroll_run_cancellation_source_guard();

CREATE FUNCTION hr_payroll_run_cancellation_receipt_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.status='cancelled' AND OLD.status!='cancelled' AND NOT EXISTS(
   SELECT 1 FROM hr_payroll_run_cancellations c WHERE c.run_id=NEW.id AND c.company_id=NEW.company_id
     AND c.prior_version=OLD.version AND c.prior_status=OLD.status AND c.run_revision=OLD.revision
     AND c.source_snapshot_json::jsonb=to_jsonb(OLD)) THEN
  RAISE EXCEPTION 'Payroll cancellation must retain its immutable owner receipt' USING ERRCODE='23514';
 END IF;
 RETURN NEW;
END $$;
CREATE CONSTRAINT TRIGGER hr_payroll_run_cancellation_receipt_guard AFTER UPDATE ON hr_payroll_runs
 DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION hr_payroll_run_cancellation_receipt_guard();

CREATE FUNCTION hr_payroll_cancelled_item_guard() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE owner_status text;
BEGIN
 IF TG_OP!='INSERT' THEN
  SELECT status INTO owner_status FROM hr_payroll_runs WHERE id=OLD.run_id FOR SHARE;
  IF owner_status='cancelled' THEN
   RAISE EXCEPTION 'Cancelled payroll calculation items are retained unchanged' USING ERRCODE='23514';
  END IF;
 END IF;
 IF TG_OP!='DELETE' THEN
  SELECT status INTO owner_status FROM hr_payroll_runs WHERE id=NEW.run_id FOR SHARE;
  IF owner_status='cancelled' THEN
   RAISE EXCEPTION 'Cancelled payroll calculation items cannot be replaced' USING ERRCODE='23514';
  END IF;
  RETURN NEW;
 END IF;
 RETURN OLD;
END $$;
CREATE TRIGGER hr_payroll_cancelled_item_guard BEFORE INSERT OR UPDATE OR DELETE ON hr_payroll_items
 FOR EACH ROW EXECUTE FUNCTION hr_payroll_cancelled_item_guard();
