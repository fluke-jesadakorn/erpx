-- Retain the exact owner source version used to prepare a matched payable.
-- Finance compares it with the current Commerce fulfillment at post time; a
-- source changed after invoice preparation is rejected instead of posted.
ALTER TABLE finance_invoices ADD COLUMN source_version BIGINT;
ALTER TABLE finance_invoices ADD CONSTRAINT finance_invoices_source_version_check
  CHECK (source_version IS NULL OR source_version >= 1);
UPDATE finance_invoices i
SET source_version = f.version
FROM commerce_fulfillments f
WHERE i.source_type = 'commerce_fulfillment'
  AND i.source_id = f.id
  AND i.source_version IS NULL;
