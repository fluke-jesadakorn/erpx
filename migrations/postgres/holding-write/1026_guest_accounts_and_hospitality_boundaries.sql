-- Terminology-only upgrade. IDs, money, timestamps and issued evidence retain their values.
SELECT set_config('erpx.pms_write','on',true);
ALTER TABLE pms_records DROP CONSTRAINT pms_records_kind_check;
ALTER TABLE pms_records ADD CONSTRAINT pms_records_kind_check CHECK(kind IN ('room_type','room','rate','guest','reservation','stay','guest_account','entry','task','block','shift','refund','item','movement','lost','credit','night')) NOT VALID;
UPDATE pms_records SET kind='guest_account' WHERE kind='folio';
ALTER TABLE pms_records VALIDATE CONSTRAINT pms_records_kind_check;
CREATE FUNCTION erpx_guest_account_json(value jsonb) RETURNS jsonb LANGUAGE plpgsql IMMUTABLE AS $$
DECLARE result jsonb; item record; new_key text;
BEGIN
 IF jsonb_typeof(value)='array' THEN
  SELECT COALESCE(jsonb_agg(erpx_guest_account_json(v) ORDER BY ord),'[]'::jsonb) INTO result FROM jsonb_array_elements(value) WITH ORDINALITY e(v,ord); RETURN result;
 ELSIF jsonb_typeof(value)='object' THEN
  result := '{}'::jsonb;
  FOR item IN SELECT * FROM jsonb_each(value) LOOP
   new_key := CASE item.key WHEN 'folio' THEN 'guestAccount' WHEN 'folios' THEN 'guestAccounts'
    WHEN 'folioId' THEN 'guestAccountId' WHEN 'toFolioId' THEN 'toGuestAccountId' WHEN 'folioIds' THEN 'guestAccountIds'
    WHEN 'folio_id' THEN 'guest_account_id' WHEN 'folioEntryId' THEN 'guestAccountEntryId' WHEN 'folio_entry_id' THEN 'guest_account_entry_id' ELSE item.key END;
   IF value ? new_key AND new_key<>item.key AND value->new_key IS DISTINCT FROM item.value THEN RAISE EXCEPTION 'GUEST_ACCOUNT_REFERENCE_CONFLICT'; END IF;
   result := result || jsonb_build_object(new_key,erpx_guest_account_json(item.value));
  END LOOP; RETURN result;
 END IF;
 RETURN value;
END $$;
UPDATE pms_records SET data_json=erpx_guest_account_json(data_json::jsonb)::text WHERE data_json ~ '"(folio|folios|folioId|toFolioId|folioIds|folio_id|folioEntryId|folio_entry_id)"[[:space:]]*:';
-- Column renames also preserve immutable allocation/document rows byte for byte.
DO $$ DECLARE col record; BEGIN
 FOR col IN SELECT table_name,column_name FROM information_schema.columns WHERE table_schema='public' AND column_name='folio_id' LOOP
  EXECUTE format('ALTER TABLE %I RENAME COLUMN %I TO guest_account_id',col.table_name,col.column_name);
 END LOOP;
END $$;
ALTER INDEX pms_transfer_requests_folio_state RENAME TO pms_transfer_requests_guest_account_state;
ALTER INDEX pms_stripe_one_active_folio RENAME TO pms_stripe_one_active_guest_account;
CREATE OR REPLACE FUNCTION pms_counter_attempt_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF (OLD.account_id,OLD.livemode,OLD.amount_minor,OLD.kind,OLD.guest_account_id,OLD.property_id,OLD.company_id,OLD.clearing_account)
 IS DISTINCT FROM (NEW.account_id,NEW.livemode,NEW.amount_minor,NEW.kind,NEW.guest_account_id,NEW.property_id,NEW.company_id,NEW.clearing_account)
 THEN RAISE EXCEPTION 'PMS immutable counter receipt'; END IF;
 RETURN NEW;
