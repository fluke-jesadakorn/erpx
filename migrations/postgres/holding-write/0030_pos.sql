-- POS owns sale/fulfilment/payment lifecycles; Commerce remains the stock master.
CREATE TABLE pos_settings (
 company_id TEXT PRIMARY KEY, enabled BOOLEAN NOT NULL DEFAULT false,
 currency TEXT NOT NULL DEFAULT 'THB', timezone TEXT NOT NULL DEFAULT 'Asia/Bangkok',
 accounts_json TEXT NOT NULL CHECK(erpx_json_valid(accounts_json)),
 cashier_discount_bps BIGINT NOT NULL DEFAULT 0 CHECK(cashier_discount_bps BETWEEN 0 AND 10000),
 version BIGINT NOT NULL DEFAULT 1
);
CREATE TABLE pos_registers (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, branch_id TEXT NOT NULL,
 warehouse_id TEXT NOT NULL REFERENCES commerce_warehouses(id), name TEXT NOT NULL,
 mode TEXT NOT NULL CHECK(mode IN ('retail','restaurant')), active BOOLEAN NOT NULL DEFAULT true,
 version BIGINT NOT NULL DEFAULT 1, UNIQUE(company_id,branch_id,name)
);
CREATE TABLE pos_tables (id TEXT PRIMARY KEY, register_id TEXT NOT NULL REFERENCES pos_registers(id), name TEXT NOT NULL, seats BIGINT NOT NULL DEFAULT 2 CHECK(seats>0), UNIQUE(register_id,name));
CREATE TABLE pos_catalog (
 product_id TEXT PRIMARY KEY REFERENCES commerce_products(id), company_id TEXT NOT NULL,
 barcode TEXT, category TEXT NOT NULL DEFAULT '', kind TEXT NOT NULL DEFAULT 'stock' CHECK(kind IN ('stock','recipe','service')),
 station TEXT NOT NULL DEFAULT 'Kitchen', recipe_json TEXT NOT NULL DEFAULT '[]' CHECK(erpx_json_valid(recipe_json)),
 modifiers_json TEXT NOT NULL DEFAULT '[]' CHECK(erpx_json_valid(modifiers_json)), tax_inclusive BOOLEAN NOT NULL DEFAULT false,
 version BIGINT NOT NULL DEFAULT 1, UNIQUE(company_id,barcode)
);
CREATE TABLE pos_shifts (
 id TEXT PRIMARY KEY, register_id TEXT NOT NULL REFERENCES pos_registers(id), company_id TEXT NOT NULL, branch_id TEXT NOT NULL,
 opened_by TEXT NOT NULL, opened_at BIGINT NOT NULL, opening_minor BIGINT NOT NULL CHECK(opening_minor>=0),
 state TEXT NOT NULL DEFAULT 'open' CHECK(state IN ('open','closed','reviewed')), counted_minor BIGINT,
 expected_minor BIGINT, variance_minor BIGINT, closed_at BIGINT, reviewed_by TEXT, review_reason TEXT,
 version BIGINT NOT NULL DEFAULT 1
);
CREATE UNIQUE INDEX pos_one_open_shift ON pos_shifts(register_id) WHERE state='open';
CREATE TABLE pos_cash_movements (
 id TEXT PRIMARY KEY, shift_id TEXT NOT NULL REFERENCES pos_shifts(id), amount_minor BIGINT NOT NULL CHECK(amount_minor<>0),
 reason TEXT NOT NULL, created_by TEXT NOT NULL, approved_by TEXT, created_at BIGINT NOT NULL
);
CREATE TABLE pos_sales (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, branch_id TEXT NOT NULL,
 register_id TEXT NOT NULL REFERENCES pos_registers(id), shift_id TEXT NOT NULL REFERENCES pos_shifts(id),
 number TEXT NOT NULL, currency TEXT NOT NULL, business_date TEXT NOT NULL, party_id TEXT REFERENCES finance_parties(id),
 table_id TEXT REFERENCES pos_tables(id), guests BIGINT NOT NULL DEFAULT 1 CHECK(guests>0),
 service_mode TEXT NOT NULL DEFAULT 'retail' CHECK(service_mode IN ('retail','dine_in','takeaway')),
 state TEXT NOT NULL DEFAULT 'open' CHECK(state IN ('open','held','payment_pending','completed','cancelled','merged')),
 subtotal_minor BIGINT NOT NULL DEFAULT 0, tax_minor BIGINT NOT NULL DEFAULT 0, discount_minor BIGINT NOT NULL DEFAULT 0,
 total_minor BIGINT NOT NULL DEFAULT 0 CHECK(total_minor>=0), invoice_id TEXT UNIQUE REFERENCES finance_invoices(id),
 created_by TEXT NOT NULL, created_at BIGINT NOT NULL, updated_at BIGINT NOT NULL, version BIGINT NOT NULL DEFAULT 1,
 discount_approved_by TEXT, discount_reason TEXT, accounts_json TEXT NOT NULL CHECK(erpx_json_valid(accounts_json)),
 UNIQUE(company_id,number)
);
CREATE UNIQUE INDEX pos_table_occupied ON pos_sales(table_id) WHERE table_id IS NOT NULL AND state IN ('open','held','payment_pending');
CREATE TABLE pos_fulfillments (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, branch_id TEXT NOT NULL, warehouse_id TEXT NOT NULL REFERENCES commerce_warehouses(id),
 product_id TEXT NOT NULL REFERENCES commerce_products(id), quantity_milli BIGINT NOT NULL CHECK(quantity_milli>0),
 kind TEXT NOT NULL CHECK(kind IN ('stock','recipe','service')), station TEXT NOT NULL, note TEXT NOT NULL DEFAULT '',
 state TEXT NOT NULL DEFAULT 'draft' CHECK(state IN ('draft','queued','preparing','ready','delivered','cancelled')),
 recipe_json TEXT NOT NULL CHECK(erpx_json_valid(recipe_json)), cost_minor BIGINT NOT NULL DEFAULT 0 CHECK(cost_minor>=0),
 accounts_json TEXT NOT NULL CHECK(erpx_json_valid(accounts_json)), currency TEXT NOT NULL, business_date TEXT NOT NULL,
 reserved BOOLEAN NOT NULL DEFAULT false, version BIGINT NOT NULL DEFAULT 1, created_at BIGINT NOT NULL
);
CREATE TABLE pos_lines (
 id TEXT PRIMARY KEY, sale_id TEXT NOT NULL REFERENCES pos_sales(id), fulfillment_id TEXT NOT NULL REFERENCES pos_fulfillments(id),
 product_id TEXT NOT NULL REFERENCES commerce_products(id), name TEXT NOT NULL, quantity_milli BIGINT NOT NULL CHECK(quantity_milli>0),
 price_minor BIGINT NOT NULL CHECK(price_minor>=0), tax_bps BIGINT NOT NULL CHECK(tax_bps BETWEEN 0 AND 10000), tax_inclusive BOOLEAN NOT NULL,
 discount_minor BIGINT NOT NULL DEFAULT 0 CHECK(discount_minor>=0), subtotal_minor BIGINT NOT NULL CHECK(subtotal_minor>=0),
 tax_minor BIGINT NOT NULL CHECK(tax_minor>=0), modifiers_json TEXT NOT NULL CHECK(erpx_json_valid(modifiers_json)),
 note TEXT NOT NULL DEFAULT '', returned_milli BIGINT NOT NULL DEFAULT 0, refunded_minor BIGINT NOT NULL DEFAULT 0,
 refunded_tax_minor BIGINT NOT NULL DEFAULT 0, returned_cost_minor BIGINT NOT NULL DEFAULT 0,
 CHECK(returned_milli BETWEEN 0 AND quantity_milli), CHECK(refunded_minor BETWEEN 0 AND subtotal_minor+tax_minor)
);
CREATE TABLE pos_reservations (
 fulfillment_id TEXT NOT NULL REFERENCES pos_fulfillments(id), warehouse_id TEXT NOT NULL, product_id TEXT NOT NULL,
 quantity_milli BIGINT NOT NULL CHECK(quantity_milli>0), PRIMARY KEY(fulfillment_id,product_id),
 FOREIGN KEY(warehouse_id,product_id) REFERENCES commerce_stock(warehouse_id,product_id)
);
CREATE INDEX pos_reserved_stock ON pos_reservations(warehouse_id,product_id);
-- Guard every stock writer, including legacy Sales, transfers and stock counts.
CREATE FUNCTION pos_guard_reserved_stock() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.quantity_milli < COALESCE((SELECT SUM(quantity_milli) FROM pos_reservations WHERE warehouse_id=NEW.warehouse_id AND product_id=NEW.product_id),0) THEN
  RAISE EXCEPTION 'POS_STOCK_RESERVED' USING ERRCODE='23514';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER pos_reserved_stock_guard BEFORE UPDATE ON commerce_stock FOR EACH ROW EXECUTE FUNCTION pos_guard_reserved_stock();
