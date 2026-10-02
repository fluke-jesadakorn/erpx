-- Corrections append replacements. Previously reviewed entries stay unchanged.
CREATE TABLE finance_vat_corrections (
  id TEXT PRIMARY KEY, company_id TEXT NOT NULL,
  chart_id TEXT NOT NULL REFERENCES finance_charts(id),
  root_entry_id TEXT NOT NULL REFERENCES finance_tax_entries(id),
  family_source_id TEXT NOT NULL,
  reason TEXT NOT NULL, family_evidence_json TEXT NOT NULL CHECK(erpx_json_valid(family_evidence_json)),
  status TEXT NOT NULL DEFAULT 'prepared' CHECK(status IN ('prepared','approved','rejected')),
  version BIGINT NOT NULL DEFAULT 1, prepared_by TEXT NOT NULL,
  reviewed_by TEXT, review_reason TEXT, created_at BIGINT NOT NULL, reviewed_at BIGINT,
  CHECK(reviewed_by IS NULL OR reviewed_by<>prepared_by)
);
CREATE UNIQUE INDEX finance_vat_correction_pending_family
  ON finance_vat_corrections(company_id,family_source_id) WHERE status='prepared';
ALTER TABLE finance_tax_entries
  ADD COLUMN replaces_entry_id TEXT REFERENCES finance_tax_entries(id),
  ADD COLUMN correction_id TEXT REFERENCES finance_vat_corrections(id),
  ADD CONSTRAINT finance_tax_entry_replacement_pair CHECK((replaces_entry_id IS NULL)=(correction_id IS NULL));
DROP INDEX finance_tax_entries_active_source;
CREATE UNIQUE INDEX finance_tax_entries_active_source
  ON finance_tax_entries(company_id,source_type,source_id) WHERE status<>'rejected' AND correction_id IS NULL;
CREATE UNIQUE INDEX finance_tax_entry_replacement_successor
  ON finance_tax_entries(replaces_entry_id) WHERE status<>'rejected' AND replaces_entry_id IS NOT NULL;
CREATE INDEX finance_tax_entries_correction ON finance_tax_entries(correction_id);
CREATE VIEW finance_current_tax_entries WITH (security_invoker=true) AS
  SELECT t.* FROM finance_tax_entries t
  WHERE (t.status='prepared' AND t.correction_id IS NULL)
     OR (t.status='approved' AND NOT EXISTS(
       SELECT 1 FROM finance_tax_entries n WHERE n.replaces_entry_id=t.id AND n.status='approved'));

CREATE OR REPLACE FUNCTION protect_reviewed_tax_entry() RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
  IF TG_OP='DELETE' OR OLD.status<>'prepared' THEN
    RAISE EXCEPTION 'REVIEWED_TAX_ENTRY_IMMUTABLE' USING ERRCODE='23514';
  END IF;
  IF (to_jsonb(NEW)-ARRAY['status','version','reviewed_by','review_reason','reviewed_at'])
    IS DISTINCT FROM (to_jsonb(OLD)-ARRAY['status','version','reviewed_by','review_reason','reviewed_at']) THEN
    RAISE EXCEPTION 'TAX_ENTRY_EVIDENCE_IMMUTABLE' USING ERRCODE='23514';
  END IF;
  RETURN NEW;
END;
$$;
CREATE FUNCTION protect_vat_correction() RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
  IF TG_OP='DELETE' OR OLD.status<>'prepared' OR
    (to_jsonb(NEW)-ARRAY['status','version','reviewed_by','review_reason','reviewed_at'])
      IS DISTINCT FROM (to_jsonb(OLD)-ARRAY['status','version','reviewed_by','review_reason','reviewed_at']) THEN
    RAISE EXCEPTION 'VAT_CORRECTION_EVIDENCE_IMMUTABLE' USING ERRCODE='23514';
  END IF;
  RETURN NEW;
END;
$$;
CREATE TRIGGER finance_vat_correction_immutable BEFORE UPDATE OR DELETE ON finance_vat_corrections
  FOR EACH ROW EXECUTE FUNCTION protect_vat_correction();

