-- Whole product/warehouse quarantine. No historical inspections are inferred.
CREATE TABLE inventory_quality_holds (
 id TEXT PRIMARY KEY, company_id TEXT NOT NULL, branch_id TEXT NOT NULL,
 warehouse_id TEXT NOT NULL, product_id TEXT NOT NULL,
 reason TEXT NOT NULL, due_date TEXT NOT NULL,
 status TEXT NOT NULL DEFAULT 'held' CHECK(status IN ('held','awaiting_release','released')),
 created_by TEXT NOT NULL, last_inspector TEXT, last_result TEXT CHECK(last_result IN ('pass','fail')),
 released_by TEXT, release_reason TEXT, released_at BIGINT,
 version BIGINT NOT NULL DEFAULT 1 CHECK(version>0), created_at BIGINT NOT NULL, updated_at BIGINT NOT NULL,
 FOREIGN KEY(warehouse_id,product_id) REFERENCES commerce_stock(warehouse_id,product_id),
 CHECK(status<>'awaiting_release' OR (last_result='pass' AND last_inspector IS NOT NULL)),
 CHECK(status<>'released' OR (last_result='pass' AND released_by IS NOT NULL AND released_by<>created_by AND released_by<>last_inspector AND release_reason IS NOT NULL AND released_at IS NOT NULL))
);
CREATE TABLE inventory_quality_inspections (
 id TEXT PRIMARY KEY, hold_id TEXT NOT NULL REFERENCES inventory_quality_holds(id),
 reviewed_version BIGINT NOT NULL, result TEXT NOT NULL CHECK(result IN ('pass','fail')),
 evidence TEXT NOT NULL, inspected_by TEXT NOT NULL, created_at BIGINT NOT NULL
);
CREATE INDEX quality_hold_stock ON inventory_quality_holds(warehouse_id,product_id,status);
CREATE INDEX quality_hold_queue ON inventory_quality_holds(company_id,branch_id,status,due_date);
CREATE FUNCTION quality_inspection_immutable() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'QUALITY_INSPECTION_IMMUTABLE'; END $$;
CREATE TRIGGER quality_inspection_immutable BEFORE UPDATE OR DELETE ON inventory_quality_inspections FOR EACH ROW EXECUTE FUNCTION quality_inspection_immutable();
CREATE OR REPLACE FUNCTION pos_guard_reserved_stock() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.quantity_milli<OLD.quantity_milli AND EXISTS(SELECT 1 FROM inventory_quality_holds WHERE warehouse_id=NEW.warehouse_id AND product_id=NEW.product_id AND status<>'released') THEN
  RAISE EXCEPTION 'QUALITY_HOLD' USING ERRCODE='23514';
 END IF;
 IF NEW.quantity_milli < COALESCE((SELECT SUM(quantity_milli) FROM inventory_active_reservations WHERE warehouse_id=NEW.warehouse_id AND product_id=NEW.product_id),0) THEN
  RAISE EXCEPTION 'STOCK_RESERVED' USING ERRCODE='23514';
 END IF;
 RETURN NEW;
END $$;
CREATE OR REPLACE FUNCTION inventory_reservation_changed() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE w TEXT; p TEXT; increasing BOOLEAN:=FALSE;
BEGIN
 w:=COALESCE(NEW.warehouse_id,OLD.warehouse_id); p:=COALESCE(NEW.product_id,OLD.product_id);
 IF TG_OP='INSERT' THEN increasing:=TRUE;
 ELSIF TG_OP='UPDATE' THEN
  IF TG_TABLE_NAME='inventory_order_reservations' THEN increasing:=NEW.quantity_milli-NEW.consumed_milli-NEW.released_milli>OLD.quantity_milli-OLD.consumed_milli-OLD.released_milli;
  ELSE increasing:=NEW.quantity_milli>OLD.quantity_milli; END IF;
 END IF;
 IF increasing AND EXISTS(SELECT 1 FROM inventory_quality_holds WHERE warehouse_id=w AND product_id=p AND status<>'released') THEN RAISE EXCEPTION 'QUALITY_HOLD' USING ERRCODE='23514'; END IF;
 UPDATE commerce_stock SET version=version+1 WHERE warehouse_id=w AND product_id=p;
 RETURN NULL;
END $$;
CREATE FUNCTION quality_hold_changed() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 UPDATE commerce_stock SET version=version+1 WHERE warehouse_id=NEW.warehouse_id AND product_id=NEW.product_id;
 RETURN NULL;
