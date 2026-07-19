BEGIN;

CREATE SCHEMA IF NOT EXISTS inventory;

CREATE TABLE IF NOT EXISTS finance.company_config (
  id smallint PRIMARY KEY DEFAULT 1 CHECK (id = 1),
  legal_name text NOT NULL,
  tax_id text,
  country_code char(2) NOT NULL DEFAULT 'TH',
  functional_currency char(3) NOT NULL DEFAULT 'THB',
  fiscal_year_start_month smallint NOT NULL DEFAULT 1 CHECK (fiscal_year_start_month BETWEEN 1 AND 12),
  vat_registered boolean NOT NULL DEFAULT true,
  updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS finance.currencies (
  code char(3) PRIMARY KEY,
  name text NOT NULL,
  decimals smallint NOT NULL DEFAULT 2 CHECK (decimals BETWEEN 0 AND 6),
  active boolean NOT NULL DEFAULT true
);

CREATE TABLE IF NOT EXISTS finance.branches (
  id bigserial PRIMARY KEY,
  code text NOT NULL UNIQUE,
  name text NOT NULL,
  name_th text,
  tax_branch_code text NOT NULL,
  address text,
  active boolean NOT NULL DEFAULT true,
  created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS finance.fx_rates (
  rate_date date NOT NULL,
  currency_code char(3) NOT NULL REFERENCES finance.currencies(code),
  rate_to_thb numeric(20,10) NOT NULL CHECK (rate_to_thb > 0),
  source text NOT NULL DEFAULT 'manual',
  approved_by integer NOT NULL,
  approved_at timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (rate_date, currency_code)
);

CREATE TABLE IF NOT EXISTS finance.fiscal_periods (
  id bigserial PRIMARY KEY,
  fiscal_year integer NOT NULL,
  period_no smallint NOT NULL CHECK (period_no BETWEEN 1 AND 12),
  starts_on date NOT NULL,
  ends_on date NOT NULL,
  status text NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'soft_closed', 'locked')),
  closed_by integer,
  closed_at timestamptz,
  reopened_by integer,
  reopened_at timestamptz,
  UNIQUE (fiscal_year, period_no),
  EXCLUDE USING gist (daterange(starts_on, ends_on, '[]') WITH &&),
  CHECK (starts_on <= ends_on)
);

CREATE TABLE IF NOT EXISTS finance.tax_codes (
  code text PRIMARY KEY,
  name text NOT NULL,
  kind text NOT NULL CHECK (kind IN ('vat_input', 'vat_output', 'wht_receivable', 'wht_payable', 'exempt')),
  rate numeric(9,6) NOT NULL CHECK (rate >= 0 AND rate <= 1),
  recoverable_rate numeric(9,6) NOT NULL DEFAULT 1 CHECK (recoverable_rate >= 0 AND recoverable_rate <= 1),
  account_code text,
  active boolean NOT NULL DEFAULT true
);

CREATE TABLE IF NOT EXISTS finance.document_sequences (
  kind text NOT NULL,
  branch_id bigint NOT NULL REFERENCES finance.branches(id),
  fiscal_year integer NOT NULL,
  last_number bigint NOT NULL DEFAULT 0,
  PRIMARY KEY (kind, branch_id, fiscal_year)
);

CREATE TABLE IF NOT EXISTS finance.accounts (
  code text PRIMARY KEY,
  name text NOT NULL,
  name_th text,
  name_de text,
  account_type text NOT NULL CHECK (account_type IN ('asset', 'liability', 'equity', 'revenue', 'expense')),
  normal_side text NOT NULL CHECK (normal_side IN ('debit', 'credit')),
  control_type text CHECK (control_type IN ('bank', 'cash', 'ar', 'ap', 'inventory', 'tax', 'grni')),
  active boolean NOT NULL DEFAULT true,
  allow_manual_posting boolean NOT NULL DEFAULT true,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now()
);

ALTER TABLE finance.tax_codes
  DROP CONSTRAINT IF EXISTS tax_codes_account_code_fkey;
ALTER TABLE finance.tax_codes
  ADD CONSTRAINT tax_codes_account_code_fkey FOREIGN KEY (account_code) REFERENCES finance.accounts(code);

CREATE TABLE IF NOT EXISTS finance.journals (
  id bigserial PRIMARY KEY,
  journal_no text UNIQUE,
  status text NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'prepared', 'posted', 'void')),
  posting_date date NOT NULL,
  document_date date NOT NULL,
  description text NOT NULL,
  currency_code char(3) NOT NULL DEFAULT 'THB' REFERENCES finance.currencies(code),
  fx_rate numeric(20,10) NOT NULL DEFAULT 1 CHECK (fx_rate > 0),
  source_type text NOT NULL,
  source_id text NOT NULL,
  source_event_key text NOT NULL UNIQUE,
  branch_id bigint NOT NULL REFERENCES finance.branches(id),
  waybill_id text,
  preparer_id integer,
  prepared_at timestamptz,
  approver_id integer,
  approved_at timestamptz,
  posted_at timestamptz,
  voided_by integer,
  voided_at timestamptz,
  reversal_of_id bigint REFERENCES finance.journals(id),
  attachment_keys text[] NOT NULL DEFAULT '{}',
  metadata jsonb NOT NULL DEFAULT '{}',
  created_by integer NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now(),
  UNIQUE (reversal_of_id)
);

CREATE TABLE IF NOT EXISTS finance.journal_lines (
  id bigserial PRIMARY KEY,
  journal_id bigint NOT NULL REFERENCES finance.journals(id) ON DELETE CASCADE,
  line_no integer NOT NULL CHECK (line_no > 0),
  account_code text NOT NULL REFERENCES finance.accounts(code),
  description text NOT NULL,
  debit_thb numeric(18,2) NOT NULL DEFAULT 0,
  credit_thb numeric(18,2) NOT NULL DEFAULT 0,
  foreign_amount numeric(18,2),
  currency_code char(3) REFERENCES finance.currencies(code),
  branch_id bigint NOT NULL REFERENCES finance.branches(id),
  department_id text,
  customer_id integer,
  vendor_id bigint,
  employee_id integer,
  product_id bigint,
  warehouse_id bigint,
  waybill_id text,
  source_document_type text,
  source_document_id text,
  metadata jsonb NOT NULL DEFAULT '{}',
  UNIQUE (journal_id, line_no),
  CHECK ((debit_thb > 0 AND credit_thb = 0) OR (credit_thb > 0 AND debit_thb = 0)),
  CHECK (debit_thb >= 0 AND credit_thb >= 0),
  CHECK ((foreign_amount IS NULL AND currency_code IS NULL) OR (foreign_amount IS NOT NULL AND currency_code IS NOT NULL))
);

CREATE TABLE IF NOT EXISTS finance.journal_events (
  id bigserial PRIMARY KEY,
  journal_id bigint NOT NULL REFERENCES finance.journals(id),
  action text NOT NULL CHECK (action IN ('created', 'prepared', 'posted', 'voided')),
  from_status text,
  to_status text NOT NULL,
  actor_id integer,
  payload jsonb NOT NULL DEFAULT '{}',
  occurred_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS finance_journals_posting_idx ON finance.journals (posting_date, branch_id) WHERE status = 'posted';
CREATE INDEX IF NOT EXISTS finance_journals_source_idx ON finance.journals (source_type, source_id);
CREATE INDEX IF NOT EXISTS finance_journal_lines_account_idx ON finance.journal_lines (account_code, branch_id);
CREATE INDEX IF NOT EXISTS finance_journal_lines_dimensions_idx ON finance.journal_lines (customer_id, vendor_id, product_id, warehouse_id);

CREATE TABLE IF NOT EXISTS finance.vendors (
  id bigserial PRIMARY KEY,
  code text NOT NULL UNIQUE,
  name text NOT NULL,
  tax_id text,
  billing_address text,
  payment_terms_days integer NOT NULL DEFAULT 30 CHECK (payment_terms_days >= 0),
  currency_code char(3) NOT NULL DEFAULT 'THB' REFERENCES finance.currencies(code),
  payable_account_code text NOT NULL DEFAULT '210100' REFERENCES finance.accounts(code),
  wht_tax_code text REFERENCES finance.tax_codes(code),
  active boolean NOT NULL DEFAULT true,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS finance.commercial_documents (
  id bigserial PRIMARY KEY,
  document_type text NOT NULL CHECK (document_type IN ('invoice', 'credit_note', 'debit_note', 'receipt', 'refund', 'wht_certificate')),
  document_no text NOT NULL UNIQUE,
  branch_id bigint NOT NULL REFERENCES finance.branches(id),
  customer_id integer,
  vendor_id bigint REFERENCES finance.vendors(id),
  source_type text NOT NULL,
  source_id text NOT NULL,
  issue_date date NOT NULL,
  currency_code char(3) NOT NULL REFERENCES finance.currencies(code),
  fx_rate numeric(20,10) NOT NULL CHECK (fx_rate > 0),
  subtotal numeric(18,2) NOT NULL,
  tax_amount numeric(18,2) NOT NULL DEFAULT 0,
  total_amount numeric(18,2) NOT NULL,
  status text NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'issued', 'void')),
  issued_by integer,
  issued_at timestamptz,
  journal_id bigint REFERENCES finance.journals(id),
  payload jsonb NOT NULL DEFAULT '{}',
  created_at timestamptz NOT NULL DEFAULT now(),
  CHECK (subtotal + tax_amount = total_amount)
);

