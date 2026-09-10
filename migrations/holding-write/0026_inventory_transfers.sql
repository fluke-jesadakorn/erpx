CREATE TABLE inventory_transfers (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL,
 source_warehouse_id TEXT NOT NULL REFERENCES commerce_warehouses(id),
 target_warehouse_id TEXT NOT NULL REFERENCES commerce_warehouses(id),
 source_branch_id TEXT NOT NULL REFERENCES organization_nodes(id),
 target_branch_id TEXT NOT NULL REFERENCES organization_nodes(id),
 source_warehouse_version INTEGER NOT NULL,target_warehouse_version INTEGER NOT NULL,
 number TEXT NOT NULL, transfer_date TEXT NOT NULL, reason TEXT NOT NULL,
 status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','submitted','rejected','posted','cancelled')),
 version INTEGER NOT NULL DEFAULT 1, total_value_minor INTEGER NOT NULL CHECK(total_value_minor>=0),
 created_by TEXT NOT NULL,prepared_by TEXT NOT NULL,approved_by TEXT,
 submitted_at INTEGER,posted_at INTEGER,created_at INTEGER NOT NULL,updated_at INTEGER NOT NULL,
 CHECK(source_warehouse_id!=target_warehouse_id), UNIQUE(company_id,number)
) STRICT;
CREATE INDEX inventory_transfers_scope_idx ON inventory_transfers(company_id,source_branch_id,target_branch_id,status);
CREATE TABLE inventory_transfer_lines (
 id TEXT PRIMARY KEY,transfer_id TEXT NOT NULL REFERENCES inventory_transfers(id),
 product_id TEXT NOT NULL REFERENCES commerce_products(id),product_version INTEGER NOT NULL,
 description TEXT NOT NULL,unit TEXT NOT NULL,quantity_milli INTEGER NOT NULL CHECK(quantity_milli>0),
 value_minor INTEGER NOT NULL CHECK(value_minor>0),
 source_quantity_milli INTEGER NOT NULL,source_value_minor INTEGER NOT NULL,source_version INTEGER NOT NULL,
 ordinal INTEGER NOT NULL,UNIQUE(transfer_id,product_id)
) STRICT;
CREATE TABLE inventory_transfer_moves (
 id TEXT PRIMARY KEY,transfer_id TEXT NOT NULL REFERENCES inventory_transfers(id),
 warehouse_id TEXT NOT NULL REFERENCES commerce_warehouses(id),product_id TEXT NOT NULL REFERENCES commerce_products(id),
 quantity_milli INTEGER NOT NULL,value_minor INTEGER NOT NULL,movement_date TEXT NOT NULL,created_at INTEGER NOT NULL,
 UNIQUE(transfer_id,warehouse_id,product_id)
) STRICT;
CREATE TABLE inventory_transfer_decisions (
 id TEXT PRIMARY KEY,transfer_id TEXT NOT NULL REFERENCES inventory_transfers(id),
 command_id TEXT NOT NULL UNIQUE REFERENCES command_receipts(command_id),
 action TEXT NOT NULL CHECK(action IN ('post','reject','cancel')),actor_id TEXT NOT NULL,
 reviewed_version INTEGER NOT NULL,reason TEXT NOT NULL,created_at INTEGER NOT NULL
) STRICT;
CREATE INDEX inventory_transfer_decisions_idx ON inventory_transfer_decisions(transfer_id,created_at);
DROP VIEW inventory_ledger;
CREATE VIEW inventory_ledger AS
 SELECT m.id,m.warehouse_id,m.product_id,m.quantity_milli,m.value_minor,m.created_at,f.fulfillment_date movement_date,
 'fulfillment' source_type,f.order_id source_id,o.number source_number
 FROM commerce_stock_moves m JOIN commerce_fulfillments f ON f.id=m.fulfillment_id JOIN commerce_orders o ON o.id=f.order_id
 UNION ALL
 SELECT m.id,m.warehouse_id,m.product_id,m.quantity_milli,m.value_minor,m.created_at,m.movement_date,
 'stock_count',m.count_id,c.number FROM inventory_adjustment_moves m JOIN inventory_counts c ON c.id=m.count_id
 UNION ALL
 SELECT m.id,m.warehouse_id,m.product_id,m.quantity_milli,m.value_minor,m.created_at,m.movement_date,
 'transfer',m.transfer_id,t.number FROM inventory_transfer_moves m JOIN inventory_transfers t ON t.id=m.transfer_id;