CREATE TABLE pos_stock_moves (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, warehouse_id TEXT NOT NULL, product_id TEXT NOT NULL,
 fulfillment_id TEXT REFERENCES pos_fulfillments(id), source_id TEXT NOT NULL, source_number TEXT NOT NULL,
 kind TEXT NOT NULL CHECK(kind IN ('sale','kitchen','return')), quantity_milli BIGINT NOT NULL, value_minor BIGINT NOT NULL,
 movement_date TEXT NOT NULL, created_at BIGINT NOT NULL
);
CREATE OR REPLACE VIEW inventory_ledger AS
 SELECT m.id,m.warehouse_id,m.product_id,m.quantity_milli,m.value_minor,m.created_at,f.fulfillment_date movement_date,
 'fulfillment' source_type,f.order_id source_id,o.number source_number
 FROM commerce_stock_moves m JOIN commerce_fulfillments f ON f.id=m.fulfillment_id JOIN commerce_orders o ON o.id=f.order_id
 UNION ALL SELECT m.id,m.warehouse_id,m.product_id,m.quantity_milli,m.value_minor,m.created_at,m.movement_date,
 'stock_count',m.count_id,c.number FROM inventory_adjustment_moves m JOIN inventory_counts c ON c.id=m.count_id
 UNION ALL SELECT m.id,m.warehouse_id,m.product_id,m.quantity_milli,m.value_minor,m.created_at,m.movement_date,
 'transfer',m.transfer_id,t.number FROM inventory_transfer_moves m JOIN inventory_transfers t ON t.id=m.transfer_id
 UNION ALL SELECT id,warehouse_id,product_id,quantity_milli,value_minor,created_at,movement_date,'pos_'||kind,source_id,source_number FROM pos_stock_moves;
