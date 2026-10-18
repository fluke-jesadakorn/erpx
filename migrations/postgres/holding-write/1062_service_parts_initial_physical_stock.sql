-- Service-only parts preparation. These receipts attest the real unchanged
-- nine-role mapping. No Sales/Supply/StockCore grant or opening equity is added.
CREATE TABLE commerce_service_parts_posting_sources (
 company_id text PRIMARY KEY,
 command_id text NOT NULL UNIQUE REFERENCES command_receipts(command_id),
 mode text NOT NULL CHECK(mode IN ('initialize','confirm-existing')),
 mapping_json jsonb NOT NULL CHECK(jsonb_typeof(mapping_json)='object'),
 proof_json jsonb NOT NULL CHECK(jsonb_typeof(proof_json)='object'),
 created_by text NOT NULL,created_at bigint NOT NULL
);
CREATE INDEX commerce_service_parts_posting_receipt_record ON command_receipts(record_id) WHERE jsonb_exists(result_json::jsonb,'_servicePartsPostingSetup');
CREATE FUNCTION commerce_service_parts_posting_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='SERVICE_PARTS_POSTING_SOURCE_CHANGED',DETAIL='The Service posting source is immutable.'; END $$;
CREATE TRIGGER commerce_service_parts_posting_immutable BEFORE UPDATE OR DELETE ON commerce_service_parts_posting_sources FOR EACH ROW EXECUTE FUNCTION commerce_service_parts_posting_immutable();
CREATE FUNCTION commerce_service_parts_posting_marker_birth() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='UPDATE' AND (OLD.result_json::jsonb->'_servicePartsPostingSetup' IS DISTINCT FROM NEW.result_json::jsonb->'_servicePartsPostingSetup' OR jsonb_exists(OLD.result_json::jsonb,'_servicePartsPostingSetup') AND (OLD.command_id IS DISTINCT FROM NEW.command_id OR OLD.operation IS DISTINCT FROM NEW.operation OR OLD.record_id IS DISTINCT FROM NEW.record_id OR OLD.actor_id IS DISTINCT FROM NEW.actor_id OR OLD.request_hash IS DISTINCT FROM NEW.request_hash OR OLD.authority_json::jsonb->>'companyId' IS DISTINCT FROM NEW.authority_json::jsonb->>'companyId')) THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='SERVICE_PARTS_POSTING_SOURCE_CHANGED',DETAIL='The Service posting marker is born only with its original receipt insert.';
 END IF;
 IF jsonb_exists(NEW.result_json::jsonb,'_servicePartsPostingSetup') IS DISTINCT FROM (NEW.operation IN ('commerce.service-parts-posting-setup.initialize','commerce.service-parts-posting-setup.confirm-existing')) THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='SERVICE_PARTS_POSTING_SOURCE_CHANGED',DETAIL='Only the named Service setup operation carries this private source.';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER commerce_service_parts_posting_marker_birth BEFORE INSERT OR UPDATE ON command_receipts FOR EACH ROW EXECUTE FUNCTION commerce_service_parts_posting_marker_birth();
CREATE FUNCTION commerce_service_parts_posting_receipt_valid(target text) RETURNS boolean LANGUAGE sql STABLE AS $$
 SELECT EXISTS(SELECT 1 FROM commerce_service_parts_posting_sources source
 JOIN command_receipts receipt ON receipt.command_id=source.command_id AND receipt.record_id=source.company_id AND receipt.actor_id=source.created_by AND receipt.authority_json::jsonb->>'companyId'=source.company_id
 AND receipt.operation=CASE source.mode WHEN 'initialize' THEN 'commerce.service-parts-posting-setup.initialize' ELSE 'commerce.service-parts-posting-setup.confirm-existing' END
 JOIN finance_charts chart ON chart.id=source.mapping_json->>'chart_id' AND chart.company_id=source.company_id
 WHERE source.command_id=target AND source.mapping_json->>'company_id'=source.company_id
 AND receipt.result_json::jsonb->>'id'=source.company_id AND receipt.result_json::jsonb->>'status'=CASE source.mode WHEN 'initialize' THEN 'configured' ELSE 'confirmed-existing' END AND receipt.result_json::jsonb->>'version'='1'
 AND receipt.result_json::jsonb->'_servicePartsPostingSetup'=source.proof_json
 AND source.proof_json=jsonb_build_object('kind','service-parts-posting-setup','companyId',source.company_id,'commandId',source.command_id,'mode',source.mode,'mapping',source.mapping_json)
 AND NOT EXISTS(SELECT 1 FROM (VALUES ('receivable_account_id'),('payable_account_id'),('revenue_account_id'),('inventory_account_id'),('clearing_account_id'),('cogs_account_id'),('input_tax_account_id'),('output_tax_account_id'),('cash_account_id')) role(field)
 WHERE NOT EXISTS(SELECT 1 FROM finance_accounts account WHERE account.id=source.mapping_json->>role.field AND account.company_id=source.company_id AND account.chart_id=chart.id)))
