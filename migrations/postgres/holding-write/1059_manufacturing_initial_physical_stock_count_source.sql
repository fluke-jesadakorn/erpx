-- Named Manufacturing initial physical count. This is current-period variance, never
-- an opening balance/acquisition. Generic Supply Count authority is unchanged.
CREATE INDEX inventory_mfg_initial_physical_receipt_record ON command_receipts(record_id) WHERE operation='inventory.count.create' AND jsonb_exists(result_json::jsonb,'_manufacturingInitialPhysicalStockCount');
CREATE TABLE inventory_mfg_initial_physical_sources (
 count_id text PRIMARY KEY REFERENCES inventory_counts(id),
 company_id text NOT NULL,
 branch_id text NOT NULL,
 warehouse_id text NOT NULL REFERENCES commerce_warehouses(id),
 product_id text NOT NULL REFERENCES commerce_products(id),
 count_command_id text NOT NULL UNIQUE REFERENCES command_receipts(command_id),
 manufacturing_setup_command_id text NOT NULL REFERENCES command_receipts(command_id),
 chart_id text NOT NULL REFERENCES finance_charts(id),
 inventory_account_id text NOT NULL REFERENCES finance_accounts(id),
 variance_account_id text NOT NULL REFERENCES finance_accounts(id),
 currency text NOT NULL CHECK(currency ~ '^[A-Z]{3}$'),
 proof_json jsonb NOT NULL CHECK(jsonb_typeof(proof_json)='object'),
 created_by text NOT NULL,
 created_at bigint NOT NULL
);
CREATE INDEX inventory_mfg_initial_physical_source_company ON inventory_mfg_initial_physical_sources(company_id,count_id);
CREATE INDEX inventory_mfg_initial_physical_source_goods_receipt ON inventory_mfg_initial_physical_sources(manufacturing_setup_command_id);
CREATE FUNCTION inventory_mfg_initial_physical_source_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='MANUFACTURING_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED',DETAIL='Initial physical count provenance is immutable.'; END $$;
CREATE TRIGGER inventory_mfg_initial_physical_source_immutable BEFORE UPDATE OR DELETE ON inventory_mfg_initial_physical_sources FOR EACH ROW EXECUTE FUNCTION inventory_mfg_initial_physical_source_immutable();

-- A native source may be born only with the original create receipt INSERT.
-- Updating an old ordinary Count receipt cannot adopt it into this feature.
CREATE FUNCTION inventory_mfg_initial_physical_marker_birth() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='UPDATE' AND (OLD.result_json::jsonb->'_manufacturingInitialPhysicalStockCount' IS DISTINCT FROM NEW.result_json::jsonb->'_manufacturingInitialPhysicalStockCount') THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='MANUFACTURING_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED',DETAIL='The private initial Count marker can only be introduced by its original receipt insert and remains immutable.';
 END IF;
 IF jsonb_exists(NEW.result_json::jsonb,'_manufacturingInitialPhysicalStockCount') AND (NEW.operation<>'inventory.count.create' OR jsonb_exists(NEW.result_json::jsonb,'_initialPhysicalStockCount')) THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='MANUFACTURING_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED',DETAIL='The private initial Count marker belongs only to its original Count create operation.';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER inventory_mfg_initial_physical_marker_birth BEFORE INSERT OR UPDATE ON command_receipts FOR EACH ROW EXECUTE FUNCTION inventory_mfg_initial_physical_marker_birth();