CREATE TABLE IF NOT EXISTS finance.ar_documents (
  id bigserial PRIMARY KEY,
  document_id bigint UNIQUE REFERENCES finance.commercial_documents(id),
  customer_id integer NOT NULL,
  branch_id bigint NOT NULL REFERENCES finance.branches(id),
  document_no text NOT NULL UNIQUE,
  document_type text NOT NULL CHECK (document_type IN ('invoice', 'credit_note')),
  document_date date NOT NULL,
  due_date date NOT NULL,
  currency_code char(3) NOT NULL REFERENCES finance.currencies(code),
  fx_rate numeric(20,10) NOT NULL CHECK (fx_rate > 0),
  original_foreign numeric(18,2) NOT NULL,
  open_foreign numeric(18,2) NOT NULL,
  original_thb numeric(18,2) NOT NULL,
  open_thb numeric(18,2) NOT NULL,
  status text NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'partially_paid', 'paid', 'void')),
  journal_id bigint NOT NULL REFERENCES finance.journals(id),
  created_at timestamptz NOT NULL DEFAULT now(),
  CHECK (abs(open_foreign) <= abs(original_foreign)),
  CHECK (abs(open_thb) <= abs(original_thb))
);

CREATE TABLE IF NOT EXISTS finance.ar_allocations (
  id bigserial PRIMARY KEY,
  ar_document_id bigint NOT NULL REFERENCES finance.ar_documents(id),
  receipt_document_id bigint REFERENCES finance.commercial_documents(id),
  allocation_date date NOT NULL,
  foreign_amount numeric(18,2) NOT NULL CHECK (foreign_amount > 0),
  functional_amount numeric(18,2) NOT NULL CHECK (functional_amount > 0),
  wht_amount_thb numeric(18,2) NOT NULL DEFAULT 0 CHECK (wht_amount_thb >= 0),
  realized_fx_thb numeric(18,2) NOT NULL DEFAULT 0,
  journal_id bigint NOT NULL REFERENCES finance.journals(id),
  allocated_by integer NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS finance.ap_documents (
  id bigserial PRIMARY KEY,
  vendor_id bigint REFERENCES finance.vendors(id),
  employee_id integer,
  branch_id bigint NOT NULL REFERENCES finance.branches(id),
  document_no text NOT NULL,
  document_type text NOT NULL CHECK (document_type IN ('vendor_invoice', 'employee_expense', 'vendor_credit')),
  source_type text NOT NULL,
  source_id text NOT NULL,
  document_date date NOT NULL,
  due_date date NOT NULL,
  currency_code char(3) NOT NULL REFERENCES finance.currencies(code),
  fx_rate numeric(20,10) NOT NULL CHECK (fx_rate > 0),
  original_foreign numeric(18,2) NOT NULL,
  open_foreign numeric(18,2) NOT NULL,
  original_thb numeric(18,2) NOT NULL,
  open_thb numeric(18,2) NOT NULL,
  status text NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'partially_paid', 'paid', 'void')),
  journal_id bigint NOT NULL REFERENCES finance.journals(id),
  created_at timestamptz NOT NULL DEFAULT now(),
  CHECK ((vendor_id IS NOT NULL) <> (employee_id IS NOT NULL)),
  UNIQUE (vendor_id, document_no),
  CHECK (abs(open_foreign) <= abs(original_foreign)),
  CHECK (abs(open_thb) <= abs(original_thb))
);

CREATE TABLE IF NOT EXISTS finance.ap_allocations (
  id bigserial PRIMARY KEY,
  ap_document_id bigint NOT NULL REFERENCES finance.ap_documents(id),
  payment_document_id bigint REFERENCES finance.commercial_documents(id),
  allocation_date date NOT NULL,
  foreign_amount numeric(18,2) NOT NULL CHECK (foreign_amount > 0),
  functional_amount numeric(18,2) NOT NULL CHECK (functional_amount > 0),
  wht_amount_thb numeric(18,2) NOT NULL DEFAULT 0 CHECK (wht_amount_thb >= 0),
  realized_fx_thb numeric(18,2) NOT NULL DEFAULT 0,
  journal_id bigint NOT NULL REFERENCES finance.journals(id),
  allocated_by integer NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS finance.bank_accounts (
  id bigserial PRIMARY KEY,
  branch_id bigint NOT NULL REFERENCES finance.branches(id),
  code text NOT NULL UNIQUE,
  bank_name text NOT NULL,
  account_name text NOT NULL,
  account_number_masked text NOT NULL,
  currency_code char(3) NOT NULL REFERENCES finance.currencies(code),
  gl_account_code text NOT NULL REFERENCES finance.accounts(code),
  active boolean NOT NULL DEFAULT true
);

CREATE TABLE IF NOT EXISTS finance.bank_import_templates (
  id bigserial PRIMARY KEY,
  bank_account_id bigint NOT NULL REFERENCES finance.bank_accounts(id),
  name text NOT NULL,
  mapping jsonb NOT NULL,
  created_by integer NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now(),
  UNIQUE (bank_account_id, name)
);

CREATE TABLE IF NOT EXISTS finance.bank_imports (
  id bigserial PRIMARY KEY,
  bank_account_id bigint NOT NULL REFERENCES finance.bank_accounts(id),
  file_name text NOT NULL,
  file_hash text NOT NULL UNIQUE,
  template_id bigint REFERENCES finance.bank_import_templates(id),
  row_count integer NOT NULL CHECK (row_count >= 0),
  status text NOT NULL DEFAULT 'committed' CHECK (status IN ('committed', 'reopened')),
  imported_by integer NOT NULL,
  imported_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS finance.bank_transactions (
  id bigserial PRIMARY KEY,
  import_id bigint NOT NULL REFERENCES finance.bank_imports(id),
  bank_account_id bigint NOT NULL REFERENCES finance.bank_accounts(id),
  row_no integer NOT NULL,
  row_fingerprint text NOT NULL,
  transaction_date date NOT NULL,
  value_date date,
  description text NOT NULL,
  reference text,
  currency_code char(3) NOT NULL REFERENCES finance.currencies(code),
  amount numeric(18,2) NOT NULL CHECK (amount <> 0),
  balance numeric(18,2),
  status text NOT NULL DEFAULT 'unmatched' CHECK (status IN ('unmatched', 'matched', 'reopened')),
  raw jsonb NOT NULL,
  UNIQUE (bank_account_id, row_fingerprint),
  UNIQUE (import_id, row_no)
);

CREATE TABLE IF NOT EXISTS finance.bank_match_groups (
  id bigserial PRIMARY KEY,
  bank_account_id bigint NOT NULL REFERENCES finance.bank_accounts(id),
  status text NOT NULL DEFAULT 'confirmed' CHECK (status IN ('confirmed', 'reopened')),
  difference_thb numeric(18,2) NOT NULL DEFAULT 0,
  fee_thb numeric(18,2) NOT NULL DEFAULT 0 CHECK (fee_thb >= 0),
  fx_difference_thb numeric(18,2) NOT NULL DEFAULT 0,
  journal_id bigint REFERENCES finance.journals(id),
  confirmed_by integer NOT NULL,
  confirmed_at timestamptz NOT NULL DEFAULT now(),
  reopened_by integer,
  reopened_at timestamptz,
  reopen_reason text
);

CREATE TABLE IF NOT EXISTS finance.bank_match_lines (
  id bigserial PRIMARY KEY,
  group_id bigint NOT NULL REFERENCES finance.bank_match_groups(id),
  bank_transaction_id bigint REFERENCES finance.bank_transactions(id),
  ar_document_id bigint REFERENCES finance.ar_documents(id),
  ap_document_id bigint REFERENCES finance.ap_documents(id),
  journal_id bigint REFERENCES finance.journals(id),
  amount_thb numeric(18,2) NOT NULL CHECK (amount_thb > 0),
  CHECK (num_nonnulls(bank_transaction_id, ar_document_id, ap_document_id, journal_id) = 1)
);

CREATE TABLE IF NOT EXISTS finance.bank_reconciliation_audit (
  id bigserial PRIMARY KEY,
  match_group_id bigint NOT NULL REFERENCES finance.bank_match_groups(id),
  action text NOT NULL CHECK (action IN ('confirmed', 'reopened')),
  actor_id integer NOT NULL,
  reason text,
  payload jsonb NOT NULL DEFAULT '{}',
  occurred_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS finance.budgets (
  id bigserial PRIMARY KEY,
  name text NOT NULL,
  fiscal_year integer NOT NULL,
  branch_id bigint REFERENCES finance.branches(id),
  status text NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'approved', 'archived')),
  approved_by integer,
  approved_at timestamptz,
  created_by integer NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now(),
  UNIQUE (name, fiscal_year, branch_id)
);

CREATE TABLE IF NOT EXISTS finance.budget_lines (
  id bigserial PRIMARY KEY,
  budget_id bigint NOT NULL REFERENCES finance.budgets(id) ON DELETE CASCADE,
  period_no smallint NOT NULL CHECK (period_no BETWEEN 1 AND 12),
  account_code text NOT NULL REFERENCES finance.accounts(code),
  department_id text,
  amount_thb numeric(18,2) NOT NULL,
  UNIQUE NULLS NOT DISTINCT (budget_id, period_no, account_code, department_id)
);

CREATE TABLE IF NOT EXISTS finance.closing_checklists (
  id bigserial PRIMARY KEY,
  period_id bigint NOT NULL REFERENCES finance.fiscal_periods(id),
  task_key text NOT NULL,
  label text NOT NULL,
  status text NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'done', 'waived')),
  completed_by integer,
  completed_at timestamptz,
  evidence jsonb NOT NULL DEFAULT '{}',
  UNIQUE (period_id, task_key)
);