$$;
CREATE FUNCTION commerce_service_assert_parts_posting(target text) RETURNS void LANGUAGE plpgsql AS $$
BEGIN
 IF (EXISTS(SELECT 1 FROM commerce_service_parts_posting_sources WHERE command_id=target) OR EXISTS(SELECT 1 FROM command_receipts WHERE command_id=target AND (operation IN('commerce.service-parts-posting-setup.initialize','commerce.service-parts-posting-setup.confirm-existing') OR jsonb_exists(result_json::jsonb,'_servicePartsPostingSetup')))) AND NOT commerce_service_parts_posting_receipt_valid(target) THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='SERVICE_PARTS_POSTING_SOURCE_CHANGED',DETAIL='The native mapping snapshot and original Service receipt must reciprocally match.';
 END IF;
END $$;
CREATE FUNCTION commerce_service_check_parts_posting() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_NARGS=1 AND TG_ARGV[0]='receipt' THEN
  IF TG_OP<>'INSERT' THEN PERFORM commerce_service_assert_parts_posting(OLD.command_id); END IF;
  IF TG_OP<>'DELETE' THEN PERFORM commerce_service_assert_parts_posting(NEW.command_id); END IF;
 ELSE
  -- Current financial semantics are birth-only. Retained history uses exact
  -- Company/chart/account identities, so legitimate recoding cannot strand it.
  IF NOT EXISTS(SELECT 1 FROM commerce_settings settings JOIN finance_charts chart ON chart.id=settings.chart_id AND chart.company_id=settings.company_id AND chart.base_currency=settings.currency AND chart.status='active'
   WHERE settings.company_id=NEW.company_id AND to_jsonb(settings)=NEW.mapping_json
   AND NOT EXISTS(SELECT 1 FROM (VALUES ('receivable_account_id','asset','receivable'),('payable_account_id','liability','payable'),('revenue_account_id','revenue','none'),('inventory_account_id','asset','none'),('clearing_account_id','liability','none'),('cogs_account_id','expense','none'),('input_tax_account_id','asset','none'),('output_tax_account_id','liability','none'),('cash_account_id','asset','bank')) role(field,category,control_kind)
    WHERE NOT EXISTS(SELECT 1 FROM finance_accounts account WHERE account.id=NEW.mapping_json->>role.field AND account.company_id=NEW.company_id AND account.chart_id=chart.id AND account.category=role.category AND account.control_kind=role.control_kind AND account.currency IN(settings.currency,chart.base_currency) AND account.status='active'))) THEN
   RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='SERVICE_PARTS_POSTING_SOURCE_CHANGED',DETAIL='Capture only the exact current valid Service nine-role mapping.';
  END IF;
  PERFORM commerce_service_assert_parts_posting(NEW.command_id);
 END IF;
 RETURN NULL;
END $$;
CREATE CONSTRAINT TRIGGER commerce_service_parts_posting_receipt AFTER INSERT OR UPDATE OR DELETE ON command_receipts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION commerce_service_check_parts_posting('receipt');
CREATE CONSTRAINT TRIGGER commerce_service_parts_posting_source AFTER INSERT ON commerce_service_parts_posting_sources DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION commerce_service_check_parts_posting();