CREATE FUNCTION inventory_assert_mfg_initial_physical_source(target text) RETURNS void LANGUAGE plpgsql AS $$
DECLARE source inventory_mfg_initial_physical_sources%ROWTYPE; marked boolean;
BEGIN
 SELECT * INTO source FROM inventory_mfg_initial_physical_sources WHERE count_id=target;
 SELECT EXISTS(SELECT 1 FROM command_receipts r WHERE r.record_id=target AND r.operation='inventory.count.create' AND jsonb_exists(r.result_json::jsonb,'_manufacturingInitialPhysicalStockCount')) INTO marked;
 IF source.count_id IS NULL AND NOT marked THEN RETURN; END IF;
 IF source.count_id IS NULL OR NOT marked OR NOT EXISTS(
  SELECT 1 FROM inventory_counts c
  JOIN inventory_count_lines line ON line.count_id=c.id AND line.product_id=source.product_id AND line.snapshot_quantity_milli=0 AND line.snapshot_value_minor=0 AND line.snapshot_version=0
  JOIN commerce_warehouses warehouse ON warehouse.id=c.warehouse_id AND warehouse.company_id=c.company_id
  JOIN commerce_products product ON product.id=line.product_id AND product.company_id=c.company_id
  JOIN finance_charts chart ON chart.id=source.chart_id AND chart.company_id=c.company_id
  JOIN finance_accounts inventory ON inventory.id=source.inventory_account_id AND inventory.company_id=c.company_id
  JOIN finance_accounts variance ON variance.id=source.variance_account_id AND variance.company_id=c.company_id
  JOIN command_receipts receipt ON receipt.command_id=source.count_command_id AND receipt.record_id=c.id AND receipt.operation='inventory.count.create' AND receipt.actor_id=c.created_by AND receipt.authority_json::jsonb->>'companyId'=c.company_id
  JOIN command_receipts goods ON goods.command_id=source.manufacturing_setup_command_id AND goods.record_id=c.company_id AND goods.operation='commerce.manufacturing-posting-setup.initialize' AND goods.authority_json::jsonb->>'companyId'=c.company_id AND goods.result_json::jsonb->>'id'=c.company_id AND goods.result_json::jsonb->>'status'='configured' AND goods.result_json::jsonb->>'version'='1'
  WHERE c.id=source.count_id AND c.company_id=source.company_id AND c.branch_id=source.branch_id AND c.warehouse_id=source.warehouse_id AND c.created_by=source.created_by AND c.id='count-'||source.count_command_id AND c.quality_hold_id IS NULL
  AND (SELECT count(*) FROM inventory_count_lines all_lines WHERE all_lines.count_id=c.id)=1
  AND NOT jsonb_exists(receipt.result_json::jsonb,'_initialPhysicalStockCount') AND receipt.result_json::jsonb->>'id'=c.id AND receipt.result_json::jsonb->>'status'='draft' AND receipt.result_json::jsonb->>'version'='1'
  AND receipt.result_json::jsonb->'_manufacturingInitialPhysicalStockCount'=source.proof_json
  AND source.proof_json=jsonb_build_object('kind','manufacturing-initial-physical-stock-count','countId',c.id,'companyId',c.company_id,'warehouseId',c.warehouse_id,'branchId',source.branch_id,'productId',line.product_id,'countCommandId',receipt.command_id,'manufacturingSetupCommandId',goods.command_id,'manufacturingSetupRequestHash',goods.request_hash,'chartId',source.chart_id,'inventoryAccountId',source.inventory_account_id,'varianceAccountId',source.variance_account_id,'currency',source.currency)
 ) THEN RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='MANUFACTURING_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED',DETAIL='The actual Count, immutable original receipt and initial source must reciprocally match.'; END IF;
