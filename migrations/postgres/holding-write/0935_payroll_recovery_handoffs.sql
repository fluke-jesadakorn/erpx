-- Employee-specific agreement, independent review, Finance recognition and exact posting consumption.
CREATE TABLE hr_payroll_recoveries (
 id text PRIMARY KEY, company_id text NOT NULL,
 original_run_id text NOT NULL, original_item_id text NOT NULL REFERENCES hr_payroll_items(id),
 engagement_id text NOT NULL REFERENCES hr_engagements(id),person_id text NOT NULL REFERENCES hr_people(id),employee_user_id text NOT NULL,
 source_ref text NOT NULL CHECK(length(btrim(source_ref)) BETWEEN 8 AND 200),
 source_snapshot_json text NOT NULL CHECK(erpx_json_valid(source_snapshot_json)),
 source_digest text NOT NULL CHECK(length(source_digest)=64),policy_json text NOT NULL CHECK(erpx_json_valid(policy_json)),policy_digest text NOT NULL CHECK(length(policy_digest)=64),
 amount_minor bigint NOT NULL CHECK(amount_minor BETWEEN 1 AND 9007199254740991),currency text NOT NULL CHECK(currency='THB'),
 installment_count integer NOT NULL CHECK(installment_count BETWEEN 1 AND 24),first_deduction_period text NOT NULL,
 reason text NOT NULL CHECK(length(btrim(reason))>=10),evidence_ref text NOT NULL CHECK(length(btrim(evidence_ref))>=8),
 status text NOT NULL DEFAULT 'consent_pending' CHECK(status IN ('consent_pending','review_pending','approved','recognized','declined','rejected','cancelled')),
 version bigint NOT NULL DEFAULT 1 CHECK(version>0),requested_by text NOT NULL,created_at bigint NOT NULL,updated_at bigint NOT NULL,
 consent_by text,consent_at bigint,consent_html text,consent_sha256 text,reviewed_by text,legal_basis_ref text,
 journal_id text REFERENCES finance_journals(id),advance_id text REFERENCES hr_payroll_advances(id),recognized_by text,
 FOREIGN KEY(company_id,original_run_id) REFERENCES hr_payroll_runs(company_id,id)
);
CREATE UNIQUE INDEX hr_payroll_recovery_source_once ON hr_payroll_recoveries(company_id,source_ref) WHERE status NOT IN ('declined','rejected','cancelled');
ALTER TABLE hr_payroll_advances ADD COLUMN recovery_id text UNIQUE REFERENCES hr_payroll_recoveries(id);
CREATE TABLE hr_payroll_recovery_events (
 id text PRIMARY KEY,recovery_id text NOT NULL REFERENCES hr_payroll_recoveries(id),company_id text NOT NULL,
 action text NOT NULL,actor_id text NOT NULL,recovery_version bigint NOT NULL,evidence_json text NOT NULL CHECK(erpx_json_valid(evidence_json)),
 command_id text NOT NULL REFERENCES command_receipts(command_id),created_at bigint NOT NULL
);
CREATE TRIGGER hr_payroll_recovery_event_immutable BEFORE UPDATE OR DELETE ON hr_payroll_recovery_events FOR EACH ROW EXECUTE FUNCTION iam_immutable();
CREATE FUNCTION hr_payroll_recovery_source_guard() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE origin_amount bigint;
BEGIN
 SELECT i.net_minor INTO origin_amount FROM hr_payroll_items i JOIN hr_payroll_runs r ON r.id=i.run_id
   JOIN hr_engagements e ON e.id=i.engagement_id AND e.company_id=r.company_id JOIN hr_people p ON p.id=e.person_id
 WHERE i.id=NEW.original_item_id AND r.id=NEW.original_run_id AND r.company_id=NEW.company_id
   AND i.engagement_id=NEW.engagement_id AND e.person_id=NEW.person_id AND p.user_id=NEW.employee_user_id AND r.status='posted' AND i.status='posted'
 FOR UPDATE OF i;
 IF origin_amount IS NULL OR NEW.amount_minor>origin_amount THEN
  RAISE EXCEPTION 'Recovery must remain within the original posted employee net source' USING ERRCODE='23514';
 END IF;
 IF NEW.status NOT IN ('declined','rejected','cancelled') AND NEW.amount_minor+(SELECT COALESCE(SUM(amount_minor),0) FROM hr_payroll_recoveries WHERE original_item_id=NEW.original_item_id AND id!=NEW.id AND status NOT IN ('declined','rejected','cancelled'))>origin_amount THEN
  RAISE EXCEPTION 'Cumulative recovery exceeds the original paid net source' USING ERRCODE='23514';
 END IF;
 IF TG_OP='UPDATE' THEN
  IF (to_jsonb(NEW)-ARRAY['status','version','updated_at','consent_by','consent_at','consent_html','consent_sha256','reviewed_by','legal_basis_ref','journal_id','advance_id','recognized_by'])
     IS DISTINCT FROM (to_jsonb(OLD)-ARRAY['status','version','updated_at','consent_by','consent_at','consent_html','consent_sha256','reviewed_by','legal_basis_ref','journal_id','advance_id','recognized_by'])
    OR NEW.version!=OLD.version+1 OR OLD.status IN ('recognized','declined','rejected','cancelled')
    OR OLD.consent_sha256 IS NOT NULL AND (NEW.consent_html IS DISTINCT FROM OLD.consent_html OR NEW.consent_sha256 IS DISTINCT FROM OLD.consent_sha256 OR NEW.consent_by IS DISTINCT FROM OLD.consent_by OR NEW.consent_at IS DISTINCT FROM OLD.consent_at) THEN
   RAISE EXCEPTION 'Recovery source and signed agreement are immutable' USING ERRCODE='23514';
  END IF;
  IF NOT (OLD.status='consent_pending' AND NEW.status IN ('review_pending','declined','cancelled')
    OR OLD.status='review_pending' AND NEW.status IN ('approved','rejected','cancelled')
    OR OLD.status='approved' AND NEW.status IN ('recognized','cancelled')) THEN
   RAISE EXCEPTION 'Recovery must follow its reviewed agreement lifecycle' USING ERRCODE='23514';
  END IF;
 END IF;
 IF NEW.status IN ('review_pending','approved','recognized') AND (NEW.consent_by IS DISTINCT FROM NEW.employee_user_id OR NEW.consent_html IS NULL OR length(NEW.consent_sha256)!=64) THEN
  RAISE EXCEPTION 'Employee own specific written agreement is required before recovery review' USING ERRCODE='23514';
 END IF;
 IF NEW.status IN ('approved','recognized') AND (NEW.reviewed_by IS NULL OR NEW.legal_basis_ref IS NULL OR iam_same_person(NEW.reviewed_by,NEW.requested_by,NEW.company_id) OR iam_same_person(NEW.reviewed_by,NEW.employee_user_id,NEW.company_id)) THEN
  RAISE EXCEPTION 'An independent recovery permissibility review is required' USING ERRCODE='23514';
 END IF;
 IF NEW.status='recognized' AND (NEW.journal_id IS NULL OR NEW.advance_id IS NULL OR NEW.recognized_by IS NULL OR iam_same_person(NEW.recognized_by,NEW.reviewed_by,NEW.company_id) OR iam_same_person(NEW.recognized_by,NEW.requested_by,NEW.company_id) OR iam_same_person(NEW.recognized_by,NEW.employee_user_id,NEW.company_id)) THEN
  RAISE EXCEPTION 'Independent Finance must recognize the source recovery before deduction' USING ERRCODE='23514';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER hr_payroll_recovery_source_guard BEFORE INSERT OR UPDATE ON hr_payroll_recoveries FOR EACH ROW EXECUTE FUNCTION hr_payroll_recovery_source_guard();
