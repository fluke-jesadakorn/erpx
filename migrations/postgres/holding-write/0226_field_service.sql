CREATE TABLE field_service_agreements (
 id TEXT PRIMARY KEY,company_id TEXT NOT NULL,branch_id TEXT NOT NULL,department_id TEXT,customer_id TEXT NOT NULL REFERENCES finance_parties(id),
 asset_id TEXT REFERENCES operational_assets(id),title TEXT NOT NULL,start_date TEXT NOT NULL,end_date TEXT NOT NULL,currency TEXT NOT NULL,visit_rate_minor BIGINT NOT NULL CHECK(visit_rate_minor>=0),
 status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','active','rejected','retired')),created_by TEXT NOT NULL,reviewed_by TEXT,reviewed_at BIGINT,
 version BIGINT NOT NULL DEFAULT 1,created_at BIGINT NOT NULL,updated_at BIGINT NOT NULL,CHECK(end_date>=start_date)
);
CREATE INDEX field_service_agreement_scope ON field_service_agreements(company_id,branch_id,department_id,status,end_date);
CREATE TABLE field_service_agreement_events (
 id TEXT PRIMARY KEY,agreement_id TEXT NOT NULL REFERENCES field_service_agreements(id),version BIGINT NOT NULL,action TEXT NOT NULL,
 actor_id TEXT NOT NULL,reason TEXT NOT NULL,status TEXT NOT NULL,created_at BIGINT NOT NULL,UNIQUE(agreement_id,version)
);
CREATE TABLE field_service_orders (
 id TEXT PRIMARY KEY,company_id TEXT NOT NULL,branch_id TEXT NOT NULL,department_id TEXT,
 ticket_id TEXT NOT NULL REFERENCES support_tickets(id),ticket_version BIGINT NOT NULL,agreement_id TEXT NOT NULL REFERENCES field_service_agreements(id),agreement_version BIGINT NOT NULL,
 asset_id TEXT REFERENCES operational_assets(id),issue TEXT NOT NULL,site TEXT NOT NULL,technician_id TEXT NOT NULL,scheduled_at BIGINT,
 status TEXT NOT NULL DEFAULT 'open' CHECK(status IN ('open','scheduled','dispatched','in_visit','exception','completed','cancelled','billed')),
 exception_reason TEXT,visit_started_at BIGINT,visit_completed_at BIGINT,labor_minutes BIGINT,completion_evidence TEXT,customer_confirmation TEXT,
 version BIGINT NOT NULL DEFAULT 1,created_by TEXT NOT NULL,created_at BIGINT NOT NULL,updated_at BIGINT NOT NULL,
 CHECK(status<>'exception' OR exception_reason IS NOT NULL),CHECK(status NOT IN ('completed','billed') OR (completion_evidence IS NOT NULL AND customer_confirmation IS NOT NULL AND labor_minutes IS NOT NULL))
);
CREATE INDEX field_service_order_queue ON field_service_orders(company_id,branch_id,department_id,status,scheduled_at);
CREATE TABLE field_service_order_events (
 id TEXT PRIMARY KEY,order_id TEXT NOT NULL REFERENCES field_service_orders(id),version BIGINT NOT NULL,action TEXT NOT NULL,actor_id TEXT NOT NULL,
 reason TEXT NOT NULL,status TEXT NOT NULL,technician_id TEXT NOT NULL,scheduled_at BIGINT,details_json JSONB NOT NULL,created_at BIGINT NOT NULL,UNIQUE(order_id,version)
);
CREATE TABLE field_service_part_reservations (
 id TEXT PRIMARY KEY,order_id TEXT NOT NULL REFERENCES field_service_orders(id),warehouse_id TEXT NOT NULL,product_id TEXT NOT NULL,
 quantity_milli BIGINT NOT NULL CHECK(quantity_milli>0),consumed_milli BIGINT NOT NULL DEFAULT 0 CHECK(consumed_milli>=0),released_milli BIGINT NOT NULL DEFAULT 0 CHECK(released_milli>=0),
 expires_at BIGINT NOT NULL,created_by TEXT NOT NULL,reason TEXT NOT NULL,release_reason TEXT,created_at BIGINT NOT NULL,
 CHECK(consumed_milli+released_milli<=quantity_milli),CHECK(expires_at>created_at),FOREIGN KEY(warehouse_id,product_id) REFERENCES commerce_stock(warehouse_id,product_id)
);
CREATE INDEX field_service_part_stock ON field_service_part_reservations(warehouse_id,product_id,expires_at);
CREATE TABLE field_service_billing_releases (
 id TEXT PRIMARY KEY,order_id TEXT NOT NULL UNIQUE REFERENCES field_service_orders(id),agreement_id TEXT NOT NULL REFERENCES field_service_agreements(id),
 customer_id TEXT NOT NULL REFERENCES finance_parties(id),currency TEXT NOT NULL,amount_minor BIGINT NOT NULL CHECK(amount_minor>=0),
 source_order_version BIGINT NOT NULL,status TEXT NOT NULL DEFAULT 'released' CHECK(status='released'),released_by TEXT NOT NULL,reason TEXT NOT NULL,created_at BIGINT NOT NULL
);
CREATE OR REPLACE VIEW inventory_active_reservations AS
 SELECT warehouse_id,product_id,quantity_milli,'pos'::text source_kind,fulfillment_id source_id,NULL::text order_line_id FROM pos_reservations
 UNION ALL SELECT warehouse_id,product_id,quantity_milli-consumed_milli-released_milli,'sales',id,order_line_id FROM inventory_order_reservations WHERE quantity_milli>consumed_milli+released_milli AND expires_at>floor(extract(epoch FROM statement_timestamp())*1000)::bigint
 UNION ALL SELECT warehouse_id,product_id,quantity_milli-consumed_milli-released_milli,'field_service',id,NULL::text FROM field_service_part_reservations WHERE quantity_milli>consumed_milli+released_milli AND expires_at>floor(extract(epoch FROM statement_timestamp())*1000)::bigint;
