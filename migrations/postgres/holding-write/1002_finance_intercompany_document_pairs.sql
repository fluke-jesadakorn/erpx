CREATE TABLE finance_intercompany_document_pairs (
 id TEXT PRIMARY KEY,holding_id TEXT NOT NULL,company_id TEXT NOT NULL,reference TEXT NOT NULL,
 source_company_id TEXT NOT NULL,source_journal_id TEXT NOT NULL REFERENCES finance_journals(id),source_account_id TEXT NOT NULL REFERENCES finance_accounts(id),
 target_company_id TEXT NOT NULL,target_journal_id TEXT NOT NULL REFERENCES finance_journals(id),target_account_id TEXT NOT NULL REFERENCES finance_accounts(id),
 currency TEXT NOT NULL,amount_minor BIGINT NOT NULL CHECK(amount_minor>0),status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','confirmed','rejected','reversed')),version BIGINT NOT NULL DEFAULT 1,
 source_json TEXT NOT NULL CHECK(erpx_json_valid(source_json)),source_hash TEXT NOT NULL,prepared_by TEXT NOT NULL,reviewed_by TEXT,reason TEXT NOT NULL,created_at BIGINT NOT NULL,updated_at BIGINT NOT NULL,
 CHECK(source_company_id<>target_company_id),CHECK(reviewed_by IS NULL OR reviewed_by<>prepared_by)
);
CREATE TABLE finance_intercompany_pair_claims (
 pair_id TEXT NOT NULL REFERENCES finance_intercompany_document_pairs(id),journal_id TEXT NOT NULL REFERENCES finance_journals(id),account_id TEXT NOT NULL REFERENCES finance_accounts(id),
 PRIMARY KEY(journal_id,account_id)
);
CREATE FUNCTION protect_finance_intercompany_pair() RETURNS TRIGGER LANGUAGE plpgsql AS $$ BEGIN
 IF TG_OP='DELETE' OR (NEW.id,NEW.holding_id,NEW.company_id,NEW.reference,NEW.source_company_id,NEW.source_journal_id,NEW.source_account_id,NEW.target_company_id,NEW.target_journal_id,NEW.target_account_id,NEW.currency,NEW.amount_minor,NEW.source_json,NEW.source_hash,NEW.prepared_by,NEW.reason,NEW.created_at)
 IS DISTINCT FROM (OLD.id,OLD.holding_id,OLD.company_id,OLD.reference,OLD.source_company_id,OLD.source_journal_id,OLD.source_account_id,OLD.target_company_id,OLD.target_journal_id,OLD.target_account_id,OLD.currency,OLD.amount_minor,OLD.source_json,OLD.source_hash,OLD.prepared_by,OLD.reason,OLD.created_at) THEN RAISE EXCEPTION 'INTERCOMPANY_PAIR_IMMUTABLE' USING ERRCODE='23514';END IF;RETURN NEW;END;$$;
CREATE TRIGGER finance_intercompany_pair_immutable BEFORE UPDATE OR DELETE ON finance_intercompany_document_pairs FOR EACH ROW EXECUTE FUNCTION protect_finance_intercompany_pair();
DO $$ DECLARE t TEXT;app_role TEXT:=regexp_replace(current_user,'_migration$','_app');BEGIN FOREACH t IN ARRAY ARRAY['finance_intercompany_document_pairs','finance_intercompany_pair_claims'] LOOP
 INSERT INTO work_cache_revisions(table_name) VALUES(t) ON CONFLICT DO NOTHING;
 EXECUTE format('CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION work_cache_changed()',t);
 EXECUTE format('CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON %I FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed()',t);
 IF right(current_user,10)='_migration' AND EXISTS(SELECT 1 FROM pg_roles WHERE rolname=app_role) THEN EXECUTE format('GRANT SELECT,INSERT,UPDATE,DELETE ON %I TO %I',t,app_role);END IF;
 END LOOP;END;$$;