CREATE TRIGGER hr_payroll_recovery_no_delete BEFORE DELETE ON hr_payroll_recoveries FOR EACH ROW EXECUTE FUNCTION iam_immutable();

CREATE FUNCTION hr_payroll_recovery_advance_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.recovery_id IS NOT NULL AND NEW.status='approved' AND NOT EXISTS(
  SELECT 1 FROM hr_payroll_recoveries r JOIN finance_journals j ON j.id=r.journal_id
   WHERE r.id=NEW.recovery_id AND r.company_id=NEW.company_id AND r.status='recognized' AND r.advance_id=NEW.id
    AND r.engagement_id=NEW.engagement_id AND r.person_id=NEW.person_id AND r.amount_minor=NEW.amount_minor
    AND r.installment_count=NEW.installment_count AND r.first_deduction_period=NEW.first_deduction_period
    AND r.currency=NEW.currency AND j.status='posted' AND j.source_type='payroll_recovery' AND j.source_id=r.id) THEN
  RAISE EXCEPTION 'Recovery deduction requires the exact independently recognized receivable handoff' USING ERRCODE='23514';
 END IF;
 RETURN NEW;
END $$;
CREATE CONSTRAINT TRIGGER hr_payroll_recovery_advance_guard AFTER INSERT OR UPDATE ON hr_payroll_advances DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION hr_payroll_recovery_advance_guard();