CREATE TRIGGER field_service_part_reservation_changed AFTER INSERT OR UPDATE OR DELETE ON field_service_part_reservations FOR EACH ROW EXECUTE FUNCTION inventory_reservation_changed();
CREATE FUNCTION field_service_guard() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
 IF TG_TABLE_NAME IN ('field_service_agreement_events','field_service_order_events','field_service_billing_releases') OR TG_OP='DELETE' THEN RAISE EXCEPTION 'FIELD_SERVICE_HISTORY_IMMUTABLE' USING ERRCODE='23514'; END IF;
 IF TG_TABLE_NAME='field_service_agreements' THEN IF (NEW.id,NEW.company_id,NEW.branch_id,NEW.department_id,NEW.customer_id,NEW.asset_id,NEW.currency,NEW.visit_rate_minor,NEW.created_by,NEW.created_at) IS DISTINCT FROM (OLD.id,OLD.company_id,OLD.branch_id,OLD.department_id,OLD.customer_id,OLD.asset_id,OLD.currency,OLD.visit_rate_minor,OLD.created_by,OLD.created_at) THEN RAISE EXCEPTION 'FIELD_SERVICE_AGREEMENT_SOURCE_IMMUTABLE' USING ERRCODE='23514'; END IF;
 ELSIF TG_TABLE_NAME='field_service_orders' THEN IF (NEW.id,NEW.company_id,NEW.branch_id,NEW.department_id,NEW.ticket_id,NEW.ticket_version,NEW.agreement_id,NEW.agreement_version,NEW.asset_id,NEW.created_by,NEW.created_at) IS DISTINCT FROM (OLD.id,OLD.company_id,OLD.branch_id,OLD.department_id,OLD.ticket_id,OLD.ticket_version,OLD.agreement_id,OLD.agreement_version,OLD.asset_id,OLD.created_by,OLD.created_at) THEN RAISE EXCEPTION 'FIELD_SERVICE_ORDER_SOURCE_IMMUTABLE' USING ERRCODE='23514'; END IF; END IF;RETURN NEW;
END $$;
CREATE TRIGGER field_service_agreement_guard BEFORE UPDATE OR DELETE ON field_service_agreements FOR EACH ROW EXECUTE FUNCTION field_service_guard();
CREATE TRIGGER field_service_agreement_event_guard BEFORE UPDATE OR DELETE ON field_service_agreement_events FOR EACH ROW EXECUTE FUNCTION field_service_guard();
CREATE TRIGGER field_service_order_guard BEFORE UPDATE OR DELETE ON field_service_orders FOR EACH ROW EXECUTE FUNCTION field_service_guard();
CREATE TRIGGER field_service_order_event_guard BEFORE UPDATE OR DELETE ON field_service_order_events FOR EACH ROW EXECUTE FUNCTION field_service_guard();
CREATE TRIGGER field_service_billing_guard BEFORE UPDATE OR DELETE ON field_service_billing_releases FOR EACH ROW EXECUTE FUNCTION field_service_guard();
DO $$ DECLARE t TEXT; BEGIN FOREACH t IN ARRAY ARRAY['field_service_agreements','field_service_orders','field_service_part_reservations','field_service_billing_releases'] LOOP INSERT INTO work_cache_revisions(table_name) VALUES(t) ON CONFLICT DO NOTHING;EXECUTE format('CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION work_cache_changed()',t);EXECUTE format('CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON %I FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed()',t);END LOOP;END $$;
