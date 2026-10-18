-- Bounded Projects worker-reference preparation/link and reviewed HR-only
-- placement. Original Directory/lifecycle receipts and employment effects remain.
CREATE INDEX people_project_worker_receipt_person ON command_receipts(record_id) WHERE jsonb_exists(result_json::jsonb,'_projectWorkerStartup');
CREATE INDEX people_project_worker_receipt_case ON command_receipts(record_id) WHERE jsonb_exists(result_json::jsonb,'_projectWorkerPlacement');
CREATE TABLE people_project_worker_starters(
 person_id text PRIMARY KEY REFERENCES hr_people(id),company_id text NOT NULL,account_id text NOT NULL,
 engagement_id text NOT NULL REFERENCES hr_engagements(id),command_id text NOT NULL UNIQUE REFERENCES command_receipts(command_id),request_hash text NOT NULL CHECK(request_hash ~ '^[a-f0-9]{64}$'),proof_json jsonb NOT NULL CHECK(jsonb_typeof(proof_json)='object')
);
CREATE INDEX people_project_worker_starter_company ON people_project_worker_starters(company_id,account_id);
CREATE TABLE people_project_worker_placements(
 case_id text PRIMARY KEY REFERENCES people_lifecycle_cases(id),company_id text NOT NULL,person_id text NOT NULL REFERENCES people_project_worker_starters(person_id),engagement_id text NOT NULL REFERENCES hr_engagements(id),starter_command_id text NOT NULL REFERENCES command_receipts(command_id),command_id text NOT NULL UNIQUE REFERENCES command_receipts(command_id),request_hash text NOT NULL CHECK(request_hash ~ '^[a-f0-9]{64}$'),proof_json jsonb NOT NULL CHECK(jsonb_typeof(proof_json)='object')
);
CREATE INDEX people_project_worker_placement_starter ON people_project_worker_placements(starter_command_id);
CREATE FUNCTION people_project_worker_source_immutable() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='PROJECT_WORKER_SOURCE_CHANGED',DETAIL='The original worker preparation and reviewed placement source are immutable.'; END $$;
CREATE TRIGGER people_project_worker_starter_immutable BEFORE UPDATE OR DELETE ON people_project_worker_starters FOR EACH ROW EXECUTE FUNCTION people_project_worker_source_immutable();
CREATE TRIGGER people_project_worker_placement_immutable BEFORE UPDATE OR DELETE ON people_project_worker_placements FOR EACH ROW EXECUTE FUNCTION people_project_worker_source_immutable();
CREATE FUNCTION people_project_worker_marker_birth() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
 IF TG_OP='UPDATE' AND (OLD.result_json::jsonb->'_projectWorkerStartup' IS DISTINCT FROM NEW.result_json::jsonb->'_projectWorkerStartup' OR OLD.result_json::jsonb->'_projectWorkerPlacement' IS DISTINCT FROM NEW.result_json::jsonb->'_projectWorkerPlacement') THEN RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='PROJECT_WORKER_SOURCE_CHANGED',DETAIL='A worker marker is born only with its original native receipt INSERT.'; END IF;
 IF jsonb_exists(NEW.result_json::jsonb,'_projectWorkerStartup') AND NEW.operation NOT IN ('people.directory.create','people.directory.update') OR jsonb_exists(NEW.result_json::jsonb,'_projectWorkerPlacement') AND NEW.operation<>'people.lifecycle.submit' OR jsonb_exists(NEW.result_json::jsonb,'_projectWorkerStartup') AND jsonb_exists(NEW.result_json::jsonb,'_projectWorkerPlacement') THEN RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='PROJECT_WORKER_SOURCE_CHANGED'; END IF;RETURN NEW;
