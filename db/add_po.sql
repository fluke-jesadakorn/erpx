-- v3 schema additions for Folio: Purchase Orders (PO) + payslip linking.
-- Additive only: never drops columns or breaks v1/v2 schemas.
-- Safe to re-run.

-- 1. Purchase Orders (PO) — created first so slips can FK to it.
CREATE TABLE IF NOT EXISTS purchase_orders (
  id                 SERIAL PRIMARY KEY,
  pr_id              INT NOT NULL REFERENCES purchase_requisitions(id) ON DELETE RESTRICT,
  po_number          VARCHAR(40) UNIQUE NOT NULL,
  vendor_name        VARCHAR(150),
  total_amount       DECIMAL(14,2) DEFAULT 0,
  currency           VARCHAR(10)  DEFAULT 'THB',
  status             VARCHAR(50)  DEFAULT 'draft',
  matched_policy_id  INT REFERENCES approval_policies(id) ON DELETE SET NULL,
  issued_at          TIMESTAMP    DEFAULT CURRENT_TIMESTAMP,
  issued_by          INT REFERENCES users(id),
  settled_at         TIMESTAMP,
  settled_by         INT REFERENCES users(id),
  settled_slip_id    INT REFERENCES slips(id) ON DELETE SET NULL,
  rejection_reason   TEXT,
  rejection_actor_id INT REFERENCES users(id),
  rejected_at        TIMESTAMP,
  created_at         TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
  updated_at         TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_po_status ON purchase_orders(status);
CREATE INDEX IF NOT EXISTS idx_po_pr     ON purchase_orders(pr_id);

-- 2. PO items.
CREATE TABLE IF NOT EXISTS po_items (
  id                  SERIAL PRIMARY KEY,
  po_id               INT NOT NULL REFERENCES purchase_orders(id) ON DELETE CASCADE,
  description         VARCHAR(255) NOT NULL,
  qty                 DECIMAL(12,2) DEFAULT 1,
  unit_price          DECIMAL(12,2) DEFAULT 0,
  mapped_account_code VARCHAR(20)  REFERENCES chart_of_accounts(code),
  created_at          TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_po_items_po ON po_items(po_id);

-- 3. PO approval log.
CREATE TABLE IF NOT EXISTS po_approval_logs (
  id              SERIAL PRIMARY KEY,
  po_id           INT NOT NULL REFERENCES purchase_orders(id) ON DELETE CASCADE,
  actor_id        INT REFERENCES users(id),
  previous_status VARCHAR(50),
  new_status      VARCHAR(50),
  comments        TEXT,
  stage           VARCHAR(50),
  chain_index     INT,
  created_at      TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_po_logs_po ON po_approval_logs(po_id, created_at DESC);

-- 4. Add po_id column to slips + FK (after purchase_orders exists).
ALTER TABLE slips
  ADD COLUMN IF NOT EXISTS po_id INT;

DO $$
BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM pg_constraint WHERE conname = 'fk_slips_po'
  ) THEN
    ALTER TABLE slips
      ADD CONSTRAINT fk_slips_po
      FOREIGN KEY (po_id) REFERENCES purchase_orders(id) ON DELETE SET NULL;
  END IF;
END$$;

CREATE INDEX IF NOT EXISTS idx_slips_po ON slips(po_id);