BEGIN;

UPDATE folio.sales_orders
SET branch_id = (
  SELECT id
  FROM finance.branches
  WHERE active
  ORDER BY id
  LIMIT 1
)
WHERE branch_id IS NULL;

ALTER TABLE folio.sales_orders
  ALTER COLUMN branch_id SET NOT NULL;

COMMIT;
