CREATE TABLE inventory_counts (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  branch_id TEXT NOT NULL REFERENCES organization_nodes(id),
  warehouse_id TEXT NOT NULL REFERENCES commerce_warehouses(id),
  number TEXT NOT NULL,
  count_date TEXT NOT NULL,
  reason TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','submitted','rejected','posted','cancelled')),
  version INTEGER NOT NULL DEFAULT 1,
  net_value_minor INTEGER NOT NULL DEFAULT 0,
  created_by TEXT NOT NULL,
  prepared_by TEXT NOT NULL,
  approved_by TEXT,
  review_reason TEXT,
  submitted_at INTEGER,
  posted_at INTEGER,
  journal_id TEXT REFERENCES finance_journals(id),
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  UNIQUE(company_id,number)
) STRICT;
CREATE INDEX inventory_counts_scope_idx ON inventory_counts(company_id,branch_id,status);
CREATE TABLE inventory_count_lines (
  id TEXT PRIMARY KEY,
  count_id TEXT NOT NULL REFERENCES inventory_counts(id),
  product_id TEXT NOT NULL REFERENCES commerce_products(id),
  product_version INTEGER NOT NULL,
  description TEXT NOT NULL,
  unit TEXT NOT NULL,
  snapshot_quantity_milli INTEGER NOT NULL CHECK(snapshot_quantity_milli>=0),
  snapshot_value_minor INTEGER NOT NULL CHECK(snapshot_value_minor>=0),
  snapshot_version INTEGER NOT NULL,
  counted_quantity_milli INTEGER NOT NULL CHECK(counted_quantity_milli>=0),
  counted_value_minor INTEGER NOT NULL CHECK(counted_value_minor>=0),
  unit_cost_minor INTEGER,
  ordinal INTEGER NOT NULL,
  UNIQUE(count_id,product_id)
) STRICT;
CREATE TABLE inventory_adjustment_moves (
  id TEXT PRIMARY KEY,
  count_id TEXT NOT NULL REFERENCES inventory_counts(id),
  warehouse_id TEXT NOT NULL REFERENCES commerce_warehouses(id),
  product_id TEXT NOT NULL REFERENCES commerce_products(id),
  quantity_milli INTEGER NOT NULL,
  value_minor INTEGER NOT NULL,
  movement_date TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  UNIQUE(count_id,product_id)
) STRICT;
CREATE VIEW inventory_ledger AS
  SELECT m.id,m.warehouse_id,m.product_id,m.quantity_milli,m.value_minor,m.created_at,f.fulfillment_date movement_date,
    'fulfillment' source_type,f.order_id source_id,o.number source_number
  FROM commerce_stock_moves m JOIN commerce_fulfillments f ON f.id=m.fulfillment_id JOIN commerce_orders o ON o.id=f.order_id
  UNION ALL
  SELECT m.id,m.warehouse_id,m.product_id,m.quantity_milli,m.value_minor,m.created_at,m.movement_date,
    'stock_count' source_type,m.count_id source_id,c.number source_number
  FROM inventory_adjustment_moves m JOIN inventory_counts c ON c.id=m.count_id;