CREATE TABLE IF NOT EXISTS inventory.units (
  code text PRIMARY KEY,
  name text NOT NULL,
  decimals smallint NOT NULL DEFAULT 3 CHECK (decimals BETWEEN 0 AND 6)
);

CREATE TABLE IF NOT EXISTS inventory.unit_conversions (
  from_unit text NOT NULL REFERENCES inventory.units(code),
  to_unit text NOT NULL REFERENCES inventory.units(code),
  factor numeric(20,8) NOT NULL CHECK (factor > 0),
  PRIMARY KEY (from_unit, to_unit),
  CHECK (from_unit <> to_unit)
);

CREATE TABLE IF NOT EXISTS inventory.categories (
  id bigserial PRIMARY KEY,
  code text NOT NULL UNIQUE,
  name text NOT NULL,
  parent_id bigint REFERENCES inventory.categories(id)
);

CREATE TABLE IF NOT EXISTS inventory.products (
  id bigserial PRIMARY KEY,
  sku text NOT NULL UNIQUE,
  name text NOT NULL,
  name_th text,
  category_id bigint REFERENCES inventory.categories(id),
  base_unit text NOT NULL REFERENCES inventory.units(code),
  lot_tracked boolean NOT NULL DEFAULT false,
  expiry_tracked boolean NOT NULL DEFAULT false,
  inventory_account_code text NOT NULL DEFAULT '120200' REFERENCES finance.accounts(code),
  revenue_account_code text NOT NULL DEFAULT '410100' REFERENCES finance.accounts(code),
  cogs_account_code text NOT NULL DEFAULT '510100' REFERENCES finance.accounts(code),
  active boolean NOT NULL DEFAULT true,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now(),
  CHECK (NOT expiry_tracked OR lot_tracked)
);

CREATE TABLE IF NOT EXISTS inventory.warehouses (
  id bigserial PRIMARY KEY,
  branch_id bigint NOT NULL REFERENCES finance.branches(id),
  code text NOT NULL UNIQUE,
  name text NOT NULL,
  active boolean NOT NULL DEFAULT true,
  created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS inventory.bins (
  id bigserial PRIMARY KEY,
  warehouse_id bigint NOT NULL REFERENCES inventory.warehouses(id) ON DELETE CASCADE,
  code text NOT NULL,
  name text,
  active boolean NOT NULL DEFAULT true,
  UNIQUE (warehouse_id, code)
);

CREATE TABLE IF NOT EXISTS inventory.lots (
  id bigserial PRIMARY KEY,
  product_id bigint NOT NULL REFERENCES inventory.products(id),
  lot_no text NOT NULL,
  manufactured_on date,
  expires_on date,
  vendor_id bigint REFERENCES finance.vendors(id),
  created_at timestamptz NOT NULL DEFAULT now(),
  UNIQUE (product_id, lot_no),
  CHECK (manufactured_on IS NULL OR expires_on IS NULL OR manufactured_on <= expires_on)
);

CREATE TABLE IF NOT EXISTS inventory.stock_movements (
  id bigserial PRIMARY KEY,
  movement_no text UNIQUE,
  kind text NOT NULL CHECK (kind IN ('receipt', 'shipment', 'customer_return', 'vendor_return', 'transfer', 'adjustment', 'count', 'write_down', 'write_down_reversal', 'recost')),
  status text NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'posted', 'reversed')),
  movement_date date NOT NULL,
  source_type text NOT NULL,
  source_id text NOT NULL,
  source_event_key text NOT NULL UNIQUE,
  branch_id bigint NOT NULL REFERENCES finance.branches(id),
  journal_id bigint REFERENCES finance.journals(id),
  reversal_of_id bigint REFERENCES inventory.stock_movements(id),
  posted_by integer,
  posted_at timestamptz,
  metadata jsonb NOT NULL DEFAULT '{}',
  created_by integer NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS inventory.stock_movement_lines (
  id bigserial PRIMARY KEY,
  movement_id bigint NOT NULL REFERENCES inventory.stock_movements(id) ON DELETE CASCADE,
  line_no integer NOT NULL CHECK (line_no > 0),
  product_id bigint NOT NULL REFERENCES inventory.products(id),
  quantity numeric(20,6) NOT NULL CHECK (quantity > 0),
  unit_code text NOT NULL REFERENCES inventory.units(code),
  unit_cost_thb numeric(20,6) CHECK (unit_cost_thb >= 0),
  extended_cost_thb numeric(18,2) CHECK (extended_cost_thb >= 0),
  from_warehouse_id bigint REFERENCES inventory.warehouses(id),
  from_bin_id bigint REFERENCES inventory.bins(id),
  to_warehouse_id bigint REFERENCES inventory.warehouses(id),
  to_bin_id bigint REFERENCES inventory.bins(id),
  lot_id bigint REFERENCES inventory.lots(id),
  source_line_id text,
  metadata jsonb NOT NULL DEFAULT '{}',
  UNIQUE (movement_id, line_no),
  CHECK (from_warehouse_id IS NOT NULL OR to_warehouse_id IS NOT NULL)
);

CREATE TABLE IF NOT EXISTS inventory.stock_balances (
  product_id bigint NOT NULL REFERENCES inventory.products(id),
  warehouse_id bigint NOT NULL REFERENCES inventory.warehouses(id),
  bin_id bigint REFERENCES inventory.bins(id),
  lot_id bigint REFERENCES inventory.lots(id),
  quantity numeric(20,6) NOT NULL DEFAULT 0 CHECK (quantity >= 0),
  avg_cost_thb numeric(20,6) NOT NULL DEFAULT 0 CHECK (avg_cost_thb >= 0),
  updated_at timestamptz NOT NULL DEFAULT now(),
  UNIQUE NULLS NOT DISTINCT (product_id, warehouse_id, bin_id, lot_id)
);

CREATE TABLE IF NOT EXISTS inventory.costing_events (
  id bigserial PRIMARY KEY,
  movement_line_id bigint NOT NULL REFERENCES inventory.stock_movement_lines(id),
  product_id bigint NOT NULL REFERENCES inventory.products(id),
  warehouse_id bigint NOT NULL REFERENCES inventory.warehouses(id),
  event_date date NOT NULL,
  quantity_delta numeric(20,6) NOT NULL,
  unit_cost_before numeric(20,6) NOT NULL,
  unit_cost_after numeric(20,6) NOT NULL,
  value_delta_thb numeric(18,2) NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS inventory.reservations (
  id bigserial PRIMARY KEY,
  product_id bigint NOT NULL REFERENCES inventory.products(id),
  warehouse_id bigint NOT NULL REFERENCES inventory.warehouses(id),
  lot_id bigint REFERENCES inventory.lots(id),
  source_type text NOT NULL,
  source_id text NOT NULL,
  quantity numeric(20,6) NOT NULL CHECK (quantity > 0),
  status text NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'released', 'fulfilled')),
  expires_at timestamptz,
  created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS inventory.reorder_settings (
  product_id bigint NOT NULL REFERENCES inventory.products(id),
  warehouse_id bigint NOT NULL REFERENCES inventory.warehouses(id),
  reorder_point numeric(20,6) NOT NULL DEFAULT 0 CHECK (reorder_point >= 0),
  reorder_quantity numeric(20,6) NOT NULL DEFAULT 0 CHECK (reorder_quantity >= 0),
  lead_time_days integer NOT NULL DEFAULT 0 CHECK (lead_time_days >= 0),
  PRIMARY KEY (product_id, warehouse_id)
);

CREATE TABLE IF NOT EXISTS inventory.stock_counts (
  id bigserial PRIMARY KEY,
  count_no text NOT NULL UNIQUE,
  warehouse_id bigint NOT NULL REFERENCES inventory.warehouses(id),
  count_date date NOT NULL,
  status text NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'counted', 'posted', 'void')),
  created_by integer NOT NULL,
  posted_by integer,
  movement_id bigint REFERENCES inventory.stock_movements(id),
  created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS inventory.stock_count_lines (
  id bigserial PRIMARY KEY,
  count_id bigint NOT NULL REFERENCES inventory.stock_counts(id) ON DELETE CASCADE,
  product_id bigint NOT NULL REFERENCES inventory.products(id),
  bin_id bigint REFERENCES inventory.bins(id),
  lot_id bigint REFERENCES inventory.lots(id),
  system_quantity numeric(20,6) NOT NULL,
  counted_quantity numeric(20,6),
  UNIQUE NULLS NOT DISTINCT (count_id, product_id, bin_id, lot_id)
);

CREATE TABLE IF NOT EXISTS inventory.purchase_receipts (
  id bigserial PRIMARY KEY,
  receipt_no text NOT NULL UNIQUE,
  po_id integer NOT NULL,
  vendor_id bigint REFERENCES finance.vendors(id),
  warehouse_id bigint NOT NULL REFERENCES inventory.warehouses(id),
  receipt_date date NOT NULL,
  status text NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'posted', 'returned')),
  movement_id bigint REFERENCES inventory.stock_movements(id),
  received_by integer NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS inventory.purchase_receipt_lines (
  id bigserial PRIMARY KEY,
  receipt_id bigint NOT NULL REFERENCES inventory.purchase_receipts(id) ON DELETE CASCADE,
  po_line_id integer NOT NULL,
  product_id bigint NOT NULL REFERENCES inventory.products(id),
  quantity numeric(20,6) NOT NULL CHECK (quantity > 0),
  unit_cost_thb numeric(20,6) NOT NULL CHECK (unit_cost_thb >= 0),
  lot_id bigint REFERENCES inventory.lots(id),
  UNIQUE (receipt_id, po_line_id, lot_id)
);