END $$;
-- Issued documents, allocations, audits, command receipts and outbox evidence
-- keep their original JSON. The central compatibility adapter reads them.
UPDATE pms_jobs SET actor_json=erpx_guest_account_json(actor_json::jsonb)::text;
UPDATE pms_booking_emails SET payload_json=erpx_guest_account_json(payload_json::jsonb)::text;

-- Explicit hotel linkage allows independent restaurants to close their own day.
ALTER TABLE pos_registers ADD COLUMN property_id TEXT REFERENCES pms_properties(id);
UPDATE pos_registers r SET property_id=p.id FROM pms_properties p WHERE r.mode='restaurant' AND p.company_id=r.company_id AND p.branch_id=r.branch_id;
CREATE INDEX pos_registers_property ON pos_registers(property_id) WHERE property_id IS NOT NULL;
CREATE TABLE pos_room_charge_reversal_requests (
 id TEXT PRIMARY KEY,sale_id TEXT NOT NULL REFERENCES pos_sales(id),company_id TEXT NOT NULL,branch_id TEXT NOT NULL,
 property_id TEXT NOT NULL REFERENCES pms_properties(id),sale_version BIGINT NOT NULL,property_version BIGINT NOT NULL,
 amount_minor BIGINT NOT NULL CHECK(amount_minor>0),reason TEXT NOT NULL,restock_line_ids_json TEXT NOT NULL CHECK(erpx_json_valid(restock_line_ids_json)),
 state TEXT NOT NULL CHECK(state IN ('requested','pos_approved','approved','rejected')),version BIGINT NOT NULL DEFAULT 1,
 requested_by TEXT NOT NULL,pos_approved_by TEXT,pms_approved_by TEXT,rejected_by TEXT,review_reason TEXT,
 created_at BIGINT NOT NULL,updated_at BIGINT NOT NULL,
 CHECK(pos_approved_by IS NULL OR pos_approved_by<>requested_by),
 CHECK(pms_approved_by IS NULL OR (pms_approved_by<>requested_by AND pms_approved_by<>pos_approved_by))
);
CREATE UNIQUE INDEX pos_room_charge_reversal_pending ON pos_room_charge_reversal_requests(sale_id) WHERE state IN ('requested','pos_approved');
CREATE TABLE hospitality_permission_changes (
 company_id TEXT NOT NULL,role_id TEXT NOT NULL,before_revision INTEGER NOT NULL,after_revision INTEGER NOT NULL,
 before_permissions JSONB NOT NULL,after_permissions JSONB NOT NULL,created_at BIGINT NOT NULL,
 PRIMARY KEY(company_id,role_id,after_revision)
);
CREATE TRIGGER hospitality_permission_changes_immutable BEFORE UPDATE OR DELETE ON hospitality_permission_changes FOR EACH ROW EXECUTE FUNCTION iam_immutable();
-- Built-in roles lose automatic operational authority. Approved custom mixtures
-- retain their grants and gain accounting equivalents of the former setup grant.
WITH selected AS (
 SELECT r.*,v.permissions_json,v.grantable_json,v.scopes_json,
 CASE WHEN r.built_in AND r.id IN ('finance_preparer','finance_approver','treasury','auditor') THEN v.permissions_json-'pms.read'-'pms.cashier'-'pms.approve'-'pos.read'-'pos.approve'-'pos.configure'
 WHEN r.built_in AND r.id IN ('pos_cashier','pos_server') THEN v.permissions_json-'pos.kitchen' ELSE v.permissions_json END AS retained
 FROM iam_roles r JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision
), revised AS (
 SELECT *,retained || CASE WHEN id='pos_manager' AND built_in THEN '["pos.configure"]'::jsonb ELSE '[]'::jsonb END
 || CASE WHEN id='finance_preparer' AND built_in THEN '["pos.accounting.prepare"]'::jsonb
 WHEN id='finance_approver' AND built_in THEN '["pos.accounting.approve"]'::jsonb
 WHEN permissions_json ? 'pos.configure' AND (NOT built_in OR id='holding_owner') THEN '["pos.accounting.prepare","pos.accounting.approve"]'::jsonb ELSE '[]'::jsonb END AS new_permissions
 FROM selected
), inserted AS (
 INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)
 SELECT company_id,id,published_revision+1,(SELECT jsonb_agg(DISTINCT p) FROM jsonb_array_elements(new_permissions) p),
 grantable_json || CASE WHEN grantable_json ? 'pos.configure' THEN '["pos.accounting.prepare","pos.accounting.approve"]'::jsonb ELSE '[]'::jsonb END,
 scopes_json,'system',floor(extract(epoch FROM clock_timestamp())*1000)::bigint FROM revised
 WHERE new_permissions IS DISTINCT FROM permissions_json OR grantable_json ? 'pos.configure'
 RETURNING company_id,role_id,revision,permissions_json,created_at
)
INSERT INTO hospitality_permission_changes
 SELECT i.company_id,i.role_id,i.revision-1,i.revision,v.permissions_json,i.permissions_json,i.created_at
 FROM inserted i JOIN iam_role_revisions v ON v.company_id=i.company_id AND v.role_id=i.role_id AND v.revision=i.revision-1;
