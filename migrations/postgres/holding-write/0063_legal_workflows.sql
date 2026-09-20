-- Typed subordinate workflows. Each table belongs to its domain aggregate.
DO $$ DECLARE t text; BEGIN
 FOREACH t IN ARRAY ARRAY['legal_case_events','legal_compliance_actions','legal_privacy_cases','legal_corporate_actions','legal_ip_actions','legal_counsel_entries','legal_obligation_schedule','legal_custody_events','legal_policy_acknowledgements','legal_intake_questions'] LOOP
 EXECUTE format('CREATE TABLE %I(id text PRIMARY KEY,item_id text NOT NULL REFERENCES legal_items(id),category text NOT NULL,title text NOT NULL,owner_id text NOT NULL,due_date text,state text NOT NULL,version integer NOT NULL DEFAULT 1,data jsonb NOT NULL,created_by text NOT NULL,updated_by text NOT NULL,updated_at bigint NOT NULL)',t);
 EXECUTE format('CREATE INDEX ON %I(item_id,due_date)',t);
 INSERT INTO work_cache_revisions(table_name) VALUES(t);
 EXECUTE format('CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION work_cache_changed()',t);
 EXECUTE format('CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON %I FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed()',t);
 END LOOP;
END $$;
CREATE TABLE legal_authorities(id text PRIMARY KEY,item_id text NOT NULL REFERENCES legal_items(id),company_id text NOT NULL,user_id text NOT NULL,document_id text NOT NULL REFERENCES legal_documents(id),currency text NOT NULL,limit_minor bigint NOT NULL CHECK(limit_minor>=0),valid_from text NOT NULL,valid_until text NOT NULL,state text NOT NULL CHECK(state IN ('proposed','approved','revoked')),created_by text NOT NULL,approved_by text,version integer NOT NULL DEFAULT 1);
CREATE TABLE legal_intake_forms(id text PRIMARY KEY,company_id text NOT NULL,name text NOT NULL,fields jsonb NOT NULL,sla_days integer NOT NULL CHECK(sla_days BETWEEN 1 AND 365),state text NOT NULL DEFAULT 'draft',created_by text NOT NULL,approved_by text,version integer NOT NULL DEFAULT 1);
ALTER TABLE legal_settings ADD COLUMN reminder_days jsonb NOT NULL DEFAULT '[90,60,30,7,1]';
ALTER TABLE legal_items ADD COLUMN escalation_owner text;
ALTER TABLE legal_items ADD COLUMN sla_started_at bigint;
ALTER TABLE legal_items ADD COLUMN sla_due_at bigint;
ALTER TABLE legal_items ADD COLUMN intake_form_id text REFERENCES legal_intake_forms(id);
ALTER TABLE legal_items ADD COLUMN intake_answers jsonb NOT NULL DEFAULT '{}';
ALTER TABLE approval_instances DROP CONSTRAINT approval_instances_record_type_check;
ALTER TABLE approval_instances ADD CONSTRAINT approval_instances_record_type_check CHECK(record_type IN ('expense','leave','leave_cancellation','task','overtime','legal'));
CREATE TABLE legal_approval_routes(item_id text PRIMARY KEY REFERENCES legal_items(id),instance_id text NOT NULL REFERENCES approval_instances(id),document_id text NOT NULL REFERENCES legal_documents(id),document_hash text NOT NULL,signers jsonb NOT NULL,source_version integer NOT NULL,created_by text NOT NULL);
CREATE TABLE legal_hold_notices(hold_id text NOT NULL REFERENCES legal_holds(id),item_id text NOT NULL REFERENCES legal_items(id),user_id text NOT NULL,notified_at bigint NOT NULL,acknowledged_at bigint,statement text,PRIMARY KEY(hold_id,item_id,user_id));
DO $$ DECLARE t text; BEGIN
 FOREACH t IN ARRAY ARRAY['legal_authorities','legal_intake_forms','legal_approval_routes','legal_hold_notices'] LOOP
 INSERT INTO work_cache_revisions(table_name) VALUES(t);
 EXECUTE format('CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION work_cache_changed()',t);
 EXECUTE format('CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON %I FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed()',t);
 END LOOP;
END $$;
CREATE TABLE legal_feature_history(id text PRIMARY KEY,item_id text NOT NULL REFERENCES legal_items(id),feature_id text NOT NULL,category text NOT NULL,version integer NOT NULL,snapshot jsonb NOT NULL,actor_id text NOT NULL,reason text NOT NULL,created_at bigint NOT NULL);
CREATE TRIGGER legal_feature_history_immutable BEFORE UPDATE OR DELETE ON legal_feature_history FOR EACH ROW EXECUTE FUNCTION legal_evidence_immutable();
INSERT INTO work_cache_revisions(table_name) VALUES('legal_feature_history');
CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON legal_feature_history FOR EACH ROW EXECUTE FUNCTION work_cache_changed();
