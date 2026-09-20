-- PMS records are typed domain documents. A property row is the serialization
-- boundary for inventory, stay transitions, folios and end-of-day operations.
CREATE TABLE pms_properties (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, branch_id TEXT NOT NULL,
 name TEXT NOT NULL, business_date TEXT NOT NULL, state TEXT NOT NULL DEFAULT 'setup'
 CHECK(state IN ('setup','open','closing')), config_json TEXT NOT NULL CHECK(erpx_json_valid(config_json)),
 version BIGINT NOT NULL DEFAULT 1, created_by TEXT NOT NULL, updated_at BIGINT NOT NULL,
 UNIQUE(company_id,branch_id)
);
CREATE TABLE pms_records (
 id TEXT PRIMARY KEY, property_id TEXT NOT NULL REFERENCES pms_properties(id),
 kind TEXT NOT NULL CHECK(kind IN ('room_type','room','rate','guest','reservation','stay','folio','entry','task','block','shift','refund','item','movement','lost','credit','night')),
 data_json TEXT NOT NULL CHECK(erpx_json_valid(data_json)), version BIGINT NOT NULL DEFAULT 1,
 updated_at BIGINT NOT NULL
);
CREATE INDEX pms_records_property_kind ON pms_records(property_id,kind);
CREATE TABLE pms_sources (
 property_id TEXT NOT NULL REFERENCES pms_properties(id), source_key TEXT NOT NULL,
 record_id TEXT NOT NULL REFERENCES pms_records(id), PRIMARY KEY(property_id,source_key)
);
CREATE TABLE pms_postings (
 id TEXT PRIMARY KEY, property_id TEXT NOT NULL REFERENCES pms_properties(id),
 source_key TEXT NOT NULL, record_id TEXT NOT NULL, journal_id TEXT NOT NULL UNIQUE REFERENCES finance_journals(id),
 business_date TEXT NOT NULL, UNIQUE(property_id,source_key)
);
CREATE TABLE pms_documents (
 id TEXT PRIMARY KEY, property_id TEXT NOT NULL REFERENCES pms_properties(id), folio_id TEXT NOT NULL,
 kind TEXT NOT NULL CHECK(kind IN ('invoice','receipt','refund','credit_note')),
 finance_id TEXT, snapshot_json TEXT NOT NULL CHECK(erpx_json_valid(snapshot_json)), created_at BIGINT NOT NULL
);
CREATE TABLE pms_jobs (
 id TEXT PRIMARY KEY, property_id TEXT NOT NULL REFERENCES pms_properties(id), business_date TEXT NOT NULL,
 state TEXT NOT NULL DEFAULT 'pending' CHECK(state IN ('pending','running','blocked','completed')),
 actor_json TEXT NOT NULL CHECK(erpx_json_valid(actor_json)), error_code TEXT, updated_at BIGINT NOT NULL,
 UNIQUE(property_id,business_date)
);
CREATE TABLE pms_stock_moves (
 id TEXT PRIMARY KEY, property_id TEXT NOT NULL REFERENCES pms_properties(id),
 warehouse_id TEXT NOT NULL REFERENCES commerce_warehouses(id), product_id TEXT NOT NULL REFERENCES commerce_products(id),
 quantity_milli BIGINT NOT NULL, value_minor BIGINT NOT NULL, movement_date TEXT NOT NULL,
 created_at BIGINT NOT NULL, source_number TEXT NOT NULL
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
 UNION ALL SELECT id,warehouse_id,product_id,quantity_milli,value_minor,created_at,movement_date,'pms',property_id,source_number FROM pms_stock_moves;

-- Shared Finance endpoints must never mutate a PMS-owned subledger record.
CREATE FUNCTION pms_protect_finance() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE protected BOOLEAN := false;
BEGIN
 IF TG_TABLE_NAME='finance_journals' THEN protected := OLD.source_type='pms';
 ELSIF TG_TABLE_NAME='finance_invoices' THEN protected := OLD.source_type='pms';
 ELSIF TG_TABLE_NAME='finance_payments' THEN
  SELECT EXISTS(SELECT 1 FROM pms_documents WHERE finance_id=OLD.id AND kind IN ('receipt','refund')) INTO protected;
 ELSIF TG_TABLE_NAME='finance_allocations' THEN
  SELECT EXISTS(SELECT 1 FROM finance_invoices WHERE id=NEW.invoice_id AND source_type='pms')
   OR EXISTS(SELECT 1 FROM pms_documents WHERE finance_id=NEW.payment_id AND kind IN ('receipt','refund')) INTO protected;
 END IF;
 IF protected AND current_setting('erpx.pms_write',true) IS DISTINCT FROM 'on' THEN
  RAISE EXCEPTION 'PMS_OWNER_WORKFLOW_REQUIRED' USING ERRCODE='23514';
 END IF;
 IF TG_OP='DELETE' THEN RETURN OLD; ELSE RETURN NEW; END IF;
END $$;
CREATE TRIGGER pms_journal_owner BEFORE UPDATE OR DELETE ON finance_journals FOR EACH ROW EXECUTE FUNCTION pms_protect_finance();
CREATE TRIGGER pms_invoice_owner BEFORE UPDATE OR DELETE ON finance_invoices FOR EACH ROW EXECUTE FUNCTION pms_protect_finance();
CREATE TRIGGER pms_payment_owner BEFORE UPDATE OR DELETE ON finance_payments FOR EACH ROW EXECUTE FUNCTION pms_protect_finance();
CREATE TRIGGER pms_allocation_owner BEFORE INSERT OR UPDATE ON finance_allocations FOR EACH ROW EXECUTE FUNCTION pms_protect_finance();

-- PMS built-in roles and immutable capability revisions.
INSERT INTO iam_permissions VALUES('module.pms.read','{"id":"module.pms.read","label":"Open pms","module":"pms","roles":["holding_owner","pms_reservation","pms_frontdesk","pms_cashier","pms_housekeeping","pms_supervisor","pms_maintenance","pms_night_auditor","pms_manager"],"conditions":[],"risk":"normal","availability":"live","namespace":"customer"}') ON CONFLICT(id) DO UPDATE SET definition=excluded.definition;
INSERT INTO iam_permissions VALUES('pms.read','{"id":"pms.read","label":"View hotel operations","module":"pms","roles":["holding_owner","pms_reservation","pms_frontdesk","pms_cashier","pms_housekeeping","pms_supervisor","pms_maintenance","pms_night_auditor","pms_manager","holding_owner","finance_preparer","finance_approver","treasury","auditor"],"conditions":[],"risk":"normal","availability":"live","namespace":"customer","permittedScopes":["holding","branch"]}') ON CONFLICT(id) DO UPDATE SET definition=excluded.definition;
INSERT INTO iam_permissions VALUES('pms.configure','{"id":"pms.configure","label":"Configure hotel rooms and rates","module":"pms","roles":["holding_owner","pms_manager"],"conditions":[],"risk":"normal","availability":"live","namespace":"customer","permittedScopes":["holding","branch"]}') ON CONFLICT(id) DO UPDATE SET definition=excluded.definition;
INSERT INTO iam_permissions VALUES('pms.reservations','{"id":"pms.reservations","label":"Manage hotel reservations","module":"pms","roles":["holding_owner","pms_reservation","pms_frontdesk","pms_manager"],"conditions":[],"risk":"normal","availability":"live","namespace":"customer","permittedScopes":["holding","branch"]}') ON CONFLICT(id) DO UPDATE SET definition=excluded.definition;
INSERT INTO iam_permissions VALUES('pms.frontdesk','{"id":"pms.frontdesk","label":"Check in and check out guests","module":"pms","roles":["holding_owner","pms_frontdesk","pms_manager"],"conditions":[],"risk":"normal","availability":"live","namespace":"customer","permittedScopes":["holding","branch"]}') ON CONFLICT(id) DO UPDATE SET definition=excluded.definition;
INSERT INTO iam_permissions VALUES('pms.cashier','{"id":"pms.cashier","label":"Manage guest folios and receipts","module":"pms","roles":["holding_owner","pms_cashier","pms_frontdesk","pms_manager","treasury"],"conditions":[],"risk":"normal","availability":"live","namespace":"customer","permittedScopes":["holding","branch"]}') ON CONFLICT(id) DO UPDATE SET definition=excluded.definition;
INSERT INTO iam_permissions VALUES('pms.housekeeping','{"id":"pms.housekeeping","label":"Work on hotel rooms and supplies","module":"pms","roles":["holding_owner","pms_housekeeping","pms_supervisor","pms_maintenance","pms_manager"],"conditions":[],"risk":"normal","availability":"live","namespace":"customer","permittedScopes":["holding","branch"]}') ON CONFLICT(id) DO UPDATE SET definition=excluded.definition;
INSERT INTO iam_permissions VALUES('pms.inspect','{"id":"pms.inspect","label":"Inspect hotel room readiness","module":"pms","roles":["holding_owner","pms_supervisor","pms_manager"],"conditions":["Independent room inspection"],"risk":"normal","availability":"live","namespace":"customer","permittedScopes":["holding","branch"]}') ON CONFLICT(id) DO UPDATE SET definition=excluded.definition;
INSERT INTO iam_permissions VALUES('pms.maintenance','{"id":"pms.maintenance","label":"Manage hotel maintenance blocks","module":"pms","roles":["holding_owner","pms_maintenance","pms_manager"],"conditions":[],"risk":"normal","availability":"live","namespace":"customer","permittedScopes":["holding","branch"]}') ON CONFLICT(id) DO UPDATE SET definition=excluded.definition;
INSERT INTO iam_permissions VALUES('pms.audit','{"id":"pms.audit","label":"Run hotel end of day","module":"pms","roles":["holding_owner","pms_night_auditor","pms_manager"],"conditions":[],"risk":"normal","availability":"live","namespace":"customer","permittedScopes":["holding","branch"]}') ON CONFLICT(id) DO UPDATE SET definition=excluded.definition;
INSERT INTO iam_permissions VALUES('pms.approve','{"id":"pms.approve","label":"Review hotel refunds, credit and cash differences","module":"pms","roles":["holding_owner","pms_manager","finance_approver"],"conditions":["Independent reviewer"],"risk":"privileged","availability":"live","namespace":"customer","permittedScopes":["holding","branch"]}') ON CONFLICT(id) DO UPDATE SET definition=excluded.definition;
INSERT INTO iam_permissions VALUES('pms.accounting.prepare','{"id":"pms.accounting.prepare","label":"Prepare PMS accounting mappings","module":"pms","roles":["holding_owner","finance_preparer"],"conditions":[],"risk":"privileged","availability":"live","namespace":"customer","permittedScopes":["holding","branch"]}') ON CONFLICT(id) DO UPDATE SET definition=excluded.definition;
INSERT INTO iam_permissions VALUES('pms.accounting.approve','{"id":"pms.accounting.approve","label":"Approve PMS accounting mappings","module":"pms","roles":["holding_owner","finance_approver"],"conditions":["Independent reviewer"],"risk":"privileged","availability":"live","namespace":"customer","permittedScopes":["holding","branch"]}') ON CONFLICT(id) DO UPDATE SET definition=excluded.definition;
INSERT INTO iam_permissions VALUES('pms.reports','{"id":"pms.reports","label":"View PMS financial reports","module":"pms","roles":["holding_owner","pms_manager","pms_night_auditor","holding_owner","finance_preparer","finance_approver","treasury","auditor"],"conditions":[],"risk":"normal","availability":"live","namespace":"customer","permittedScopes":["holding","branch"]}') ON CONFLICT(id) DO UPDATE SET definition=excluded.definition;
INSERT INTO iam_roles VALUES('*','pms_reservation','pms reservation',true,1,'active');
INSERT INTO iam_role_revisions VALUES('*','pms_reservation',1,'["iam.self","module.pms.read","records.read","records.write","workflow.pms_reservation","self.requests","self.tasks","pms.read","pms.reservations"]','[]','["holding","branch"]','system',0);
INSERT INTO iam_roles VALUES('*','pms_frontdesk','pms frontdesk',true,1,'active');
INSERT INTO iam_role_revisions VALUES('*','pms_frontdesk',1,'["iam.self","module.pms.read","records.read","records.write","workflow.pms_frontdesk","self.requests","self.tasks","pms.read","pms.reservations","pms.frontdesk","pms.cashier"]','[]','["holding","branch"]','system',0);
INSERT INTO iam_roles VALUES('*','pms_cashier','pms cashier',true,1,'active');
INSERT INTO iam_role_revisions VALUES('*','pms_cashier',1,'["iam.self","module.pms.read","records.read","records.write","workflow.pms_cashier","self.requests","self.tasks","pms.read","pms.cashier"]','[]','["holding","branch"]','system',0);
INSERT INTO iam_roles VALUES('*','pms_housekeeping','pms housekeeping',true,1,'active');
INSERT INTO iam_role_revisions VALUES('*','pms_housekeeping',1,'["iam.self","module.pms.read","records.read","records.write","workflow.pms_housekeeping","self.requests","self.tasks","pms.read","pms.housekeeping"]','[]','["holding","branch"]','system',0);
INSERT INTO iam_roles VALUES('*','pms_supervisor','pms supervisor',true,1,'active');
INSERT INTO iam_role_revisions VALUES('*','pms_supervisor',1,'["iam.self","module.pms.read","records.read","records.write","workflow.pms_supervisor","self.requests","self.tasks","pms.read","pms.housekeeping","pms.inspect"]','[]','["holding","branch"]','system',0);
INSERT INTO iam_roles VALUES('*','pms_maintenance','pms maintenance',true,1,'active');
INSERT INTO iam_role_revisions VALUES('*','pms_maintenance',1,'["iam.self","module.pms.read","records.read","records.write","workflow.pms_maintenance","self.requests","self.tasks","pms.read","pms.housekeeping","pms.maintenance"]','[]','["holding","branch"]','system',0);
INSERT INTO iam_roles VALUES('*','pms_night_auditor','pms night auditor',true,1,'active');
INSERT INTO iam_role_revisions VALUES('*','pms_night_auditor',1,'["iam.self","module.pms.read","records.read","records.write","workflow.pms_night_auditor","self.requests","self.tasks","pms.read","pms.audit","pms.reports"]','[]','["holding","branch"]','system',0);
INSERT INTO iam_roles VALUES('*','pms_manager','pms manager',true,1,'active');
INSERT INTO iam_role_revisions VALUES('*','pms_manager',1,'["iam.self","module.pms.read","records.read","records.write","workflow.pms_manager","self.requests","self.tasks","pms.read","pms.configure","pms.reservations","pms.frontdesk","pms.cashier","pms.housekeeping","pms.inspect","pms.maintenance","pms.audit","pms.approve","pms.reports"]','[]','["holding","branch"]','system',0);
INSERT INTO iam_role_revisions SELECT r.company_id,r.id,r.published_revision+1,v.permissions_json||'["module.pms.read","pms.read","pms.configure","pms.reservations","pms.frontdesk","pms.cashier","pms.housekeeping","pms.inspect","pms.maintenance","pms.audit","pms.approve","pms.accounting.prepare","pms.accounting.approve","pms.reports"]'::jsonb,v.grantable_json||'["module.pms.read","pms.read","pms.configure","pms.reservations","pms.frontdesk","pms.cashier","pms.housekeeping","pms.inspect","pms.maintenance","pms.audit","pms.approve","pms.accounting.prepare","pms.accounting.approve","pms.reports"]'::jsonb,v.scopes_json,'system',floor(extract(epoch FROM clock_timestamp())*1000)::bigint FROM iam_roles r JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision WHERE r.company_id='*' AND r.id='holding_owner';
UPDATE iam_roles SET published_revision=published_revision+1 WHERE company_id='*' AND id='holding_owner';
INSERT INTO iam_role_revisions SELECT r.company_id,r.id,r.published_revision+1,v.permissions_json||'["pms.read","pms.accounting.prepare","pms.reports"]'::jsonb,v.grantable_json||'[]'::jsonb,v.scopes_json,'system',floor(extract(epoch FROM clock_timestamp())*1000)::bigint FROM iam_roles r JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision WHERE r.company_id='*' AND r.id='finance_preparer';
UPDATE iam_roles SET published_revision=published_revision+1 WHERE company_id='*' AND id='finance_preparer';
INSERT INTO iam_role_revisions SELECT r.company_id,r.id,r.published_revision+1,v.permissions_json||'["pms.read","pms.approve","pms.accounting.approve","pms.reports"]'::jsonb,v.grantable_json||'[]'::jsonb,v.scopes_json,'system',floor(extract(epoch FROM clock_timestamp())*1000)::bigint FROM iam_roles r JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision WHERE r.company_id='*' AND r.id='finance_approver';
UPDATE iam_roles SET published_revision=published_revision+1 WHERE company_id='*' AND id='finance_approver';
INSERT INTO iam_role_revisions SELECT r.company_id,r.id,r.published_revision+1,v.permissions_json||'["pms.read","pms.cashier","pms.reports"]'::jsonb,v.grantable_json||'[]'::jsonb,v.scopes_json,'system',floor(extract(epoch FROM clock_timestamp())*1000)::bigint FROM iam_roles r JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision WHERE r.company_id='*' AND r.id='treasury';
UPDATE iam_roles SET published_revision=published_revision+1 WHERE company_id='*' AND id='treasury';
INSERT INTO iam_role_revisions SELECT r.company_id,r.id,r.published_revision+1,v.permissions_json||'["pms.read","pms.reports"]'::jsonb,v.grantable_json||'[]'::jsonb,v.scopes_json,'system',floor(extract(epoch FROM clock_timestamp())*1000)::bigint FROM iam_roles r JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision WHERE r.company_id='*' AND r.id='auditor';
UPDATE iam_roles SET published_revision=published_revision+1 WHERE company_id='*' AND id='auditor';
INSERT INTO iam_role_revisions SELECT r.company_id,r.id,r.published_revision+1,v.permissions_json||'[]'::jsonb,v.grantable_json||'["module.pms.read","pms.read","pms.configure","pms.reservations","pms.frontdesk","pms.cashier","pms.housekeeping","pms.inspect","pms.maintenance","pms.audit","pms.approve","pms.accounting.prepare","pms.accounting.approve","pms.reports"]'::jsonb,v.scopes_json,'system',floor(extract(epoch FROM clock_timestamp())*1000)::bigint FROM iam_roles r JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision WHERE r.company_id='*' AND r.id='hr_admin';
UPDATE iam_roles SET published_revision=published_revision+1 WHERE company_id='*' AND id='hr_admin';