-- New posts consume source installments exactly once. Retained historical posts
-- are not assigned fabricated receipts or rewritten by this additive migration.
CREATE TABLE hr_payroll_deduction_consumptions (
 id text PRIMARY KEY,company_id text NOT NULL,
 repayment_id text NOT NULL UNIQUE REFERENCES hr_payroll_advance_repayments(id),
 source_version bigint NOT NULL CHECK(source_version>0),
 run_id text NOT NULL,item_id text NOT NULL REFERENCES hr_payroll_items(id),
 journal_id text NOT NULL REFERENCES finance_journals(id),receivable_account_id text NOT NULL REFERENCES finance_accounts(id),
 amount_minor bigint NOT NULL CHECK(amount_minor BETWEEN 1 AND 9007199254740991),
 command_id text NOT NULL REFERENCES command_receipts(command_id),created_at bigint NOT NULL,
 FOREIGN KEY(company_id,run_id) REFERENCES hr_payroll_runs(company_id,id)
);
CREATE INDEX hr_payroll_deduction_run ON hr_payroll_deduction_consumptions(run_id,item_id);
CREATE TRIGGER hr_payroll_deduction_consumption_immutable BEFORE UPDATE OR DELETE ON hr_payroll_deduction_consumptions FOR EACH ROW EXECUTE FUNCTION iam_immutable();
CREATE FUNCTION hr_payroll_deduction_consumption_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NOT EXISTS(SELECT 1 FROM hr_payroll_advance_repayments r JOIN hr_payroll_advances a ON a.id=r.advance_id
    JOIN hr_payroll_items i ON i.id=NEW.item_id AND i.engagement_id=a.engagement_id
    JOIN hr_payroll_runs p ON p.id=i.run_id AND p.company_id=a.company_id
    JOIN hr_payroll_postings posting ON posting.run_id=p.id
    JOIN finance_journals j ON j.id=posting.journal_id
    JOIN finance_accounts ledger ON ledger.id=NEW.receivable_account_id AND ledger.chart_id=j.chart_id AND ledger.company_id=j.company_id
   WHERE r.id=NEW.repayment_id AND r.company_id=NEW.company_id AND a.company_id=NEW.company_id
    AND r.status='collected' AND r.payroll_run_id=NEW.run_id AND r.version=NEW.source_version+1 AND r.amount_minor=NEW.amount_minor
    AND a.status IN ('approved','closed') AND p.id=NEW.run_id AND p.status='posted' AND i.status='posted'
    AND j.id=NEW.journal_id AND j.status='posted' AND j.company_id=NEW.company_id AND j.source_type='payroll' AND j.source_id=p.id
    AND ledger.category='asset' AND ledger.currency=p.currency
    AND EXISTS(SELECT 1 FROM jsonb_array_elements(i.input_snapshot_json::jsonb->'advanceRepayments') s
      WHERE s->>'id'=r.id AND (s->>'version')::bigint=NEW.source_version AND (s->>'amount_minor')::bigint=NEW.amount_minor)
    AND EXISTS(SELECT 1 FROM jsonb_array_elements(i.result_json::jsonb->'explanation'->'components') c
      WHERE c->>'code'='payroll-advance:'||r.id AND c->>'kind'='deduction' AND (c->>'amountMinor')::bigint=NEW.amount_minor)) THEN
  RAISE EXCEPTION 'Payroll deduction requires exact source version, posted item and receivable journal' USING ERRCODE='23514';
 END IF;
 IF (SELECT COALESCE(SUM(amount_minor),0) FROM hr_payroll_deduction_consumptions WHERE journal_id=NEW.journal_id AND receivable_account_id=NEW.receivable_account_id)
     !=(SELECT COALESCE(SUM(credit_minor-debit_minor),0) FROM finance_journal_lines WHERE journal_id=NEW.journal_id AND account_id=NEW.receivable_account_id) THEN
  RAISE EXCEPTION 'Consumed payroll installments must equal the posted receivable credit' USING ERRCODE='23514';
 END IF;
 RETURN NEW;
