-- db/2026-07-16-A-sales-orders-foundation.sql
--
-- Sales / Income waybill foundation:
--   1. Extend waybills.origin CHECK to add 'so'
--   2. Extend journal_entries.step CHECK to add sales_vat / sales_accrual / sales_settlement
--   3. Extend journal_entries.draft_source CHECK to add 'so'
--   4. Extend waybill_events.kind CHECK to add 9 sales kinds
--   5. Create sales_orders + so_items tables
--   6. Create next_sales_order_number(fy) sequence function
--   7. Create get_ar_aging_buckets() SQL function (4-bucket aging)
--
-- Run with:
--   PGPASSWORD=contractpw psql -h localhost -U contract -d folio_db \
--     -v ON_ERROR_STOP=1 -f folio/db/2026-07-16-A-sales-orders-foundation.sql

BEGIN;

-- ============================================================================
-- 1. Extend waybills.origin CHECK
-- ============================================================================

ALTER TABLE waybills DROP CONSTRAINT IF EXISTS waybills_origin_check;
ALTER TABLE waybills ADD CONSTRAINT waybills_origin_check
  CHECK (origin IN ('expense', 'pr', 'po', 'so'));

-- ============================================================================
-- 2. Extend journal_entries.step CHECK
-- ============================================================================
-- (existing values: 'reimbursement', 'accrual', 'settlement')

ALTER TABLE journal_entries DROP CONSTRAINT IF EXISTS journal_entries_step_check;
ALTER TABLE journal_entries ADD CONSTRAINT journal_entries_step_check
  CHECK (step IN ('reimbursement', 'accrual', 'settlement', 'sales_vat', 'sales_accrual', 'sales_settlement'));

-- ============================================================================
-- 3. Extend journal_entries.draft_source CHECK
-- ============================================================================

ALTER TABLE journal_entries DROP CONSTRAINT IF EXISTS journal_entries_draft_source_check;
DO $$
BEGIN
  IF EXISTS (
    SELECT 1 FROM information_schema.table_constraints
    WHERE constraint_name = 'journal_entries_draft_source_check'
      AND table_schema = 'public'
  ) THEN
    ALTER TABLE journal_entries DROP CONSTRAINT journal_entries_draft_source_check;
  END IF;
END
$$;

DO $$
BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM information_schema.table_constraints
    WHERE constraint_name = 'journal_entries_draft_source_chk'
      AND table_schema = 'public'
  ) THEN
    ALTER TABLE journal_entries ADD CONSTRAINT journal_entries_draft_source_chk
      CHECK (draft_source IS NULL OR draft_source IN ('expense', 'pr', 'po', 'so'));
  END IF;
END
$$;

-- ============================================================================
-- 4. Extend waybill_events.kind CHECK (9 new sales kinds)
-- ============================================================================

ALTER TABLE waybill_events DROP CONSTRAINT IF EXISTS waybill_events_kind_check;
ALTER TABLE waybill_events ADD CONSTRAINT waybill_events_kind_check CHECK (kind IN (
  'created','submitted','advanced','rejected','corrected','settled',
  'posted-to-gl','gl-confirmed','slip-attached','attached','signed-off',
  'reversed','authorization-overridden','resubmitted','superseded',
  'pr-created','po-issued',
  'posted-to-gl-accrual','gl-confirmed-accrual',
  'posted-to-gl-settlement','gl-confirmed-settlement',
  'created-draft-gl-accrual','created-draft-gl-settlement',
  -- Sales kinds (9)
  'so-created','so-submitted','so-auto-approved','so-reviewed',
  'so-credit-checked','so-invoiced','so-paid','so-rejected',
  'posted-to-gl-sales-vat','gl-confirmed-sales-vat',
  'posted-to-gl-sales-accrual','gl-confirmed-sales-accrual',
  'posted-to-gl-sales-settlement','gl-confirmed-sales-settlement',
  'created-draft-gl-sales-vat','created-draft-gl-sales-accrual',
  'created-draft-gl-sales-settlement'
));

-- ============================================================================
-- 5. sales_orders + so_items + customers table
-- ============================================================================

