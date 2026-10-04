CREATE TABLE operations_queue_policies (
 id TEXT PRIMARY KEY,company_id TEXT NOT NULL,area TEXT NOT NULL,name TEXT NOT NULL,revision BIGINT NOT NULL,
 source_kinds_json TEXT NOT NULL CHECK(erpx_json_valid(source_kinds_json)),target_minutes BIGINT NOT NULL CHECK(target_minutes BETWEEN 1 AND 10080),
 status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','published','rejected','retired')),
 created_by TEXT NOT NULL,reviewed_by TEXT,version BIGINT NOT NULL DEFAULT 1,created_at BIGINT NOT NULL,updated_at BIGINT NOT NULL,
 UNIQUE(company_id,area,revision),CHECK(status<>'published' OR reviewed_by IS NOT NULL AND reviewed_by<>created_by)
);
CREATE UNIQUE INDEX operations_current_policy ON operations_queue_policies(company_id,area) WHERE status='published';
CREATE TABLE operations_exceptions (
 id TEXT PRIMARY KEY,company_id TEXT NOT NULL,branch_id TEXT,department_id TEXT,area TEXT NOT NULL,
 source_kind TEXT NOT NULL CHECK(source_kind IN ('commerce_order','support_ticket','field_service_order','staff_task','pos_sale','pms_record','asset')),
 source_id TEXT NOT NULL,source_version BIGINT NOT NULL,source_reference_json TEXT NOT NULL CHECK(erpx_json_valid(source_reference_json)),
 policy_id TEXT NOT NULL REFERENCES operations_queue_policies(id),policy_snapshot_json TEXT NOT NULL CHECK(erpx_json_valid(policy_snapshot_json)),
 title TEXT NOT NULL,issue TEXT NOT NULL,priority TEXT NOT NULL CHECK(priority IN ('normal','high','critical')),owner_id TEXT NOT NULL,
 status TEXT NOT NULL DEFAULT 'open' CHECK(status IN ('open','acknowledged','in_progress','escalated','awaiting_review','resolved','closed','cancelled')),
 due_at BIGINT NOT NULL,escalation_owner_id TEXT,resolution TEXT,resolved_by TEXT,reviewed_by TEXT,
 version BIGINT NOT NULL DEFAULT 1,created_by TEXT NOT NULL,created_at BIGINT NOT NULL,updated_at BIGINT NOT NULL,
 CHECK(status NOT IN ('awaiting_review','resolved','closed') OR resolution IS NOT NULL AND resolved_by IS NOT NULL),
 CHECK(status NOT IN ('resolved','closed') OR reviewed_by IS NOT NULL AND reviewed_by<>resolved_by)
);
CREATE INDEX operations_exception_queue ON operations_exceptions(company_id,branch_id,department_id,status,priority,due_at);
CREATE TABLE operations_exception_events (
 id TEXT PRIMARY KEY,exception_id TEXT NOT NULL REFERENCES operations_exceptions(id),version BIGINT NOT NULL,
 action TEXT NOT NULL,actor_id TEXT NOT NULL,reason TEXT NOT NULL,status TEXT NOT NULL,details_json TEXT NOT NULL CHECK(erpx_json_valid(details_json)),
 created_at BIGINT NOT NULL,UNIQUE(exception_id,version)
);
CREATE FUNCTION operations_evidence_guard() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
 IF TG_OP='DELETE' OR TG_TABLE_NAME='operations_exception_events' THEN RAISE EXCEPTION 'OPERATIONS_EVIDENCE_IMMUTABLE' USING ERRCODE='23514'; END IF;
 IF TG_TABLE_NAME='operations_exceptions' THEN
  IF (NEW.id,NEW.company_id,NEW.branch_id,NEW.department_id,NEW.area,NEW.source_kind,NEW.source_id,NEW.source_version,NEW.source_reference_json,NEW.policy_id,NEW.policy_snapshot_json,NEW.created_by,NEW.created_at)
   IS DISTINCT FROM (OLD.id,OLD.company_id,OLD.branch_id,OLD.department_id,OLD.area,OLD.source_kind,OLD.source_id,OLD.source_version,OLD.source_reference_json,OLD.policy_id,OLD.policy_snapshot_json,OLD.created_by,OLD.created_at)
  THEN RAISE EXCEPTION 'OPERATIONS_SOURCE_IMMUTABLE' USING ERRCODE='23514'; END IF;
 ELSE
  IF (NEW.id,NEW.company_id,NEW.area,NEW.name,NEW.revision,NEW.source_kinds_json,NEW.target_minutes,NEW.created_by,NEW.created_at)
   IS DISTINCT FROM (OLD.id,OLD.company_id,OLD.area,OLD.name,OLD.revision,OLD.source_kinds_json,OLD.target_minutes,OLD.created_by,OLD.created_at)
  THEN RAISE EXCEPTION 'OPERATIONS_POLICY_IMMUTABLE' USING ERRCODE='23514'; END IF;
 END IF;RETURN NEW;
END $$;
DO $$ DECLARE t TEXT; BEGIN
 FOREACH t IN ARRAY ARRAY['operations_queue_policies','operations_exceptions','operations_exception_events'] LOOP
  EXECUTE format('CREATE TRIGGER operations_evidence_guard BEFORE UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION operations_evidence_guard()',t);
  INSERT INTO work_cache_revisions(table_name) VALUES(t) ON CONFLICT DO NOTHING;
  EXECUTE format('CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION work_cache_changed()',t);
  EXECUTE format('CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON %I FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed()',t);
 END LOOP;
END $$;
DO $$ DECLARE app_role TEXT; jobs_role TEXT; runtime_role TEXT; BEGIN
 IF current_user !~ '_migration$' THEN RETURN; END IF;
 app_role:=regexp_replace(current_user,'_migration$','_app');jobs_role:=regexp_replace(current_user,'_migration$','_jobs');
 FOREACH runtime_role IN ARRAY ARRAY[app_role,jobs_role] LOOP
  IF EXISTS(SELECT 1 FROM pg_roles WHERE rolname=runtime_role) THEN EXECUTE format('GRANT SELECT ON operations_queue_policies,operations_exceptions,operations_exception_events TO %I',runtime_role);END IF;
 END LOOP;
 IF EXISTS(SELECT 1 FROM pg_roles WHERE rolname=app_role) THEN
  EXECUTE format('GRANT INSERT,UPDATE ON operations_queue_policies,operations_exceptions TO %I',app_role);
  EXECUTE format('GRANT INSERT ON operations_exception_events TO %I',app_role);
 END IF;
END $$;