CREATE TABLE pos_payments (
 id TEXT PRIMARY KEY, sale_id TEXT NOT NULL REFERENCES pos_sales(id), company_id TEXT NOT NULL,
 method TEXT NOT NULL CHECK(method IN ('cash','transfer','edc','stripe_card','stripe_promptpay')),
 amount_minor BIGINT NOT NULL CHECK(amount_minor>0), tendered_minor BIGINT NOT NULL CHECK(tendered_minor>=amount_minor),
 reference TEXT NOT NULL DEFAULT '', state TEXT NOT NULL CHECK(state IN ('requested','pending','succeeded','failed','cancelled','exception')),
 stripe_account TEXT, livemode BOOLEAN NOT NULL DEFAULT false, provider_id TEXT, checkout_url TEXT,
 finance_payment_id TEXT REFERENCES finance_payments(id), refunded_minor BIGINT NOT NULL DEFAULT 0 CHECK(refunded_minor BETWEEN 0 AND amount_minor),
 created_by TEXT NOT NULL, created_at BIGINT NOT NULL, updated_at BIGINT NOT NULL, version BIGINT NOT NULL DEFAULT 1,
 UNIQUE(stripe_account,livemode,provider_id)
);
CREATE TABLE pos_refunds (
 id TEXT PRIMARY KEY, sale_id TEXT NOT NULL REFERENCES pos_sales(id), company_id TEXT NOT NULL, branch_id TEXT NOT NULL,
 payout_shift_id TEXT REFERENCES pos_shifts(id), kind TEXT NOT NULL DEFAULT 'sale' CHECK(kind IN ('sale','advance')),
 reason TEXT NOT NULL, lines_json TEXT NOT NULL CHECK(erpx_json_valid(lines_json)), amount_minor BIGINT NOT NULL CHECK(amount_minor>0),
 state TEXT NOT NULL DEFAULT 'requested' CHECK(state IN ('requested','approved','rejected','processing','completed','exception')),
 requested_by TEXT NOT NULL, approved_by TEXT, created_at BIGINT NOT NULL, version BIGINT NOT NULL DEFAULT 1,
 accounting_date TEXT NOT NULL, tax_minor BIGINT NOT NULL DEFAULT 0
);
CREATE TABLE pos_refund_payments (
 id TEXT PRIMARY KEY, refund_id TEXT NOT NULL REFERENCES pos_refunds(id), payment_id TEXT NOT NULL REFERENCES pos_payments(id),
 amount_minor BIGINT NOT NULL CHECK(amount_minor>0), state TEXT NOT NULL DEFAULT 'requested' CHECK(state IN ('requested','pending','succeeded','failed')),
 provider_id TEXT, UNIQUE(refund_id,payment_id)
);
CREATE TABLE pos_connections (
 company_id TEXT PRIMARY KEY, account_id TEXT, livemode BOOLEAN NOT NULL DEFAULT false,
 charges_enabled BOOLEAN NOT NULL DEFAULT false, payouts_enabled BOOLEAN NOT NULL DEFAULT false,
 card_enabled BOOLEAN NOT NULL DEFAULT false, promptpay_enabled BOOLEAN NOT NULL DEFAULT false,
 requirements_json TEXT NOT NULL DEFAULT '[]', state TEXT NOT NULL DEFAULT 'unconfigured', version BIGINT NOT NULL DEFAULT 1,
 UNIQUE(account_id,livemode)
);
CREATE TABLE pos_connection_history (company_id TEXT NOT NULL, account_id TEXT NOT NULL, livemode BOOLEAN NOT NULL, PRIMARY KEY(account_id,livemode));
CREATE TABLE pos_oauth_states (id TEXT PRIMARY KEY, company_id TEXT NOT NULL, user_id TEXT NOT NULL, expires_at BIGINT NOT NULL, used_at BIGINT);
CREATE TABLE pos_jobs (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, kind TEXT NOT NULL, record_id TEXT NOT NULL,
 state TEXT NOT NULL DEFAULT 'pending' CHECK(state IN ('pending','running','completed','exception')),
 attempts BIGINT NOT NULL DEFAULT 0, next_attempt_at BIGINT NOT NULL DEFAULT 0, lease_until BIGINT, lease_token TEXT,
 last_error TEXT, payload_json TEXT NOT NULL DEFAULT '{}', created_at BIGINT NOT NULL,
 UNIQUE(kind,record_id)
);
CREATE TABLE pos_stripe_events (
 id TEXT PRIMARY KEY, account_id TEXT NOT NULL, livemode BOOLEAN NOT NULL, company_id TEXT NOT NULL,
 event_type TEXT NOT NULL, payload_json TEXT NOT NULL, received_at BIGINT NOT NULL, applied_at BIGINT
);
CREATE TABLE pos_settlements (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, account_id TEXT NOT NULL, livemode BOOLEAN NOT NULL,
 kind TEXT NOT NULL, provider_id TEXT NOT NULL, amount_minor BIGINT NOT NULL, fee_minor BIGINT NOT NULL DEFAULT 0,
 currency TEXT NOT NULL, state TEXT NOT NULL, details_json TEXT NOT NULL DEFAULT '{}', journal_id TEXT, bank_transaction_id TEXT UNIQUE,
 created_at BIGINT NOT NULL, UNIQUE(account_id,livemode,kind,provider_id)
);
CREATE TABLE pos_receipts (id TEXT PRIMARY KEY, company_id TEXT NOT NULL, request_hash TEXT NOT NULL, result_json TEXT NOT NULL, created_at BIGINT NOT NULL);
CREATE TABLE pos_audit (id TEXT PRIMARY KEY, company_id TEXT NOT NULL, branch_id TEXT, actor_id TEXT NOT NULL, operation TEXT NOT NULL, record_id TEXT NOT NULL, details_json TEXT NOT NULL, created_at BIGINT NOT NULL);
CREATE TABLE pos_postings (source_key TEXT PRIMARY KEY, company_id TEXT NOT NULL, journal_id TEXT NOT NULL UNIQUE REFERENCES finance_journals(id));
CREATE INDEX pos_sales_company_branch ON pos_sales(company_id,branch_id,created_at DESC);
CREATE INDEX pos_jobs_due ON pos_jobs(state,next_attempt_at);
-- Publish new built-in revisions; never rewrite previous grants/revisions.
INSERT INTO iam_roles VALUES('*','pos_cashier','POS cashier',true,1,'active'),('*','pos_server','POS server',true,1,'active'),('*','pos_kitchen','POS kitchen',true,1,'active'),('*','pos_manager','POS manager',true,1,'active');
INSERT INTO iam_role_revisions VALUES
('*','pos_cashier',1,'["iam.self","self.requests","self.tasks","pos.read","pos.sell","pos.kitchen"]','[]','["holding","branch"]','system',0),
('*','pos_server',1,'["iam.self","self.requests","self.tasks","pos.read","pos.sell","pos.kitchen"]','[]','["holding","branch"]','system',0),
('*','pos_kitchen',1,'["iam.self","self.requests","self.tasks","pos.read","pos.kitchen"]','[]','["holding","branch"]','system',0),
('*','pos_manager',1,'["iam.self","self.requests","self.tasks","pos.read","pos.sell","pos.kitchen","pos.approve","pos.reports"]','[]','["holding","branch"]','system',0);
WITH additions(role_id,permissions) AS (VALUES
('holding_owner','["pos.read","pos.sell","pos.kitchen","pos.approve","pos.configure","pos.payments.configure","pos.reports"]'::jsonb),
('finance_approver','["pos.read","pos.configure","pos.payments.configure","pos.reports"]'::jsonb),
('finance_preparer','["pos.read","pos.reports"]'::jsonb),('treasury','["pos.read","pos.reports"]'::jsonb),('auditor','["pos.read","pos.reports"]'::jsonb))
INSERT INTO iam_role_revisions SELECT r.company_id,r.id,r.published_revision+1,v.permissions_json||a.permissions,
 CASE WHEN r.id='holding_owner' THEN v.grantable_json||a.permissions ELSE v.grantable_json END,v.scopes_json,'system',floor(extract(epoch FROM clock_timestamp())*1000)::bigint
 FROM iam_roles r JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision JOIN additions a ON a.role_id=r.id WHERE r.company_id='*';