END $$;
CREATE TRIGGER quality_hold_changed AFTER INSERT OR UPDATE ON inventory_quality_holds FOR EACH ROW EXECUTE FUNCTION quality_hold_changed();
DO $$ DECLARE t TEXT; BEGIN
 FOREACH t IN ARRAY ARRAY['inventory_quality_holds','inventory_quality_inspections'] LOOP
 INSERT INTO work_cache_revisions(table_name) VALUES(t) ON CONFLICT DO NOTHING;
 EXECUTE format('CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION work_cache_changed()',t);
 EXECUTE format('CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON %I FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed()',t);
 END LOOP;
END $$;
INSERT INTO iam_roles VALUES('*','quality_inspector','Quality inspector',true,1,'active'),('*','quality_manager','Quality manager',true,1,'active');
INSERT INTO iam_role_revisions VALUES
('*','quality_inspector',1,'["iam.self","self.requests","self.tasks","self.roster","leave.self","quality.read","quality.hold","quality.inspect"]','[]','["holding","branch"]','system',0),
('*','quality_manager',1,'["iam.self","self.requests","self.tasks","self.roster","leave.self","quality.read","quality.hold","quality.inspect","quality.release"]','[]','["holding","branch"]','system',0);
INSERT INTO iam_permissions VALUES('quality.read','{"id": "quality.read", "label": "quality.read", "module": "quality", "roles": ["holding_owner", "warehouse_manager", "operations_manager", "quality_inspector", "quality_manager", "auditor"], "conditions": ["Whole warehouse/product scope; independent inspection and release"], "risk": "normal", "availability": "live", "namespace": "customer", "permittedScopes": ["holding", "branch"]}');
INSERT INTO iam_permissions VALUES('quality.hold','{"id": "quality.hold", "label": "quality.hold", "module": "quality", "roles": ["holding_owner", "warehouse_manager", "operations_manager", "quality_inspector", "quality_manager"], "conditions": ["Whole warehouse/product scope; independent inspection and release"], "risk": "normal", "availability": "live", "namespace": "customer", "permittedScopes": ["holding", "branch"]}');
INSERT INTO iam_permissions VALUES('quality.inspect','{"id": "quality.inspect", "label": "quality.inspect", "module": "quality", "roles": ["holding_owner", "quality_inspector", "quality_manager"], "conditions": ["Whole warehouse/product scope; independent inspection and release"], "risk": "normal", "availability": "live", "namespace": "customer", "permittedScopes": ["holding", "branch"]}');
INSERT INTO iam_permissions VALUES('quality.release','{"id": "quality.release", "label": "quality.release", "module": "quality", "roles": ["holding_owner", "quality_manager"], "conditions": ["Whole warehouse/product scope; independent inspection and release"], "risk": "privileged", "availability": "live", "namespace": "customer", "permittedScopes": ["holding", "branch"]}');
INSERT INTO iam_role_revisions SELECT r.company_id,r.id,r.published_revision+1,v.permissions_json||'["quality.read", "quality.hold", "quality.inspect", "quality.release"]'::jsonb,v.grantable_json||'["quality.read", "quality.hold", "quality.inspect", "quality.release"]'::jsonb,v.scopes_json,'system',floor(extract(epoch FROM clock_timestamp())*1000)::bigint FROM iam_roles r JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision WHERE r.company_id='*' AND r.id='holding_owner';
UPDATE iam_roles SET published_revision=published_revision+1 WHERE company_id='*' AND id='holding_owner';
INSERT INTO iam_role_revisions SELECT r.company_id,r.id,r.published_revision+1,v.permissions_json||'["quality.read", "quality.hold"]'::jsonb,v.grantable_json,v.scopes_json,'system',floor(extract(epoch FROM clock_timestamp())*1000)::bigint FROM iam_roles r JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision WHERE r.company_id='*' AND r.id='warehouse_manager';
UPDATE iam_roles SET published_revision=published_revision+1 WHERE company_id='*' AND id='warehouse_manager';
INSERT INTO iam_role_revisions SELECT r.company_id,r.id,r.published_revision+1,v.permissions_json||'["quality.read", "quality.hold"]'::jsonb,v.grantable_json,v.scopes_json,'system',floor(extract(epoch FROM clock_timestamp())*1000)::bigint FROM iam_roles r JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision WHERE r.company_id='*' AND r.id='operations_manager';
UPDATE iam_roles SET published_revision=published_revision+1 WHERE company_id='*' AND id='operations_manager';
INSERT INTO iam_role_revisions SELECT r.company_id,r.id,r.published_revision+1,v.permissions_json||'["quality.read"]'::jsonb,v.grantable_json,v.scopes_json,'system',floor(extract(epoch FROM clock_timestamp())*1000)::bigint FROM iam_roles r JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision WHERE r.company_id='*' AND r.id='auditor';
UPDATE iam_roles SET published_revision=published_revision+1 WHERE company_id='*' AND id='auditor';