CREATE TABLE IF NOT EXISTS inventory.recost_jobs (
  id bigserial PRIMARY KEY,
  product_id bigint NOT NULL REFERENCES inventory.products(id),
  warehouse_id bigint NOT NULL REFERENCES inventory.warehouses(id),
  from_date date NOT NULL,
  status text NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'running', 'completed', 'failed')),
  adjustment_journal_id bigint REFERENCES finance.journals(id),
  requested_by integer NOT NULL,
  requested_at timestamptz NOT NULL DEFAULT now(),
  completed_at timestamptz,
  error text
);

CREATE TABLE IF NOT EXISTS inventory.landed_cost_allocations (
  id bigserial PRIMARY KEY,
  ap_document_id bigint NOT NULL REFERENCES finance.ap_documents(id),
  receipt_line_id bigint NOT NULL REFERENCES inventory.purchase_receipt_lines(id),
  allocation_basis text NOT NULL CHECK (allocation_basis IN ('quantity', 'value', 'weight', 'manual')),
  amount_thb numeric(18,2) NOT NULL CHECK (amount_thb > 0),
  journal_id bigint REFERENCES finance.journals(id),
  created_at timestamptz NOT NULL DEFAULT now(),
  UNIQUE (ap_document_id, receipt_line_id)
);

CREATE TABLE IF NOT EXISTS inventory.sales_shipments (
  id bigserial PRIMARY KEY,
  shipment_no text NOT NULL UNIQUE,
  sales_order_id integer NOT NULL,
  warehouse_id bigint NOT NULL REFERENCES inventory.warehouses(id),
  shipment_date date NOT NULL,
  status text NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'posted', 'returned')),
  movement_id bigint REFERENCES inventory.stock_movements(id),
  shipped_by integer NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX IF NOT EXISTS inventory_sales_shipments_movement_unique
  ON inventory.sales_shipments(movement_id) WHERE movement_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS inventory.sales_shipment_lines (
  id bigserial PRIMARY KEY,
  shipment_id bigint NOT NULL REFERENCES inventory.sales_shipments(id) ON DELETE CASCADE,
  sales_order_line_id integer NOT NULL,
  product_id bigint NOT NULL REFERENCES inventory.products(id),
  quantity numeric(20,6) NOT NULL CHECK (quantity > 0),
  lot_id bigint REFERENCES inventory.lots(id),
  UNIQUE (shipment_id, sales_order_line_id, lot_id)
);

ALTER TABLE folio.expenses ADD COLUMN IF NOT EXISTS currency_code char(3) NOT NULL DEFAULT 'THB';
ALTER TABLE folio.expenses ADD COLUMN IF NOT EXISTS fx_rate numeric(20,10) NOT NULL DEFAULT 1;
ALTER TABLE folio.expenses ADD COLUMN IF NOT EXISTS branch_id bigint;
ALTER TABLE folio.expenses ADD COLUMN IF NOT EXISTS department_id text;
ALTER TABLE folio.expenses ADD COLUMN IF NOT EXISTS tax_code text;
ALTER TABLE folio.expenses ADD COLUMN IF NOT EXISTS vendor_id bigint;
ALTER TABLE folio.expenses ADD COLUMN IF NOT EXISTS invoice_number text;
ALTER TABLE folio.expenses ADD COLUMN IF NOT EXISTS accounting_reviewed_at timestamptz;
ALTER TABLE folio.sales_orders ADD COLUMN IF NOT EXISTS branch_id bigint;
ALTER TABLE folio.sales_orders ADD COLUMN IF NOT EXISTS fx_rate numeric(20,10) NOT NULL DEFAULT 1;
ALTER TABLE folio.so_items ADD COLUMN IF NOT EXISTS product_id bigint;
ALTER TABLE folio.so_items ADD COLUMN IF NOT EXISTS reserved_qty numeric(20,6) NOT NULL DEFAULT 0;
ALTER TABLE folio.so_items ADD COLUMN IF NOT EXISTS shipped_qty numeric(20,6) NOT NULL DEFAULT 0;
ALTER TABLE folio.so_items ADD COLUMN IF NOT EXISTS invoiced_qty numeric(20,6) NOT NULL DEFAULT 0;
ALTER TABLE folio.so_items ADD COLUMN IF NOT EXISTS returned_qty numeric(20,6) NOT NULL DEFAULT 0;
ALTER TABLE folio.purchase_orders ADD COLUMN IF NOT EXISTS branch_id bigint;
ALTER TABLE folio.purchase_orders ADD COLUMN IF NOT EXISTS fx_rate numeric(20,10) NOT NULL DEFAULT 1;
ALTER TABLE folio.purchase_orders ADD COLUMN IF NOT EXISTS vendor_id bigint;
ALTER TABLE folio.po_items ADD COLUMN IF NOT EXISTS product_id bigint;
ALTER TABLE folio.po_items ADD COLUMN IF NOT EXISTS unit_code text;
ALTER TABLE folio.po_items ADD COLUMN IF NOT EXISTS received_qty numeric(20,6) NOT NULL DEFAULT 0;
ALTER TABLE folio.po_items ADD COLUMN IF NOT EXISTS invoiced_qty numeric(20,6) NOT NULL DEFAULT 0;
ALTER TABLE folio.po_items ADD COLUMN IF NOT EXISTS returned_qty numeric(20,6) NOT NULL DEFAULT 0;
ALTER TABLE folio.po_items ADD COLUMN IF NOT EXISTS variance_override_by integer;
ALTER TABLE folio.po_items ADD COLUMN IF NOT EXISTS variance_override_reason text;

UPDATE folio.expenses SET branch_id = (SELECT id FROM finance.branches WHERE code = 'HQ') WHERE branch_id IS NULL;
UPDATE folio.sales_orders SET branch_id = (SELECT id FROM finance.branches WHERE code = 'HQ') WHERE branch_id IS NULL;
UPDATE folio.purchase_orders SET branch_id = (SELECT id FROM finance.branches WHERE code = 'HQ') WHERE branch_id IS NULL;
UPDATE folio.po_items SET unit_code = 'EA' WHERE unit_code IS NULL;

ALTER TABLE finance.journals
  DROP CONSTRAINT IF EXISTS finance_journals_waybill_fkey,
  ADD CONSTRAINT finance_journals_waybill_fkey FOREIGN KEY (waybill_id) REFERENCES folio.waybills(id);
ALTER TABLE finance.journal_lines
  DROP CONSTRAINT IF EXISTS finance_journal_lines_department_fkey,
  DROP CONSTRAINT IF EXISTS finance_journal_lines_customer_fkey,
  DROP CONSTRAINT IF EXISTS finance_journal_lines_vendor_fkey,
  DROP CONSTRAINT IF EXISTS finance_journal_lines_employee_fkey,
  DROP CONSTRAINT IF EXISTS finance_journal_lines_product_fkey,
  DROP CONSTRAINT IF EXISTS finance_journal_lines_warehouse_fkey,
  DROP CONSTRAINT IF EXISTS finance_journal_lines_waybill_fkey,
  ADD CONSTRAINT finance_journal_lines_department_fkey FOREIGN KEY (department_id) REFERENCES perm.departments(id),
  ADD CONSTRAINT finance_journal_lines_customer_fkey FOREIGN KEY (customer_id) REFERENCES folio.customers(id),
  ADD CONSTRAINT finance_journal_lines_vendor_fkey FOREIGN KEY (vendor_id) REFERENCES finance.vendors(id),
  ADD CONSTRAINT finance_journal_lines_employee_fkey FOREIGN KEY (employee_id) REFERENCES folio.users(id),
  ADD CONSTRAINT finance_journal_lines_product_fkey FOREIGN KEY (product_id) REFERENCES inventory.products(id),
  ADD CONSTRAINT finance_journal_lines_warehouse_fkey FOREIGN KEY (warehouse_id) REFERENCES inventory.warehouses(id),
  ADD CONSTRAINT finance_journal_lines_waybill_fkey FOREIGN KEY (waybill_id) REFERENCES folio.waybills(id);
ALTER TABLE finance.commercial_documents
  DROP CONSTRAINT IF EXISTS finance_commercial_documents_customer_fkey,
  ADD CONSTRAINT finance_commercial_documents_customer_fkey FOREIGN KEY (customer_id) REFERENCES folio.customers(id);
ALTER TABLE finance.ar_documents
  DROP CONSTRAINT IF EXISTS finance_ar_documents_customer_fkey,
  ADD CONSTRAINT finance_ar_documents_customer_fkey FOREIGN KEY (customer_id) REFERENCES folio.customers(id);
ALTER TABLE finance.ap_documents
  DROP CONSTRAINT IF EXISTS finance_ap_documents_employee_fkey,
  ADD CONSTRAINT finance_ap_documents_employee_fkey FOREIGN KEY (employee_id) REFERENCES folio.users(id);

CREATE UNIQUE INDEX IF NOT EXISTS folio_expense_vendor_invoice_unique
  ON folio.expenses(vendor_id, invoice_number)
  WHERE vendor_id IS NOT NULL AND invoice_number IS NOT NULL AND status::text <> 'rejected';

CREATE OR REPLACE FUNCTION finance.next_document_number(p_kind text, p_branch_id bigint, p_date date)
RETURNS text
LANGUAGE plpgsql
AS $fn$
DECLARE
  n bigint;
  branch_code text;
  fy integer := extract(year from p_date)::integer;
