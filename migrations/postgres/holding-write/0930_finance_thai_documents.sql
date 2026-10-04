CREATE TABLE finance_tax_identities (
 id TEXT PRIMARY KEY,company_id TEXT NOT NULL,party_id TEXT REFERENCES finance_parties(id),
 legal_name TEXT NOT NULL,tax_id TEXT NOT NULL CHECK(tax_id~'^[0-9]{13}$'),address TEXT NOT NULL,branch_code TEXT NOT NULL CHECK(branch_code~'^[0-9]{5}$'),
 entity_kind TEXT NOT NULL CHECK(entity_kind IN ('domestic_company','resident_individual')),
 vat_registered BOOLEAN NOT NULL DEFAULT false,vat_registration_evidence TEXT,
 effective_from TEXT NOT NULL,effective_to TEXT NOT NULL,previous_identity_id TEXT REFERENCES finance_tax_identities(id),
 signer_name TEXT,signer_authority TEXT,signature_link_id TEXT REFERENCES attachment_links(id),signature_sha256 TEXT,signature_media_type TEXT,signature_data_uri TEXT,
 rule_pack_version TEXT NOT NULL,evidence_ref TEXT NOT NULL,
 status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','approved','rejected','cancelled','superseded')),
 created_by TEXT NOT NULL,reviewed_by TEXT,review_reason TEXT,reviewed_at BIGINT,version BIGINT NOT NULL DEFAULT 1,
 created_at BIGINT NOT NULL,updated_at BIGINT NOT NULL,
 CHECK(effective_from<=effective_to),
 CHECK(party_id IS NOT NULL OR entity_kind='domestic_company'),
 CHECK(status NOT IN ('approved','superseded') OR (reviewed_by IS NOT NULL AND reviewed_by<>created_by)),
 CHECK(status NOT IN ('approved','superseded') OR party_id IS NOT NULL OR (signature_link_id IS NOT NULL AND signature_sha256 IS NOT NULL AND signature_data_uri IS NOT NULL))
);
CREATE UNIQUE INDEX finance_tax_identity_current ON finance_tax_identities(company_id,COALESCE(party_id,'')) WHERE status='approved';

CREATE TABLE finance_withholding_assessments (
 id TEXT PRIMARY KEY,company_id TEXT NOT NULL,payment_id TEXT NOT NULL REFERENCES finance_payments(id),payment_version BIGINT NOT NULL,
 payer_identity_id TEXT NOT NULL REFERENCES finance_tax_identities(id),payer_identity_version BIGINT NOT NULL,
 payee_identity_id TEXT NOT NULL REFERENCES finance_tax_identities(id),payee_identity_version BIGINT NOT NULL,
 liability_account_id TEXT NOT NULL REFERENCES finance_accounts(id),liability_account_version BIGINT NOT NULL,
 payment_date TEXT NOT NULL,form_kind TEXT NOT NULL CHECK(form_kind IN ('PND3','PND53')),gross_minor BIGINT NOT NULL CHECK(gross_minor>0),
 income_minor BIGINT NOT NULL CHECK(income_minor>0),tax_minor BIGINT NOT NULL CHECK(tax_minor>=0 AND tax_minor<gross_minor),
 source_fingerprint TEXT NOT NULL CHECK(length(source_fingerprint)=64),source_snapshot_json TEXT NOT NULL,
 rule_pack_version TEXT NOT NULL,rule_pack_digest TEXT NOT NULL CHECK(length(rule_pack_digest)=64),
 classification_evidence TEXT NOT NULL,status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','approved','rejected','cancelled','consumed','reversed')),
 prepared_by TEXT NOT NULL,reviewed_by TEXT,review_reason TEXT,reviewed_at BIGINT,consumed_at BIGINT,reversal_journal_id TEXT REFERENCES finance_journals(id),
 version BIGINT NOT NULL DEFAULT 1,created_at BIGINT NOT NULL,updated_at BIGINT NOT NULL,
 CHECK(status NOT IN ('approved','consumed','reversed') OR (reviewed_by IS NOT NULL AND reviewed_by<>prepared_by))
);
CREATE UNIQUE INDEX finance_withholding_live_payment ON finance_withholding_assessments(payment_id) WHERE status IN ('draft','approved','consumed');
ALTER TABLE finance_payments ADD COLUMN withholding_assessment_id TEXT REFERENCES finance_withholding_assessments(id),ADD COLUMN withheld_minor BIGINT NOT NULL DEFAULT 0 CHECK(withheld_minor>=0 AND withheld_minor<amount_minor);

