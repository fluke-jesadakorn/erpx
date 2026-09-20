CREATE TABLE benefit_plans (
 id TEXT PRIMARY KEY,company_id TEXT NOT NULL,name TEXT NOT NULL,provider TEXT NOT NULL,coverage_type TEXT NOT NULL CHECK(coverage_type IN ('employee','family')),
 currency TEXT NOT NULL CHECK(length(currency)=3),employee_deduction_minor BIGINT NOT NULL CHECK(employee_deduction_minor>=0),employer_cost_minor BIGINT NOT NULL CHECK(employer_cost_minor>=0),
 effective_from TEXT NOT NULL,effective_to TEXT,status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','published','rejected','retired')),
 created_by TEXT NOT NULL,reviewed_by TEXT,version BIGINT NOT NULL DEFAULT 1,created_at BIGINT NOT NULL,updated_at BIGINT NOT NULL
);
CREATE TABLE benefit_plan_events (id TEXT PRIMARY KEY,plan_id TEXT NOT NULL REFERENCES benefit_plans(id),version BIGINT NOT NULL,action TEXT NOT NULL,actor_id TEXT NOT NULL,reason TEXT NOT NULL,status TEXT NOT NULL,created_at BIGINT NOT NULL,UNIQUE(plan_id,version));
CREATE TABLE benefit_dependents (
 id TEXT PRIMARY KEY,company_id TEXT NOT NULL,person_id TEXT NOT NULL REFERENCES hr_people(id),name TEXT NOT NULL,relationship TEXT NOT NULL,birth_date TEXT NOT NULL,
 status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','verified','rejected','removed')),created_by TEXT NOT NULL,reviewed_by TEXT,version BIGINT NOT NULL DEFAULT 1,created_at BIGINT NOT NULL,updated_at BIGINT NOT NULL
);
CREATE TABLE benefit_dependent_events (id TEXT PRIMARY KEY,dependent_id TEXT NOT NULL REFERENCES benefit_dependents(id),version BIGINT NOT NULL,action TEXT NOT NULL,actor_id TEXT NOT NULL,reason TEXT NOT NULL,status TEXT NOT NULL,created_at BIGINT NOT NULL,UNIQUE(dependent_id,version));
CREATE TABLE benefit_enrollment_windows (
 id TEXT PRIMARY KEY,company_id TEXT NOT NULL,name TEXT NOT NULL,opens_on TEXT NOT NULL,closes_on TEXT NOT NULL,effective_on TEXT NOT NULL,
 status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','open','closed','cancelled')),created_by TEXT NOT NULL,reviewed_by TEXT,version BIGINT NOT NULL DEFAULT 1,created_at BIGINT NOT NULL,updated_at BIGINT NOT NULL,CHECK(opens_on<=closes_on)
);
CREATE TABLE benefit_window_events (id TEXT PRIMARY KEY,window_id TEXT NOT NULL REFERENCES benefit_enrollment_windows(id),version BIGINT NOT NULL,action TEXT NOT NULL,actor_id TEXT NOT NULL,reason TEXT NOT NULL,status TEXT NOT NULL,created_at BIGINT NOT NULL,UNIQUE(window_id,version));
CREATE TABLE benefit_elections (
 id TEXT PRIMARY KEY,company_id TEXT NOT NULL,person_id TEXT NOT NULL REFERENCES hr_people(id),engagement_id TEXT NOT NULL REFERENCES hr_engagements(id),plan_id TEXT NOT NULL REFERENCES benefit_plans(id),plan_version BIGINT NOT NULL,
 window_id TEXT NOT NULL REFERENCES benefit_enrollment_windows(id),window_version BIGINT NOT NULL,coverage_type TEXT NOT NULL,dependent_ids_json TEXT NOT NULL DEFAULT '[]',currency TEXT NOT NULL,
 employee_deduction_minor BIGINT NOT NULL,employer_cost_minor BIGINT NOT NULL,effective_from TEXT NOT NULL,effective_to TEXT,
 status TEXT NOT NULL DEFAULT 'elected' CHECK(status IN ('elected','submitted','acknowledged','released','cancelled')),created_by TEXT NOT NULL,submitted_by TEXT,provider_reference TEXT,acknowledged_by TEXT,released_by TEXT,
 version BIGINT NOT NULL DEFAULT 1,created_at BIGINT NOT NULL,updated_at BIGINT NOT NULL,UNIQUE(person_id,plan_id,window_id)
);
CREATE INDEX benefit_election_queue ON benefit_elections(company_id,status,updated_at);
CREATE TABLE benefit_election_events (id TEXT PRIMARY KEY,election_id TEXT NOT NULL REFERENCES benefit_elections(id),version BIGINT NOT NULL,action TEXT NOT NULL,actor_id TEXT NOT NULL,reason TEXT NOT NULL,status TEXT NOT NULL,created_at BIGINT NOT NULL,UNIQUE(election_id,version));
CREATE TABLE benefit_payroll_inputs (
 id TEXT PRIMARY KEY,election_id TEXT NOT NULL UNIQUE REFERENCES benefit_elections(id),company_id TEXT NOT NULL,person_id TEXT NOT NULL,engagement_id TEXT NOT NULL,currency TEXT NOT NULL,
 employee_deduction_minor BIGINT NOT NULL,employer_cost_minor BIGINT NOT NULL,effective_from TEXT NOT NULL,effective_to TEXT,source_election_version BIGINT NOT NULL,status TEXT NOT NULL CHECK(status='released'),released_by TEXT NOT NULL,created_at BIGINT NOT NULL
);
CREATE FUNCTION benefit_guard() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
 IF TG_TABLE_NAME IN ('benefit_plan_events','benefit_dependent_events','benefit_window_events','benefit_election_events','benefit_payroll_inputs') OR TG_OP='DELETE' THEN RAISE EXCEPTION 'BENEFIT_HISTORY_IMMUTABLE' USING ERRCODE='23514'; END IF;
 IF TG_TABLE_NAME='benefit_plans' THEN IF (NEW.id,NEW.company_id,NEW.currency,NEW.created_by,NEW.created_at) IS DISTINCT FROM (OLD.id,OLD.company_id,OLD.currency,OLD.created_by,OLD.created_at) THEN RAISE EXCEPTION 'BENEFIT_PLAN_SOURCE_IMMUTABLE' USING ERRCODE='23514'; END IF;
 ELSIF TG_TABLE_NAME='benefit_dependents' THEN IF (NEW.id,NEW.company_id,NEW.person_id,NEW.created_by,NEW.created_at) IS DISTINCT FROM (OLD.id,OLD.company_id,OLD.person_id,OLD.created_by,OLD.created_at) THEN RAISE EXCEPTION 'BENEFIT_DEPENDENT_SOURCE_IMMUTABLE' USING ERRCODE='23514'; END IF;
 ELSIF TG_TABLE_NAME='benefit_enrollment_windows' THEN IF (NEW.id,NEW.company_id,NEW.created_by,NEW.created_at) IS DISTINCT FROM (OLD.id,OLD.company_id,OLD.created_by,OLD.created_at) THEN RAISE EXCEPTION 'BENEFIT_WINDOW_SOURCE_IMMUTABLE' USING ERRCODE='23514'; END IF;
 ELSIF TG_TABLE_NAME='benefit_elections' THEN IF (NEW.id,NEW.company_id,NEW.person_id,NEW.engagement_id,NEW.plan_id,NEW.plan_version,NEW.window_id,NEW.window_version,NEW.coverage_type,NEW.dependent_ids_json,NEW.currency,NEW.employee_deduction_minor,NEW.employer_cost_minor,NEW.effective_from,NEW.effective_to,NEW.created_by,NEW.created_at) IS DISTINCT FROM (OLD.id,OLD.company_id,OLD.person_id,OLD.engagement_id,OLD.plan_id,OLD.plan_version,OLD.window_id,OLD.window_version,OLD.coverage_type,OLD.dependent_ids_json,OLD.currency,OLD.employee_deduction_minor,OLD.employer_cost_minor,OLD.effective_from,OLD.effective_to,OLD.created_by,OLD.created_at) THEN RAISE EXCEPTION 'BENEFIT_ELECTION_SOURCE_IMMUTABLE' USING ERRCODE='23514'; END IF; END IF; RETURN NEW;