-- Named Service initial physical count. This is current-period variance, never
-- an opening balance/acquisition. Generic Supply Count authority is unchanged.
CREATE INDEX inventory_service_initial_physical_receipt_record ON command_receipts(record_id) WHERE operation='inventory.count.create' AND jsonb_exists(result_json::jsonb,'_serviceInitialPhysicalStockCount');
CREATE TABLE inventory_service_initial_physical_sources (
 count_id text PRIMARY KEY REFERENCES inventory_counts(id),
 company_id text NOT NULL,
 branch_id text NOT NULL,
 warehouse_id text NOT NULL REFERENCES commerce_warehouses(id),
 product_id text NOT NULL REFERENCES commerce_products(id),
 count_command_id text NOT NULL UNIQUE REFERENCES command_receipts(command_id),
 service_setup_command_id text NOT NULL REFERENCES command_receipts(command_id),
 chart_id text NOT NULL REFERENCES finance_charts(id),
 inventory_account_id text NOT NULL REFERENCES finance_accounts(id),
 variance_account_id text NOT NULL REFERENCES finance_accounts(id),
 currency text NOT NULL CHECK(currency ~ '^[A-Z]{3}$'),
 proof_json jsonb NOT NULL CHECK(jsonb_typeof(proof_json)='object'),
 created_by text NOT NULL,
 created_at bigint NOT NULL
);
CREATE INDEX inventory_service_initial_physical_source_company ON inventory_service_initial_physical_sources(company_id,count_id);
CREATE INDEX inventory_service_initial_physical_source_goods_receipt ON inventory_service_initial_physical_sources(service_setup_command_id);
CREATE FUNCTION inventory_service_initial_physical_source_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='SERVICE_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED',DETAIL='Initial physical count provenance is immutable.'; END $$;
CREATE TRIGGER inventory_service_initial_physical_source_immutable BEFORE UPDATE OR DELETE ON inventory_service_initial_physical_sources FOR EACH ROW EXECUTE FUNCTION inventory_service_initial_physical_source_immutable();

-- A native source may be born only with the original create receipt INSERT.
-- Updating an old ordinary Count receipt cannot adopt it into this feature.
CREATE FUNCTION inventory_service_initial_physical_marker_birth() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='UPDATE' AND (OLD.result_json::jsonb->'_serviceInitialPhysicalStockCount' IS DISTINCT FROM NEW.result_json::jsonb->'_serviceInitialPhysicalStockCount' OR jsonb_exists(OLD.result_json::jsonb,'_serviceInitialPhysicalStockCount') AND (OLD.command_id IS DISTINCT FROM NEW.command_id OR OLD.operation IS DISTINCT FROM NEW.operation OR OLD.record_id IS DISTINCT FROM NEW.record_id OR OLD.actor_id IS DISTINCT FROM NEW.actor_id OR OLD.request_hash IS DISTINCT FROM NEW.request_hash OR OLD.authority_json::jsonb->>'companyId' IS DISTINCT FROM NEW.authority_json::jsonb->>'companyId')) THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='SERVICE_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED',DETAIL='The private initial Count marker can only be introduced by its original receipt insert and remains immutable.';
 END IF;
 IF jsonb_exists(NEW.result_json::jsonb,'_serviceInitialPhysicalStockCount') AND (NEW.operation<>'inventory.count.create' OR (jsonb_exists(NEW.result_json::jsonb,'_initialPhysicalStockCount') OR jsonb_exists(NEW.result_json::jsonb,'_manufacturingInitialPhysicalStockCount') OR jsonb_exists(NEW.result_json::jsonb,'_posInitialPhysicalStockCount'))) THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='SERVICE_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED',DETAIL='The private initial Count marker belongs only to its original Count create operation.';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER inventory_service_initial_physical_marker_birth BEFORE INSERT OR UPDATE ON command_receipts FOR EACH ROW EXECUTE FUNCTION inventory_service_initial_physical_marker_birth();