END $$;
CREATE TRIGGER people_project_worker_marker_birth BEFORE INSERT OR UPDATE ON command_receipts FOR EACH ROW EXECUTE FUNCTION people_project_worker_marker_birth();
CREATE FUNCTION people_assert_project_worker_starter(target text) RETURNS void LANGUAGE plpgsql AS $$
DECLARE source people_project_worker_starters%ROWTYPE;marked boolean;
BEGIN
 SELECT * INTO source FROM people_project_worker_starters WHERE person_id=target;
 SELECT EXISTS(SELECT 1 FROM command_receipts r WHERE r.record_id=target AND jsonb_exists(r.result_json::jsonb,'_projectWorkerStartup')) INTO marked;
 IF source.person_id IS NULL AND NOT marked THEN RETURN; END IF;
 IF source.person_id IS NULL OR NOT marked OR NOT EXISTS(SELECT 1 FROM (SELECT p.id person_id,p.company_id,e.id engagement_id,r.command_id,r.request_hash,r.result_json::jsonb->'_projectWorkerStartup' proof,r.result_json::jsonb->'_projectWorkerStartup'->>'accountId' account_id
 FROM command_receipts r JOIN hr_people p ON p.id=r.record_id AND p.company_id=r.authority_json::jsonb->>'companyId'
 JOIN hr_engagements e ON e.id=r.result_json::jsonb->'_projectWorkerStartup'->>'engagementId' AND e.person_id=p.id AND e.company_id=p.company_id
 WHERE to_regclass('people_project_worker_starters') IS NOT NULL AND r.operation IN ('people.directory.create','people.directory.update') AND jsonb_exists(r.result_json::jsonb,'_projectWorkerStartup')
 AND jsonb_typeof(r.result_json::jsonb->'_projectWorkerStartup'->'resultVersion')='number' AND r.result_json::jsonb->'_projectWorkerStartup'->>'resultVersion'~'^[1-9][0-9]*$'
 AND r.result_json::jsonb->>'id'=p.id AND r.result_json::jsonb->>'version'=r.result_json::jsonb->'_projectWorkerStartup'->>'resultVersion'
 AND r.result_json::jsonb->'_projectWorkerStartup'->>'kind'='project-worker-startup' AND r.result_json::jsonb->'_projectWorkerStartup'->>'personId'=p.id AND r.result_json::jsonb->'_projectWorkerStartup'->>'companyId'=p.company_id AND r.result_json::jsonb->'_projectWorkerStartup'->>'commandId'=r.command_id AND r.result_json::jsonb->'_projectWorkerStartup'->>'operation'=r.operation
 AND ((r.operation='people.directory.create' AND r.result_json::jsonb->'_projectWorkerStartup'->>'mode'='create' AND p.id='person-'||r.command_id AND e.id='eng-'||r.command_id) OR (r.operation='people.directory.update' AND r.result_json::jsonb->'_projectWorkerStartup'->>'mode'='link'))) actual WHERE actual.person_id=source.person_id AND actual.company_id=source.company_id AND actual.engagement_id=source.engagement_id AND actual.account_id=source.account_id AND actual.command_id=source.command_id AND actual.request_hash=source.request_hash AND actual.proof=source.proof_json
 AND actual.proof=jsonb_build_object('kind',actual.proof->'kind','companyId',actual.proof->'companyId','accountId',actual.proof->'accountId','personId',actual.proof->'personId','engagementId',actual.proof->'engagementId','commandId',actual.proof->'commandId','operation',actual.proof->'operation','mode',actual.proof->'mode','resultVersion',actual.proof->'resultVersion')) THEN RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='PROJECT_WORKER_SOURCE_CHANGED',DETAIL='Worker identity and its original native Directory receipt must reciprocally match.'; END IF;
