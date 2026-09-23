CREATE TABLE hospitality_tax_documents (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  invoice_id TEXT NOT NULL,
  source_type TEXT NOT NULL CHECK (source_type IN ('pms','pos')),
  issuer TEXT NOT NULL,
  external_reference TEXT NOT NULL,
  issued_on TEXT NOT NULL,
  currency TEXT NOT NULL,
  total_minor BIGINT NOT NULL CHECK (total_minor >= 0),
  tax_minor BIGINT NOT NULL CHECK (tax_minor >= 0),
  state TEXT NOT NULL CHECK (state IN ('recorded','verified','voided')),
  version BIGINT NOT NULL DEFAULT 1,
  recorded_by TEXT NOT NULL,
  verified_by TEXT,
  voided_by TEXT,
  reason TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL
);
CREATE UNIQUE INDEX hospitality_tax_documents_active_invoice
  ON hospitality_tax_documents(company_id,invoice_id) WHERE state <> 'voided';
CREATE UNIQUE INDEX hospitality_tax_documents_active_reference
  ON hospitality_tax_documents(company_id,issuer,external_reference) WHERE state <> 'voided';
CREATE INDEX hospitality_tax_documents_invoice_history
  ON hospitality_tax_documents(company_id,invoice_id,created_at DESC);