BEGIN
  SELECT code INTO branch_code FROM finance.branches WHERE id = p_branch_id AND active;
  IF branch_code IS NULL THEN
    RAISE EXCEPTION 'Invalid or inactive branch %', p_branch_id;
  END IF;
  INSERT INTO finance.document_sequences(kind, branch_id, fiscal_year, last_number)
  VALUES (p_kind, p_branch_id, fy, 1)
  ON CONFLICT (kind, branch_id, fiscal_year)
  DO UPDATE SET last_number = finance.document_sequences.last_number + 1
  RETURNING last_number INTO n;
  RETURN upper(p_kind) || '-' || branch_code || '-' || fy || '-' || lpad(n::text, 6, '0');
END
$fn$;

CREATE OR REPLACE FUNCTION finance.assert_journal_balanced(p_journal_id bigint)
RETURNS void
LANGUAGE plpgsql
AS $fn$
DECLARE
  d numeric(18,2);
  c numeric(18,2);
  n integer;
  invalid_accounts integer;
BEGIN
  SELECT count(*), coalesce(sum(debit_thb), 0), coalesce(sum(credit_thb), 0)
    INTO n, d, c
    FROM finance.journal_lines
   WHERE journal_id = p_journal_id;
  IF n < 2 THEN
    RAISE EXCEPTION 'Journal % must have at least two lines', p_journal_id;
  END IF;
  IF d <= 0 OR c <= 0 OR d <> c THEN
    RAISE EXCEPTION 'Journal % is not balanced: debit %, credit %', p_journal_id, d, c;
  END IF;
  SELECT count(*) INTO invalid_accounts
    FROM finance.journal_lines l
    JOIN finance.accounts a ON a.code = l.account_code
   WHERE l.journal_id = p_journal_id
     AND NOT a.active;
  IF invalid_accounts > 0 THEN
    RAISE EXCEPTION 'Journal % contains inactive accounts', p_journal_id;
  END IF;
END
$fn$;

CREATE OR REPLACE FUNCTION finance.post_journal(p_journal_id bigint, p_actor_id integer)
RETURNS finance.journals
LANGUAGE plpgsql
AS $fn$
DECLARE
  j finance.journals;
  p finance.fiscal_periods;
BEGIN
  SELECT * INTO j FROM finance.journals WHERE id = p_journal_id FOR UPDATE;
  IF j.id IS NULL THEN
    RAISE EXCEPTION 'Journal % not found', p_journal_id;
  END IF;
  IF j.status <> 'prepared' THEN
    RAISE EXCEPTION 'Only prepared journals can be posted';
  END IF;
  IF j.preparer_id IS NULL THEN
    RAISE EXCEPTION 'Journal has no preparer';
  END IF;
  SELECT * INTO p FROM finance.fiscal_periods
   WHERE j.posting_date BETWEEN starts_on AND ends_on
   FOR SHARE;
  IF p.id IS NULL OR p.status <> 'open' THEN
    RAISE EXCEPTION 'Posting date % is not in an open fiscal period', j.posting_date;
  END IF;
  PERFORM finance.assert_journal_balanced(j.id);
  UPDATE finance.journals
     SET status = 'posted',
         approver_id = p_actor_id,
         approved_at = now(),
         posted_at = now(),
         journal_no = coalesce(journal_no, finance.next_document_number('JV', branch_id, posting_date)),
         updated_at = now()
   WHERE id = j.id
   RETURNING * INTO j;
  RETURN j;
END
$fn$;

CREATE OR REPLACE FUNCTION finance.guard_posted_journal()
RETURNS trigger
LANGUAGE plpgsql
AS $fn$
BEGIN
  IF TG_OP = 'DELETE' AND OLD.status = 'posted' THEN
    RAISE EXCEPTION 'Posted journals cannot be deleted';
  END IF;
  IF TG_OP = 'UPDATE' AND OLD.status = 'posted' THEN
    RAISE EXCEPTION 'Posted journals are immutable; create a reversal';
  END IF;
  RETURN CASE WHEN TG_OP = 'DELETE' THEN OLD ELSE NEW END;
END
$fn$;

CREATE OR REPLACE FUNCTION finance.guard_posted_lines()
RETURNS trigger
LANGUAGE plpgsql
AS $fn$
DECLARE
  journal_status text;
  target_id bigint;
BEGIN
  target_id := CASE WHEN TG_OP = 'DELETE' THEN OLD.journal_id ELSE NEW.journal_id END;
  SELECT status INTO journal_status FROM finance.journals WHERE id = target_id;
  IF journal_status = 'posted' THEN
    RAISE EXCEPTION 'Lines of posted journals are immutable';
  END IF;
  RETURN CASE WHEN TG_OP = 'DELETE' THEN OLD ELSE NEW END;
END
$fn$;

CREATE OR REPLACE FUNCTION finance.audit_journal_status()
RETURNS trigger
LANGUAGE plpgsql
AS $fn$
DECLARE
  action_name text;
  actor integer;
BEGIN
  IF TG_OP = 'INSERT' THEN
    action_name := 'created';
    actor := NEW.created_by;
    INSERT INTO finance.journal_events(journal_id, action, from_status, to_status, actor_id)
    VALUES (NEW.id, action_name, NULL, NEW.status, actor);
  ELSIF OLD.status IS DISTINCT FROM NEW.status THEN
    action_name := CASE NEW.status WHEN 'prepared' THEN 'prepared' WHEN 'posted' THEN 'posted' WHEN 'void' THEN 'voided' ELSE 'created' END;
    actor := CASE NEW.status WHEN 'prepared' THEN NEW.preparer_id WHEN 'posted' THEN NEW.approver_id WHEN 'void' THEN NEW.voided_by ELSE NEW.created_by END;
    INSERT INTO finance.journal_events(journal_id, action, from_status, to_status, actor_id)
    VALUES (NEW.id, action_name, OLD.status, NEW.status, actor);
  END IF;
  RETURN NEW;
END
$fn$;

DROP TRIGGER IF EXISTS finance_journal_immutable ON finance.journals;
CREATE TRIGGER finance_journal_immutable
BEFORE UPDATE OR DELETE ON finance.journals
FOR EACH ROW EXECUTE FUNCTION finance.guard_posted_journal();

DROP TRIGGER IF EXISTS finance_journal_lines_immutable ON finance.journal_lines;
CREATE TRIGGER finance_journal_lines_immutable
BEFORE INSERT OR UPDATE OR DELETE ON finance.journal_lines
FOR EACH ROW EXECUTE FUNCTION finance.guard_posted_lines();

DROP TRIGGER IF EXISTS finance_journal_status_audit ON finance.journals;
CREATE TRIGGER finance_journal_status_audit
AFTER INSERT OR UPDATE ON finance.journals
FOR EACH ROW EXECUTE FUNCTION finance.audit_journal_status();

CREATE OR REPLACE FUNCTION finance.guard_issued_document()
RETURNS trigger
LANGUAGE plpgsql
AS $fn$
BEGIN
  IF OLD.status = 'issued' THEN
    RAISE EXCEPTION 'Issued commercial documents are immutable';
  END IF;
  RETURN CASE WHEN TG_OP = 'DELETE' THEN OLD ELSE NEW END;
END
$fn$;

DROP TRIGGER IF EXISTS finance_commercial_document_immutable ON finance.commercial_documents;
CREATE TRIGGER finance_commercial_document_immutable
BEFORE UPDATE OR DELETE ON finance.commercial_documents
FOR EACH ROW EXECUTE FUNCTION finance.guard_issued_document();

CREATE OR REPLACE FUNCTION inventory.guard_posted_movement()
RETURNS trigger
LANGUAGE plpgsql
AS $fn$
BEGIN
  IF OLD.status = 'posted' THEN
    RAISE EXCEPTION 'Posted stock movements are immutable';
  END IF;
  RETURN CASE WHEN TG_OP = 'DELETE' THEN OLD ELSE NEW END;
END
$fn$;

CREATE OR REPLACE FUNCTION inventory.guard_posted_movement_line()
RETURNS trigger
LANGUAGE plpgsql
AS $fn$
DECLARE
  movement_status text;
  target_id bigint;
BEGIN
  target_id := CASE WHEN TG_OP = 'DELETE' THEN OLD.movement_id ELSE NEW.movement_id END;
  SELECT status INTO movement_status FROM inventory.stock_movements WHERE id = target_id;
  IF movement_status = 'posted' THEN
    RAISE EXCEPTION 'Lines of posted stock movements are immutable';
  END IF;
  RETURN CASE WHEN TG_OP = 'DELETE' THEN OLD ELSE NEW END;
END
$fn$;

DROP TRIGGER IF EXISTS inventory_stock_movement_immutable ON inventory.stock_movements;
CREATE TRIGGER inventory_stock_movement_immutable
BEFORE UPDATE OR DELETE ON inventory.stock_movements
FOR EACH ROW EXECUTE FUNCTION inventory.guard_posted_movement();

DROP TRIGGER IF EXISTS inventory_stock_movement_lines_immutable ON inventory.stock_movement_lines;
CREATE TRIGGER inventory_stock_movement_lines_immutable
BEFORE INSERT OR UPDATE OR DELETE ON inventory.stock_movement_lines
FOR EACH ROW EXECUTE FUNCTION inventory.guard_posted_movement_line();

CREATE OR REPLACE VIEW finance.v_posted_lines AS
SELECT j.id AS journal_id,
       j.journal_no,
       j.posting_date,
       j.document_date,
       j.description AS journal_description,
       j.source_type,
       j.source_id,
       j.source_event_key,
       j.currency_code AS document_currency,
       j.fx_rate,
       j.branch_id AS journal_branch_id,
       l.id AS line_id,
       l.line_no,
       l.account_code,
       a.name AS account_name,
       a.name_th AS account_name_th,
       a.account_type,
       a.control_type,
       l.description,
       l.debit_thb,
       l.credit_thb,
       l.foreign_amount,
       l.currency_code,
       l.branch_id,
       l.department_id,
       l.customer_id,
       l.vendor_id,
       l.employee_id,
       l.product_id,
       l.warehouse_id,
       l.waybill_id,
       l.source_document_type,
       l.source_document_id
  FROM finance.journals j
  JOIN finance.journal_lines l ON l.journal_id = j.id
  JOIN finance.accounts a ON a.code = l.account_code
 WHERE j.status = 'posted';

