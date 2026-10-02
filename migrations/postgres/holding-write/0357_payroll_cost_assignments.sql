-- Cost units are dated evidence. Existing placements cannot be assumed to have
-- been stable since hire, so their imported interval requires HR verification.
CREATE TABLE hr_payroll_cost_assignments (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  engagement_id TEXT NOT NULL REFERENCES hr_engagements(id),
  effective_from TEXT NOT NULL,
  effective_to TEXT,
  branch_id TEXT,
  department_id TEXT,
  evidence_status TEXT NOT NULL CHECK(evidence_status IN ('verified','legacy_unverified')),
  source TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active','superseded')),
  version BIGINT NOT NULL DEFAULT 1,
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL,
  CHECK(effective_to IS NULL OR effective_to >= effective_from)
);
CREATE UNIQUE INDEX hr_payroll_cost_assignment_start ON hr_payroll_cost_assignments(engagement_id,effective_from) WHERE status='active';
CREATE INDEX hr_payroll_cost_assignment_period ON hr_payroll_cost_assignments(engagement_id,effective_from,effective_to) WHERE status='active';

CREATE TABLE hr_payroll_cost_assignment_verifications (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  assignment_id TEXT NOT NULL REFERENCES hr_payroll_cost_assignments(id),
  verified_by TEXT NOT NULL,
  evidence_ref TEXT NOT NULL CHECK(length(trim(evidence_ref)) >= 8),
  previous_version BIGINT NOT NULL,
  created_at BIGINT NOT NULL,
  UNIQUE(assignment_id,previous_version)
);

INSERT INTO hr_payroll_cost_assignments(id,company_id,engagement_id,effective_from,branch_id,department_id,evidence_status,source,created_at,updated_at)
SELECT 'legacy-cost-'||e.id,e.company_id,e.id,e.start_date,e.branch_id,e.department_id,'legacy_unverified','migration',floor(extract(epoch FROM clock_timestamp())*1000)::bigint,floor(extract(epoch FROM clock_timestamp())*1000)::bigint
FROM hr_engagements e;

CREATE FUNCTION hr_track_payroll_cost_assignment() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE effective_date TEXT;
DECLARE prior hr_payroll_cost_assignments%ROWTYPE;
DECLARE recorded_at BIGINT;
BEGIN
  recorded_at := floor(extract(epoch FROM clock_timestamp())*1000)::bigint;
  IF TG_OP='INSERT' THEN
    INSERT INTO hr_payroll_cost_assignments(id,company_id,engagement_id,effective_from,branch_id,department_id,evidence_status,source,created_at,updated_at)
    VALUES('cost-create-'||NEW.id,NEW.company_id,NEW.id,NEW.start_date,NEW.branch_id,NEW.department_id,'verified','engagement_create',recorded_at,recorded_at);
    RETURN NEW;
  END IF;
  IF NEW.branch_id IS NOT DISTINCT FROM OLD.branch_id AND NEW.department_id IS NOT DISTINCT FROM OLD.department_id THEN RETURN NEW; END IF;
  effective_date := NULLIF(current_setting('erpx.payroll_assignment_date',true),'');
  IF effective_date IS NULL THEN effective_date := to_char(clock_timestamp() AT TIME ZONE 'Asia/Bangkok','YYYY-MM-DD'); END IF;
  IF effective_date < NEW.start_date THEN effective_date := NEW.start_date; END IF;
  SELECT * INTO prior FROM hr_payroll_cost_assignments WHERE engagement_id=NEW.id AND status='active' ORDER BY effective_from DESC LIMIT 1 FOR UPDATE;
  IF NOT FOUND THEN RAISE EXCEPTION 'PAYROLL_COST_ASSIGNMENT_MISSING' USING ERRCODE='23514'; END IF;
  IF effective_date < prior.effective_from THEN RAISE EXCEPTION 'PAYROLL_COST_ASSIGNMENT_BACKDATE_REQUIRED' USING ERRCODE='23514'; END IF;
  IF effective_date = prior.effective_from THEN
    IF prior.evidence_status='legacy_unverified' AND prior.effective_from<to_char(clock_timestamp() AT TIME ZONE 'Asia/Bangkok','YYYY-MM-DD') THEN
      RAISE EXCEPTION 'PAYROLL_COST_ASSIGNMENT_REVIEW_REQUIRED' USING ERRCODE='23514';
    END IF;
    UPDATE hr_payroll_cost_assignments SET branch_id=NEW.branch_id,department_id=NEW.department_id,evidence_status='verified',source='engagement_update',version=version+1,updated_at=recorded_at WHERE id=prior.id;
  ELSE
    UPDATE hr_payroll_cost_assignments SET effective_to=to_char(effective_date::date-1,'YYYY-MM-DD'),version=version+1,updated_at=recorded_at WHERE id=prior.id;
    INSERT INTO hr_payroll_cost_assignments(id,company_id,engagement_id,effective_from,branch_id,department_id,evidence_status,source,created_at,updated_at)
    VALUES('cost-change-'||NEW.id||'-'||effective_date,NEW.company_id,NEW.id,effective_date,NEW.branch_id,NEW.department_id,'verified','engagement_update',recorded_at,recorded_at);
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER hr_payroll_cost_assignment_insert AFTER INSERT ON hr_engagements FOR EACH ROW EXECUTE FUNCTION hr_track_payroll_cost_assignment();
CREATE TRIGGER hr_payroll_cost_assignment_change AFTER UPDATE OF branch_id,department_id ON hr_engagements FOR EACH ROW EXECUTE FUNCTION hr_track_payroll_cost_assignment();

