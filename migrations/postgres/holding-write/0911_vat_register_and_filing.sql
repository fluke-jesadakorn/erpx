-- VAT facts retain their document and journal authority. No statutory amount is
-- accepted from a calculator or a filing form.
CREATE TABLE finance_tax_entries (
  id TEXT PRIMARY KEY, company_id TEXT NOT NULL,
  chart_id TEXT NOT NULL REFERENCES finance_charts(id),
  financial_event_id TEXT NOT NULL REFERENCES finance_financial_events(id),
  tax_account_id TEXT NOT NULL REFERENCES finance_accounts(id),
  source_type TEXT NOT NULL CHECK(source_type IN ('invoice','commerce_return')),
  source_id TEXT NOT NULL, source_version BIGINT NOT NULL,
  original_entry_id TEXT REFERENCES finance_tax_entries(id),
  direction TEXT NOT NULL CHECK(direction IN ('output','input')),
  document_date TEXT NOT NULL, accounting_date TEXT NOT NULL,
  tax_point_date TEXT NOT NULL, claim_month TEXT NOT NULL,
  document_reference TEXT NOT NULL,
  treatment TEXT NOT NULL CHECK(treatment IN ('standard','zero','exempt','disallowed','allocated')),
  deductible_bps BIGINT NOT NULL CHECK(deductible_bps BETWEEN 0 AND 10000),
  taxable_minor BIGINT NOT NULL, tax_minor BIGINT NOT NULL,
  deductible_minor BIGINT NOT NULL,
  source_hash TEXT NOT NULL,
  evidence_json TEXT NOT NULL CHECK(erpx_json_valid(evidence_json)),
  status TEXT NOT NULL DEFAULT 'prepared' CHECK(status IN ('prepared','approved','rejected')),
  version BIGINT NOT NULL DEFAULT 1, prepared_by TEXT NOT NULL,
  reviewed_by TEXT, review_reason TEXT, created_at BIGINT NOT NULL, reviewed_at BIGINT,
  CHECK(reviewed_by IS NULL OR reviewed_by<>prepared_by)
);
CREATE UNIQUE INDEX finance_tax_entries_active_source
  ON finance_tax_entries(company_id,source_type,source_id) WHERE status<>'rejected';
CREATE INDEX finance_tax_entries_register
  ON finance_tax_entries(company_id,chart_id,claim_month,direction,status);

ALTER TABLE finance_statutory_packages
  ADD COLUMN chart_id TEXT REFERENCES finance_charts(id),
  ADD COLUMN filing_month TEXT,
  ADD COLUMN source_hash TEXT,
  ADD COLUMN snapshot_json TEXT CHECK(snapshot_json IS NULL OR erpx_json_valid(snapshot_json)),
  ADD COLUMN original_package_id TEXT REFERENCES finance_statutory_packages(id),
  ADD COLUMN review_reason TEXT,
  ADD COLUMN export_content TEXT,
  ADD COLUMN payment_id TEXT REFERENCES finance_payments(id);
CREATE UNIQUE INDEX finance_vat_package_original
  ON finance_statutory_packages(company_id,chart_id,filing_month,form_code)
  WHERE filing_month IS NOT NULL AND filing_kind='original' AND status<>'superseded';
CREATE TABLE finance_statutory_package_events (
  id TEXT PRIMARY KEY, package_id TEXT NOT NULL REFERENCES finance_statutory_packages(id),
  command_id TEXT NOT NULL REFERENCES command_receipts(command_id),
  version BIGINT NOT NULL, action TEXT NOT NULL, actor_id TEXT NOT NULL,
  evidence_json TEXT NOT NULL CHECK(erpx_json_valid(evidence_json)), created_at BIGINT NOT NULL,
  UNIQUE(package_id,version)
);
CREATE FUNCTION protect_reviewed_tax_entry() RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
  IF OLD.status<>'prepared' THEN RAISE EXCEPTION 'REVIEWED_TAX_ENTRY_IMMUTABLE' USING ERRCODE='23514'; END IF;
  IF TG_OP='DELETE' THEN RETURN OLD; END IF; RETURN NEW;
END;
$$;
CREATE TRIGGER finance_tax_entry_immutable BEFORE UPDATE OR DELETE ON finance_tax_entries
  FOR EACH ROW EXECUTE FUNCTION protect_reviewed_tax_entry();
CREATE TRIGGER finance_statutory_package_event_immutable BEFORE UPDATE OR DELETE ON finance_statutory_package_events
  FOR EACH ROW EXECUTE FUNCTION protect_financial_event();
CREATE FUNCTION protect_vat_package_snapshot() RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
  IF OLD.filing_month IS NULL THEN RETURN NEW; END IF;
  IF (NEW.company_id,NEW.chart_id,NEW.form_code,NEW.filing_month,NEW.filing_kind,NEW.source_hash,NEW.snapshot_json,NEW.original_package_id,NEW.calculation_run_id,NEW.prepared_by)
    IS DISTINCT FROM (OLD.company_id,OLD.chart_id,OLD.form_code,OLD.filing_month,OLD.filing_kind,OLD.source_hash,OLD.snapshot_json,OLD.original_package_id,OLD.calculation_run_id,OLD.prepared_by)
    THEN RAISE EXCEPTION 'STATUTORY_SNAPSHOT_IMMUTABLE' USING ERRCODE='23514'; END IF;
  IF OLD.export_hash IS NOT NULL AND (NEW.export_hash,NEW.export_content) IS DISTINCT FROM (OLD.export_hash,OLD.export_content)
    THEN RAISE EXCEPTION 'STATUTORY_EXPORT_IMMUTABLE' USING ERRCODE='23514'; END IF;
  RETURN NEW;
END;
$$;
CREATE TRIGGER finance_vat_package_snapshot_immutable BEFORE UPDATE ON finance_statutory_packages
  FOR EACH ROW EXECUTE FUNCTION protect_vat_package_snapshot();
DO $$ DECLARE t TEXT; app_role TEXT:=replace(current_user,'_migrate','_app'); BEGIN
  FOREACH t IN ARRAY ARRAY['finance_tax_entries','finance_statutory_package_events'] LOOP
    INSERT INTO work_cache_revisions(table_name) VALUES(t) ON CONFLICT DO NOTHING;
    EXECUTE format('CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION work_cache_changed()',t);
    EXECUTE format('CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON %I FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed()',t);
  END LOOP;
  IF EXISTS(SELECT 1 FROM pg_roles WHERE rolname=app_role) THEN
    EXECUTE format('GRANT SELECT, INSERT, UPDATE ON finance_tax_entries TO %I',app_role);
    EXECUTE format('GRANT SELECT, INSERT ON finance_statutory_package_events TO %I',app_role);
  END IF;
END $$;
