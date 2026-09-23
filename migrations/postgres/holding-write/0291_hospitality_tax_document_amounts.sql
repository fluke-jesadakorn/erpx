ALTER TABLE hospitality_tax_documents
  ADD COLUMN document_total_minor BIGINT CHECK (document_total_minor >= 0),
  ADD COLUMN document_tax_minor BIGINT CHECK (document_tax_minor >= 0);
