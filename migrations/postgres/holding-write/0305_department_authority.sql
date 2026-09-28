-- Appointments are business authority. Directory delivery has its own status.
CREATE TABLE authority_seats (
 company_id text NOT NULL,
 kind text NOT NULL CHECK(kind IN ('ceo','hr','head')),
 scope_id text NOT NULL,
 version integer NOT NULL DEFAULT 0,
 position_id text,
 person_id text,
 effective_at bigint,
 PRIMARY KEY(company_id,kind,scope_id),
 FOREIGN KEY(company_id,position_id) REFERENCES organization_positions(company_id,id),
 FOREIGN KEY(person_id) REFERENCES hr_people(id)
);
CREATE TABLE authority_appointments (
 id text PRIMARY KEY,
 company_id text NOT NULL,
 kind text NOT NULL CHECK(kind IN ('ceo','hr','head')),
 scope_id text NOT NULL,
 version integer NOT NULL,
 position_id text,
 person_id text,
 effective_at bigint NOT NULL,
 appointed_by text,
 reason text NOT NULL,
 provenance text NOT NULL CHECK(provenance IN ('explicit','legacy')),
 source_reference text,
 command_id text UNIQUE,
 created_at bigint NOT NULL,
 UNIQUE(company_id,kind,scope_id,version),
 FOREIGN KEY(company_id,kind,scope_id) REFERENCES authority_seats(company_id,kind,scope_id)
);
CREATE INDEX authority_appointment_person ON authority_seats(company_id,person_id) WHERE person_id IS NOT NULL;

CREATE TABLE department_flow_versions (
 id text PRIMARY KEY,
 company_id text NOT NULL,
 department_id text NOT NULL REFERENCES organization_nodes(id),
 version integer NOT NULL,
 steps_json jsonb NOT NULL CHECK(jsonb_typeof(steps_json)='array' AND jsonb_array_length(steps_json) BETWEEN 1 AND 20),
 published_by text NOT NULL,
 reason text NOT NULL,
 command_id text NOT NULL UNIQUE,
 created_at bigint NOT NULL,
 UNIQUE(company_id,department_id,version)
);

CREATE TABLE position_gpo_policies (
 company_id text NOT NULL,
 position_id text NOT NULL,
 version integer NOT NULL DEFAULT 0,
 revision integer NOT NULL DEFAULT 0,
 connection_id text,
 gpo_id text,
 user_filter_group_id text,
 computer_target_ou_id text,
 directory_config_status text NOT NULL DEFAULT 'pending' CHECK(directory_config_status IN ('pending','directory_applied','blocked')),
 directory_config_error text,
 updated_at bigint NOT NULL,
 PRIMARY KEY(company_id,position_id),
 FOREIGN KEY(company_id,position_id) REFERENCES organization_positions(company_id,id),
 FOREIGN KEY(connection_id) REFERENCES iam_connections(id)
);
CREATE TABLE position_gpo_revisions (
 id text PRIMARY KEY,
 company_id text NOT NULL,
 position_id text NOT NULL,
 revision integer NOT NULL,
 connection_id text NOT NULL REFERENCES iam_connections(id),
 gpo_id text NOT NULL,
 user_filter_group_id text NOT NULL,
 computer_target_ou_id text NOT NULL,
 directory_gpo_version text NOT NULL,
 affected_person_ids jsonb NOT NULL,
 published_by text NOT NULL,
 reason text NOT NULL,
 command_id text NOT NULL UNIQUE,
 created_at bigint NOT NULL,
 UNIQUE(company_id,position_id,revision)
);
CREATE TABLE position_gpo_jobs (
 id text PRIMARY KEY,
 company_id text NOT NULL,
 position_id text NOT NULL,
 person_id text NOT NULL,
 revision integer NOT NULL,
 engagement_version integer NOT NULL,
 connection_id text NOT NULL,
 gpo_id text NOT NULL,
 filter_group_id text NOT NULL,
 computer_target_ou_id text NOT NULL,
 desired_action text NOT NULL CHECK(desired_action IN ('add','remove')),
 status text NOT NULL CHECK(status IN ('queued','running','retry','directory_applied','blocked','superseded')),
 version integer NOT NULL DEFAULT 1,
 attempts integer NOT NULL DEFAULT 0,
 last_error text,
 directory_observed_at bigint,
 endpoint_observed_at bigint,
 created_at bigint NOT NULL,
 updated_at bigint NOT NULL,
 UNIQUE(company_id,position_id,person_id,revision,engagement_version,desired_action)
);
CREATE INDEX position_gpo_jobs_pending ON position_gpo_jobs(status,updated_at);

