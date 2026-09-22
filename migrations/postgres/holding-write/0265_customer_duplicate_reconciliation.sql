-- Preserve Finance totals before safe customer source repointing.
CREATE TABLE customer_duplicate_merge_reconciliations (
  review_id TEXT PRIMARY KEY REFERENCES customer_duplicate_reviews(id),
  company_id TEXT NOT NULL,
  canonical_customer_id TEXT NOT NULL REFERENCES finance_parties(id),
  merged_customer_id TEXT NOT NULL REFERENCES finance_parties(id),
  currency TEXT NOT NULL,
  invoice_count BIGINT NOT NULL,
  invoice_total_minor BIGINT NOT NULL,
  invoice_outstanding_minor BIGINT NOT NULL,
  payment_count BIGINT NOT NULL,
  payment_total_minor BIGINT NOT NULL,
  payment_outstanding_minor BIGINT NOT NULL,
  refund_count BIGINT NOT NULL,
  refund_total_minor BIGINT NOT NULL,
  refund_outstanding_minor BIGINT NOT NULL,
  created_by TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  CHECK(canonical_customer_id<>merged_customer_id)
);
CREATE INDEX customer_duplicate_merge_reconciliation_company ON customer_duplicate_merge_reconciliations(company_id,created_at);
