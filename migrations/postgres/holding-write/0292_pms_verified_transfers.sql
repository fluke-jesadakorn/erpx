CREATE TABLE pms_transfer_requests (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  property_id TEXT NOT NULL REFERENCES pms_properties(id),
  folio_id TEXT NOT NULL REFERENCES pms_records(id),
  shift_id TEXT NOT NULL REFERENCES pms_records(id),
  kind TEXT NOT NULL CHECK (kind IN ('receipt','deposit')),
  business_date TEXT NOT NULL,
  currency TEXT NOT NULL,
  amount_minor BIGINT NOT NULL CHECK (amount_minor > 0),
  reference TEXT NOT NULL,
  state TEXT NOT NULL CHECK (state IN ('pending','verified','cancelled')),
  version BIGINT NOT NULL DEFAULT 1,
  recorded_by TEXT NOT NULL,
  verified_by TEXT,
  cancelled_by TEXT,
  bank_transaction_id TEXT UNIQUE REFERENCES finance_bank_transactions(id),
  entry_id TEXT UNIQUE REFERENCES pms_records(id),
  finance_payment_id TEXT UNIQUE REFERENCES finance_payments(id),
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL
);
CREATE UNIQUE INDEX pms_transfer_reference_active
  ON pms_transfer_requests(company_id,reference) WHERE state <> 'cancelled';
CREATE INDEX pms_transfer_requests_folio_state
  ON pms_transfer_requests(property_id,folio_id,state,created_at DESC);

INSERT INTO work_cache_revisions(table_name) VALUES('pms_transfer_requests') ON CONFLICT DO NOTHING;
CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON pms_transfer_requests
  FOR EACH ROW EXECUTE FUNCTION work_cache_changed();
CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON pms_transfer_requests
  FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed();