END $$;
CREATE FUNCTION inventory_check_mfg_initial_physical_source() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE target text; goods_source record;
BEGIN
 IF TG_TABLE_NAME='inventory_mfg_initial_physical_sources' THEN
  target=NEW.count_id;
  -- Current mapping is a creation precondition, not a perpetual history gate.
  IF NOT EXISTS(SELECT 1 FROM commerce_settings settings
   JOIN commerce_warehouses warehouse ON warehouse.id=NEW.warehouse_id AND warehouse.company_id=NEW.company_id AND warehouse.branch_id=NEW.branch_id AND warehouse.status='active'
   JOIN commerce_products product ON product.id=NEW.product_id AND product.company_id=NEW.company_id AND product.item_type='stock' AND product.status='active'
   JOIN finance_charts chart ON chart.id=NEW.chart_id AND chart.company_id=NEW.company_id AND chart.status='active' AND chart.base_currency=NEW.currency AND NEW.currency='THB'
   JOIN finance_accounts inventory ON inventory.id=NEW.inventory_account_id AND inventory.company_id=NEW.company_id AND inventory.chart_id=chart.id AND inventory.currency=NEW.currency AND inventory.category='asset' AND inventory.control_kind='none' AND inventory.status='active'
   JOIN finance_accounts variance ON variance.id=NEW.variance_account_id AND variance.company_id=NEW.company_id AND variance.chart_id=chart.id AND variance.currency=NEW.currency AND variance.category='expense' AND variance.code='5400' AND variance.status='active'
   WHERE settings.company_id=NEW.company_id AND settings.chart_id=NEW.chart_id AND settings.inventory_account_id=NEW.inventory_account_id) THEN
   RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='MANUFACTURING_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED',DETAIL='Initial source must capture the actual Manufacturing mapping.';
  END IF;
 ELSIF TG_TABLE_NAME='inventory_count_lines' THEN
  IF TG_OP<>'INSERT' THEN PERFORM inventory_assert_mfg_initial_physical_source(OLD.count_id); END IF;
  IF TG_OP<>'DELETE' THEN target=NEW.count_id; END IF;
 ELSIF TG_TABLE_NAME='inventory_counts' THEN
  target=CASE WHEN TG_OP='DELETE' THEN OLD.id ELSE NEW.id END;
 ELSIF TG_NARGS=1 AND TG_ARGV[0]='receipt' THEN
  IF TG_OP<>'INSERT' THEN
   IF OLD.operation='inventory.count.create' THEN PERFORM inventory_assert_mfg_initial_physical_source(OLD.record_id); END IF;
   IF OLD.operation='commerce.manufacturing-posting-setup.initialize' THEN
   FOR goods_source IN SELECT count_id FROM inventory_mfg_initial_physical_sources WHERE manufacturing_setup_command_id=OLD.command_id LOOP PERFORM inventory_assert_mfg_initial_physical_source(goods_source.count_id); END LOOP;
   END IF;
  END IF;
  IF TG_OP<>'DELETE' AND NEW.operation='inventory.count.create' THEN target=NEW.record_id; END IF;
 END IF;
 IF target IS NOT NULL THEN PERFORM inventory_assert_mfg_initial_physical_source(target); END IF;
 RETURN NULL;
END $$;
CREATE CONSTRAINT TRIGGER inventory_mfg_initial_source_receipt AFTER INSERT OR UPDATE OR DELETE ON command_receipts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION inventory_check_mfg_initial_physical_source('receipt');
CREATE CONSTRAINT TRIGGER inventory_mfg_initial_source_count AFTER INSERT OR UPDATE OR DELETE ON inventory_counts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION inventory_check_mfg_initial_physical_source();
CREATE CONSTRAINT TRIGGER inventory_mfg_initial_source_line AFTER INSERT OR UPDATE OR DELETE ON inventory_count_lines DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION inventory_check_mfg_initial_physical_source();
CREATE CONSTRAINT TRIGGER inventory_mfg_initial_source_relation AFTER INSERT ON inventory_mfg_initial_physical_sources DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION inventory_check_mfg_initial_physical_source();
-- A marked receipt cannot predate the new native source relation silently.
DO $$ DECLARE target record; BEGIN
 FOR target IN SELECT record_id FROM command_receipts WHERE operation='inventory.count.create' AND jsonb_exists(result_json::jsonb,'_manufacturingInitialPhysicalStockCount') LOOP
  PERFORM inventory_assert_mfg_initial_physical_source(target.record_id);
 END LOOP;
END $$;