DROP VIEW IF EXISTS finance.v_trial_balance CASCADE;
CREATE VIEW finance.v_trial_balance AS
SELECT a.code,
       a.name,
       a.name_th,
       a.account_type,
       coalesce(sum(l.debit_thb), 0)::numeric(18,2) AS period_debit,
       coalesce(sum(l.credit_thb), 0)::numeric(18,2) AS period_credit,
       (coalesce(sum(l.debit_thb), 0) - coalesce(sum(l.credit_thb), 0))::numeric(18,2) AS net
  FROM finance.accounts a
  LEFT JOIN finance.v_posted_lines l ON l.account_code = a.code
 GROUP BY a.code, a.name, a.name_th, a.account_type;

DROP VIEW IF EXISTS finance.v_income_statement;
CREATE VIEW finance.v_income_statement AS
SELECT code,
       name,
       name_th,
       account_type,
       CASE
         WHEN account_type = 'revenue' THEN period_credit - period_debit
         WHEN account_type = 'expense' THEN period_debit - period_credit
         ELSE 0
       END::numeric(18,2) AS amount
  FROM finance.v_trial_balance
 WHERE account_type IN ('revenue', 'expense');

DROP VIEW IF EXISTS finance.v_balance_sheet;
CREATE VIEW finance.v_balance_sheet AS
SELECT code,
       name,
       name_th,
       account_type,
       CASE
         WHEN account_type = 'asset' THEN period_debit - period_credit
         ELSE period_credit - period_debit
       END::numeric(18,2) AS balance
  FROM finance.v_trial_balance
 WHERE account_type IN ('asset', 'liability', 'equity');

DROP VIEW IF EXISTS finance.v_period_summary;
CREATE VIEW finance.v_period_summary AS
SELECT j.id AS journal_id,
       j.journal_no,
       j.posting_date,
       j.description,
       j.branch_id,
       j.source_type,
       j.source_id,
       sum(l.debit_thb)::numeric(18,2) AS total_debit,
       sum(l.credit_thb)::numeric(18,2) AS total_credit
  FROM finance.journals j
  JOIN finance.journal_lines l ON l.journal_id = j.id
 WHERE j.status = 'posted'
 GROUP BY j.id;

CREATE OR REPLACE FUNCTION finance.trial_balance(p_from date, p_to date, p_branch_id bigint DEFAULT NULL)
RETURNS TABLE(code text, name text, account_type text, debit numeric, credit numeric, balance numeric)
LANGUAGE sql
STABLE
AS $fn$
  SELECT a.code,
         a.name,
         a.account_type,
         coalesce(sum(l.debit_thb), 0)::numeric(18,2),
         coalesce(sum(l.credit_thb), 0)::numeric(18,2),
         CASE WHEN a.normal_side = 'debit'
              THEN (coalesce(sum(l.debit_thb), 0) - coalesce(sum(l.credit_thb), 0))::numeric(18,2)
              ELSE (coalesce(sum(l.credit_thb), 0) - coalesce(sum(l.debit_thb), 0))::numeric(18,2)
          END
    FROM finance.accounts a
    LEFT JOIN finance.v_posted_lines l
      ON l.account_code = a.code
     AND l.posting_date BETWEEN p_from AND p_to
     AND (p_branch_id IS NULL OR l.branch_id = p_branch_id)
   GROUP BY a.code, a.name, a.account_type, a.normal_side
   ORDER BY a.code
$fn$;

CREATE OR REPLACE FUNCTION finance.general_ledger(p_from date, p_to date, p_branch_id bigint DEFAULT NULL, p_account_code text DEFAULT NULL)
RETURNS SETOF finance.v_posted_lines
LANGUAGE sql
STABLE
AS $fn$
  SELECT * FROM finance.v_posted_lines
   WHERE posting_date BETWEEN p_from AND p_to
     AND (p_branch_id IS NULL OR branch_id = p_branch_id)
     AND (p_account_code IS NULL OR account_code = p_account_code)
   ORDER BY posting_date, journal_id, line_no
$fn$;

CREATE OR REPLACE FUNCTION finance.profit_and_loss(p_from date, p_to date, p_branch_id bigint DEFAULT NULL)
RETURNS TABLE(code text, name text, account_type text, amount numeric)
LANGUAGE sql
STABLE
AS $fn$
  SELECT a.code,
         a.name,
         a.account_type,
         CASE WHEN a.account_type = 'revenue'
              THEN (coalesce(sum(l.credit_thb), 0) - coalesce(sum(l.debit_thb), 0))::numeric(18,2)
              ELSE (coalesce(sum(l.debit_thb), 0) - coalesce(sum(l.credit_thb), 0))::numeric(18,2)
          END
    FROM finance.accounts a
    LEFT JOIN finance.v_posted_lines l
      ON l.account_code = a.code
     AND l.posting_date BETWEEN p_from AND p_to
     AND (p_branch_id IS NULL OR l.branch_id = p_branch_id)
   WHERE a.account_type IN ('revenue', 'expense')
   GROUP BY a.code, a.name, a.account_type
   ORDER BY a.code
$fn$;

CREATE OR REPLACE FUNCTION finance.balance_sheet(p_as_of date, p_branch_id bigint DEFAULT NULL)
RETURNS TABLE(code text, name text, account_type text, amount numeric)
LANGUAGE sql
STABLE
AS $fn$
  SELECT a.code,
         a.name,
         a.account_type,
         CASE WHEN a.account_type = 'asset'
              THEN (coalesce(sum(l.debit_thb), 0) - coalesce(sum(l.credit_thb), 0))::numeric(18,2)
              ELSE (coalesce(sum(l.credit_thb), 0) - coalesce(sum(l.debit_thb), 0))::numeric(18,2)
          END
    FROM finance.accounts a
    LEFT JOIN finance.v_posted_lines l
      ON l.account_code = a.code
     AND l.posting_date <= p_as_of
     AND (p_branch_id IS NULL OR l.branch_id = p_branch_id)
   WHERE a.account_type IN ('asset', 'liability', 'equity')
   GROUP BY a.code, a.name, a.account_type
   ORDER BY a.code
$fn$;

CREATE OR REPLACE VIEW finance.v_ar_aging AS
SELECT d.*,
       current_date - d.due_date AS days_past_due,
       CASE
         WHEN current_date <= d.due_date THEN 'current'
         WHEN current_date - d.due_date <= 30 THEN '1_30'
         WHEN current_date - d.due_date <= 60 THEN '31_60'
         WHEN current_date - d.due_date <= 90 THEN '61_90'
         ELSE 'over_90'
       END AS aging_bucket
  FROM finance.ar_documents d
 WHERE d.status IN ('open', 'partially_paid');

CREATE OR REPLACE VIEW finance.v_ap_aging AS
SELECT d.*,
       current_date - d.due_date AS days_past_due,
       CASE
         WHEN current_date <= d.due_date THEN 'current'
         WHEN current_date - d.due_date <= 30 THEN '1_30'
         WHEN current_date - d.due_date <= 60 THEN '31_60'
         WHEN current_date - d.due_date <= 90 THEN '61_90'
         ELSE 'over_90'
       END AS aging_bucket
  FROM finance.ap_documents d
 WHERE d.status IN ('open', 'partially_paid');

CREATE OR REPLACE VIEW finance.v_fx_exposure AS
SELECT 'AR'::text AS subledger,
       currency_code,
       sum(open_foreign)::numeric(18,2) AS open_foreign,
       sum(open_thb)::numeric(18,2) AS carrying_thb
  FROM finance.ar_documents
 WHERE status IN ('open', 'partially_paid') AND currency_code <> 'THB'
 GROUP BY currency_code
UNION ALL
SELECT 'AP', currency_code, sum(open_foreign)::numeric(18,2), sum(open_thb)::numeric(18,2)
  FROM finance.ap_documents
 WHERE status IN ('open', 'partially_paid') AND currency_code <> 'THB'
 GROUP BY currency_code;

CREATE OR REPLACE FUNCTION finance.ar_balance_as_of(p_as_of date, p_branch_id bigint DEFAULT NULL)
RETURNS TABLE(id bigint, branch_id bigint, customer_id integer, document_no text, document_type text, document_date date, due_date date, currency_code char(3), open_foreign numeric, open_thb numeric)
LANGUAGE sql
STABLE
AS $fn$
  SELECT d.id,
         d.branch_id,
         d.customer_id,
         d.document_no,
         d.document_type,
         d.document_date,
         d.due_date,
         d.currency_code,
         CASE WHEN d.document_type = 'credit_note'
              THEN d.original_foreign + coalesce(sum(a.foreign_amount) FILTER (WHERE a.allocation_date <= p_as_of), 0)
              ELSE d.original_foreign - coalesce(sum(a.foreign_amount) FILTER (WHERE a.allocation_date <= p_as_of), 0)
          END::numeric(18,2),
         CASE WHEN d.document_type = 'credit_note'
              THEN d.original_thb + coalesce(sum(a.functional_amount - a.realized_fx_thb) FILTER (WHERE a.allocation_date <= p_as_of), 0)
              ELSE d.original_thb - coalesce(sum(a.functional_amount - a.realized_fx_thb) FILTER (WHERE a.allocation_date <= p_as_of), 0)
          END::numeric(18,2)
    FROM finance.ar_documents d
    LEFT JOIN finance.ar_allocations a ON a.ar_document_id = d.id
   WHERE d.document_date <= p_as_of
     AND d.status <> 'void'
     AND (p_branch_id IS NULL OR d.branch_id = p_branch_id)
   GROUP BY d.id
  HAVING abs(CASE WHEN d.document_type = 'credit_note'
                  THEN d.original_thb + coalesce(sum(a.functional_amount - a.realized_fx_thb) FILTER (WHERE a.allocation_date <= p_as_of), 0)
                  ELSE d.original_thb - coalesce(sum(a.functional_amount - a.realized_fx_thb) FILTER (WHERE a.allocation_date <= p_as_of), 0)
              END) > 0.004
