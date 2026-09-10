CREATE TABLE commerce_settings (
  company_id TEXT PRIMARY KEY,
  chart_id TEXT NOT NULL REFERENCES finance_charts(id),
  currency TEXT NOT NULL,
  receivable_account_id TEXT NOT NULL REFERENCES finance_accounts(id),
  payable_account_id TEXT NOT NULL REFERENCES finance_accounts(id),
  revenue_account_id TEXT NOT NULL REFERENCES finance_accounts(id),
  inventory_account_id TEXT NOT NULL REFERENCES finance_accounts(id),
  clearing_account_id TEXT NOT NULL REFERENCES finance_accounts(id),
  cogs_account_id TEXT NOT NULL REFERENCES finance_accounts(id),
  input_tax_account_id TEXT NOT NULL REFERENCES finance_accounts(id),
  output_tax_account_id TEXT NOT NULL REFERENCES finance_accounts(id),
  cash_account_id TEXT NOT NULL REFERENCES finance_accounts(id)
) STRICT;
CREATE TABLE commerce_products (
  id TEXT PRIMARY KEY,company_id TEXT NOT NULL,sku TEXT NOT NULL,name TEXT NOT NULL,
  unit TEXT NOT NULL DEFAULT 'each',price_minor INTEGER NOT NULL CHECK(price_minor>=0),
  tax_bps INTEGER NOT NULL DEFAULT 0 CHECK(tax_bps BETWEEN 0 AND 10000),
  status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active','archived')),
  version INTEGER NOT NULL DEFAULT 1,UNIQUE(company_id,sku)
) STRICT;
CREATE TABLE commerce_warehouses (
  id TEXT PRIMARY KEY,company_id TEXT NOT NULL,branch_id TEXT NOT NULL REFERENCES organization_nodes(id),
  code TEXT NOT NULL,name TEXT NOT NULL,status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active','archived')),
  version INTEGER NOT NULL DEFAULT 1,UNIQUE(company_id,code)
) STRICT;
CREATE TABLE commerce_orders (
  id TEXT PRIMARY KEY,company_id TEXT NOT NULL,branch_id TEXT NOT NULL REFERENCES organization_nodes(id),
  department_id TEXT,kind TEXT NOT NULL CHECK(kind IN ('quote','sales','purchase')),
  number TEXT NOT NULL,party_id TEXT NOT NULL REFERENCES finance_parties(id),
  warehouse_id TEXT NOT NULL REFERENCES commerce_warehouses(id),currency TEXT NOT NULL,
  order_date TEXT NOT NULL,due_date TEXT NOT NULL,note TEXT NOT NULL DEFAULT '',
  state TEXT NOT NULL DEFAULT 'draft' CHECK(state IN ('draft','submitted','confirmed','partially_fulfilled','fulfilled','converted','cancelled')),
  subtotal_minor INTEGER NOT NULL CHECK(subtotal_minor>=0),tax_minor INTEGER NOT NULL CHECK(tax_minor>=0),
  source_id TEXT REFERENCES commerce_orders(id),version INTEGER NOT NULL DEFAULT 1,
  created_by TEXT NOT NULL,approved_by TEXT,created_at INTEGER NOT NULL,updated_at INTEGER NOT NULL,
  UNIQUE(company_id,number),UNIQUE(source_id)
) STRICT;
CREATE INDEX commerce_order_scope_idx ON commerce_orders(company_id,branch_id,kind,state);
CREATE TABLE commerce_order_lines (
  id TEXT PRIMARY KEY,order_id TEXT NOT NULL REFERENCES commerce_orders(id),product_id TEXT NOT NULL REFERENCES commerce_products(id),
  description TEXT NOT NULL,quantity_milli INTEGER NOT NULL CHECK(quantity_milli>0),
  fulfilled_milli INTEGER NOT NULL DEFAULT 0 CHECK(fulfilled_milli>=0 AND fulfilled_milli<=quantity_milli),
  price_minor INTEGER NOT NULL CHECK(price_minor>=0),tax_bps INTEGER NOT NULL CHECK(tax_bps BETWEEN 0 AND 10000),
  subtotal_minor INTEGER NOT NULL CHECK(subtotal_minor>=0),tax_minor INTEGER NOT NULL CHECK(tax_minor>=0),
  ordinal INTEGER NOT NULL,UNIQUE(order_id,product_id)
) STRICT;
CREATE TABLE commerce_stock (
  warehouse_id TEXT NOT NULL REFERENCES commerce_warehouses(id),product_id TEXT NOT NULL REFERENCES commerce_products(id),
  quantity_milli INTEGER NOT NULL DEFAULT 0 CHECK(quantity_milli>=0),value_minor INTEGER NOT NULL DEFAULT 0 CHECK(value_minor>=0),
  version INTEGER NOT NULL DEFAULT 1,PRIMARY KEY(warehouse_id,product_id)
) STRICT;
CREATE TABLE commerce_fulfillments (
  id TEXT PRIMARY KEY,order_id TEXT NOT NULL REFERENCES commerce_orders(id),number TEXT NOT NULL,
  fulfillment_date TEXT NOT NULL,created_by TEXT NOT NULL,created_at INTEGER NOT NULL,
  journal_id TEXT REFERENCES finance_journals(id),invoice_id TEXT UNIQUE REFERENCES finance_invoices(id),version INTEGER NOT NULL DEFAULT 1
) STRICT;
CREATE TABLE commerce_fulfillment_lines (
  id TEXT PRIMARY KEY,fulfillment_id TEXT NOT NULL REFERENCES commerce_fulfillments(id),order_line_id TEXT NOT NULL REFERENCES commerce_order_lines(id),
  quantity_milli INTEGER NOT NULL CHECK(quantity_milli>0),value_minor INTEGER NOT NULL CHECK(value_minor>=0),
  subtotal_minor INTEGER NOT NULL CHECK(subtotal_minor>=0),tax_minor INTEGER NOT NULL CHECK(tax_minor>=0),UNIQUE(fulfillment_id,order_line_id)
) STRICT;
CREATE TABLE commerce_stock_moves (
  id TEXT PRIMARY KEY,warehouse_id TEXT NOT NULL REFERENCES commerce_warehouses(id),product_id TEXT NOT NULL REFERENCES commerce_products(id),
  fulfillment_id TEXT NOT NULL REFERENCES commerce_fulfillments(id),quantity_milli INTEGER NOT NULL,value_minor INTEGER NOT NULL,
  created_at INTEGER NOT NULL
) STRICT;
ALTER TABLE finance_invoices ADD COLUMN tax_account_id TEXT REFERENCES finance_accounts(id);