ALTER TABLE hr_time_entries ADD COLUMN cost_assignment_id TEXT REFERENCES hr_payroll_cost_assignments(id);
ALTER TABLE hr_time_entries ADD COLUMN cost_branch_id TEXT;
ALTER TABLE hr_time_entries ADD COLUMN cost_department_id TEXT;

UPDATE hr_time_entries t SET cost_assignment_id=a.id,cost_branch_id=a.branch_id,cost_department_id=a.department_id
FROM hr_payroll_cost_assignments a WHERE t.engagement_id=a.engagement_id AND t.status='approved' AND a.status='active' AND a.evidence_status='verified'
  AND a.effective_from<=t.work_date AND (a.effective_to IS NULL OR a.effective_to>=t.work_date);

CREATE FUNCTION hr_snapshot_payroll_time_cost() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE assignment hr_payroll_cost_assignments%ROWTYPE;
BEGIN
  IF NEW.status='approved' AND (TG_OP='INSERT' OR OLD.status IS DISTINCT FROM 'approved') THEN
    SELECT * INTO assignment FROM hr_payroll_cost_assignments WHERE engagement_id=NEW.engagement_id AND company_id=NEW.company_id
      AND status='active' AND evidence_status='verified' AND effective_from<=NEW.work_date AND (effective_to IS NULL OR effective_to>=NEW.work_date)
      ORDER BY effective_from DESC LIMIT 1;
    IF FOUND THEN
      NEW.cost_assignment_id:=assignment.id;NEW.cost_branch_id:=assignment.branch_id;NEW.cost_department_id:=assignment.department_id;
    ELSE
      NEW.cost_assignment_id:=NULL;NEW.cost_branch_id:=NULL;NEW.cost_department_id:=NULL;
    END IF;
  ELSIF TG_OP='UPDATE' AND OLD.status='approved' AND (NEW.cost_assignment_id IS DISTINCT FROM OLD.cost_assignment_id OR NEW.cost_branch_id IS DISTINCT FROM OLD.cost_branch_id OR NEW.cost_department_id IS DISTINCT FROM OLD.cost_department_id)
    AND current_setting('erpx.payroll_cost_repair',true) IS DISTINCT FROM COALESCE(NEW.cost_assignment_id,'__none__') THEN
    RAISE EXCEPTION 'PAYROLL_APPROVED_TIME_COST_IMMUTABLE' USING ERRCODE='23514';
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER hr_payroll_time_cost_snapshot BEFORE INSERT OR UPDATE ON hr_time_entries FOR EACH ROW EXECUTE FUNCTION hr_snapshot_payroll_time_cost();

DO $$
DECLARE migration_role text := current_user;
DECLARE app_role text;
DECLARE jobs_role text;
BEGIN
  IF migration_role !~ '_migration$' THEN RETURN; END IF;
  app_role := regexp_replace(migration_role, '_migration$', '_app');
  jobs_role := regexp_replace(migration_role, '_migration$', '_jobs');
  EXECUTE format('GRANT SELECT, INSERT, UPDATE ON hr_payroll_cost_assignments, hr_payroll_cost_assignment_verifications TO %I', app_role);
  EXECUTE format('GRANT SELECT, INSERT, UPDATE ON hr_payroll_cost_assignments TO %I', jobs_role);
END $$;
