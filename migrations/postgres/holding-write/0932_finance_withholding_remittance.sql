CREATE TABLE finance_withholding_packages (
 id text PRIMARY KEY,company_id text NOT NULL,month text NOT NULL CHECK(month ~ '^\d{4}-\d{2}$'),form_kind text NOT NULL CHECK(form_kind IN ('PND3','PND53')),
 previous_package_id text REFERENCES finance_withholding_packages(id),edition bigint NOT NULL CHECK(edition>0),
 tax_minor bigint NOT NULL CHECK(tax_minor>0),monthly_tax_minor bigint NOT NULL CHECK(monthly_tax_minor>=tax_minor),chart_id text NOT NULL REFERENCES finance_charts(id),
 source_fingerprint text NOT NULL CHECK(source_fingerprint ~ '^[a-f0-9]{64}$'),snapshot_json text NOT NULL CHECK(erpx_json_valid(snapshot_json)),export_csv text NOT NULL,export_sha256 text NOT NULL CHECK(export_sha256 ~ '^[a-f0-9]{64}$'),
 status text NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','approved','rejected','cancelled','settled','reversed')),
 prepared_by text NOT NULL,reviewed_by text,review_reason text,reviewed_at bigint,reason text NOT NULL,version bigint NOT NULL DEFAULT 1,
 command_id text NOT NULL,created_at bigint NOT NULL,updated_at bigint NOT NULL,
 UNIQUE(company_id,month,form_kind,edition),CHECK(reviewed_by IS NULL OR reviewed_by<>prepared_by)
);
CREATE UNIQUE INDEX finance_withholding_package_pending ON finance_withholding_packages(company_id,month,form_kind) WHERE status IN ('draft','approved');
CREATE TABLE finance_withholding_remittances (
 id text PRIMARY KEY,company_id text NOT NULL,package_id text NOT NULL UNIQUE REFERENCES finance_withholding_packages(id),
 amount_minor bigint NOT NULL CHECK(amount_minor>0),liability_accounts_json text NOT NULL CHECK(erpx_json_valid(liability_accounts_json)),
 bank_account_id text NOT NULL REFERENCES finance_accounts(id),bank_transaction_id text NOT NULL UNIQUE REFERENCES finance_bank_transactions(id),journal_id text NOT NULL UNIQUE REFERENCES finance_journals(id),
 filing_channel text NOT NULL CHECK(filing_channel='manual'),filing_evidence text NOT NULL,bank_reference text NOT NULL,recorded_by text NOT NULL,recorded_at bigint NOT NULL,
 status text NOT NULL DEFAULT 'confirmed' CHECK(status IN ('confirmed','reversed')),reversal_bank_transaction_id text UNIQUE REFERENCES finance_bank_transactions(id),reversal_journal_id text UNIQUE REFERENCES finance_journals(id),reversal_evidence text,reversed_by text,reversed_at bigint,
 version bigint NOT NULL DEFAULT 1,
 CHECK((status='confirmed' AND reversal_journal_id IS NULL) OR (status='reversed' AND reversal_journal_id IS NOT NULL AND reversal_bank_transaction_id IS NOT NULL AND reversal_evidence IS NOT NULL))
);
CREATE TABLE finance_withholding_remittance_items (
 remittance_id text NOT NULL REFERENCES finance_withholding_remittances(id),assessment_id text NOT NULL REFERENCES finance_withholding_assessments(id),company_id text NOT NULL,tax_minor bigint NOT NULL CHECK(tax_minor>0),PRIMARY KEY(remittance_id,assessment_id)
);
CREATE FUNCTION finance_withholding_package_source_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='DELETE' OR (to_jsonb(NEW)-ARRAY['status','reviewed_by','review_reason','reviewed_at','version','updated_at']) IS DISTINCT FROM (to_jsonb(OLD)-ARRAY['status','reviewed_by','review_reason','reviewed_at','version','updated_at']) THEN
  RAISE EXCEPTION 'WITHHOLDING_PACKAGE_SOURCE_IMMUTABLE' USING ERRCODE='23514';
 END IF;
 IF NEW.version<>OLD.version+1 OR NOT(OLD.status='draft' AND NEW.status IN ('approved','rejected','cancelled') OR OLD.status='approved' AND NEW.status='settled' OR OLD.status='settled' AND NEW.status='reversed') THEN
  RAISE EXCEPTION 'WITHHOLDING_PACKAGE_TRANSITION_INVALID' USING ERRCODE='23514';
 END IF;
 RETURN NEW;
