-- Company-owned evaluation; immutable revisions retain the evidence used in decisions.
CREATE TABLE hr_evaluation_definitions (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, kind TEXT NOT NULL CHECK(kind IN ('skill','role','policy','cycle','band','benchmark')),
 version INTEGER NOT NULL DEFAULT 1 CHECK(version>0), payload_json TEXT NOT NULL CHECK(erpx_json_valid(payload_json)),
 created_by TEXT NOT NULL, updated_by TEXT NOT NULL, reason TEXT NOT NULL, created_at BIGINT NOT NULL, updated_at BIGINT NOT NULL,
 UNIQUE(company_id,id)
);
CREATE TABLE hr_evaluation_definition_history (
 id TEXT NOT NULL REFERENCES hr_evaluation_definitions(id), company_id TEXT NOT NULL, version INTEGER NOT NULL,
 kind TEXT NOT NULL, payload_json TEXT NOT NULL, actor_id TEXT NOT NULL, reason TEXT NOT NULL, created_at BIGINT NOT NULL,
 PRIMARY KEY(id,version)
);
CREATE TABLE hr_evaluation_assessments (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, engagement_id TEXT NOT NULL REFERENCES hr_engagements(id),
 cycle_id TEXT NOT NULL REFERENCES hr_evaluation_definitions(id), role_id TEXT NOT NULL REFERENCES hr_evaluation_definitions(id), role_version INTEGER NOT NULL,
 period_start TEXT NOT NULL, period_end TEXT NOT NULL, status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','submitted','calibrating','confirmed')),
 content_json TEXT NOT NULL CHECK(erpx_json_valid(content_json)), version INTEGER NOT NULL DEFAULT 1, created_by TEXT NOT NULL,
 reviewed_by TEXT, confirmed_by TEXT, reason TEXT NOT NULL, created_at BIGINT NOT NULL, updated_at BIGINT NOT NULL,
 CHECK(period_start<=period_end), FOREIGN KEY(role_id,role_version) REFERENCES hr_evaluation_definition_history(id,version),
 UNIQUE(company_id,id), UNIQUE(engagement_id,cycle_id,period_start)
);
CREATE TABLE hr_evaluation_assessment_history (
 id TEXT NOT NULL REFERENCES hr_evaluation_assessments(id), company_id TEXT NOT NULL, version INTEGER NOT NULL, status TEXT NOT NULL,
 content_json TEXT NOT NULL, actor_id TEXT NOT NULL, reason TEXT NOT NULL, created_at BIGINT NOT NULL, PRIMARY KEY(id,version)
);
CREATE TABLE hr_evaluation_evidence (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, assessment_id TEXT NOT NULL REFERENCES hr_evaluation_assessments(id),
 source_type TEXT NOT NULL CHECK(source_type IN ('kpi','task','project','learning','file')), source_id TEXT,
 description TEXT NOT NULL, observed_on TEXT NOT NULL, source_json TEXT NOT NULL CHECK(erpx_json_valid(source_json)),
 created_by TEXT NOT NULL, created_at BIGINT NOT NULL
);
CREATE TABLE hr_evaluation_pay_basis (
 engagement_id TEXT NOT NULL REFERENCES hr_engagements(id), company_id TEXT NOT NULL, version INTEGER NOT NULL,
 effective_from TEXT NOT NULL, payload_json TEXT NOT NULL, confirmed_by TEXT NOT NULL, created_at BIGINT NOT NULL,
 PRIMARY KEY(engagement_id,version), UNIQUE(engagement_id,effective_from)
);
CREATE TABLE hr_evaluation_compensation_drafts (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, assessment_id TEXT NOT NULL REFERENCES hr_evaluation_assessments(id), assessment_version INTEGER NOT NULL,
 compensation_id TEXT NOT NULL UNIQUE REFERENCES hr_compensation_revisions(id), previous_id TEXT NOT NULL REFERENCES hr_compensation_revisions(id), previous_version INTEGER NOT NULL,
 engagement_version INTEGER NOT NULL, policy_id TEXT NOT NULL, policy_version INTEGER NOT NULL,
 basis_version INTEGER NOT NULL, reason TEXT NOT NULL, created_by TEXT NOT NULL, approved_by TEXT, status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','approved')),
 version INTEGER NOT NULL DEFAULT 1, created_at BIGINT NOT NULL, updated_at BIGINT NOT NULL,
 UNIQUE(assessment_id,assessment_version), CHECK(approved_by IS NULL OR approved_by<>created_by)
);
CREATE TABLE hr_evaluation_ai_jobs (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, actor_json TEXT NOT NULL, query_json TEXT NOT NULL, source_hash TEXT NOT NULL,
 provider_json TEXT NOT NULL, state TEXT NOT NULL DEFAULT 'queued' CHECK(state IN ('queued','running','ready','failed','stale')),
 result_json TEXT, error TEXT, attempts INTEGER NOT NULL DEFAULT 0, lease_token TEXT, claim_until BIGINT,
 created_by TEXT NOT NULL, created_at BIGINT NOT NULL, updated_at BIGINT NOT NULL
);
CREATE INDEX hr_evaluation_company ON hr_evaluation_definitions(company_id,kind);
CREATE INDEX hr_evaluation_employee ON hr_evaluation_assessments(company_id,cycle_id,engagement_id);
CREATE INDEX hr_evaluation_evidence_assessment ON hr_evaluation_evidence(assessment_id);

CREATE FUNCTION hr_evaluation_history() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_TABLE_NAME='hr_evaluation_definitions' THEN
  INSERT INTO hr_evaluation_definition_history(id,company_id,version,kind,payload_json,actor_id,reason,created_at)
  VALUES(NEW.id,NEW.company_id,NEW.version,NEW.kind,NEW.payload_json,NEW.updated_by,NEW.reason,NEW.updated_at);
 ELSE
  INSERT INTO hr_evaluation_assessment_history(id,company_id,version,status,content_json,actor_id,reason,created_at)
  VALUES(NEW.id,NEW.company_id,NEW.version,NEW.status,NEW.content_json,COALESCE(NEW.confirmed_by,NEW.reviewed_by,NEW.created_by),NEW.reason,NEW.updated_at);
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER hr_evaluation_definition_history AFTER INSERT OR UPDATE ON hr_evaluation_definitions FOR EACH ROW EXECUTE FUNCTION hr_evaluation_history();
CREATE TRIGGER hr_evaluation_assessment_history AFTER INSERT OR UPDATE ON hr_evaluation_assessments FOR EACH ROW EXECUTE FUNCTION hr_evaluation_history();
CREATE FUNCTION hr_evaluation_immutable() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'EVALUATION_EVIDENCE_IMMUTABLE'; END $$;
CREATE TRIGGER hr_evaluation_definition_immutable BEFORE UPDATE OR DELETE ON hr_evaluation_definition_history FOR EACH ROW EXECUTE FUNCTION hr_evaluation_immutable();
CREATE TRIGGER hr_evaluation_assessment_immutable BEFORE UPDATE OR DELETE ON hr_evaluation_assessment_history FOR EACH ROW EXECUTE FUNCTION hr_evaluation_immutable();
CREATE TRIGGER hr_evaluation_evidence_immutable BEFORE UPDATE OR DELETE ON hr_evaluation_evidence FOR EACH ROW EXECUTE FUNCTION hr_evaluation_immutable();
CREATE TRIGGER hr_evaluation_basis_immutable BEFORE UPDATE OR DELETE ON hr_evaluation_pay_basis FOR EACH ROW EXECUTE FUNCTION hr_evaluation_immutable();
