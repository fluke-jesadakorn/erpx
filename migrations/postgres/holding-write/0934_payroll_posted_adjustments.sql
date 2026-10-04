-- Independently reviewed additional pay preserves posted sources and immutable revisions.
ALTER TABLE hr_payroll_runs ADD COLUMN adjustment_of_run_id text;
ALTER TABLE hr_payroll_runs ADD CONSTRAINT hr_payroll_adjustment_origin_fk
 FOREIGN KEY(company_id,adjustment_of_run_id) REFERENCES hr_payroll_runs(company_id,id);
DROP INDEX hr_payroll_runs_active_period_revision;
CREATE UNIQUE INDEX hr_payroll_runs_active_period_revision
 ON hr_payroll_runs(company_id,pay_group_id,period_start,period_end,revision)
 WHERE status!='cancelled' AND adjustment_of_run_id IS NULL;

CREATE TABLE hr_payroll_adjustments (
 run_id text PRIMARY KEY REFERENCES hr_payroll_runs(id),
 company_id text NOT NULL,
 original_run_id text NOT NULL,
 original_item_id text NOT NULL REFERENCES hr_payroll_items(id),
 engagement_id text NOT NULL REFERENCES hr_engagements(id),
 source_ref text NOT NULL CHECK(length(btrim(source_ref)) BETWEEN 8 AND 200),
 source_snapshot_json text NOT NULL CHECK(erpx_json_valid(source_snapshot_json)),
 source_digest text NOT NULL CHECK(length(source_digest)=64),
 source_revision bigint NOT NULL DEFAULT 1 CHECK(source_revision>0),
 active boolean NOT NULL DEFAULT true,
 created_by text NOT NULL,
 created_at bigint NOT NULL,
 FOREIGN KEY(company_id,run_id) REFERENCES hr_payroll_runs(company_id,id),
 FOREIGN KEY(company_id,original_run_id) REFERENCES hr_payroll_runs(company_id,id)
);
CREATE UNIQUE INDEX hr_payroll_adjustment_source_once ON hr_payroll_adjustments(company_id,engagement_id,source_ref) WHERE active;
CREATE TABLE hr_payroll_adjustment_revisions (
 id text PRIMARY KEY,
 run_id text NOT NULL REFERENCES hr_payroll_adjustments(run_id),
 revision bigint NOT NULL CHECK(revision>0),
 taxable_arrears_minor bigint NOT NULL CHECK(taxable_arrears_minor BETWEEN 0 AND 9007199254740991),
 non_taxable_arrears_minor bigint NOT NULL CHECK(non_taxable_arrears_minor BETWEEN 0 AND 9007199254740991),
 social_security_wage_minor bigint NOT NULL CHECK(social_security_wage_minor BETWEEN 0 AND taxable_arrears_minor),
 time_entries_json text NOT NULL CHECK(erpx_json_valid(time_entries_json)),
 reason text NOT NULL CHECK(length(btrim(reason)) BETWEEN 10 AND 2000),
 evidence_ref text NOT NULL CHECK(length(btrim(evidence_ref)) BETWEEN 8 AND 2000),
 created_by text NOT NULL, command_id text NOT NULL REFERENCES command_receipts(command_id), created_at bigint NOT NULL,
 UNIQUE(run_id,revision)
);
CREATE TABLE hr_payroll_adjustment_time_claims (
 run_id text NOT NULL REFERENCES hr_payroll_adjustments(run_id), source_revision bigint NOT NULL,
 time_entry_id text NOT NULL REFERENCES hr_time_entries(id), active boolean NOT NULL DEFAULT true,
 PRIMARY KEY(run_id,source_revision,time_entry_id),
 FOREIGN KEY(run_id,source_revision) REFERENCES hr_payroll_adjustment_revisions(run_id,revision)
);
CREATE UNIQUE INDEX hr_payroll_adjustment_time_once ON hr_payroll_adjustment_time_claims(time_entry_id) WHERE active;
CREATE FUNCTION hr_payroll_adjustment_revision_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN RAISE EXCEPTION 'Payroll adjustment revisions are immutable' USING ERRCODE='23514'; END $$;
CREATE TRIGGER hr_payroll_adjustment_revision_immutable BEFORE UPDATE OR DELETE ON hr_payroll_adjustment_revisions
 FOR EACH ROW EXECUTE FUNCTION hr_payroll_adjustment_revision_immutable();