END; $$;
CREATE TRIGGER finance_withholding_package_source_immutable BEFORE UPDATE OR DELETE ON finance_withholding_packages FOR EACH ROW EXECUTE FUNCTION finance_withholding_package_source_immutable();
CREATE FUNCTION finance_withholding_remittance_source_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_TABLE_NAME='finance_withholding_remittance_items' OR TG_OP='DELETE' THEN RAISE EXCEPTION 'WITHHOLDING_REMITTANCE_SOURCE_IMMUTABLE' USING ERRCODE='23514'; END IF;
 IF (to_jsonb(NEW)-ARRAY['status','reversal_bank_transaction_id','reversal_journal_id','reversal_evidence','reversed_by','reversed_at','version']) IS DISTINCT FROM (to_jsonb(OLD)-ARRAY['status','reversal_bank_transaction_id','reversal_journal_id','reversal_evidence','reversed_by','reversed_at','version']) OR OLD.status<>'confirmed' OR NEW.status<>'reversed' OR NEW.version<>OLD.version+1 THEN
  RAISE EXCEPTION 'WITHHOLDING_REMITTANCE_SOURCE_IMMUTABLE' USING ERRCODE='23514';
 END IF;
 RETURN NEW;
END; $$;
CREATE TRIGGER finance_withholding_remittance_source_immutable BEFORE UPDATE OR DELETE ON finance_withholding_remittances FOR EACH ROW EXECUTE FUNCTION finance_withholding_remittance_source_immutable();
CREATE TRIGGER finance_withholding_remittance_item_immutable BEFORE UPDATE OR DELETE ON finance_withholding_remittance_items FOR EACH ROW EXECUTE FUNCTION finance_withholding_remittance_source_immutable();
CREATE FUNCTION finance_supplier_statutory_owner_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.kind='payment' AND NEW.status='settled' AND EXISTS(SELECT 1 FROM finance_parties WHERE id=NEW.party_id AND company_id=NEW.company_id AND kind='supplier' AND person_id IS NULL) THEN
  PERFORM pg_advisory_xact_lock(hashtextextended('finance-taxpayer:'||NEW.company_id,0));
  IF EXISTS(SELECT 1 FROM finance_tax_identities WHERE company_id=NEW.company_id AND party_id IS NULL AND status='approved' AND effective_from<=NEW.payment_date AND effective_to>=NEW.payment_date)
   AND NOT EXISTS(SELECT 1 FROM finance_withholding_assessments a WHERE a.id=NEW.withholding_assessment_id AND a.payment_id=NEW.id AND a.company_id=NEW.company_id AND a.status='consumed' AND a.gross_minor=NEW.amount_minor AND a.tax_minor=NEW.withheld_minor) THEN
   RAISE EXCEPTION 'WITHHOLDING_OWNER_REQUIRED' USING ERRCODE='23514';
  END IF;
 END IF;
 RETURN NEW;
END; $$;
CREATE CONSTRAINT TRIGGER finance_supplier_statutory_owner_guard AFTER INSERT OR UPDATE ON finance_payments DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION finance_supplier_statutory_owner_guard();
CREATE FUNCTION finance_withholding_remitted_source_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF OLD.status='consumed' AND NEW.status='reversed' AND EXISTS(SELECT 1 FROM finance_withholding_remittance_items i JOIN finance_withholding_remittances r ON r.id=i.remittance_id WHERE i.assessment_id=OLD.id AND r.status='confirmed') THEN
  RAISE EXCEPTION 'WITHHOLDING_REMITTANCE_RECOVERY_REQUIRED' USING ERRCODE='23514';
 END IF;
 RETURN NEW;