END $$;
CREATE FUNCTION people_assert_project_worker_placement(target text) RETURNS void LANGUAGE plpgsql AS $$
DECLARE source people_project_worker_placements%ROWTYPE;marked boolean;
BEGIN
 SELECT * INTO source FROM people_project_worker_placements WHERE case_id=target;
 SELECT EXISTS(SELECT 1 FROM command_receipts r WHERE r.record_id=target AND jsonb_exists(r.result_json::jsonb,'_projectWorkerPlacement')) INTO marked;
 IF source.case_id IS NULL AND NOT marked THEN RETURN; END IF;
 IF source.case_id IS NULL OR NOT marked OR NOT EXISTS(SELECT 1 FROM (SELECT c.id,c.company_id,c.person_id,c.engagement_id,starter.account_id,starter.command_id starter_command_id,r.command_id,r.request_hash,r.result_json::jsonb->'_projectWorkerPlacement' proof
 FROM people_lifecycle_cases c JOIN command_receipts r ON r.record_id=c.id AND r.operation='people.lifecycle.submit' AND r.actor_id=c.created_by AND r.authority_json::jsonb->>'companyId'=c.company_id
 JOIN (SELECT p.id person_id,p.company_id,e.id engagement_id,r.command_id,r.request_hash,r.result_json::jsonb->'_projectWorkerStartup' proof,r.result_json::jsonb->'_projectWorkerStartup'->>'accountId' account_id
 FROM command_receipts r JOIN hr_people p ON p.id=r.record_id AND p.company_id=r.authority_json::jsonb->>'companyId'
 JOIN hr_engagements e ON e.id=r.result_json::jsonb->'_projectWorkerStartup'->>'engagementId' AND e.person_id=p.id AND e.company_id=p.company_id
 WHERE to_regclass('people_project_worker_starters') IS NOT NULL AND r.operation IN ('people.directory.create','people.directory.update') AND jsonb_exists(r.result_json::jsonb,'_projectWorkerStartup')
 AND jsonb_typeof(r.result_json::jsonb->'_projectWorkerStartup'->'resultVersion')='number' AND r.result_json::jsonb->'_projectWorkerStartup'->>'resultVersion'~'^[1-9][0-9]*$'
 AND r.result_json::jsonb->>'id'=p.id AND r.result_json::jsonb->>'version'=r.result_json::jsonb->'_projectWorkerStartup'->>'resultVersion'
 AND r.result_json::jsonb->'_projectWorkerStartup'->>'kind'='project-worker-startup' AND r.result_json::jsonb->'_projectWorkerStartup'->>'personId'=p.id AND r.result_json::jsonb->'_projectWorkerStartup'->>'companyId'=p.company_id AND r.result_json::jsonb->'_projectWorkerStartup'->>'commandId'=r.command_id AND r.result_json::jsonb->'_projectWorkerStartup'->>'operation'=r.operation
 AND ((r.operation='people.directory.create' AND r.result_json::jsonb->'_projectWorkerStartup'->>'mode'='create' AND p.id='person-'||r.command_id AND e.id='eng-'||r.command_id) OR (r.operation='people.directory.update' AND r.result_json::jsonb->'_projectWorkerStartup'->>'mode'='link'))) starter ON starter.person_id=c.person_id AND starter.engagement_id=c.engagement_id AND starter.company_id=c.company_id AND starter.command_id=r.result_json::jsonb->'_projectWorkerPlacement'->>'starterCommandId' AND starter.request_hash=r.result_json::jsonb->'_projectWorkerPlacement'->>'starterRequestHash'
 WHERE to_regclass('people_project_worker_placements') IS NOT NULL AND jsonb_exists(r.result_json::jsonb,'_projectWorkerPlacement') AND c.id='lifecycle-'||r.command_id AND c.workflow='placement' AND c.kind='onboarding' AND c.package_id IS NULL AND c.package_revision IS NULL
 AND r.result_json::jsonb->>'id'=c.id AND r.result_json::jsonb->>'status'='scheduled' AND r.result_json::jsonb->>'version'='1'
 AND r.result_json::jsonb->'_projectWorkerPlacement'->>'kind'='project-worker-placement' AND r.result_json::jsonb->'_projectWorkerPlacement'->>'caseId'=c.id AND r.result_json::jsonb->'_projectWorkerPlacement'->>'commandId'=r.command_id
 AND r.result_json::jsonb->'_projectWorkerPlacement'->>'companyId'=c.company_id AND r.result_json::jsonb->'_projectWorkerPlacement'->>'personId'=c.person_id AND r.result_json::jsonb->'_projectWorkerPlacement'->>'engagementId'=c.engagement_id AND r.result_json::jsonb->'_projectWorkerPlacement'->>'accountId'=starter.account_id
 AND r.result_json::jsonb->'_projectWorkerPlacement'->>'authorizingAssignmentId'=c.authorizing_assignment_id AND r.result_json::jsonb->'_projectWorkerPlacement'->'definition'=c.definition AND r.result_json::jsonb->'_projectWorkerPlacement'->'reviewSnapshot'=c.review_snapshot
 AND r.authority_json::jsonb->>'assignmentId'=c.authorizing_assignment_id AND c.definition->>'kind'='onboarding' AND c.definition->>'personId'=c.person_id AND c.definition->>'engagementId'=c.engagement_id AND c.definition->>'expectedVersion'='0' AND c.definition->>'effectiveAt'=c.effective_at::text
 AND c.definition->>'acceptRecommendedPay'='false' AND c.definition->>'acceptRecommendedLeave'='false') actual WHERE actual.id=source.case_id AND actual.company_id=source.company_id AND actual.person_id=source.person_id AND actual.engagement_id=source.engagement_id AND actual.command_id=source.command_id AND actual.request_hash=source.request_hash AND actual.starter_command_id=source.starter_command_id AND actual.proof=source.proof_json
 AND actual.proof=jsonb_build_object('kind',actual.proof->'kind','companyId',actual.proof->'companyId','accountId',actual.proof->'accountId','personId',actual.proof->'personId','engagementId',actual.proof->'engagementId','starterCommandId',actual.proof->'starterCommandId','starterRequestHash',actual.proof->'starterRequestHash','caseId',actual.proof->'caseId','commandId',actual.proof->'commandId','authorizingAssignmentId',actual.proof->'authorizingAssignmentId','definition',actual.proof->'definition','reviewSnapshot',actual.proof->'reviewSnapshot')) THEN RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='PROJECT_WORKER_SOURCE_CHANGED',DETAIL='Placement must retain its exact worker, reviewed definition, original author and native submit receipt.'; END IF;
 IF EXISTS(SELECT 1 FROM people_lifecycle_cases c WHERE c.id=target AND c.status IN ('completed','cancelled') AND NOT EXISTS(SELECT 1 FROM (SELECT c.id,c.company_id FROM people_lifecycle_cases c WHERE
 (c.status='completed' AND EXISTS(SELECT 1 FROM (SELECT id,company_id,actor_id,assignment_id,action,target_id,evidence FROM iam_audit) a JOIN (SELECT id,command_id,event_type,aggregate_id,aggregate_version,payload_json FROM outbox_events) o ON o.id='event-applied-'||c.id AND o.command_id='system-apply-'||c.id AND o.event_type='people.lifecycle.applied' AND o.aggregate_id=c.id AND o.aggregate_version=c.version
 WHERE a.id='applied-'||c.id AND a.company_id=c.company_id AND a.target_id=c.id AND a.action='people.lifecycle.applied' AND a.actor_id='system:people-lifecycle' AND a.assignment_id='system:people-lifecycle' AND a.evidence->>'personId'=c.person_id AND a.evidence->>'kind'=c.kind AND a.evidence->>'effectiveAt'=c.effective_at::text AND o.payload_json::jsonb->>'companyId'=c.company_id AND o.payload_json::jsonb->>'personId'=c.person_id AND o.payload_json::jsonb->>'state'='completed'))
 OR (c.status='cancelled' AND EXISTS(SELECT 1 FROM (SELECT command_id,operation,record_id,actor_id,authority_json,result_json FROM command_receipts) r JOIN (SELECT command_id,action,record_id,record_version FROM audit_events) a ON a.command_id=r.command_id AND a.action='people.lifecycle.cancel' AND a.record_id=c.id AND a.record_version=c.version JOIN (SELECT id,command_id,event_type,aggregate_id,aggregate_version,payload_json FROM outbox_events) o ON o.command_id=r.command_id AND o.event_type='people.lifecycle.cancel' AND o.aggregate_id=c.id AND o.aggregate_version=c.version WHERE r.operation='people.lifecycle.cancel' AND r.record_id=c.id AND r.authority_json::jsonb->>'companyId'=c.company_id AND r.result_json::jsonb->>'id'=c.id AND r.result_json::jsonb->>'status'='cancelled'))) terminal WHERE terminal.id=c.id AND terminal.company_id=c.company_id)) THEN RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='PROJECT_WORKER_SOURCE_CHANGED',DETAIL='Terminal placement requires the original native worker or cancellation evidence.'; END IF;
 PERFORM people_assert_project_worker_starter(source.person_id);
