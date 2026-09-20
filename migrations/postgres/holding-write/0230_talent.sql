CREATE TABLE talent_requisitions (
 id TEXT PRIMARY KEY,company_id TEXT NOT NULL,branch_id TEXT NOT NULL,department_id TEXT,job_title TEXT NOT NULL,openings BIGINT NOT NULL CHECK(openings>0),target_start_date TEXT NOT NULL,
 description TEXT NOT NULL,status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','approved','open','closed','cancelled','rejected')),created_by TEXT NOT NULL,reviewed_by TEXT,version BIGINT NOT NULL DEFAULT 1,created_at BIGINT NOT NULL,updated_at BIGINT NOT NULL
);
CREATE TABLE talent_requisition_events (id TEXT PRIMARY KEY,requisition_id TEXT NOT NULL REFERENCES talent_requisitions(id),version BIGINT NOT NULL,action TEXT NOT NULL,actor_id TEXT NOT NULL,reason TEXT NOT NULL,status TEXT NOT NULL,created_at BIGINT NOT NULL,UNIQUE(requisition_id,version));
CREATE TABLE talent_candidates (
 id TEXT PRIMARY KEY,company_id TEXT NOT NULL,legal_name TEXT NOT NULL,email TEXT NOT NULL,phone TEXT,consent_evidence TEXT NOT NULL,consented_at BIGINT NOT NULL,retention_until TEXT NOT NULL,
 status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active','withdrawn','erased')),created_by TEXT NOT NULL,version BIGINT NOT NULL DEFAULT 1,created_at BIGINT NOT NULL,updated_at BIGINT NOT NULL,UNIQUE(company_id,email)
);
CREATE TABLE talent_candidate_events (id TEXT PRIMARY KEY,candidate_id TEXT NOT NULL REFERENCES talent_candidates(id),version BIGINT NOT NULL,action TEXT NOT NULL,actor_id TEXT NOT NULL,reason TEXT NOT NULL,status TEXT NOT NULL,created_at BIGINT NOT NULL,UNIQUE(candidate_id,version));
CREATE TABLE talent_applications (
 id TEXT PRIMARY KEY,company_id TEXT NOT NULL,requisition_id TEXT NOT NULL REFERENCES talent_requisitions(id),requisition_version BIGINT NOT NULL,candidate_id TEXT NOT NULL REFERENCES talent_candidates(id),
 status TEXT NOT NULL DEFAULT 'applied' CHECK(status IN ('applied','interview','selected','offered','accepted','rejected','withdrawn')),owner_id TEXT NOT NULL,created_by TEXT NOT NULL,version BIGINT NOT NULL DEFAULT 1,created_at BIGINT NOT NULL,updated_at BIGINT NOT NULL,UNIQUE(requisition_id,candidate_id)
);
CREATE INDEX talent_application_queue ON talent_applications(company_id,owner_id,status,updated_at);
CREATE TABLE talent_application_events (id TEXT PRIMARY KEY,application_id TEXT NOT NULL REFERENCES talent_applications(id),version BIGINT NOT NULL,action TEXT NOT NULL,actor_id TEXT NOT NULL,reason TEXT NOT NULL,status TEXT NOT NULL,created_at BIGINT NOT NULL,UNIQUE(application_id,version));
CREATE TABLE talent_proposals (
 id TEXT PRIMARY KEY,application_id TEXT NOT NULL UNIQUE REFERENCES talent_applications(id),application_version BIGINT NOT NULL,recommendation TEXT NOT NULL CHECK(recommendation IN ('hire','hold','reject')),score_bps BIGINT NOT NULL CHECK(score_bps BETWEEN 0 AND 10000),evidence TEXT NOT NULL,
 status TEXT NOT NULL DEFAULT 'submitted' CHECK(status IN ('submitted','accepted','rejected')),submitted_by TEXT NOT NULL,reviewed_by TEXT,version BIGINT NOT NULL DEFAULT 1,created_at BIGINT NOT NULL,updated_at BIGINT NOT NULL
);
CREATE TABLE talent_offers (
 id TEXT PRIMARY KEY,company_id TEXT NOT NULL,application_id TEXT NOT NULL UNIQUE REFERENCES talent_applications(id),application_version BIGINT NOT NULL,currency TEXT NOT NULL,base_amount_minor BIGINT NOT NULL CHECK(base_amount_minor>=0),start_date TEXT NOT NULL,expires_on TEXT NOT NULL,terms TEXT NOT NULL,
 status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','approved','sent','accepted','declined','withdrawn')),created_by TEXT NOT NULL,approved_by TEXT,accepted_evidence TEXT,version BIGINT NOT NULL DEFAULT 1,created_at BIGINT NOT NULL,updated_at BIGINT NOT NULL
);
CREATE TABLE talent_offer_events (id TEXT PRIMARY KEY,offer_id TEXT NOT NULL REFERENCES talent_offers(id),version BIGINT NOT NULL,action TEXT NOT NULL,actor_id TEXT NOT NULL,reason TEXT NOT NULL,status TEXT NOT NULL,created_at BIGINT NOT NULL,UNIQUE(offer_id,version));
CREATE TABLE talent_hire_handoffs (
 id TEXT PRIMARY KEY,offer_id TEXT NOT NULL UNIQUE REFERENCES talent_offers(id),company_id TEXT NOT NULL,branch_id TEXT NOT NULL,department_id TEXT,candidate_id TEXT NOT NULL,source_offer_version BIGINT NOT NULL,status TEXT NOT NULL DEFAULT 'open' CHECK(status='open'),created_at BIGINT NOT NULL
);
CREATE FUNCTION talent_guard() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
 IF TG_TABLE_NAME IN ('talent_requisition_events','talent_candidate_events','talent_application_events','talent_offer_events','talent_hire_handoffs') OR TG_OP='DELETE' THEN RAISE EXCEPTION 'TALENT_HISTORY_IMMUTABLE' USING ERRCODE='23514'; END IF;
 IF TG_TABLE_NAME='talent_requisitions' THEN IF (NEW.id,NEW.company_id,NEW.branch_id,NEW.department_id,NEW.created_by,NEW.created_at) IS DISTINCT FROM (OLD.id,OLD.company_id,OLD.branch_id,OLD.department_id,OLD.created_by,OLD.created_at) THEN RAISE EXCEPTION 'TALENT_REQUISITION_SOURCE_IMMUTABLE' USING ERRCODE='23514'; END IF;
 ELSIF TG_TABLE_NAME='talent_candidates' THEN IF (NEW.id,NEW.company_id,NEW.created_by,NEW.created_at,NEW.consented_at) IS DISTINCT FROM (OLD.id,OLD.company_id,OLD.created_by,OLD.created_at,OLD.consented_at) THEN RAISE EXCEPTION 'TALENT_CANDIDATE_SOURCE_IMMUTABLE' USING ERRCODE='23514'; END IF;
 ELSIF TG_TABLE_NAME='talent_applications' THEN IF (NEW.id,NEW.company_id,NEW.requisition_id,NEW.requisition_version,NEW.candidate_id,NEW.created_by,NEW.created_at) IS DISTINCT FROM (OLD.id,OLD.company_id,OLD.requisition_id,OLD.requisition_version,OLD.candidate_id,OLD.created_by,OLD.created_at) THEN RAISE EXCEPTION 'TALENT_APPLICATION_SOURCE_IMMUTABLE' USING ERRCODE='23514'; END IF;
 ELSIF TG_TABLE_NAME='talent_proposals' THEN IF (NEW.id,NEW.application_id,NEW.application_version,NEW.submitted_by,NEW.created_at) IS DISTINCT FROM (OLD.id,OLD.application_id,OLD.application_version,OLD.submitted_by,OLD.created_at) THEN RAISE EXCEPTION 'TALENT_PROPOSAL_SOURCE_IMMUTABLE' USING ERRCODE='23514'; END IF;
 ELSIF TG_TABLE_NAME='talent_offers' THEN IF (NEW.id,NEW.company_id,NEW.application_id,NEW.application_version,NEW.currency,NEW.base_amount_minor,NEW.start_date,NEW.expires_on,NEW.terms,NEW.created_by,NEW.created_at) IS DISTINCT FROM (OLD.id,OLD.company_id,OLD.application_id,OLD.application_version,OLD.currency,OLD.base_amount_minor,OLD.start_date,OLD.expires_on,OLD.terms,OLD.created_by,OLD.created_at) THEN RAISE EXCEPTION 'TALENT_OFFER_SOURCE_IMMUTABLE' USING ERRCODE='23514'; END IF; END IF; RETURN NEW;
END $$;
DO $$ DECLARE t TEXT; BEGIN FOREACH t IN ARRAY ARRAY['talent_requisitions','talent_requisition_events','talent_candidates','talent_candidate_events','talent_applications','talent_application_events','talent_proposals','talent_offers','talent_offer_events','talent_hire_handoffs'] LOOP EXECUTE format('CREATE TRIGGER talent_guard BEFORE UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION talent_guard()',t);END LOOP;END $$;
DO $$ DECLARE t TEXT; BEGIN FOREACH t IN ARRAY ARRAY['talent_requisitions','talent_candidates','talent_applications','talent_proposals','talent_offers','talent_hire_handoffs'] LOOP INSERT INTO work_cache_revisions(table_name) VALUES(t) ON CONFLICT DO NOTHING;EXECUTE format('CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION work_cache_changed()',t);EXECUTE format('CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON %I FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed()',t);END LOOP;END $$;
