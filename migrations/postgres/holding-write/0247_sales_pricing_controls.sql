-- Versioned sales price lists and independently reviewed discount evidence.
CREATE TABLE sales_price_lists (
 id TEXT PRIMARY KEY,
 company_id TEXT NOT NULL,
 name TEXT NOT NULL,
 currency TEXT NOT NULL CHECK(length(currency)=3 AND currency=upper(currency)),
 customer_id TEXT REFERENCES finance_parties(id),
 valid_from TEXT NOT NULL,
 valid_until TEXT NOT NULL,
 max_auto_discount_bps BIGINT NOT NULL DEFAULT 0 CHECK(max_auto_discount_bps BETWEEN 0 AND 10000),
 status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','active','superseded','cancelled')),
 version BIGINT NOT NULL DEFAULT 1 CHECK(version>0),
 created_by TEXT NOT NULL,
 approved_by TEXT,
 approval_reason TEXT,
 activated_at BIGINT,
 created_at BIGINT NOT NULL,
 updated_at BIGINT NOT NULL,
 CHECK(valid_until>=valid_from),
 CHECK(status<>'active' OR (approved_by IS NOT NULL AND approved_by<>created_by AND approval_reason IS NOT NULL AND activated_at IS NOT NULL))
);
CREATE UNIQUE INDEX sales_price_lists_active_scope ON sales_price_lists(company_id,COALESCE(customer_id,'')) WHERE status='active';
CREATE INDEX sales_price_lists_lookup ON sales_price_lists(company_id,customer_id,status,valid_from,valid_until);

CREATE TABLE sales_price_list_items (
 id TEXT PRIMARY KEY,
 price_list_id TEXT NOT NULL REFERENCES sales_price_lists(id),
 product_id TEXT NOT NULL REFERENCES commerce_products(id),
 price_minor BIGINT NOT NULL CHECK(price_minor>=0),
 tax_bps BIGINT NOT NULL CHECK(tax_bps BETWEEN 0 AND 10000),
 ordinal BIGINT NOT NULL,
 UNIQUE(price_list_id,product_id),
 UNIQUE(price_list_id,ordinal)
);

ALTER TABLE commerce_orders ADD COLUMN price_list_id TEXT REFERENCES sales_price_lists(id);
ALTER TABLE commerce_orders ADD COLUMN price_list_version BIGINT;
ALTER TABLE commerce_orders ADD COLUMN discount_reason TEXT;
ALTER TABLE commerce_orders ADD COLUMN discount_approval_required BIGINT NOT NULL DEFAULT 0 CHECK(discount_approval_required IN (0,1));
ALTER TABLE commerce_orders ADD COLUMN discount_approved_by TEXT;
ALTER TABLE commerce_orders ADD COLUMN discount_approved_at BIGINT;
ALTER TABLE commerce_order_lines ADD COLUMN list_price_minor BIGINT CHECK(list_price_minor>=0);
ALTER TABLE commerce_order_lines ADD COLUMN discount_bps BIGINT NOT NULL DEFAULT 0 CHECK(discount_bps BETWEEN 0 AND 10000);

CREATE FUNCTION sales_price_item_guard() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE state TEXT;
BEGIN
 SELECT status INTO state FROM sales_price_lists WHERE id=COALESCE(NEW.price_list_id,OLD.price_list_id);
 IF state<>'draft' THEN RAISE EXCEPTION 'SALES_PRICE_LIST_IMMUTABLE' USING ERRCODE='23514'; END IF;
 RETURN COALESCE(NEW,OLD);
END $$;
CREATE TRIGGER sales_price_item_guard BEFORE INSERT OR UPDATE OR DELETE ON sales_price_list_items FOR EACH ROW EXECUTE FUNCTION sales_price_item_guard();

CREATE FUNCTION sales_price_list_guard() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='DELETE' THEN RAISE EXCEPTION 'SALES_PRICE_LIST_IMMUTABLE' USING ERRCODE='23514'; END IF;
 IF OLD.status IN ('active','superseded','cancelled') AND NEW IS DISTINCT FROM OLD AND NOT (OLD.status='active' AND NEW.status='superseded' AND NEW.version=OLD.version+1) THEN
  RAISE EXCEPTION 'SALES_PRICE_LIST_IMMUTABLE' USING ERRCODE='23514';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER sales_price_list_guard BEFORE UPDATE OR DELETE ON sales_price_lists FOR EACH ROW EXECUTE FUNCTION sales_price_list_guard();

DO $$ DECLARE t TEXT; BEGIN
 FOREACH t IN ARRAY ARRAY['sales_price_lists','sales_price_list_items'] LOOP
  INSERT INTO work_cache_revisions(table_name) VALUES(t) ON CONFLICT DO NOTHING;
  EXECUTE format('CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION work_cache_changed()',t);
  EXECUTE format('CREATE TRIGGER work_cache_truncate AFTER TRUNCATE ON %I FOR EACH STATEMENT EXECUTE FUNCTION work_cache_changed()',t);
 END LOOP;
END $$;