CREATE FUNCTION hr_payroll_adjustment_source_guard() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE current_run hr_payroll_runs; original hr_payroll_runs;
BEGIN
 SELECT * INTO current_run FROM hr_payroll_runs WHERE id=NEW.run_id FOR UPDATE;
 SELECT * INTO original FROM hr_payroll_runs WHERE id=NEW.original_run_id FOR SHARE;
 IF original.status!='posted' OR original.mode!='production' OR original.adjustment_of_run_id IS NOT NULL
   OR current_run.company_id!=NEW.company_id OR original.company_id!=NEW.company_id
   OR current_run.adjustment_of_run_id IS DISTINCT FROM original.id OR current_run.scope_engagement_id IS DISTINCT FROM NEW.engagement_id
   OR current_run.mode!='production' OR current_run.currency!='THB' OR current_run.pay_date<=original.pay_date
   OR current_run.period_start!=original.period_start OR current_run.period_end!=original.period_end
   OR NOT EXISTS(SELECT 1 FROM hr_payroll_items i WHERE i.id=NEW.original_item_id AND i.run_id=original.id AND i.engagement_id=NEW.engagement_id AND i.status='posted') THEN
  RAISE EXCEPTION 'Payroll adjustment must retain its posted Company employee source' USING ERRCODE='23514';
 END IF;
 IF TG_OP='UPDATE' THEN
  IF (to_jsonb(NEW)-ARRAY['source_revision','active']) IS DISTINCT FROM (to_jsonb(OLD)-ARRAY['source_revision','active'])
    OR NEW.source_revision<OLD.source_revision OR NEW.source_revision>OLD.source_revision+1
    OR NEW.source_revision!=OLD.source_revision AND (current_run.status NOT IN ('draft','calculating','preview','failed') OR current_run.submitted_by IS NOT NULL)
    OR NEW.active IS DISTINCT FROM OLD.active AND NOT (OLD.active AND NOT NEW.active AND current_run.status='cancelled') THEN
   RAISE EXCEPTION 'Reviewed adjustment source cannot be rewritten' USING ERRCODE='23514';
  END IF;
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER hr_payroll_adjustment_source_guard BEFORE INSERT OR UPDATE ON hr_payroll_adjustments
 FOR EACH ROW EXECUTE FUNCTION hr_payroll_adjustment_source_guard();
CREATE FUNCTION hr_payroll_adjustment_cancel_claims() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.status='cancelled' AND OLD.status!='cancelled' AND NEW.adjustment_of_run_id IS NOT NULL THEN
  UPDATE hr_payroll_adjustments SET active=false WHERE run_id=NEW.id;
  UPDATE hr_payroll_adjustment_time_claims SET active=false WHERE run_id=NEW.id AND active;
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER hr_payroll_adjustment_cancel_claims AFTER UPDATE ON hr_payroll_runs
 FOR EACH ROW EXECUTE FUNCTION hr_payroll_adjustment_cancel_claims();

-- Paid bank records remain separate; payroll, issued copies, and the reviewed
-- source result used by subsequent YTD cannot change after their GL is posted.
CREATE FUNCTION hr_payroll_posted_source_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF OLD.status='posted' THEN RAISE EXCEPTION 'Posted payroll source is immutable; prepare an independently reviewed adjustment' USING ERRCODE='23514'; END IF;
 IF TG_OP='DELETE' THEN RETURN OLD; END IF; RETURN NEW;
END $$;
CREATE TRIGGER hr_payroll_posted_source_immutable BEFORE UPDATE OR DELETE ON hr_payroll_runs
 FOR EACH ROW EXECUTE FUNCTION hr_payroll_posted_source_immutable();
CREATE FUNCTION hr_payroll_posted_item_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP!='INSERT' AND EXISTS(SELECT 1 FROM hr_payroll_runs WHERE id=OLD.run_id AND status='posted') THEN
  IF TG_OP='DELETE' OR OLD.status='posted' OR NEW.status!='posted'
    OR (to_jsonb(NEW)-'status') IS DISTINCT FROM (to_jsonb(OLD)-'status') THEN
   RAISE EXCEPTION 'Posted payroll item is immutable' USING ERRCODE='23514';
  END IF;
 ELSIF TG_OP='INSERT' AND EXISTS(SELECT 1 FROM hr_payroll_runs WHERE id=NEW.run_id AND status='posted') THEN
  RAISE EXCEPTION 'Posted payroll items cannot be appended' USING ERRCODE='23514';
 END IF;
 IF TG_OP='DELETE' THEN RETURN OLD; END IF; RETURN NEW;
END $$;
CREATE TRIGGER hr_payroll_posted_item_immutable BEFORE UPDATE OR DELETE ON hr_payroll_items
 FOR EACH ROW EXECUTE FUNCTION hr_payroll_posted_item_immutable();