CREATE TABLE IF NOT EXISTS customers (
  id                serial PRIMARY KEY,
  code              text UNIQUE NOT NULL,
  name              text NOT NULL,
  name_th           text,
  tax_id            text,
  billing_address   text,
  shipping_address  text,
  contact_name      text,
  contact_email     text,
  contact_phone     text,
  credit_limit_thb  numeric(14,2) NOT NULL DEFAULT 0,
  payment_terms     text NOT NULL DEFAULT 'Net 30',
  blacklist         boolean NOT NULL DEFAULT false,
  is_active         boolean NOT NULL DEFAULT true,
  created_at        timestamptz NOT NULL DEFAULT now(),
  updated_at        timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS customers_active_idx ON customers (is_active) WHERE is_active;
CREATE INDEX IF NOT EXISTS customers_name_idx ON customers (name);

DROP TRIGGER IF EXISTS customers_touch ON customers;
CREATE TRIGGER customers_touch BEFORE UPDATE ON customers
  FOR EACH ROW EXECUTE FUNCTION perm.touch_updated_at();

CREATE TABLE IF NOT EXISTS customer_contacts (
  id          serial PRIMARY KEY,
  customer_id int NOT NULL REFERENCES customers(id) ON DELETE CASCADE,
  fullname    text NOT NULL,
  role        text,
  email       text,
  phone       text,
  notes       text,
  created_at  timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS customer_contacts_customer_idx ON customer_contacts (customer_id);

CREATE TABLE IF NOT EXISTS sales_orders (
  id                       serial PRIMARY KEY,
  so_number                text UNIQUE NOT NULL,
  customer_id              int NOT NULL REFERENCES customers(id) ON DELETE RESTRICT,
  sales_rep_id             int NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
  status                   text NOT NULL DEFAULT 'so_draft'
                           CHECK (status IN ('so_draft','so_sales_review','so_credit_check',
                                             'so_invoiced','so_paid','rejected')),
  payment_terms            text NOT NULL DEFAULT 'Net 30',
  due_date                 date,
  invoice_number           text,
  invoice_issued_at        timestamptz,
  ar_slip_id               int,
  vat_account_code         text NOT NULL DEFAULT '210200',
  ar_account_code          text NOT NULL DEFAULT '120200',
  cash_account_code        text NOT NULL DEFAULT '110200',
  revenue_account_code     text NOT NULL DEFAULT '410100',
  subtotal                 numeric(14,2) NOT NULL DEFAULT 0,
  vat_total                numeric(14,2) NOT NULL DEFAULT 0,
  total_amount             numeric(14,2) NOT NULL DEFAULT 0,
  currency                 text NOT NULL DEFAULT 'THB',
  rejection_reason         text,
  rejection_actor_id       int REFERENCES users(id),
  rejected_at              timestamptz,
  invoice_issuer_id        int REFERENCES users(id),
  paid_by_id               int REFERENCES users(id),
  paid_at                  timestamptz,
  created_at               timestamptz NOT NULL DEFAULT now(),
  updated_at               timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS sales_orders_status_idx ON sales_orders (status);
CREATE INDEX IF NOT EXISTS sales_orders_customer_idx ON sales_orders (customer_id);
CREATE INDEX IF NOT EXISTS sales_orders_rep_idx ON sales_orders (sales_rep_id);
CREATE INDEX IF NOT EXISTS sales_orders_due_idx ON sales_orders (due_date) WHERE status NOT IN ('so_paid', 'rejected');

DROP TRIGGER IF EXISTS sales_orders_touch ON sales_orders;
CREATE TRIGGER sales_orders_touch BEFORE UPDATE ON sales_orders
  FOR EACH ROW EXECUTE FUNCTION perm.touch_updated_at();

CREATE TABLE IF NOT EXISTS so_items (
  id                          serial PRIMARY KEY,
  sales_order_id              int NOT NULL REFERENCES sales_orders(id) ON DELETE CASCADE,
  description                 text NOT NULL,
  qty                         numeric(12,2) NOT NULL DEFAULT 1,
  unit_price                  numeric(14,2) NOT NULL DEFAULT 0,
  vat_amount                  numeric(14,2) NOT NULL DEFAULT 0,
  line_total                  numeric(14,2) NOT NULL DEFAULT 0,
  mapped_revenue_account_code text REFERENCES chart_of_accounts(code),
  confidence_score            numeric(5,3),
  created_at                  timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS so_items_so_idx ON so_items (sales_order_id);

CREATE OR REPLACE VIEW customer_ar_history AS
  SELECT
    c.id                                                       AS customer_id,
    c.code                                                     AS customer_code,
    c.name                                                     AS customer_name,
    c.credit_limit_thb                                         AS credit_limit,
    COALESCE(SUM(so.total_amount), 0)::numeric(14,2)           AS total_invoiced,
    COALESCE(SUM(CASE WHEN so.status NOT IN ('so_paid','rejected')
                      THEN so.total_amount ELSE 0 END), 0)::numeric(14,2) AS outstanding_ar,
    COALESCE(SUM(CASE WHEN so.status = 'so_paid'
                      THEN so.total_amount ELSE 0 END), 0)::numeric(14,2) AS total_paid,
    COUNT(so.id)::int                                          AS so_count
  FROM customers c
  LEFT JOIN sales_orders so ON so.customer_id = c.id
  GROUP BY c.id, c.code, c.name, c.credit_limit_thb;

-- ============================================================================
-- 6. next_sales_order_number(fy) sequence function (mirrors next_waybill_number)
-- ============================================================================

CREATE OR REPLACE FUNCTION next_sales_order_number(p_fiscal_year smallint)
RETURNS text
LANGUAGE plpgsql
AS $$
DECLARE
  seq_name text := 'sales_orders_fy_' || p_fiscal_year || '_seq';
  next_n   int;
  result   text;
BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_class WHERE relname = seq_name) THEN
    EXECUTE 'CREATE SEQUENCE ' || seq_name || ' START 1 INCREMENT 1 NO CYCLE';
  END IF;
  EXECUTE 'SELECT nextval(' || quote_literal(seq_name) || ')' INTO next_n;
  result := 'SO-' || p_fiscal_year::text || '-' || lpad(next_n::text, 6, '0');
  RETURN result;
END
$$;

GRANT EXECUTE ON FUNCTION next_sales_order_number(smallint) TO contract, n8n_user;

-- ============================================================================
-- 7. get_ar_aging_buckets() SQL function
-- Returns 4 rows: 0-30 / 31-60 / 61-90 / 90+ days of outstanding AR
-- ============================================================================

CREATE OR REPLACE FUNCTION get_ar_aging_buckets()
RETURNS TABLE(bucket text, days_from int, days_to int, amount_thb numeric, so_count bigint)
LANGUAGE sql
STABLE
AS $$
  SELECT
    bucket,
    days_from,
    days_to,
    amount_thb,
    so_count
  FROM (
    SELECT
      CASE
        WHEN days_overdue BETWEEN 0 AND 30  THEN '0-30'
        WHEN days_overdue BETWEEN 31 AND 60 THEN '31-60'
        WHEN days_overdue BETWEEN 61 AND 90 THEN '61-90'
        ELSE '90+'
      END                                                AS bucket,
      CASE
        WHEN days_overdue BETWEEN 0  AND 30 THEN 0
        WHEN days_overdue BETWEEN 31 AND 60 THEN 31
        WHEN days_overdue BETWEEN 61 AND 90 THEN 61
        ELSE 90
      END                                                AS days_from,
      CASE
        WHEN days_overdue BETWEEN 0  AND 30 THEN 30
        WHEN days_overdue BETWEEN 31 AND 60 THEN 60
        WHEN days_overdue BETWEEN 61 AND 90 THEN 90
        ELSE 9999
      END                                                AS days_to,
      SUM(total_amount)::numeric(14,2)                   AS amount_thb,
      COUNT(*)::bigint                                   AS so_count
    FROM (
      SELECT
        so.id,
        so.total_amount,
        GREATEST(0, (CURRENT_DATE - so.due_date)::int) AS days_overdue
      FROM sales_orders so
      WHERE so.status IN ('so_invoiced', 'so_credit_check')
        AND so.due_date IS NOT NULL
        AND so.due_date <= CURRENT_DATE
    ) aged
    GROUP BY 1, 2, 3
  ) b
  ORDER BY days_from;
$$;

GRANT EXECUTE ON FUNCTION get_ar_aging_buckets() TO contract, n8n_user;

-- ============================================================================
-- 8. Audit row
-- ============================================================================

INSERT INTO perm.audit (kind, actor, target)
VALUES (
  'schema.migration',
  'system',
  jsonb_build_object(
    'migration', '2026-07-16-A-sales-orders-foundation',
    'description', 'sales_orders + so_items + customers + customer_contacts + sales/AR aging sequence fn'
  )
);

COMMIT;