END; $$;
CREATE TRIGGER finance_withholding_remitted_source_guard BEFORE UPDATE ON finance_withholding_assessments FOR EACH ROW EXECUTE FUNCTION finance_withholding_remitted_source_guard();
CREATE FUNCTION finance_withholding_remittance_integrity() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE r finance_withholding_remittances; p finance_withholding_packages; item record; account record;
BEGIN
 IF TG_TABLE_NAME='finance_withholding_packages' THEN
  IF NEW.status NOT IN ('settled','reversed') THEN RETURN NEW; END IF;
  SELECT * INTO r FROM finance_withholding_remittances WHERE package_id=NEW.id;
 ELSIF TG_TABLE_NAME='finance_withholding_remittance_items' THEN
  SELECT * INTO r FROM finance_withholding_remittances WHERE id=NEW.remittance_id;
 ELSE
  SELECT * INTO r FROM finance_withholding_remittances WHERE id=NEW.id;
 END IF;
 SELECT * INTO p FROM finance_withholding_packages WHERE id=r.package_id;
 IF r.id IS NULL OR p.id IS NULL OR p.company_id<>r.company_id OR p.tax_minor<>r.amount_minor OR
  (r.status='confirmed' AND p.status<>'settled') OR (r.status='reversed' AND p.status<>'reversed') OR
  r.amount_minor<>(SELECT COALESCE(SUM(tax_minor),0) FROM finance_withholding_remittance_items WHERE remittance_id=r.id) OR
  r.amount_minor<>(SELECT COALESCE(SUM(value::bigint),0) FROM jsonb_each_text(r.liability_accounts_json::jsonb)) OR
  NOT EXISTS(SELECT 1 FROM finance_journals j WHERE j.id=r.journal_id AND j.company_id=r.company_id AND j.chart_id=p.chart_id AND j.currency='THB' AND j.status='posted' AND j.source_type='withholding_remittance' AND j.source_id=r.id) OR
  NOT EXISTS(SELECT 1 FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id WHERE t.id=r.bank_transaction_id AND t.company_id=r.company_id AND t.currency='THB' AND t.amount_minor=-r.amount_minor AND t.match_status='matched' AND t.matched_type='withholding_remittance' AND t.matched_id=r.id AND s.bank_account=r.bank_account_id) OR
  r.amount_minor<>(SELECT COALESCE(SUM(credit_minor-debit_minor),0) FROM finance_journal_lines WHERE journal_id=r.journal_id AND account_id=r.bank_account_id) THEN
  RAISE EXCEPTION 'WITHHOLDING_REMITTANCE_SOURCE_MISMATCH' USING ERRCODE='23514';
 END IF;
 FOR item IN SELECT i.*,a.payment_id,a.status assessment_status,a.tax_minor actual_tax FROM finance_withholding_remittance_items i JOIN finance_withholding_assessments a ON a.id=i.assessment_id AND a.company_id=i.company_id WHERE i.remittance_id=r.id LOOP
  IF item.company_id<>r.company_id OR item.tax_minor<>item.actual_tax OR (r.status='confirmed' AND item.assessment_status<>'consumed') OR
   (SELECT COUNT(*) FROM finance_withholding_remittance_items i JOIN finance_withholding_remittances x ON x.id=i.remittance_id WHERE i.assessment_id=item.assessment_id AND x.status='confirmed')>1 THEN
   RAISE EXCEPTION 'WITHHOLDING_REMITTANCE_SOURCE_MISMATCH' USING ERRCODE='23514';
  END IF;
 END LOOP;
 FOR account IN SELECT key,value::bigint amount FROM jsonb_each_text(r.liability_accounts_json::jsonb) LOOP
  IF account.amount<>(SELECT COALESCE(SUM(i.tax_minor),0) FROM finance_withholding_remittance_items i JOIN finance_withholding_assessments a ON a.id=i.assessment_id WHERE i.remittance_id=r.id AND a.source_snapshot_json::jsonb->>'liabilityAccountId'=account.key) OR account.amount<>(SELECT COALESCE(SUM(debit_minor-credit_minor),0) FROM finance_journal_lines WHERE journal_id=r.journal_id AND account_id=account.key) THEN
   RAISE EXCEPTION 'WITHHOLDING_REMITTANCE_SOURCE_MISMATCH' USING ERRCODE='23514';
  END IF;
 END LOOP;
 IF r.status='reversed' AND (r.reversed_by=r.recorded_by OR NOT EXISTS(SELECT 1 FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id WHERE t.id=r.reversal_bank_transaction_id AND t.company_id=r.company_id AND t.currency='THB' AND t.amount_minor=r.amount_minor AND t.match_status='matched' AND t.matched_type='withholding_remittance_reversal' AND t.matched_id=r.id AND s.bank_account=r.bank_account_id) OR
  NOT EXISTS(SELECT 1 FROM finance_journals j WHERE j.id=r.reversal_journal_id AND j.company_id=r.company_id AND j.chart_id=p.chart_id AND j.currency='THB' AND j.status='posted' AND j.source_type='withholding_remittance_reversal' AND j.source_id=r.id) OR
  EXISTS(SELECT account_id,SUM(debit_minor-credit_minor) FROM finance_journal_lines WHERE journal_id IN (r.journal_id,r.reversal_journal_id) GROUP BY account_id HAVING SUM(debit_minor-credit_minor)<>0)) THEN
  RAISE EXCEPTION 'WITHHOLDING_REMITTANCE_RECOVERY_MISMATCH' USING ERRCODE='23514';
 END IF;
 RETURN NEW;
END; $$;
CREATE CONSTRAINT TRIGGER finance_withholding_remittance_integrity AFTER INSERT OR UPDATE ON finance_withholding_remittances DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION finance_withholding_remittance_integrity();
CREATE CONSTRAINT TRIGGER finance_withholding_remittance_items_integrity AFTER INSERT ON finance_withholding_remittance_items DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION finance_withholding_remittance_integrity();
CREATE CONSTRAINT TRIGGER finance_withholding_package_remittance_integrity AFTER UPDATE ON finance_withholding_packages DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION finance_withholding_remittance_integrity();
