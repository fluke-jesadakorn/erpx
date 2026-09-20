CREATE TABLE commerce_returns (
  id TEXT PRIMARY KEY,
  company_id TEXT NOT NULL,
  branch_id TEXT NOT NULL,
  order_id TEXT NOT NULL REFERENCES commerce_orders(id),
  fulfillment_id TEXT NOT NULL REFERENCES commerce_fulfillments(id),
  invoice_id TEXT NOT NULL REFERENCES finance_invoices(id),
  kind TEXT NOT NULL CHECK(kind IN ('customer_return','supplier_return')),
  number TEXT NOT NULL,
  return_date TEXT NOT NULL,
  reason TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'requested' CHECK(status IN ('requested','approved','rejected','posted')),
  subtotal_minor BIGINT NOT NULL CHECK(subtotal_minor>0),
  tax_minor BIGINT NOT NULL DEFAULT 0 CHECK(tax_minor>=0),
  total_minor BIGINT NOT NULL CHECK(total_minor=subtotal_minor+tax_minor),
  stock_journal_id TEXT REFERENCES finance_journals(id),
  credit_journal_id TEXT REFERENCES finance_journals(id),
  version BIGINT NOT NULL DEFAULT 1,
  requested_by TEXT NOT NULL,
  approved_by TEXT,
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL,
  UNIQUE(company_id,number)
);

CREATE TABLE commerce_return_lines (
  id TEXT PRIMARY KEY,
  return_id TEXT NOT NULL REFERENCES commerce_returns(id),
  fulfillment_line_id TEXT NOT NULL REFERENCES commerce_fulfillment_lines(id),
  product_id TEXT NOT NULL REFERENCES commerce_products(id),
  quantity_milli BIGINT NOT NULL CHECK(quantity_milli>0),
  value_minor BIGINT NOT NULL CHECK(value_minor>=0),
  subtotal_minor BIGINT NOT NULL CHECK(subtotal_minor>=0),
  tax_minor BIGINT NOT NULL CHECK(tax_minor>=0),
  ordinal BIGINT NOT NULL,
  UNIQUE(return_id,fulfillment_line_id)
);

CREATE TABLE commerce_return_stock_moves (
  id TEXT PRIMARY KEY,
  return_id TEXT NOT NULL REFERENCES commerce_returns(id),
  warehouse_id TEXT NOT NULL REFERENCES commerce_warehouses(id),
  product_id TEXT NOT NULL REFERENCES commerce_products(id),
  quantity_milli BIGINT NOT NULL,
  value_minor BIGINT NOT NULL,
  movement_date TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  UNIQUE(return_id,product_id)
);

CREATE OR REPLACE VIEW inventory_ledger AS
 SELECT m.id,m.warehouse_id,m.product_id,m.quantity_milli,m.value_minor,m.created_at,f.fulfillment_date movement_date,
 'fulfillment' source_type,f.order_id source_id,o.number source_number
 FROM commerce_stock_moves m JOIN commerce_fulfillments f ON f.id=m.fulfillment_id JOIN commerce_orders o ON o.id=f.order_id
 UNION ALL SELECT m.id,m.warehouse_id,m.product_id,m.quantity_milli,m.value_minor,m.created_at,m.movement_date,'stock_count',m.count_id,c.number
 FROM inventory_adjustment_moves m JOIN inventory_counts c ON c.id=m.count_id
 UNION ALL SELECT m.id,m.warehouse_id,m.product_id,m.quantity_milli,m.value_minor,m.created_at,m.movement_date,'transfer',m.transfer_id,t.number
 FROM inventory_transfer_moves m JOIN inventory_transfers t ON t.id=m.transfer_id
 UNION ALL SELECT id,warehouse_id,product_id,quantity_milli,value_minor,created_at,movement_date,'pos_'||kind,source_id,source_number FROM pos_stock_moves
 UNION ALL SELECT id,warehouse_id,product_id,quantity_milli,value_minor,created_at,movement_date,'pms',property_id,source_number FROM pms_stock_moves
 UNION ALL SELECT m.id,m.warehouse_id,m.product_id,m.quantity_milli,m.value_minor,m.created_at,m.movement_date,r.kind,r.order_id,r.number
 FROM commerce_return_stock_moves m JOIN commerce_returns r ON r.id=m.return_id;