UPDATE iam_roles r SET published_revision=(SELECT MAX(revision) FROM iam_role_revisions v WHERE v.company_id=r.company_id AND v.role_id=r.id) WHERE company_id='*' AND id IN ('holding_owner','finance_approver','finance_preparer','treasury','auditor');
INSERT INTO iam_permissions VALUES('pos.read','{"id": "pos.read", "label": "pos.read", "module": "pos", "roles": ["holding_owner", "pos_cashier", "pos_server", "pos_kitchen", "pos_manager", "finance_preparer", "finance_approver", "treasury", "auditor"], "conditions": [], "risk": "normal", "availability": "live", "namespace": "customer", "permittedScopes": ["holding", "branch"]}') ON CONFLICT(id) DO UPDATE SET definition=excluded.definition;
INSERT INTO iam_permissions VALUES('pos.sell','{"id": "pos.sell", "label": "pos.sell", "module": "pos", "roles": ["holding_owner", "pos_cashier", "pos_server", "pos_manager"], "conditions": [], "risk": "normal", "availability": "live", "namespace": "customer", "permittedScopes": ["holding", "branch"]}') ON CONFLICT(id) DO UPDATE SET definition=excluded.definition;
INSERT INTO iam_permissions VALUES('pos.kitchen','{"id": "pos.kitchen", "label": "pos.kitchen", "module": "pos", "roles": ["holding_owner", "pos_cashier", "pos_server", "pos_kitchen", "pos_manager"], "conditions": [], "risk": "normal", "availability": "live", "namespace": "customer", "permittedScopes": ["holding", "branch"]}') ON CONFLICT(id) DO UPDATE SET definition=excluded.definition;
INSERT INTO iam_permissions VALUES('pos.approve','{"id": "pos.approve", "label": "pos.approve", "module": "pos", "roles": ["holding_owner", "pos_manager"], "conditions": [], "risk": "privileged", "availability": "live", "namespace": "customer", "permittedScopes": ["holding", "branch"]}') ON CONFLICT(id) DO UPDATE SET definition=excluded.definition;
INSERT INTO iam_permissions VALUES('pos.configure','{"id": "pos.configure", "label": "pos.configure", "module": "pos", "roles": ["holding_owner", "finance_approver"], "conditions": [], "risk": "privileged", "availability": "live", "namespace": "customer", "permittedScopes": ["holding"]}') ON CONFLICT(id) DO UPDATE SET definition=excluded.definition;
INSERT INTO iam_permissions VALUES('pos.payments.configure','{"id": "pos.payments.configure", "label": "pos.payments.configure", "module": "pos", "roles": ["holding_owner", "finance_approver"], "conditions": [], "risk": "privileged", "availability": "live", "namespace": "customer", "permittedScopes": ["holding"]}') ON CONFLICT(id) DO UPDATE SET definition=excluded.definition;
INSERT INTO iam_permissions VALUES('pos.reports','{"id": "pos.reports", "label": "pos.reports", "module": "pos", "roles": ["holding_owner", "pos_manager", "finance_preparer", "finance_approver", "treasury", "auditor"], "conditions": [], "risk": "normal", "availability": "live", "namespace": "customer", "permittedScopes": ["holding", "branch"]}') ON CONFLICT(id) DO UPDATE SET definition=excluded.definition;