CREATE TABLE finance_withholding_income_lines (
 id TEXT PRIMARY KEY,assessment_id TEXT NOT NULL REFERENCES finance_withholding_assessments(id),company_id TEXT NOT NULL,
 invoice_id TEXT NOT NULL REFERENCES finance_invoices(id),invoice_version BIGINT NOT NULL,invoice_line_revision BIGINT NOT NULL,
 source_line_id TEXT NOT NULL,description TEXT NOT NULL,category TEXT NOT NULL,contract_ref TEXT NOT NULL,contract_total_minor BIGINT NOT NULL CHECK(contract_total_minor>0),
 allocated_gross_minor BIGINT NOT NULL CHECK(allocated_gross_minor>0),prior_paid_gross_minor BIGINT NOT NULL CHECK(prior_paid_gross_minor>=0),
 original_income_minor BIGINT NOT NULL CHECK(original_income_minor>0),income_minor BIGINT NOT NULL CHECK(income_minor>=0),
 prior_withheld_minor BIGINT NOT NULL CHECK(prior_withheld_minor>=0),rate_bps INTEGER NOT NULL CHECK(rate_bps BETWEEN 0 AND 10000),tax_minor BIGINT NOT NULL CHECK(tax_minor>=0),
 source_snapshot_json TEXT NOT NULL,ordinal INTEGER NOT NULL,created_at BIGINT NOT NULL,
 UNIQUE(assessment_id,invoice_id,source_line_id)
);

CREATE TABLE finance_thai_document_sequences (
 company_id TEXT NOT NULL,document_kind TEXT NOT NULL CHECK(document_kind IN ('withholding_certificate','tax_invoice')),
 fiscal_year INTEGER NOT NULL,last_number BIGINT NOT NULL DEFAULT 0 CHECK(last_number>=0),PRIMARY KEY(company_id,document_kind,fiscal_year)
);
CREATE TABLE finance_issued_thai_documents (
 id TEXT PRIMARY KEY,company_id TEXT NOT NULL,document_kind TEXT NOT NULL CHECK(document_kind IN ('withholding_certificate','tax_invoice')),
 document_number TEXT NOT NULL,issue_date TEXT NOT NULL,payment_id TEXT REFERENCES finance_payments(id),invoice_id TEXT REFERENCES finance_invoices(id),assessment_id TEXT REFERENCES finance_withholding_assessments(id),
 previous_document_id TEXT REFERENCES finance_issued_thai_documents(id),edition INTEGER NOT NULL CHECK(edition>0),
 payer_identity_id TEXT NOT NULL REFERENCES finance_tax_identities(id),payee_identity_id TEXT NOT NULL REFERENCES finance_tax_identities(id),
 source_fingerprint TEXT NOT NULL CHECK(length(source_fingerprint)=64),snapshot_json TEXT NOT NULL,
 export_html TEXT NOT NULL,export_sha256 TEXT NOT NULL CHECK(length(export_sha256)=64),
 reason TEXT NOT NULL,issued_by TEXT NOT NULL,command_id TEXT NOT NULL UNIQUE,created_at BIGINT NOT NULL,
 UNIQUE(company_id,document_kind,document_number),
 CHECK((document_kind='withholding_certificate' AND payment_id IS NOT NULL AND assessment_id IS NOT NULL) OR (document_kind='tax_invoice' AND invoice_id IS NOT NULL))
);
CREATE UNIQUE INDEX finance_thai_document_payment_edition ON finance_issued_thai_documents(payment_id,edition) WHERE document_kind='withholding_certificate';
CREATE UNIQUE INDEX finance_thai_document_invoice_edition ON finance_issued_thai_documents(invoice_id,edition) WHERE document_kind='tax_invoice';
CREATE TABLE finance_thai_document_events (
 id TEXT PRIMARY KEY,document_id TEXT NOT NULL REFERENCES finance_issued_thai_documents(id),company_id TEXT NOT NULL,
 action TEXT NOT NULL CHECK(action IN ('issued','superseded','cancelled')),replacement_document_id TEXT REFERENCES finance_issued_thai_documents(id),
 reason TEXT NOT NULL,actor_id TEXT NOT NULL,command_id TEXT NOT NULL,created_at BIGINT NOT NULL,UNIQUE(document_id,action)
);
CREATE TABLE finance_thai_document_changes (
 id TEXT PRIMARY KEY,company_id TEXT NOT NULL,document_id TEXT NOT NULL REFERENCES finance_issued_thai_documents(id),
 corrected_payer_identity_id TEXT NOT NULL REFERENCES finance_tax_identities(id),corrected_payee_identity_id TEXT NOT NULL REFERENCES finance_tax_identities(id),
 issue_date TEXT NOT NULL,reason TEXT NOT NULL,source_fingerprint TEXT NOT NULL,source_snapshot_json TEXT NOT NULL,
 status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','issued','rejected','cancelled')),replacement_document_id TEXT REFERENCES finance_issued_thai_documents(id),
 prepared_by TEXT NOT NULL,reviewed_by TEXT,review_reason TEXT,reviewed_at BIGINT,version BIGINT NOT NULL DEFAULT 1,created_at BIGINT NOT NULL,updated_at BIGINT NOT NULL,
 CHECK(status!='issued' OR (replacement_document_id IS NOT NULL AND reviewed_by IS NOT NULL AND reviewed_by<>prepared_by))
);
CREATE UNIQUE INDEX finance_thai_document_one_change ON finance_thai_document_changes(document_id) WHERE status IN ('draft','issued');

