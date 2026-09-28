-- Expense document pages are physical attachment derivatives. Count each
-- object once in the canonical storage ledger so quota, reconciliation and
-- destruction use the same source of truth as previews and publications.
ALTER TABLE attachment_storage_ledger
  DROP CONSTRAINT attachment_storage_ledger_variant_check;

ALTER TABLE attachment_storage_ledger
  ADD CONSTRAINT attachment_storage_ledger_variant_check
  CHECK (variant IN ('original','preview','publication','derivative'));

INSERT INTO attachment_storage_ledger(
  id,company_id,attachment_id,variant,object_key,bytes,state,created_at
)
SELECT
  'attachment-storage-derivative-' || derivative.id,
  derivative.company_id,
  derivative.attachment_id,
  'derivative',
  derivative.object_key,
  derivative.bytes,
  'stored',
  derivative.created_at
FROM attachment_derivatives derivative
ON CONFLICT(object_key) DO NOTHING;