UPDATE iam_roles r SET published_revision=c.after_revision FROM hospitality_permission_changes c WHERE c.company_id=r.company_id AND c.role_id=r.id;
UPDATE active_work_contexts SET revision=revision+1 WHERE assignment_id IS NOT NULL;

INSERT INTO iam_permissions(id,definition) VALUES('pms.read','{"id":"pms.read","label":"View hotel operations","module":"pms","roles":["holding_owner","pms_reservation","pms_frontdesk","pms_cashier","pms_housekeeping","pms_supervisor","pms_maintenance","pms_night_auditor","pms_manager"],"conditions":[],"risk":"normal","availability":"live","namespace":"customer","permittedScopes":["holding","branch"]}'::jsonb) ON CONFLICT(id) DO UPDATE SET definition=excluded.definition;
INSERT INTO iam_permissions(id,definition) VALUES('pms.configure','{"id":"pms.configure","label":"Configure hotel rooms and rates","module":"pms","roles":["holding_owner","pms_manager"],"conditions":[],"risk":"normal","availability":"live","namespace":"customer","permittedScopes":["holding","branch"]}'::jsonb) ON CONFLICT(id) DO UPDATE SET definition=excluded.definition;
INSERT INTO iam_permissions(id,definition) VALUES('pms.reservations','{"id":"pms.reservations","label":"Manage hotel reservations","module":"pms","roles":["holding_owner","pms_reservation","pms_frontdesk","pms_manager"],"conditions":[],"risk":"normal","availability":"live","namespace":"customer","permittedScopes":["holding","branch"]}'::jsonb) ON CONFLICT(id) DO UPDATE SET definition=excluded.definition;
INSERT INTO iam_permissions(id,definition) VALUES('pms.frontdesk','{"id":"pms.frontdesk","label":"Check in and check out guests","module":"pms","roles":["holding_owner","pms_frontdesk","pms_manager"],"conditions":[],"risk":"normal","availability":"live","namespace":"customer","permittedScopes":["holding","branch"]}'::jsonb) ON CONFLICT(id) DO UPDATE SET definition=excluded.definition;
INSERT INTO iam_permissions(id,definition) VALUES('pms.cashier','{"id":"pms.cashier","label":"Manage Guest Accounts and receipts","module":"pms","roles":["holding_owner","pms_cashier","pms_frontdesk","pms_manager"],"conditions":[],"risk":"normal","availability":"live","namespace":"customer","permittedScopes":["holding","branch"]}'::jsonb) ON CONFLICT(id) DO UPDATE SET definition=excluded.definition;
INSERT INTO iam_permissions(id,definition) VALUES('pms.housekeeping','{"id":"pms.housekeeping","label":"Work on hotel rooms and supplies","module":"pms","roles":["holding_owner","pms_housekeeping","pms_supervisor","pms_maintenance","pms_manager"],"conditions":[],"risk":"normal","availability":"live","namespace":"customer","permittedScopes":["holding","branch"]}'::jsonb) ON CONFLICT(id) DO UPDATE SET definition=excluded.definition;
INSERT INTO iam_permissions(id,definition) VALUES('pms.inspect','{"id":"pms.inspect","label":"Inspect hotel room readiness","module":"pms","roles":["holding_owner","pms_supervisor","pms_manager"],"conditions":["Independent room inspection"],"risk":"normal","availability":"live","namespace":"customer","permittedScopes":["holding","branch"]}'::jsonb) ON CONFLICT(id) DO UPDATE SET definition=excluded.definition;
INSERT INTO iam_permissions(id,definition) VALUES('pms.maintenance','{"id":"pms.maintenance","label":"Manage hotel maintenance blocks","module":"pms","roles":["holding_owner","pms_maintenance","pms_manager"],"conditions":[],"risk":"normal","availability":"live","namespace":"customer","permittedScopes":["holding","branch"]}'::jsonb) ON CONFLICT(id) DO UPDATE SET definition=excluded.definition;
INSERT INTO iam_permissions(id,definition) VALUES('pms.audit','{"id":"pms.audit","label":"Run hotel end of day","module":"pms","roles":["holding_owner","pms_night_auditor","pms_manager"],"conditions":[],"risk":"normal","availability":"live","namespace":"customer","permittedScopes":["holding","branch"]}'::jsonb) ON CONFLICT(id) DO UPDATE SET definition=excluded.definition;
INSERT INTO iam_permissions(id,definition) VALUES('pms.approve','{"id":"pms.approve","label":"Review hotel refunds, credit and cash differences","module":"pms","roles":["holding_owner","pms_manager"],"conditions":["Independent reviewer"],"risk":"privileged","availability":"live","namespace":"customer","permittedScopes":["holding","branch"]}'::jsonb) ON CONFLICT(id) DO UPDATE SET definition=excluded.definition;
INSERT INTO iam_permissions(id,definition) VALUES('pms.accounting.prepare','{"id":"pms.accounting.prepare","label":"Prepare PMS accounting mappings","module":"pms","roles":["holding_owner","finance_preparer"],"conditions":[],"risk":"privileged","availability":"live","namespace":"customer","permittedScopes":["holding","branch"]}'::jsonb) ON CONFLICT(id) DO UPDATE SET definition=excluded.definition;
INSERT INTO iam_permissions(id,definition) VALUES('pms.accounting.approve','{"id":"pms.accounting.approve","label":"Approve PMS accounting mappings","module":"pms","roles":["holding_owner","finance_approver"],"conditions":["Independent reviewer"],"risk":"privileged","availability":"live","namespace":"customer","permittedScopes":["holding","branch"]}'::jsonb) ON CONFLICT(id) DO UPDATE SET definition=excluded.definition;
INSERT INTO iam_permissions(id,definition) VALUES('pms.reports','{"id":"pms.reports","label":"View PMS financial reports","module":"pms","roles":["holding_owner","pms_manager","pms_night_auditor","holding_owner","finance_preparer","finance_approver","treasury","auditor"],"conditions":[],"risk":"normal","availability":"live","namespace":"customer","permittedScopes":["holding","branch"]}'::jsonb) ON CONFLICT(id) DO UPDATE SET definition=excluded.definition;
INSERT INTO iam_permissions(id,definition) VALUES('pos.read','{"id":"pos.read","label":"View point of sale","module":"pos","roles":["holding_owner","pos_cashier","pos_server","pos_kitchen","pos_manager"],"conditions":[],"risk":"normal","availability":"live","namespace":"customer","permittedScopes":["holding","branch"]}'::jsonb) ON CONFLICT(id) DO UPDATE SET definition=excluded.definition;
INSERT INTO iam_permissions(id,definition) VALUES('pos.sell','{"id":"pos.sell","label":"Sell and manage POS shifts","module":"pos","roles":["holding_owner","pos_cashier","pos_server","pos_manager"],"conditions":[],"risk":"normal","availability":"live","namespace":"customer","permittedScopes":["holding","branch"]}'::jsonb) ON CONFLICT(id) DO UPDATE SET definition=excluded.definition;
INSERT INTO iam_permissions(id,definition) VALUES('pos.kitchen','{"id":"pos.kitchen","label":"Operate POS kitchen","module":"pos","roles":["holding_owner","pos_kitchen","pos_manager"],"conditions":[],"risk":"normal","availability":"live","namespace":"customer","permittedScopes":["holding","branch"]}'::jsonb) ON CONFLICT(id) DO UPDATE SET definition=excluded.definition;
INSERT INTO iam_permissions(id,definition) VALUES('pos.approve','{"id":"pos.approve","label":"Review POS refunds and cash differences","module":"pos","roles":["holding_owner","pos_manager"],"conditions":["Independent reviewer"],"risk":"privileged","availability":"live","namespace":"customer","permittedScopes":["holding","branch"]}'::jsonb) ON CONFLICT(id) DO UPDATE SET definition=excluded.definition;
INSERT INTO iam_permissions(id,definition) VALUES('pos.configure','{"id":"pos.configure","label":"Configure POS products, menus, tables and registers","module":"pos","roles":["holding_owner","pos_manager"],"conditions":[],"risk":"normal","availability":"live","namespace":"customer","permittedScopes":["holding","branch"]}'::jsonb) ON CONFLICT(id) DO UPDATE SET definition=excluded.definition;
INSERT INTO iam_permissions(id,definition) VALUES('pos.accounting.prepare','{"id":"pos.accounting.prepare","label":"Prepare POS accounting mappings","module":"pos","roles":["holding_owner","finance_preparer"],"conditions":[],"risk":"privileged","availability":"live","namespace":"customer","permittedScopes":["holding"]}'::jsonb) ON CONFLICT(id) DO UPDATE SET definition=excluded.definition;
INSERT INTO iam_permissions(id,definition) VALUES('pos.accounting.approve','{"id":"pos.accounting.approve","label":"Approve POS accounting mappings","module":"pos","roles":["holding_owner","finance_approver"],"conditions":["Independent reviewer"],"risk":"privileged","availability":"live","namespace":"customer","permittedScopes":["holding"]}'::jsonb) ON CONFLICT(id) DO UPDATE SET definition=excluded.definition;
INSERT INTO iam_permissions(id,definition) VALUES('pos.payments.configure','{"id":"pos.payments.configure","label":"Connect company Stripe account","module":"pos","roles":["holding_owner","finance_approver"],"conditions":[],"risk":"privileged","availability":"live","namespace":"customer","permittedScopes":["holding"]}'::jsonb) ON CONFLICT(id) DO UPDATE SET definition=excluded.definition;
INSERT INTO iam_permissions(id,definition) VALUES('pos.reports','{"id":"pos.reports","label":"Read POS financial reports","module":"pos","roles":["holding_owner","pos_manager","holding_owner","finance_preparer","finance_approver","treasury","auditor"],"conditions":[],"risk":"normal","availability":"live","namespace":"customer","permittedScopes":["holding","branch"]}'::jsonb) ON CONFLICT(id) DO UPDATE SET definition=excluded.definition;

-- Existing reviewed mappings retain their acceptance; new revisions need two people.
ALTER TABLE pos_settings ADD COLUMN accounting_prepared_by TEXT;
ALTER TABLE pos_settings ADD COLUMN accounting_approved_by TEXT;
UPDATE pos_settings SET accounting_prepared_by='legacy-reviewed',accounting_approved_by='legacy-reviewed';
INSERT INTO work_cache_revisions(table_name) VALUES('pos_room_charge_reversal_requests') ON CONFLICT DO NOTHING;
CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON pos_room_charge_reversal_requests FOR EACH ROW EXECUTE FUNCTION work_cache_changed();