$fn$;

CREATE OR REPLACE FUNCTION finance.ap_balance_as_of(p_as_of date, p_branch_id bigint DEFAULT NULL)
RETURNS TABLE(id bigint, branch_id bigint, vendor_id bigint, employee_id integer, document_no text, document_type text, document_date date, due_date date, currency_code char(3), open_foreign numeric, open_thb numeric)
LANGUAGE sql
STABLE
AS $fn$
  SELECT d.id,
         d.branch_id,
         d.vendor_id,
         d.employee_id,
         d.document_no,
         d.document_type,
         d.document_date,
         d.due_date,
         d.currency_code,
         CASE WHEN d.document_type = 'vendor_credit'
              THEN d.original_foreign + coalesce(sum(a.foreign_amount) FILTER (WHERE a.allocation_date <= p_as_of), 0)
              ELSE d.original_foreign - coalesce(sum(a.foreign_amount) FILTER (WHERE a.allocation_date <= p_as_of), 0)
          END::numeric(18,2),
         CASE WHEN d.document_type = 'vendor_credit'
              THEN d.original_thb + coalesce(sum(a.functional_amount - a.realized_fx_thb) FILTER (WHERE a.allocation_date <= p_as_of), 0)
              ELSE d.original_thb - coalesce(sum(a.functional_amount - a.realized_fx_thb) FILTER (WHERE a.allocation_date <= p_as_of), 0)
          END::numeric(18,2)
    FROM finance.ap_documents d
    LEFT JOIN finance.ap_allocations a ON a.ap_document_id = d.id
   WHERE d.document_date <= p_as_of
     AND d.status <> 'void'
     AND (p_branch_id IS NULL OR d.branch_id = p_branch_id)
   GROUP BY d.id
  HAVING abs(CASE WHEN d.document_type = 'vendor_credit'
                  THEN d.original_thb + coalesce(sum(a.functional_amount - a.realized_fx_thb) FILTER (WHERE a.allocation_date <= p_as_of), 0)
                  ELSE d.original_thb - coalesce(sum(a.functional_amount - a.realized_fx_thb) FILTER (WHERE a.allocation_date <= p_as_of), 0)
              END) > 0.004
$fn$;

CREATE OR REPLACE FUNCTION finance.ar_aging(p_as_of date, p_branch_id bigint DEFAULT NULL)
RETURNS TABLE(aging_bucket text, amount numeric, document_count bigint)
LANGUAGE sql
STABLE
AS $fn$
  SELECT CASE
           WHEN p_as_of <= due_date THEN 'current'
           WHEN p_as_of - due_date <= 30 THEN '1_30'
           WHEN p_as_of - due_date <= 60 THEN '31_60'
           WHEN p_as_of - due_date <= 90 THEN '61_90'
           ELSE 'over_90'
         END,
         sum(open_thb)::numeric(18,2),
         count(*)
    FROM finance.ar_balance_as_of(p_as_of, p_branch_id)
   GROUP BY 1
$fn$;

CREATE OR REPLACE FUNCTION finance.ap_aging(p_as_of date, p_branch_id bigint DEFAULT NULL)
RETURNS TABLE(aging_bucket text, amount numeric, document_count bigint)
LANGUAGE sql
STABLE
AS $fn$
  SELECT CASE
           WHEN p_as_of <= due_date THEN 'current'
           WHEN p_as_of - due_date <= 30 THEN '1_30'
           WHEN p_as_of - due_date <= 60 THEN '31_60'
           WHEN p_as_of - due_date <= 90 THEN '61_90'
           ELSE 'over_90'
         END,
         sum(open_thb)::numeric(18,2),
         count(*)
    FROM finance.ap_balance_as_of(p_as_of, p_branch_id)
   GROUP BY 1
$fn$;

CREATE OR REPLACE FUNCTION inventory.valuation_as_of(p_as_of date, p_branch_id bigint DEFAULT NULL)
RETURNS TABLE(product_id bigint, warehouse_id bigint, lot_id bigint, quantity numeric, value_thb numeric)
LANGUAGE sql
STABLE
AS $fn$
  SELECT c.product_id,
         c.warehouse_id,
         l.lot_id,
         sum(c.quantity_delta)::numeric(20,6),
         sum(c.value_delta_thb)::numeric(18,2)
    FROM inventory.costing_events c
    JOIN inventory.stock_movement_lines l ON l.id = c.movement_line_id
    JOIN inventory.warehouses w ON w.id = c.warehouse_id
   WHERE c.event_date <= p_as_of
     AND (p_branch_id IS NULL OR w.branch_id = p_branch_id)
   GROUP BY c.product_id, c.warehouse_id, l.lot_id
  HAVING abs(sum(c.quantity_delta)) > 0.0000004 OR abs(sum(c.value_delta_thb)) > 0.004
$fn$;

CREATE OR REPLACE VIEW inventory.v_valuation AS
SELECT b.product_id,
       p.sku,
       p.name,
       b.warehouse_id,
       w.code AS warehouse_code,
       b.bin_id,
       b.lot_id,
       l.lot_no,
       l.expires_on,
       b.quantity,
       b.avg_cost_thb,
       round(b.quantity * b.avg_cost_thb, 2)::numeric(18,2) AS value_thb
  FROM inventory.stock_balances b
  JOIN inventory.products p ON p.id = b.product_id
  JOIN inventory.warehouses w ON w.id = b.warehouse_id
  LEFT JOIN inventory.lots l ON l.id = b.lot_id;

CREATE OR REPLACE VIEW finance.v_control_tieouts AS
WITH gl AS (
  SELECT coalesce(sum(debit_thb), 0) AS debit,
         coalesce(sum(credit_thb), 0) AS credit,
         coalesce(sum(CASE WHEN control_type = 'ar' THEN debit_thb - credit_thb ELSE 0 END), 0) AS ar,
         coalesce(sum(CASE WHEN control_type = 'ap' THEN credit_thb - debit_thb ELSE 0 END), 0) AS ap,
         coalesce(sum(CASE WHEN control_type = 'inventory' THEN debit_thb - credit_thb ELSE 0 END), 0) AS inventory
    FROM finance.v_posted_lines
), sub AS (
  SELECT coalesce((SELECT sum(open_thb) FROM finance.ar_documents WHERE status IN ('open', 'partially_paid')), 0) AS ar,
         coalesce((SELECT sum(open_thb) FROM finance.ap_documents WHERE status IN ('open', 'partially_paid')), 0) AS ap,
         coalesce((SELECT sum(value_thb) FROM inventory.v_valuation), 0) AS inventory
)
SELECT gl.debit::numeric(18,2) AS total_debit,
       gl.credit::numeric(18,2) AS total_credit,
       (gl.debit = gl.credit) AS journal_balanced,
       gl.ar::numeric(18,2) AS ar_gl,
       sub.ar::numeric(18,2) AS ar_subledger,
       (gl.ar = sub.ar) AS ar_tied,
       gl.ap::numeric(18,2) AS ap_gl,
       sub.ap::numeric(18,2) AS ap_subledger,
       (gl.ap = sub.ap) AS ap_tied,
       gl.inventory::numeric(18,2) AS inventory_gl,
       sub.inventory::numeric(18,2) AS inventory_subledger,
       (gl.inventory = sub.inventory) AS inventory_tied
  FROM gl CROSS JOIN sub;

INSERT INTO finance.company_config(id, legal_name, tax_id)
VALUES (1, 'Folio Company Limited', NULL)
ON CONFLICT (id) DO NOTHING;

INSERT INTO finance.currencies(code, name, decimals) VALUES
  ('THB', 'Thai Baht', 2),
  ('USD', 'US Dollar', 2),
  ('EUR', 'Euro', 2),
  ('JPY', 'Japanese Yen', 0),
  ('CNY', 'Chinese Yuan', 2),
  ('SGD', 'Singapore Dollar', 2)
ON CONFLICT (code) DO UPDATE SET name = excluded.name, decimals = excluded.decimals;

INSERT INTO finance.branches(code, name, name_th, tax_branch_code)
VALUES ('HQ', 'Head Office', 'สำนักงานใหญ่', '00000')
ON CONFLICT (code) DO NOTHING;

INSERT INTO finance.accounts(code, name, name_th, account_type, normal_side)
SELECT code,
       name,
       name_th,
       CASE lower(account_type::text)
         WHEN 'asset' THEN 'asset'
         WHEN 'liability' THEN 'liability'
         WHEN 'equity' THEN 'equity'
         WHEN 'revenue' THEN 'revenue'
         ELSE 'expense'
       END,
       normal_side
  FROM folio.chart_of_accounts
ON CONFLICT (code) DO UPDATE
SET name = excluded.name,
    name_th = excluded.name_th,
    account_type = excluded.account_type,
    normal_side = excluded.normal_side;