CREATE FUNCTION queue_position_gpo_change() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE policy position_gpo_revisions%ROWTYPE; now_ms bigint := floor(extract(epoch FROM clock_timestamp())*1000)::bigint;
BEGIN
 IF TG_OP='UPDATE' AND OLD.position_id IS NOT NULL AND (OLD.position_id IS DISTINCT FROM NEW.position_id OR OLD.status='active' AND NEW.status<>'active') THEN
  SELECT r.* INTO policy FROM position_gpo_policies p JOIN position_gpo_revisions r ON r.company_id=p.company_id AND r.position_id=p.position_id AND r.revision=p.revision WHERE p.company_id=OLD.company_id AND p.position_id=OLD.position_id;
  IF FOUND THEN
   INSERT INTO position_gpo_jobs(id,company_id,position_id,person_id,revision,engagement_version,connection_id,gpo_id,filter_group_id,computer_target_ou_id,desired_action,status,created_at,updated_at)
   VALUES('gpo:leave:'||OLD.id||':'||NEW.version||':'||policy.revision,OLD.company_id,OLD.position_id,OLD.person_id,policy.revision,NEW.version,policy.connection_id,policy.gpo_id,policy.user_filter_group_id,policy.computer_target_ou_id,'remove','queued',now_ms,now_ms) ON CONFLICT DO NOTHING;
  END IF;
 END IF;
 IF NEW.position_id IS NOT NULL AND NEW.status='active' AND (TG_OP='INSERT' OR OLD.position_id IS DISTINCT FROM NEW.position_id OR OLD.status IS DISTINCT FROM NEW.status) THEN
  SELECT r.* INTO policy FROM position_gpo_policies p JOIN position_gpo_revisions r ON r.company_id=p.company_id AND r.position_id=p.position_id AND r.revision=p.revision WHERE p.company_id=NEW.company_id AND p.position_id=NEW.position_id;
  IF FOUND THEN
   INSERT INTO position_gpo_jobs(id,company_id,position_id,person_id,revision,engagement_version,connection_id,gpo_id,filter_group_id,computer_target_ou_id,desired_action,status,created_at,updated_at)
   VALUES('gpo:join:'||NEW.id||':'||NEW.version||':'||policy.revision,NEW.company_id,NEW.position_id,NEW.person_id,policy.revision,NEW.version,policy.connection_id,policy.gpo_id,policy.user_filter_group_id,policy.computer_target_ou_id,'add','queued',now_ms,now_ms) ON CONFLICT DO NOTHING;
  END IF;
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER queue_position_gpo_change AFTER INSERT OR UPDATE OF position_id,status ON hr_engagements FOR EACH ROW EXECUTE FUNCTION queue_position_gpo_change();

-- Existing seats are imported as observations. No historic appointor is invented.
INSERT INTO authority_seats(company_id,kind,scope_id,version,position_id,person_id,effective_at)
SELECT c.id,'ceo',c.id,CASE WHEN p.id IS NULL THEN 0 ELSE 1 END,p.id,
 (SELECT e.person_id FROM hr_engagements e WHERE e.company_id=c.id AND e.position_id=p.id AND e.status='active' ORDER BY e.updated_at DESC,e.id LIMIT 1),
 CASE WHEN p.id IS NULL THEN NULL ELSE floor(extract(epoch FROM clock_timestamp())*1000)::bigint END