CREATE FUNCTION validate_vat_replacement() RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE previous finance_tax_entries%ROWTYPE; correction finance_vat_corrections%ROWTYPE;
BEGIN
  IF NEW.correction_id IS NULL THEN RETURN NEW; END IF;
  SELECT * INTO previous FROM finance_tax_entries WHERE id=NEW.replaces_entry_id;
  SELECT * INTO correction FROM finance_vat_corrections WHERE id=NEW.correction_id;
  IF previous.status IS DISTINCT FROM 'approved' OR correction.status IS DISTINCT FROM 'prepared' OR
    (NEW.company_id,NEW.chart_id,NEW.financial_event_id,NEW.tax_account_id,NEW.source_type,NEW.source_id,NEW.direction,NEW.document_date,NEW.accounting_date,NEW.taxable_minor,NEW.tax_minor)
      IS DISTINCT FROM (previous.company_id,previous.chart_id,previous.financial_event_id,previous.tax_account_id,previous.source_type,previous.source_id,previous.direction,previous.document_date,previous.accounting_date,previous.taxable_minor,previous.tax_minor) OR
    (NEW.company_id,NEW.chart_id,NEW.prepared_by) IS DISTINCT FROM (correction.company_id,correction.chart_id,correction.prepared_by) OR
    (NEW.source_type='invoice' AND NEW.source_id<>correction.family_source_id) OR
    (NEW.source_type='commerce_return' AND NOT EXISTS(SELECT 1 FROM commerce_returns WHERE id=NEW.source_id AND company_id=NEW.company_id AND invoice_id=correction.family_source_id)) OR
    EXISTS(SELECT 1 FROM finance_tax_entries successor WHERE successor.replaces_entry_id=previous.id AND successor.status='approved') THEN
    RAISE EXCEPTION 'VAT_REPLACEMENT_SOURCE_INVALID' USING ERRCODE='23514';
  END IF;
  RETURN NEW;
END;
$$;
CREATE TRIGGER finance_vat_replacement_valid BEFORE INSERT ON finance_tax_entries
  FOR EACH ROW EXECUTE FUNCTION validate_vat_replacement();
CREATE FUNCTION validate_vat_correction_atomicity() RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE correction_key TEXT; correction_status TEXT;
BEGIN
  IF TG_TABLE_NAME='finance_tax_entries' THEN correction_key:=NEW.correction_id;
  ELSE correction_key:=NEW.id; END IF;
  IF correction_key IS NULL THEN RETURN NEW; END IF;
  SELECT status INTO correction_status FROM finance_vat_corrections WHERE id=correction_key;
  IF NOT EXISTS(SELECT 1 FROM finance_tax_entries WHERE correction_id=correction_key) OR
    EXISTS(SELECT 1 FROM finance_tax_entries WHERE correction_id=correction_key AND status<>correction_status) THEN
    RAISE EXCEPTION 'VAT_CORRECTION_NOT_ATOMIC' USING ERRCODE='23514';
  END IF;
  RETURN NEW;
END;
$$;
CREATE CONSTRAINT TRIGGER finance_vat_correction_atomic AFTER INSERT OR UPDATE ON finance_vat_corrections
  DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION validate_vat_correction_atomicity();
CREATE CONSTRAINT TRIGGER finance_tax_replacement_atomic AFTER INSERT OR UPDATE ON finance_tax_entries
  DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION validate_vat_correction_atomicity();
DO $$ DECLARE app_role TEXT:=regexp_replace(current_user,'_migration$','_app'); BEGIN
  INSERT INTO work_cache_revisions(table_name) VALUES('finance_vat_corrections') ON CONFLICT DO NOTHING;
  CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON finance_vat_corrections FOR EACH ROW EXECUTE FUNCTION work_cache_changed();
  CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON finance_vat_corrections FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed();
  IF EXISTS(SELECT 1 FROM pg_roles WHERE rolname=app_role) THEN
    EXECUTE format('GRANT SELECT, INSERT, UPDATE ON finance_vat_corrections TO %I',app_role);
    EXECUTE format('GRANT SELECT ON finance_current_tax_entries TO %I',app_role);
  END IF;
END $$;