CREATE FUNCTION finance_thai_immutable() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'FINANCE_THAI_EVIDENCE_IMMUTABLE' USING ERRCODE='23514'; END $$;
DO $$ DECLARE t TEXT; BEGIN
 FOREACH t IN ARRAY ARRAY['finance_withholding_income_lines','finance_issued_thai_documents','finance_thai_document_events'] LOOP
  EXECUTE format('CREATE TRIGGER finance_thai_immutable BEFORE UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION finance_thai_immutable()',t);
 END LOOP;
END $$;

CREATE FUNCTION finance_tax_identity_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='DELETE' THEN RAISE EXCEPTION 'FINANCE_TAX_IDENTITY_IMMUTABLE' USING ERRCODE='23514'; END IF;
 IF OLD.status!='draft' AND (to_jsonb(NEW)-ARRAY['status','version','updated_at']) IS DISTINCT FROM (to_jsonb(OLD)-ARRAY['status','version','updated_at']) THEN RAISE EXCEPTION 'FINANCE_TAX_IDENTITY_IMMUTABLE' USING ERRCODE='23514'; END IF;
 IF OLD.status!='draft' AND NEW.status!=OLD.status AND NOT(OLD.status='approved' AND NEW.status='superseded') THEN RAISE EXCEPTION 'FINANCE_TAX_IDENTITY_STATE' USING ERRCODE='23514'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER finance_tax_identity_guard BEFORE UPDATE OR DELETE ON finance_tax_identities FOR EACH ROW EXECUTE FUNCTION finance_tax_identity_guard();

CREATE FUNCTION finance_withholding_review_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='DELETE' THEN RAISE EXCEPTION 'FINANCE_WITHHOLDING_SOURCE_IMMUTABLE' USING ERRCODE='23514'; END IF;
 IF (to_jsonb(NEW)-ARRAY['status','reviewed_by','review_reason','reviewed_at','consumed_at','reversal_journal_id','replacement_document_id','version','updated_at']) IS DISTINCT FROM (to_jsonb(OLD)-ARRAY['status','reviewed_by','review_reason','reviewed_at','consumed_at','reversal_journal_id','replacement_document_id','version','updated_at']) THEN RAISE EXCEPTION 'FINANCE_WITHHOLDING_SOURCE_IMMUTABLE' USING ERRCODE='23514'; END IF;
 IF NEW.status IN ('approved','consumed','reversed','issued') AND NEW.reviewed_by=NEW.prepared_by THEN RAISE EXCEPTION 'FINANCE_WITHHOLDING_SELF_REVIEW' USING ERRCODE='23514'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER finance_withholding_review_guard BEFORE UPDATE OR DELETE ON finance_withholding_assessments FOR EACH ROW EXECUTE FUNCTION finance_withholding_review_guard();
CREATE TRIGGER finance_thai_document_change_guard BEFORE UPDATE OR DELETE ON finance_thai_document_changes FOR EACH ROW EXECUTE FUNCTION finance_withholding_review_guard();

CREATE FUNCTION finance_withheld_payment_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.withholding_assessment_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM finance_withholding_assessments a WHERE a.id=NEW.withholding_assessment_id AND a.company_id=NEW.company_id AND a.payment_id=NEW.id AND a.tax_minor=NEW.withheld_minor AND a.gross_minor=NEW.amount_minor)
 THEN RAISE EXCEPTION 'FINANCE_PAYMENT_WITHHOLDING_MISMATCH' USING ERRCODE='23514'; END IF;
 IF NEW.status='settled' AND NEW.withheld_minor>0 AND (
   NOT EXISTS(SELECT 1 FROM finance_withholding_assessments WHERE id=NEW.withholding_assessment_id AND status='consumed')
   OR NOT EXISTS(SELECT 1 FROM finance_issued_thai_documents WHERE payment_id=NEW.id AND document_kind='withholding_certificate')
 ) THEN RAISE EXCEPTION 'FINANCE_WITHHOLDING_CERTIFICATE_REQUIRED' USING ERRCODE='23514'; END IF;
 RETURN NULL;
END $$;
CREATE CONSTRAINT TRIGGER finance_withheld_payment_guard AFTER INSERT OR UPDATE ON finance_payments DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION finance_withheld_payment_guard();