END $$;
CREATE TRIGGER benefit_plan_guard BEFORE UPDATE OR DELETE ON benefit_plans FOR EACH ROW EXECUTE FUNCTION benefit_guard();
CREATE TRIGGER benefit_plan_event_guard BEFORE UPDATE OR DELETE ON benefit_plan_events FOR EACH ROW EXECUTE FUNCTION benefit_guard();
CREATE TRIGGER benefit_dependent_guard BEFORE UPDATE OR DELETE ON benefit_dependents FOR EACH ROW EXECUTE FUNCTION benefit_guard();
CREATE TRIGGER benefit_dependent_event_guard BEFORE UPDATE OR DELETE ON benefit_dependent_events FOR EACH ROW EXECUTE FUNCTION benefit_guard();
CREATE TRIGGER benefit_window_guard BEFORE UPDATE OR DELETE ON benefit_enrollment_windows FOR EACH ROW EXECUTE FUNCTION benefit_guard();
CREATE TRIGGER benefit_window_event_guard BEFORE UPDATE OR DELETE ON benefit_window_events FOR EACH ROW EXECUTE FUNCTION benefit_guard();
CREATE TRIGGER benefit_election_guard BEFORE UPDATE OR DELETE ON benefit_elections FOR EACH ROW EXECUTE FUNCTION benefit_guard();
CREATE TRIGGER benefit_election_event_guard BEFORE UPDATE OR DELETE ON benefit_election_events FOR EACH ROW EXECUTE FUNCTION benefit_guard();
CREATE TRIGGER benefit_payroll_input_guard BEFORE UPDATE OR DELETE ON benefit_payroll_inputs FOR EACH ROW EXECUTE FUNCTION benefit_guard();
DO $$ DECLARE t TEXT; BEGIN FOREACH t IN ARRAY ARRAY['benefit_plans','benefit_dependents','benefit_enrollment_windows','benefit_elections','benefit_payroll_inputs'] LOOP INSERT INTO work_cache_revisions(table_name) VALUES(t) ON CONFLICT DO NOTHING;EXECUTE format('CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION work_cache_changed()',t);EXECUTE format('CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON %I FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed()',t);END LOOP;END $$;