CREATE FUNCTION inventory_assert_service_initial_physical_source(target text) RETURNS void LANGUAGE plpgsql AS $$
DECLARE source inventory_service_initial_physical_sources%ROWTYPE; marked boolean;
BEGIN
 SELECT * INTO source FROM inventory_service_initial_physical_sources WHERE count_id=target;
 SELECT EXISTS(SELECT 1 FROM command_receipts r WHERE r.record_id=target AND r.operation='inventory.count.create' AND jsonb_exists(r.result_json::jsonb,'_serviceInitialPhysicalStockCount')) INTO marked;
 IF source.count_id IS NULL AND NOT marked THEN RETURN; END IF;
 IF source.count_id IS NULL OR NOT marked OR NOT EXISTS(
  SELECT 1 FROM inventory_counts c
  JOIN inventory_count_lines line ON line.count_id=c.id AND line.product_id=source.product_id AND line.snapshot_quantity_milli=0 AND line.snapshot_value_minor=0 AND line.snapshot_version=0
  JOIN commerce_warehouses warehouse ON warehouse.id=c.warehouse_id AND warehouse.company_id=c.company_id
  JOIN commerce_products product ON product.id=line.product_id AND product.company_id=c.company_id
  JOIN finance_charts chart ON chart.id=source.chart_id AND chart.company_id=c.company_id
  JOIN finance_accounts inventory ON inventory.id=source.inventory_account_id AND inventory.company_id=c.company_id AND inventory.chart_id=chart.id
  JOIN finance_accounts variance ON variance.id=source.variance_account_id AND variance.company_id=c.company_id AND variance.chart_id=chart.id
  JOIN command_receipts receipt ON receipt.command_id=source.count_command_id AND receipt.record_id=c.id AND receipt.operation='inventory.count.create' AND receipt.actor_id=c.created_by AND receipt.authority_json::jsonb->>'companyId'=c.company_id
  JOIN command_receipts goods ON goods.command_id=source.service_setup_command_id AND goods.record_id=c.company_id AND goods.operation IN('commerce.service-parts-posting-setup.initialize','commerce.service-parts-posting-setup.confirm-existing') AND goods.authority_json::jsonb->>'companyId'=c.company_id AND goods.result_json::jsonb->>'id'=c.company_id AND commerce_service_parts_posting_receipt_valid(goods.command_id) AND goods.result_json::jsonb->'_servicePartsPostingSetup'->'mapping'->>'chart_id'=source.chart_id AND goods.result_json::jsonb->'_servicePartsPostingSetup'->'mapping'->>'inventory_account_id'=source.inventory_account_id AND goods.result_json::jsonb->'_servicePartsPostingSetup'->'mapping'->>'currency'=source.currency AND goods.result_json::jsonb->>'version'='1'
  WHERE c.id=source.count_id AND c.company_id=source.company_id AND c.branch_id=source.branch_id AND c.warehouse_id=source.warehouse_id AND c.created_by=source.created_by AND c.id='count-'||source.count_command_id AND c.quality_hold_id IS NULL
  AND (SELECT count(*) FROM inventory_count_lines all_lines WHERE all_lines.count_id=c.id)=1
  AND NOT jsonb_exists(receipt.result_json::jsonb,'_initialPhysicalStockCount') AND NOT jsonb_exists(receipt.result_json::jsonb,'_manufacturingInitialPhysicalStockCount') AND NOT jsonb_exists(receipt.result_json::jsonb,'_posInitialPhysicalStockCount') AND receipt.result_json::jsonb->>'id'=c.id AND receipt.result_json::jsonb->>'status'='draft' AND receipt.result_json::jsonb->>'version'='1'
  AND receipt.result_json::jsonb->'_serviceInitialPhysicalStockCount'=source.proof_json
  AND source.proof_json=jsonb_build_object('kind','service-initial-physical-stock-count','countId',c.id,'companyId',c.company_id,'warehouseId',c.warehouse_id,'branchId',source.branch_id,'productId',line.product_id,'countCommandId',receipt.command_id,'serviceSetupCommandId',goods.command_id,'serviceSetupRequestHash',goods.request_hash,'chartId',source.chart_id,'inventoryAccountId',source.inventory_account_id,'varianceAccountId',source.variance_account_id,'currency',source.currency)
 ) THEN RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='SERVICE_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED',DETAIL='The actual Count, immutable original receipt and initial source must reciprocally match.'; END IF;