END $$;
CREATE CONSTRAINT TRIGGER hr_payroll_deduction_consumption_guard AFTER INSERT ON hr_payroll_deduction_consumptions DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION hr_payroll_deduction_consumption_guard();
CREATE FUNCTION hr_payroll_installment_collection_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='DELETE' AND OLD.status='collected' THEN
  RAISE EXCEPTION 'Collected payroll installment is immutable' USING ERRCODE='23514';
 END IF;
 IF TG_OP='UPDATE' AND (OLD.status='collected'
    OR (to_jsonb(NEW)-ARRAY['status','payroll_run_id','version']) IS DISTINCT FROM (to_jsonb(OLD)-ARRAY['status','payroll_run_id','version'])
    OR NEW.version!=OLD.version+1) THEN
  RAISE EXCEPTION 'Payroll installment source and collected evidence are immutable' USING ERRCODE='23514';
 END IF;
 IF TG_OP='UPDATE' AND NEW.status='collected' AND (OLD.status NOT IN ('open','reserved') OR NEW.payroll_run_id IS NULL) THEN
  RAISE EXCEPTION 'Only an unconsumed installment can be collected by posted payroll' USING ERRCODE='23514';
 END IF;
 RETURN CASE WHEN TG_OP='DELETE' THEN OLD ELSE NEW END;
END $$;
CREATE TRIGGER hr_payroll_installment_collection_guard BEFORE UPDATE OR DELETE ON hr_payroll_advance_repayments FOR EACH ROW EXECUTE FUNCTION hr_payroll_installment_collection_guard();
CREATE FUNCTION hr_payroll_installment_receipt_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.status='collected' AND NOT EXISTS(SELECT 1 FROM hr_payroll_deduction_consumptions c WHERE c.repayment_id=NEW.id AND c.source_version=NEW.version-1 AND c.run_id=NEW.payroll_run_id AND c.company_id=NEW.company_id AND c.amount_minor=NEW.amount_minor) THEN
  RAISE EXCEPTION 'Collected payroll installment requires its immutable posting consumption receipt' USING ERRCODE='23514';
 END IF;
 RETURN NEW;
END $$;
CREATE CONSTRAINT TRIGGER hr_payroll_installment_receipt_guard AFTER INSERT OR UPDATE ON hr_payroll_advance_repayments DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION hr_payroll_installment_receipt_guard();
