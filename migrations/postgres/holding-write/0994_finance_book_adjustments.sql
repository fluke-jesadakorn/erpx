CREATE TABLE finance_book_adjustments (
 id TEXT PRIMARY KEY,company_id TEXT NOT NULL,chart_id TEXT NOT NULL REFERENCES finance_charts(id),
 primary_journal_id TEXT NOT NULL REFERENCES finance_journals(id),source_version BIGINT NOT NULL,
 content_json TEXT NOT NULL CHECK(erpx_json_valid(content_json)),source_hash TEXT NOT NULL,
 status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','posted','rejected','reversed')),
 version BIGINT NOT NULL DEFAULT 1,prepared_by TEXT NOT NULL,reviewed_by TEXT,
 journal_id TEXT REFERENCES finance_journals(id),reversal_journal_id TEXT REFERENCES finance_journals(id),
 reason TEXT NOT NULL,created_at BIGINT NOT NULL,updated_at BIGINT NOT NULL,
 CHECK(reviewed_by IS NULL OR reviewed_by<>prepared_by)
);
CREATE FUNCTION protect_finance_book_adjustment() RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='DELETE' OR OLD.status IN ('rejected','reversed') OR (NEW.id,NEW.company_id,NEW.chart_id,NEW.primary_journal_id,NEW.source_version,NEW.content_json,NEW.source_hash,NEW.prepared_by,NEW.created_at)
 IS DISTINCT FROM (OLD.id,OLD.company_id,OLD.chart_id,OLD.primary_journal_id,OLD.source_version,OLD.content_json,OLD.source_hash,OLD.prepared_by,OLD.created_at) THEN RAISE EXCEPTION 'BOOK_ADJUSTMENT_EVIDENCE_IMMUTABLE' USING ERRCODE='23514';END IF;RETURN NEW;
END;$$;
CREATE TRIGGER finance_book_adjustment_immutable BEFORE UPDATE OR DELETE ON finance_book_adjustments FOR EACH ROW EXECUTE FUNCTION protect_finance_book_adjustment();
-- Exact reversals carry the historical dimension evidence. New required values
-- must not prevent correcting an old posting; arbitrary new lines still validate.
CREATE OR REPLACE FUNCTION validate_finance_dimensions() RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE source finance_journals; original finance_journals; dims JSONB; item RECORD;
BEGIN
 SELECT * INTO source FROM finance_journals WHERE id=NEW.id;
 IF source.status NOT IN ('posted','reversed') OR (TG_OP='UPDATE' AND OLD.status IN ('posted','reversed')) THEN RETURN NULL; END IF;
 IF source.source_type IN ('book_projection','book_opening') THEN RETURN NULL;END IF;
 IF source.source_type IN ('reversal','book_adjustment') THEN
  SELECT * INTO original FROM finance_journals WHERE id=source.source_id AND company_id=source.company_id AND chart_id=source.chart_id;
  IF FOUND AND (source.source_type='reversal' OR original.source_type='book_adjustment') AND original.status IN ('posted','reversed') AND source.currency=original.currency AND source.fx_rate_micros=original.fx_rate_micros AND source.entry_date>=original.entry_date AND
   (SELECT COUNT(*) FROM finance_journal_lines WHERE journal_id=source.id)=(SELECT COUNT(*) FROM finance_journal_lines WHERE journal_id=original.id) AND
   NOT EXISTS((SELECT ordinal,account_id,debit_minor,credit_minor,base_debit_minor,base_credit_minor,dimension_json::jsonb FROM finance_journal_lines WHERE journal_id=source.id) EXCEPT ALL (SELECT ordinal,account_id,credit_minor,debit_minor,base_credit_minor,base_debit_minor,dimension_json::jsonb FROM finance_journal_lines WHERE journal_id=original.id)) THEN RETURN NULL;END IF;
 END IF;
 FOR item IN SELECT dimension_json FROM finance_journal_lines WHERE journal_id=source.id LOOP
  dims:=item.dimension_json::jsonb;
  IF jsonb_typeof(dims)<>'object' THEN RAISE EXCEPTION 'DIMENSION_OBJECT_REQUIRED' USING ERRCODE='23514'; END IF;
  IF EXISTS(SELECT 1 FROM finance_dimension_definitions d WHERE d.company_id=source.company_id AND d.status='active' AND d.required=1 AND NOT dims ? d.code) OR
     EXISTS(SELECT 1 FROM jsonb_each_text(dims) kv JOIN finance_dimension_definitions d ON d.code=kv.key AND d.company_id=source.company_id
        WHERE d.status<>'active' OR NOT EXISTS(SELECT 1 FROM finance_dimension_values v WHERE v.dimension_id=d.id AND v.company_id=source.company_id AND v.code=kv.value AND v.status='active')) THEN
    RAISE EXCEPTION 'DIMENSION_VALUE_REQUIRED' USING ERRCODE='23514';
  END IF;
 END LOOP;
 RETURN NULL;
END; $$;
DO $$ DECLARE app_role TEXT:=regexp_replace(current_user,'_migration$','_app');BEGIN IF right(current_user,10)='_migration' AND EXISTS(SELECT 1 FROM pg_roles WHERE rolname=app_role) THEN EXECUTE format('GRANT SELECT,INSERT,UPDATE ON finance_book_adjustments TO %I',app_role);END IF;END;$$;
INSERT INTO work_cache_revisions(table_name) VALUES('finance_book_adjustments') ON CONFLICT DO NOTHING;
