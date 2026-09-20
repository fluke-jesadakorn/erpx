CREATE TABLE learning_courses (
 id TEXT PRIMARY KEY,company_id TEXT NOT NULL,title TEXT NOT NULL,description TEXT NOT NULL,provider TEXT NOT NULL,duration_minutes BIGINT NOT NULL CHECK(duration_minutes>0),
 required_for TEXT,status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','published','retired','rejected')),created_by TEXT NOT NULL,reviewed_by TEXT,reviewed_at BIGINT,
 version BIGINT NOT NULL DEFAULT 1,created_at BIGINT NOT NULL,updated_at BIGINT NOT NULL
);
CREATE TABLE learning_course_events (id TEXT PRIMARY KEY,course_id TEXT NOT NULL REFERENCES learning_courses(id),version BIGINT NOT NULL,action TEXT NOT NULL,actor_id TEXT NOT NULL,reason TEXT NOT NULL,status TEXT NOT NULL,created_at BIGINT NOT NULL,UNIQUE(course_id,version));
CREATE TABLE learning_assignments (
 id TEXT PRIMARY KEY,company_id TEXT NOT NULL,branch_id TEXT NOT NULL,department_id TEXT,course_id TEXT NOT NULL REFERENCES learning_courses(id),course_version BIGINT NOT NULL,
 learner_id TEXT NOT NULL,due_date TEXT NOT NULL,status TEXT NOT NULL DEFAULT 'assigned' CHECK(status IN ('assigned','in_progress','completed','waived','cancelled')),
 attendance TEXT CHECK(attendance IN ('attended','absent')),score_bps BIGINT CHECK(score_bps BETWEEN 0 AND 10000),result_evidence TEXT,completion_reason TEXT,
 created_by TEXT NOT NULL,version BIGINT NOT NULL DEFAULT 1,created_at BIGINT NOT NULL,updated_at BIGINT NOT NULL,CHECK(status NOT IN ('completed','waived') OR completion_reason IS NOT NULL)
);
CREATE INDEX learning_assignment_queue ON learning_assignments(company_id,learner_id,status,due_date);
CREATE TABLE learning_assignment_events (id TEXT PRIMARY KEY,assignment_id TEXT NOT NULL REFERENCES learning_assignments(id),version BIGINT NOT NULL,action TEXT NOT NULL,actor_id TEXT NOT NULL,reason TEXT NOT NULL,status TEXT NOT NULL,created_at BIGINT NOT NULL,UNIQUE(assignment_id,version));
CREATE TABLE learning_eligibility_handoffs (
 id TEXT PRIMARY KEY,assignment_id TEXT NOT NULL UNIQUE REFERENCES learning_assignments(id),company_id TEXT NOT NULL,person_id TEXT NOT NULL,eligibility_code TEXT NOT NULL,
 source_assignment_version BIGINT NOT NULL,status TEXT NOT NULL DEFAULT 'open' CHECK(status='open'),created_at BIGINT NOT NULL
);
CREATE FUNCTION learning_guard() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
 IF TG_TABLE_NAME IN ('learning_course_events','learning_assignment_events','learning_eligibility_handoffs') OR TG_OP='DELETE' THEN RAISE EXCEPTION 'LEARNING_HISTORY_IMMUTABLE' USING ERRCODE='23514'; END IF;
 IF TG_TABLE_NAME='learning_courses' THEN IF (NEW.id,NEW.company_id,NEW.created_by,NEW.created_at) IS DISTINCT FROM (OLD.id,OLD.company_id,OLD.created_by,OLD.created_at) THEN RAISE EXCEPTION 'LEARNING_COURSE_SOURCE_IMMUTABLE' USING ERRCODE='23514'; END IF;
 ELSIF TG_TABLE_NAME='learning_assignments' THEN IF (NEW.id,NEW.company_id,NEW.branch_id,NEW.department_id,NEW.course_id,NEW.course_version,NEW.learner_id,NEW.created_by,NEW.created_at) IS DISTINCT FROM (OLD.id,OLD.company_id,OLD.branch_id,OLD.department_id,OLD.course_id,OLD.course_version,OLD.learner_id,OLD.created_by,OLD.created_at) THEN RAISE EXCEPTION 'LEARNING_ASSIGNMENT_SOURCE_IMMUTABLE' USING ERRCODE='23514'; END IF; END IF;RETURN NEW;
END $$;
CREATE TRIGGER learning_course_guard BEFORE UPDATE OR DELETE ON learning_courses FOR EACH ROW EXECUTE FUNCTION learning_guard();
CREATE TRIGGER learning_course_event_guard BEFORE UPDATE OR DELETE ON learning_course_events FOR EACH ROW EXECUTE FUNCTION learning_guard();
CREATE TRIGGER learning_assignment_guard BEFORE UPDATE OR DELETE ON learning_assignments FOR EACH ROW EXECUTE FUNCTION learning_guard();
CREATE TRIGGER learning_assignment_event_guard BEFORE UPDATE OR DELETE ON learning_assignment_events FOR EACH ROW EXECUTE FUNCTION learning_guard();
CREATE TRIGGER learning_eligibility_guard BEFORE UPDATE OR DELETE ON learning_eligibility_handoffs FOR EACH ROW EXECUTE FUNCTION learning_guard();
DO $$ DECLARE t TEXT; BEGIN FOREACH t IN ARRAY ARRAY['learning_courses','learning_assignments','learning_eligibility_handoffs'] LOOP INSERT INTO work_cache_revisions(table_name) VALUES(t) ON CONFLICT DO NOTHING;EXECUTE format('CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION work_cache_changed()',t);EXECUTE format('CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON %I FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed()',t);END LOOP;END $$;