INSERT INTO finance.accounts(code, name, name_th, account_type, normal_side, control_type) VALUES
  ('110100', 'Cash on Hand', 'เงินสดในมือ', 'asset', 'debit', 'cash'),
  ('110200', 'Cash at Bank', 'เงินฝากธนาคาร', 'asset', 'debit', 'bank'),
  ('110400', 'Accounts Receivable', 'ลูกหนี้การค้า', 'asset', 'debit', 'ar'),
  ('110500', 'Input VAT Receivable', 'ภาษีซื้อ', 'asset', 'debit', 'tax'),
  ('110600', 'Withholding Tax Receivable', 'ภาษีหัก ณ ที่จ่ายรอรับ', 'asset', 'debit', 'tax'),
  ('120200', 'Inventory', 'สินค้าคงเหลือ', 'asset', 'debit', 'inventory'),
  ('210100', 'Accounts Payable', 'เจ้าหนี้การค้า', 'liability', 'credit', 'ap'),
  ('210300', 'Output VAT Payable', 'ภาษีขาย', 'liability', 'credit', 'tax'),
  ('210400', 'Withholding Tax Payable', 'ภาษีหัก ณ ที่จ่ายค้างจ่าย', 'liability', 'credit', 'tax'),
  ('210500', 'Employee Payable', 'เจ้าหนี้พนักงาน', 'liability', 'credit', 'ap'),
  ('210700', 'Goods Received Not Invoiced', 'รับสินค้าแล้วยังไม่ได้รับใบแจ้งหนี้', 'liability', 'credit', 'grni'),
  ('410100', 'Sales Revenue', 'รายได้จากการขาย', 'revenue', 'credit', NULL),
  ('420100', 'Realized Foreign Exchange Gain', 'กำไรจากอัตราแลกเปลี่ยน', 'revenue', 'credit', NULL),
  ('420200', 'Unrealized Foreign Exchange Gain', 'กำไรที่ยังไม่เกิดขึ้นจากอัตราแลกเปลี่ยน', 'revenue', 'credit', NULL),
  ('510100', 'Cost of Goods Sold', 'ต้นทุนขาย', 'expense', 'debit', NULL),
  ('510300', 'General Expense', 'ค่าใช้จ่ายทั่วไป', 'expense', 'debit', NULL),
  ('510500', 'Inventory Adjustment', 'ค่าใช้จ่ายปรับปรุงสินค้าคงเหลือ', 'expense', 'debit', NULL),
  ('520100', 'Realized Foreign Exchange Loss', 'ขาดทุนจากอัตราแลกเปลี่ยน', 'expense', 'debit', NULL),
  ('520200', 'Inventory Impairment', 'ขาดทุนจากการด้อยค่าสินค้าคงเหลือ', 'expense', 'debit', NULL),
  ('520300', 'Unrealized Foreign Exchange Loss', 'ขาดทุนที่ยังไม่เกิดขึ้นจากอัตราแลกเปลี่ยน', 'expense', 'debit', NULL)
ON CONFLICT (code) DO UPDATE
SET name = excluded.name,
    name_th = excluded.name_th,
    account_type = excluded.account_type,
    normal_side = excluded.normal_side,
    control_type = coalesce(excluded.control_type, finance.accounts.control_type);

INSERT INTO finance.tax_codes(code, name, kind, rate, account_code) VALUES
  ('VAT7-IN', 'Input VAT 7%', 'vat_input', 0.07, '110500'),
  ('VAT7-OUT', 'Output VAT 7%', 'vat_output', 0.07, '210300'),
  ('VAT0-IN', 'Input VAT 0%', 'vat_input', 0, '110500'),
  ('VAT0-OUT', 'Output VAT 0%', 'vat_output', 0, '210300'),
  ('VAT-EXEMPT', 'VAT Exempt', 'exempt', 0, NULL),
  ('WHT1-REC', 'WHT Receivable 1%', 'wht_receivable', 0.01, '110600'),
  ('WHT3-REC', 'WHT Receivable 3%', 'wht_receivable', 0.03, '110600'),
  ('WHT3-PAY', 'WHT Payable 3%', 'wht_payable', 0.03, '210400'),
  ('WHT5-PAY', 'WHT Payable 5%', 'wht_payable', 0.05, '210400')
ON CONFLICT (code) DO UPDATE SET name = excluded.name, kind = excluded.kind, rate = excluded.rate, account_code = excluded.account_code;

INSERT INTO inventory.units(code, name, decimals) VALUES
  ('EA', 'Each', 0),
  ('KG', 'Kilogram', 3),
  ('G', 'Gram', 3),
  ('L', 'Litre', 3),
  ('ML', 'Millilitre', 3),
  ('M', 'Metre', 3),
  ('BOX', 'Box', 0),
  ('PACK', 'Pack', 0)
ON CONFLICT (code) DO UPDATE SET name = excluded.name, decimals = excluded.decimals;

INSERT INTO inventory.warehouses(branch_id, code, name)
SELECT id, code || '-MAIN', name || ' Main Warehouse'
  FROM finance.branches
 WHERE code = 'HQ'
ON CONFLICT (code) DO NOTHING;

INSERT INTO finance.fiscal_periods(fiscal_year, period_no, starts_on, ends_on)
SELECT y, m, make_date(y, m, 1), (make_date(y, m, 1) + interval '1 month - 1 day')::date
  FROM generate_series(extract(year from current_date)::integer - 1, extract(year from current_date)::integer + 1) y
 CROSS JOIN generate_series(1, 12) m
ON CONFLICT (fiscal_year, period_no) DO NOTHING;

INSERT INTO finance.closing_checklists(period_id, task_key, label)
SELECT p.id, t.task_key, t.label
  FROM finance.fiscal_periods p
 CROSS JOIN (VALUES
   ('bank_reconciled', 'All bank accounts reconciled'),
   ('ar_ap_tied', 'AR and AP subledgers tied to control accounts'),
   ('inventory_tied', 'Inventory valuation tied to the GL'),
   ('fx_revalued', 'Foreign balances revalued'),
   ('tax_reviewed', 'VAT and WHT registers reviewed'),
   ('trial_balance_reviewed', 'Trial balance reviewed and balanced')
 ) AS t(task_key, label)
ON CONFLICT (period_id, task_key) DO NOTHING;

INSERT INTO perm.permissions(id, description) VALUES
  ('finance:journal:prepare::allow', 'Prepare accounting journals'),
  ('finance:journal:approve::allow', 'Approve and post prepared journals'),
  ('finance:journal:reverse::allow', 'Reverse posted journals'),
  ('finance:journal:manual::allow', 'Create manual journals'),
  ('finance:coa:manage::allow', 'Manage chart of accounts'),
  ('finance:period:close::allow', 'Soft close and lock fiscal periods'),
  ('finance:period:reopen::allow', 'Reopen fiscal periods'),
  ('finance:fx:manage::allow', 'Manage approved FX rates and revaluation'),
  ('finance:bank:import::allow', 'Import bank statements'),
  ('finance:bank:match::allow', 'Confirm bank matches'),
  ('finance:bank:reopen::allow', 'Reopen bank reconciliations'),
  ('finance:report:view::allow', 'View posted financial reports'),
  ('finance:report:export::allow', 'Export posted financial reports'),
  ('finance:budget:manage::allow', 'Manage and approve budgets'),
  ('inventory:stock:view::allow', 'View inventory balances and movements'),
  ('inventory:stock:receive::allow', 'Receive inventory'),
  ('inventory:stock:ship::allow', 'Ship inventory'),
  ('inventory:stock:transfer::allow', 'Transfer inventory'),
  ('inventory:stock:count::allow', 'Count inventory'),
  ('inventory:stock:adjust::allow', 'Adjust, recost, and write down inventory')
ON CONFLICT (id) DO UPDATE SET description = excluded.description;

INSERT INTO perm.role_permissions(role_id, role_kind, permission_id, granted_by)
SELECT r.id, r.kind, p.id, 'accounting-cutover'
  FROM perm.roles r
 CROSS JOIN perm.permissions p
 WHERE r.id IN ('accounting_manager', 'cfo', 'ceo')
   AND p.id IN (
     'finance:journal:prepare::allow', 'finance:journal:approve::allow', 'finance:journal:reverse::allow',
     'finance:journal:manual::allow', 'finance:coa:manage::allow', 'finance:period:close::allow',
     'finance:period:reopen::allow', 'finance:fx:manage::allow', 'finance:bank:import::allow',
     'finance:bank:match::allow', 'finance:bank:reopen::allow', 'finance:report:view::allow',
     'finance:report:export::allow', 'finance:budget:manage::allow', 'inventory:stock:view::allow',
     'inventory:stock:receive::allow', 'inventory:stock:ship::allow', 'inventory:stock:transfer::allow',
     'inventory:stock:count::allow', 'inventory:stock:adjust::allow'
   )
ON CONFLICT (role_id, permission_id) DO NOTHING;

INSERT INTO perm.role_permissions(role_id, role_kind, permission_id, granted_by)
SELECT r.id, r.kind, p.id, 'accounting-cutover'
  FROM perm.roles r
 CROSS JOIN perm.permissions p
 WHERE r.id IN ('accounting_officer', 'finance_officer', 'finance_manager')
   AND p.id IN (
     'finance:journal:prepare::allow', 'finance:journal:manual::allow', 'finance:bank:import::allow',
     'finance:bank:match::allow', 'finance:report:view::allow', 'inventory:stock:view::allow',
     'inventory:stock:receive::allow', 'inventory:stock:ship::allow', 'inventory:stock:transfer::allow',
     'inventory:stock:count::allow'
   )
ON CONFLICT (role_id, permission_id) DO NOTHING;

COMMIT;