END $$;
CREATE FUNCTION inventory_check_service_initial_physical_source() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE target text; goods_source record;
BEGIN
 IF TG_TABLE_NAME='inventory_service_initial_physical_sources' THEN
  target=NEW.count_id;
  -- Current mapping is a creation precondition, not a perpetual history gate.
  IF NOT EXISTS(SELECT 1 FROM commerce_settings settings
   JOIN commerce_warehouses warehouse ON warehouse.id=NEW.warehouse_id AND warehouse.company_id=NEW.company_id AND warehouse.branch_id=NEW.branch_id AND warehouse.status='active'
   JOIN commerce_products product ON product.id=NEW.product_id AND product.company_id=NEW.company_id AND product.item_type='stock' AND product.status='active'
   JOIN finance_charts chart ON chart.id=NEW.chart_id AND chart.company_id=NEW.company_id AND chart.status='active' AND chart.base_currency=NEW.currency
   JOIN finance_accounts inventory ON inventory.id=NEW.inventory_account_id AND inventory.company_id=NEW.company_id AND inventory.chart_id=chart.id AND inventory.currency=NEW.currency AND inventory.category='asset' AND inventory.control_kind='none' AND inventory.status='active'
   JOIN finance_accounts variance ON variance.id=NEW.variance_account_id AND variance.company_id=NEW.company_id AND variance.chart_id=chart.id AND variance.currency=NEW.currency AND variance.category='expense' AND variance.code='5400' AND variance.status='active'
   WHERE EXISTS(SELECT 1 FROM command_receipts setup WHERE setup.command_id=NEW.service_setup_command_id AND commerce_service_parts_posting_receipt_valid(setup.command_id) AND setup.result_json::jsonb->'_servicePartsPostingSetup'->'mapping'=to_jsonb(settings)) AND settings.company_id=NEW.company_id AND settings.currency=NEW.currency AND settings.chart_id=NEW.chart_id AND settings.inventory_account_id=NEW.inventory_account_id) THEN
   RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='SERVICE_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED',DETAIL='Initial source must capture the actual Service mapping.';
  END IF;
 ELSIF TG_TABLE_NAME='inventory_count_lines' THEN
  IF TG_OP<>'INSERT' THEN PERFORM inventory_assert_service_initial_physical_source(OLD.count_id); END IF;
  IF TG_OP<>'DELETE' THEN target=NEW.count_id; END IF;
 ELSIF TG_TABLE_NAME='inventory_counts' THEN
  target=CASE WHEN TG_OP='DELETE' THEN OLD.id ELSE NEW.id END;
 ELSIF TG_NARGS=1 AND TG_ARGV[0]='receipt' THEN
  IF TG_OP<>'INSERT' THEN
   IF OLD.operation='inventory.count.create' THEN PERFORM inventory_assert_service_initial_physical_source(OLD.record_id); END IF;
   IF OLD.operation IN('commerce.service-parts-posting-setup.initialize','commerce.service-parts-posting-setup.confirm-existing') THEN
   FOR goods_source IN SELECT count_id FROM inventory_service_initial_physical_sources WHERE service_setup_command_id=OLD.command_id LOOP PERFORM inventory_assert_service_initial_physical_source(goods_source.count_id); END LOOP;
   END IF;
  END IF;
  IF TG_OP<>'DELETE' AND NEW.operation='inventory.count.create' THEN target=NEW.record_id; END IF;
 END IF;
 IF target IS NOT NULL THEN PERFORM inventory_assert_service_initial_physical_source(target); END IF;
 RETURN NULL;
END $$;
CREATE CONSTRAINT TRIGGER inventory_service_initial_source_receipt AFTER INSERT OR UPDATE OR DELETE ON command_receipts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION inventory_check_service_initial_physical_source('receipt');
CREATE CONSTRAINT TRIGGER inventory_service_initial_source_count AFTER INSERT OR UPDATE OR DELETE ON inventory_counts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION inventory_check_service_initial_physical_source();
CREATE CONSTRAINT TRIGGER inventory_service_initial_source_line AFTER INSERT OR UPDATE OR DELETE ON inventory_count_lines DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION inventory_check_service_initial_physical_source();
CREATE CONSTRAINT TRIGGER inventory_service_initial_source_relation AFTER INSERT ON inventory_service_initial_physical_sources DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION inventory_check_service_initial_physical_source();
-- A marked receipt cannot predate the new native source relation silently.
DO $$ DECLARE target record; BEGIN
 FOR target IN SELECT record_id FROM command_receipts WHERE operation='inventory.count.create' AND jsonb_exists(result_json::jsonb,'_serviceInitialPhysicalStockCount') LOOP
  PERFORM inventory_assert_service_initial_physical_source(target.record_id);
 END LOOP;
END $$;

DO $$ DECLARE target record; BEGIN
 FOR target IN SELECT command_id FROM command_receipts WHERE operation IN('commerce.service-parts-posting-setup.initialize','commerce.service-parts-posting-setup.confirm-existing') OR jsonb_exists(result_json::jsonb,'_servicePartsPostingSetup') LOOP PERFORM commerce_service_assert_parts_posting(target.command_id); END LOOP;
END $$;