FROM organization_nodes c LEFT JOIN organization_positions p ON p.company_id=c.id AND p.is_ceo=1 AND p.active=1
WHERE c.kind='company';
INSERT INTO authority_appointments(id,company_id,kind,scope_id,version,position_id,person_id,effective_at,appointed_by,reason,provenance,source_reference,created_at)
SELECT 'legacy:ceo:'||company_id,company_id,kind,scope_id,1,position_id,person_id,effective_at,NULL,
 'Imported active CEO position and engagement','legacy','organization_positions:'||position_id,effective_at
FROM authority_seats WHERE kind='ceo' AND version=1;

INSERT INTO authority_seats(company_id,kind,scope_id,version,position_id,person_id,effective_at)
SELECT c.id,'hr',c.id,CASE WHEN h.person_id IS NULL THEN 0 ELSE 1 END,h.position_id,h.person_id,
 CASE WHEN h.person_id IS NULL THEN NULL ELSE floor(extract(epoch FROM clock_timestamp())*1000)::bigint END
FROM organization_nodes c LEFT JOIN LATERAL (
 SELECT e.person_id,e.position_id FROM assignments a JOIN hr_people p ON p.user_id=a.user_id AND p.company_id=a.company_id
 JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id AND e.status='active'
 WHERE a.company_id=c.id AND a.role='hr_admin' AND a.revoked_at IS NULL AND a.valid_from<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint AND a.valid_until>floor(extract(epoch FROM clock_timestamp())*1000)::bigint
 ORDER BY a.valid_from DESC,a.id LIMIT 1
) h ON true WHERE c.kind='company';
INSERT INTO authority_appointments(id,company_id,kind,scope_id,version,position_id,person_id,effective_at,appointed_by,reason,provenance,source_reference,created_at)
SELECT 'legacy:hr:'||company_id,company_id,kind,scope_id,1,position_id,person_id,effective_at,NULL,
 'Imported active HR assignment','legacy','assignments:hr_admin',effective_at
FROM authority_seats WHERE kind='hr' AND version=1;

INSERT INTO authority_seats(company_id,kind,scope_id,version,position_id,person_id,effective_at)
SELECT h.company_id,'head',h.department_id,1,e.position_id,h.person_id,h.updated_at
FROM department_heads h LEFT JOIN LATERAL (
 SELECT position_id FROM hr_engagements WHERE company_id=h.company_id AND person_id=h.person_id AND status='active' ORDER BY updated_at DESC,id LIMIT 1
) e ON true;
INSERT INTO authority_appointments(id,company_id,kind,scope_id,version,position_id,person_id,effective_at,appointed_by,reason,provenance,source_reference,created_at)
SELECT 'legacy:head:'||scope_id,company_id,kind,scope_id,1,position_id,person_id,effective_at,NULL,
 'Imported existing department head','legacy','department_heads:'||scope_id,effective_at
FROM authority_seats WHERE kind='head';

-- Future companies start with a vacant CEO position and vacant appointment seat.
CREATE FUNCTION initialize_company_ceo_seat() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.kind='company' THEN
  INSERT INTO authority_seats(company_id,kind,scope_id) VALUES(NEW.id,'ceo',NEW.id) ON CONFLICT DO NOTHING;
  INSERT INTO authority_seats(company_id,kind,scope_id) VALUES(NEW.id,'hr',NEW.id) ON CONFLICT DO NOTHING;
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER initialize_company_ceo_seat AFTER INSERT ON organization_nodes FOR EACH ROW EXECUTE FUNCTION initialize_company_ceo_seat();

ALTER TABLE approval_decisions DROP CONSTRAINT IF EXISTS approval_decisions_action_check;
ALTER TABLE approval_decisions ADD CONSTRAINT approval_decisions_action_check CHECK(action IN ('approve','reject','cancel','revise','reroute','initialize','override'));
