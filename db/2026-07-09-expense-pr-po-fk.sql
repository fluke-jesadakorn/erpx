BEGIN;

ALTER TABLE expenses
  ADD COLUMN IF NOT EXISTS pr_id          INT REFERENCES purchase_requisitions(id) ON DELETE SET NULL,
  ADD COLUMN IF NOT EXISTS po_id          INT REFERENCES purchase_orders(id)       ON DELETE SET NULL,
  ADD COLUMN IF NOT EXISTS journal_entry_id INT REFERENCES journal_entries(id)     ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_expenses_pr ON expenses(pr_id);
CREATE INDEX IF NOT EXISTS idx_expenses_po ON expenses(po_id);
CREATE INDEX IF NOT EXISTS idx_expenses_jv ON expenses(journal_entry_id);

ALTER TABLE journal_entries
  ADD COLUMN IF NOT EXISTS pr_id INT REFERENCES purchase_requisitions(id) ON DELETE SET NULL,
  ADD COLUMN IF NOT EXISTS po_id INT REFERENCES purchase_orders(id)       ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_journal_entries_pr ON journal_entries(pr_id);
CREATE INDEX IF NOT EXISTS idx_journal_entries_po ON journal_entries(po_id);

ALTER TABLE purchase_requisitions
  ADD COLUMN IF NOT EXISTS pr_number TEXT
  GENERATED ALWAYS AS (
    'PR-' || extract(year FROM created_at)::text || '-' || lpad(id::text, 6, '0')
  ) STORED;

CREATE UNIQUE INDEX IF NOT EXISTS idx_pr_pr_number ON purchase_requisitions(pr_number);

ALTER TABLE purchase_requisitions
  ADD COLUMN IF NOT EXISTS vendor_country CHAR(2);
ALTER TABLE purchase_orders
  ADD COLUMN IF NOT EXISTS vendor_country CHAR(2);

CREATE OR REPLACE FUNCTION next_purchase_order_number(p_year INT)
RETURNS TEXT LANGUAGE plpgsql AS $$
DECLARE n INT;
BEGIN
  SELECT COALESCE(MAX(
    CAST(regexp_replace(po_number, '^PO-'||p_year||'-', '') AS INT)
  ), 0) + 1 INTO n
  FROM purchase_orders
  WHERE po_number LIKE 'PO-'||p_year||'-%';
  RETURN 'PO-'||p_year||'-'||lpad(n::text, 6, '0');
END $$;

COMMIT;