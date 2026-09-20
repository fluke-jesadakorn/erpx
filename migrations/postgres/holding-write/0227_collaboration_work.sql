CREATE TABLE collaboration_record_relations (
 id TEXT PRIMARY KEY,company_id TEXT NOT NULL,branch_id TEXT,department_id TEXT,
 source_type TEXT NOT NULL CHECK(source_type IN ('commerce_order','support_ticket','field_service_order','staff_task')),
 source_id TEXT NOT NULL,source_version BIGINT NOT NULL,target_type TEXT NOT NULL CHECK(target_type IN ('commerce_order','support_ticket','field_service_order','staff_task')),
 target_id TEXT NOT NULL,target_version BIGINT NOT NULL,relation TEXT NOT NULL CHECK(relation IN ('related','blocks','duplicates','depends_on')),
 status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active','removed')),reason TEXT NOT NULL,created_by TEXT NOT NULL,removed_by TEXT,removed_at BIGINT,
 version BIGINT NOT NULL DEFAULT 1,created_at BIGINT NOT NULL,updated_at BIGINT NOT NULL,CHECK(source_type<>target_type OR source_id<>target_id),
 UNIQUE(company_id,source_type,source_id,target_type,target_id,relation)
);
CREATE INDEX collaboration_relation_source ON collaboration_record_relations(company_id,source_type,source_id,status);
CREATE INDEX collaboration_relation_target ON collaboration_record_relations(company_id,target_type,target_id,status);
CREATE TABLE collaboration_relation_events (
 id TEXT PRIMARY KEY,relation_id TEXT NOT NULL REFERENCES collaboration_record_relations(id),version BIGINT NOT NULL,action TEXT NOT NULL,
 actor_id TEXT NOT NULL,reason TEXT NOT NULL,status TEXT NOT NULL,created_at BIGINT NOT NULL,UNIQUE(relation_id,version)
);
CREATE TABLE collaboration_action_cards (
 id TEXT PRIMARY KEY,company_id TEXT NOT NULL,branch_id TEXT,department_id TEXT,
 source_type TEXT NOT NULL CHECK(source_type IN ('commerce_order','support_ticket','field_service_order','staff_task')),source_id TEXT NOT NULL,source_version BIGINT NOT NULL,
 title TEXT NOT NULL,owner_id TEXT NOT NULL,due_at BIGINT NOT NULL,status TEXT NOT NULL DEFAULT 'open' CHECK(status IN ('open','completed','cancelled')),
 completion_evidence TEXT,created_by TEXT NOT NULL,version BIGINT NOT NULL DEFAULT 1,created_at BIGINT NOT NULL,updated_at BIGINT NOT NULL,
 CHECK(status<>'completed' OR completion_evidence IS NOT NULL)
);
CREATE INDEX collaboration_action_queue ON collaboration_action_cards(company_id,owner_id,status,due_at);
CREATE TABLE collaboration_action_events (
 id TEXT PRIMARY KEY,card_id TEXT NOT NULL REFERENCES collaboration_action_cards(id),version BIGINT NOT NULL,action TEXT NOT NULL,actor_id TEXT NOT NULL,
 reason TEXT NOT NULL,status TEXT NOT NULL,owner_id TEXT NOT NULL,due_at BIGINT NOT NULL,created_at BIGINT NOT NULL,UNIQUE(card_id,version)
);
CREATE FUNCTION collaboration_work_guard() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
 IF TG_TABLE_NAME IN ('collaboration_relation_events','collaboration_action_events') OR TG_OP='DELETE' THEN RAISE EXCEPTION 'COLLABORATION_HISTORY_IMMUTABLE' USING ERRCODE='23514'; END IF;
 IF TG_TABLE_NAME='collaboration_record_relations' THEN
  IF (NEW.id,NEW.company_id,NEW.source_type,NEW.source_id,NEW.source_version,NEW.target_type,NEW.target_id,NEW.target_version,NEW.relation,NEW.created_by,NEW.created_at) IS DISTINCT FROM (OLD.id,OLD.company_id,OLD.source_type,OLD.source_id,OLD.source_version,OLD.target_type,OLD.target_id,OLD.target_version,OLD.relation,OLD.created_by,OLD.created_at) THEN RAISE EXCEPTION 'COLLABORATION_RELATION_SOURCE_IMMUTABLE' USING ERRCODE='23514'; END IF;
 ELSIF TG_TABLE_NAME='collaboration_action_cards' THEN
  IF (NEW.id,NEW.company_id,NEW.branch_id,NEW.department_id,NEW.source_type,NEW.source_id,NEW.source_version,NEW.created_by,NEW.created_at) IS DISTINCT FROM (OLD.id,OLD.company_id,OLD.branch_id,OLD.department_id,OLD.source_type,OLD.source_id,OLD.source_version,OLD.created_by,OLD.created_at) THEN RAISE EXCEPTION 'COLLABORATION_ACTION_SOURCE_IMMUTABLE' USING ERRCODE='23514'; END IF;
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER collaboration_relation_guard BEFORE UPDATE OR DELETE ON collaboration_record_relations FOR EACH ROW EXECUTE FUNCTION collaboration_work_guard();
CREATE TRIGGER collaboration_relation_event_guard BEFORE UPDATE OR DELETE ON collaboration_relation_events FOR EACH ROW EXECUTE FUNCTION collaboration_work_guard();
CREATE TRIGGER collaboration_action_guard BEFORE UPDATE OR DELETE ON collaboration_action_cards FOR EACH ROW EXECUTE FUNCTION collaboration_work_guard();
CREATE TRIGGER collaboration_action_event_guard BEFORE UPDATE OR DELETE ON collaboration_action_events FOR EACH ROW EXECUTE FUNCTION collaboration_work_guard();
DO $$ DECLARE t TEXT; BEGIN FOREACH t IN ARRAY ARRAY['collaboration_record_relations','collaboration_action_cards'] LOOP INSERT INTO work_cache_revisions(table_name) VALUES(t) ON CONFLICT DO NOTHING;EXECUTE format('CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION work_cache_changed()',t);EXECUTE format('CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON %I FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed()',t);END LOOP;END $$;
