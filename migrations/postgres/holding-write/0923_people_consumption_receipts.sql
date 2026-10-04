ALTER TABLE talent_candidates ADD COLUMN privacy_hold BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE talent_candidates ADD COLUMN privacy_requested_by TEXT;
CREATE TABLE talent_candidate_privacy_events (
 id TEXT PRIMARY KEY,candidate_id TEXT NOT NULL REFERENCES talent_candidates(id),company_id TEXT NOT NULL,
 action TEXT NOT NULL CHECK(action IN ('hold','release','withdraw','erase')),candidate_version BIGINT NOT NULL,
 actor_id TEXT NOT NULL,reason TEXT NOT NULL,command_id TEXT NOT NULL UNIQUE,created_at BIGINT NOT NULL
);
CREATE TABLE talent_hire_consumption_receipts (
 id TEXT PRIMARY KEY,handoff_id TEXT NOT NULL UNIQUE REFERENCES talent_hire_handoffs(id),company_id TEXT NOT NULL,
 person_id TEXT NOT NULL REFERENCES hr_people(id),engagement_id TEXT NOT NULL UNIQUE REFERENCES hr_engagements(id),
 compensation_id TEXT NOT NULL UNIQUE REFERENCES hr_compensation_revisions(id),source_offer_version BIGINT NOT NULL,
 source_candidate_version BIGINT NOT NULL,source_digest TEXT NOT NULL CHECK(source_digest ~ '^[a-f0-9]{64}$'),
 purpose TEXT NOT NULL,actor_id TEXT NOT NULL,command_id TEXT NOT NULL UNIQUE,created_at BIGINT NOT NULL
);
CREATE TABLE learning_certificates (
 id TEXT PRIMARY KEY,handoff_id TEXT NOT NULL UNIQUE REFERENCES learning_eligibility_handoffs(id),company_id TEXT NOT NULL,
 person_id TEXT NOT NULL REFERENCES hr_people(id),assignment_id TEXT NOT NULL REFERENCES learning_assignments(id),
 source_assignment_version BIGINT NOT NULL,eligibility_code TEXT NOT NULL,valid_from TEXT NOT NULL,expires_on TEXT,
 completion_kind TEXT NOT NULL CHECK(completion_kind IN ('completed','waived')),evidence_ref TEXT NOT NULL,
 issued_by TEXT NOT NULL,created_at BIGINT NOT NULL,CHECK(expires_on IS NULL OR expires_on>=valid_from)
);
CREATE TABLE learning_certificate_revocations (
 id TEXT PRIMARY KEY,certificate_id TEXT NOT NULL UNIQUE REFERENCES learning_certificates(id),company_id TEXT NOT NULL,
 actor_id TEXT NOT NULL,reason TEXT NOT NULL,command_id TEXT NOT NULL UNIQUE,created_at BIGINT NOT NULL
);
ALTER TABLE benefit_plans ADD COLUMN required_eligibility_code TEXT;
ALTER TABLE benefit_elections ADD COLUMN learning_certificate_id TEXT REFERENCES learning_certificates(id);
CREATE FUNCTION benefit_learning_source_guard() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
 IF TG_TABLE_NAME='benefit_plans' THEN
  IF NEW.required_eligibility_code IS DISTINCT FROM OLD.required_eligibility_code THEN RAISE EXCEPTION 'BENEFIT_LEARNING_REQUIREMENT_IMMUTABLE' USING ERRCODE='23514'; END IF;
 ELSIF TG_TABLE_NAME='benefit_elections' THEN
  IF OLD.learning_certificate_id IS NOT NULL AND NEW.learning_certificate_id IS DISTINCT FROM OLD.learning_certificate_id THEN RAISE EXCEPTION 'BENEFIT_LEARNING_RECEIPT_IMMUTABLE' USING ERRCODE='23514'; END IF;
 END IF;RETURN NEW;
END $$;
CREATE TRIGGER benefit_learning_requirement_guard BEFORE UPDATE ON benefit_plans FOR EACH ROW EXECUTE FUNCTION benefit_learning_source_guard();
CREATE TRIGGER benefit_learning_receipt_guard BEFORE UPDATE ON benefit_elections FOR EACH ROW EXECUTE FUNCTION benefit_learning_source_guard();
CREATE TABLE learning_benefit_consumption_receipts (
 id TEXT PRIMARY KEY,handoff_id TEXT NOT NULL REFERENCES learning_eligibility_handoffs(id),certificate_id TEXT NOT NULL REFERENCES learning_certificates(id),
 election_id TEXT NOT NULL UNIQUE REFERENCES benefit_elections(id),company_id TEXT NOT NULL,person_id TEXT NOT NULL REFERENCES hr_people(id),
 source_assignment_version BIGINT NOT NULL,eligibility_code TEXT NOT NULL,source_digest TEXT NOT NULL CHECK(source_digest ~ '^[a-f0-9]{64}$'),
 purpose TEXT NOT NULL,actor_id TEXT NOT NULL,command_id TEXT NOT NULL UNIQUE,created_at BIGINT NOT NULL,UNIQUE(handoff_id,election_id)
);
CREATE FUNCTION people_consumption_immutable() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
 RAISE EXCEPTION 'PEOPLE_CONSUMPTION_EVIDENCE_IMMUTABLE' USING ERRCODE='23514';
END $$;
DO $$ DECLARE t TEXT; BEGIN FOREACH t IN ARRAY ARRAY['talent_candidate_privacy_events','talent_hire_consumption_receipts','learning_certificates','learning_certificate_revocations','learning_benefit_consumption_receipts'] LOOP
 EXECUTE format('CREATE TRIGGER evidence_immutable BEFORE UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION people_consumption_immutable()',t);
 INSERT INTO work_cache_revisions(table_name) VALUES(t) ON CONFLICT DO NOTHING;
 EXECUTE format('CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION work_cache_changed()',t);
 EXECUTE format('CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON %I FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed()',t);
END LOOP; END $$;
DO $$ DECLARE runtime_role TEXT; BEGIN
 IF current_user !~ '_migration$' THEN RETURN; END IF;
 FOREACH runtime_role IN ARRAY ARRAY[regexp_replace(current_user,'_migration$','_app'),regexp_replace(current_user,'_migration$','_jobs')] LOOP
  EXECUTE format('GRANT SELECT,INSERT ON talent_candidate_privacy_events,talent_hire_consumption_receipts,learning_certificates,learning_certificate_revocations,learning_benefit_consumption_receipts TO %I',runtime_role);
 END LOOP;
END $$;