END $$;
CREATE FUNCTION people_check_project_worker_source() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE target text;affected record;
BEGIN
 IF TG_TABLE_NAME='people_project_worker_starters' THEN
  target=NEW.person_id;
  IF NOT EXISTS(SELECT 1 FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id JOIN command_receipts r ON r.command_id=NEW.command_id JOIN assignments author ON author.id=r.authority_json::jsonb->>'assignmentId' AND author.user_id=r.actor_id AND author.company_id=p.company_id
   WHERE p.id=NEW.person_id AND p.company_id=NEW.company_id AND e.id=NEW.engagement_id AND p.user_id=NEW.account_id AND p.version::text=NEW.proof_json->>'resultVersion' AND p.merged_into_id IS NULL AND p.status='active' AND e.worker_type='employee' AND e.status IN ('onboarding','active') AND author.scope_kind='holding' AND iam_assignment_active(author.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(author.user_id,author.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(author.role,author.company_id,'people.manage') AND iam_role_allows(author.role,author.company_id,'organization.manage') AND EXISTS(SELECT 1 FROM assignments member WHERE member.user_id=NEW.account_id AND member.company_id=p.company_id AND iam_assignment_active(member.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(member.user_id,member.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint))) THEN RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='PROJECT_WORKER_SOURCE_CHANGED',DETAIL='Preparation/link must capture one current existing Company account using original HR and Organization authority.'; END IF;
 ELSIF TG_TABLE_NAME='people_project_worker_placements' THEN
  PERFORM people_assert_project_worker_placement(NEW.case_id);RETURN NULL;
 ELSIF TG_TABLE_NAME='hr_people' THEN IF TG_OP<>'INSERT' THEN PERFORM people_assert_project_worker_starter(OLD.id);END IF;target=CASE WHEN TG_OP='DELETE' THEN OLD.id ELSE NEW.id END;
 ELSIF TG_TABLE_NAME='hr_engagements' THEN IF TG_OP<>'INSERT' THEN PERFORM people_assert_project_worker_starter(OLD.person_id);END IF;target=CASE WHEN TG_OP='DELETE' THEN OLD.person_id ELSE NEW.person_id END;
 ELSIF TG_TABLE_NAME='people_lifecycle_cases' THEN PERFORM people_assert_project_worker_placement(CASE WHEN TG_OP='DELETE' THEN OLD.id ELSE NEW.id END);RETURN NULL;
 ELSIF TG_NARGS=1 AND TG_ARGV[0]='receipt' THEN
  IF TG_OP<>'INSERT' THEN
   IF jsonb_exists(OLD.result_json::jsonb,'_projectWorkerStartup') THEN PERFORM people_assert_project_worker_starter(OLD.record_id);FOR affected IN SELECT case_id FROM people_project_worker_placements WHERE starter_command_id=OLD.command_id LOOP PERFORM people_assert_project_worker_placement(affected.case_id);END LOOP;END IF;
   IF jsonb_exists(OLD.result_json::jsonb,'_projectWorkerPlacement') THEN PERFORM people_assert_project_worker_placement(OLD.record_id);END IF;
  END IF;
  IF TG_OP<>'DELETE' THEN
   IF jsonb_exists(NEW.result_json::jsonb,'_projectWorkerStartup') THEN target=NEW.record_id;END IF;
   IF jsonb_exists(NEW.result_json::jsonb,'_projectWorkerPlacement') THEN PERFORM people_assert_project_worker_placement(NEW.record_id);END IF;
  END IF;
 END IF;
 IF target IS NOT NULL THEN PERFORM people_assert_project_worker_starter(target);END IF;RETURN NULL;
END $$;
CREATE CONSTRAINT TRIGGER people_project_worker_starter_source AFTER INSERT ON people_project_worker_starters DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION people_check_project_worker_source();
CREATE CONSTRAINT TRIGGER people_project_worker_placement_source AFTER INSERT ON people_project_worker_placements DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION people_check_project_worker_source();
CREATE CONSTRAINT TRIGGER people_project_worker_person_source AFTER UPDATE OR DELETE ON hr_people DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION people_check_project_worker_source();
CREATE CONSTRAINT TRIGGER people_project_worker_engagement_source AFTER UPDATE OR DELETE ON hr_engagements DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION people_check_project_worker_source();
CREATE CONSTRAINT TRIGGER people_project_worker_case_source AFTER UPDATE OR DELETE ON people_lifecycle_cases DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION people_check_project_worker_source();
CREATE CONSTRAINT TRIGGER people_project_worker_receipt_source AFTER INSERT OR UPDATE OR DELETE ON command_receipts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION people_check_project_worker_source('receipt');
DO $$ DECLARE target record;BEGIN FOR target IN SELECT record_id FROM command_receipts WHERE jsonb_exists(result_json::jsonb,'_projectWorkerStartup') LOOP PERFORM people_assert_project_worker_starter(target.record_id);END LOOP;FOR target IN SELECT record_id FROM command_receipts WHERE jsonb_exists(result_json::jsonb,'_projectWorkerPlacement') LOOP PERFORM people_assert_project_worker_placement(target.record_id);END LOOP;END $$;
