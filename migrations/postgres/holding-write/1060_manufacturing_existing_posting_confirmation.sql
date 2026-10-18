-- Existing Company Manufacturing posting confirmation. No settings/Stock/Finance
-- business row is created or rebound; this records the unchanged real mapping.
CREATE TABLE commerce_mfg_existing_posting_confirmations (
 company_id text PRIMARY KEY,
 command_id text NOT NULL UNIQUE REFERENCES command_receipts(command_id),
 request_hash text NOT NULL,
 chart_id text NOT NULL REFERENCES finance_charts(id),
 inventory_account_id text NOT NULL REFERENCES finance_accounts(id),
 version integer NOT NULL CHECK(version=1),
 mapping_json jsonb NOT NULL CHECK(jsonb_typeof(mapping_json)='object'),
 proof_json jsonb NOT NULL CHECK(jsonb_typeof(proof_json)='object'),
 created_by text NOT NULL,
 created_at bigint NOT NULL
);
CREATE INDEX commerce_mfg_existing_posting_receipt_record ON command_receipts(record_id) WHERE operation='commerce.manufacturing-posting-setup.confirm-existing' AND jsonb_exists(result_json::jsonb,'_manufacturingExistingPostingSetup');
CREATE FUNCTION commerce_mfg_existing_posting_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='MANUFACTURING_POSTING_SOURCE_CHANGED',DETAIL='Existing posting confirmation provenance is immutable.'; END $$;
CREATE TRIGGER commerce_mfg_existing_posting_immutable BEFORE UPDATE OR DELETE ON commerce_mfg_existing_posting_confirmations FOR EACH ROW EXECUTE FUNCTION commerce_mfg_existing_posting_immutable();

CREATE FUNCTION commerce_mfg_existing_posting_marker_birth() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='UPDATE' AND (
  OLD.result_json::jsonb->'_manufacturingExistingPostingSetup' IS DISTINCT FROM NEW.result_json::jsonb->'_manufacturingExistingPostingSetup'
  OR jsonb_exists(OLD.result_json::jsonb,'_manufacturingExistingPostingSetup') AND
   (OLD.command_id IS DISTINCT FROM NEW.command_id OR OLD.operation IS DISTINCT FROM NEW.operation OR OLD.record_id IS DISTINCT FROM NEW.record_id OR OLD.actor_id IS DISTINCT FROM NEW.actor_id OR OLD.request_hash IS DISTINCT FROM NEW.request_hash OR OLD.authority_json::jsonb->>'companyId' IS DISTINCT FROM NEW.authority_json::jsonb->>'companyId')
 ) THEN RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='MANUFACTURING_POSTING_SOURCE_CHANGED',DETAIL='The existing mapping marker is born only with its original confirmation receipt insert.'; END IF;
 IF (jsonb_exists(NEW.result_json::jsonb,'_manufacturingExistingPostingSetup') AND NEW.operation<>'commerce.manufacturing-posting-setup.confirm-existing') OR (NEW.operation='commerce.manufacturing-posting-setup.confirm-existing' AND NOT jsonb_exists(NEW.result_json::jsonb,'_manufacturingExistingPostingSetup')) THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='MANUFACTURING_POSTING_SOURCE_CHANGED',DETAIL='Only the distinct existing mapping confirmation may carry its private proof.';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER commerce_mfg_existing_posting_marker_birth BEFORE INSERT OR UPDATE ON command_receipts FOR EACH ROW EXECUTE FUNCTION commerce_mfg_existing_posting_marker_birth();

CREATE FUNCTION commerce_mfg_existing_posting_receipt_valid(target text) RETURNS boolean LANGUAGE sql STABLE AS $$
 SELECT EXISTS(SELECT 1 FROM commerce_mfg_existing_posting_confirmations source
 JOIN command_receipts receipt ON receipt.command_id=source.command_id AND receipt.operation='commerce.manufacturing-posting-setup.confirm-existing' AND receipt.record_id=source.company_id AND receipt.actor_id=source.created_by AND receipt.request_hash=source.request_hash AND receipt.authority_json::jsonb->>'companyId'=source.company_id
 JOIN finance_charts chart ON chart.id=source.chart_id AND chart.company_id=source.company_id
 JOIN finance_accounts inventory ON inventory.id=source.inventory_account_id AND inventory.company_id=source.company_id
 WHERE source.command_id=target AND source.version=1 AND source.mapping_json->>'company_id'=source.company_id AND source.mapping_json->>'chart_id'=source.chart_id AND source.mapping_json->>'inventory_account_id'=source.inventory_account_id
 AND receipt.result_json::jsonb->>'id'=source.company_id AND receipt.result_json::jsonb->>'status'='confirmed-existing' AND receipt.result_json::jsonb->>'version'='1'
 AND receipt.result_json::jsonb->'_manufacturingExistingPostingSetup'=source.proof_json
 AND source.proof_json=jsonb_build_object('kind','manufacturing-existing-posting-setup','companyId',source.company_id,'confirmationCommandId',source.command_id,'mapping',source.mapping_json,'expectedSalesDefaults',source.proof_json->'expectedSalesDefaults','effectiveService',source.proof_json->'effectiveService'))
$$;
CREATE FUNCTION commerce_mfg_assert_existing_posting(target text) RETURNS void LANGUAGE plpgsql AS $$
BEGIN
 IF (EXISTS(SELECT 1 FROM commerce_mfg_existing_posting_confirmations WHERE command_id=target) OR EXISTS(SELECT 1 FROM command_receipts r WHERE command_id=target AND (r.operation='commerce.manufacturing-posting-setup.confirm-existing' OR jsonb_exists(r.result_json::jsonb,'_manufacturingExistingPostingSetup'))))
 AND NOT commerce_mfg_existing_posting_receipt_valid(target) THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='MANUFACTURING_POSTING_SOURCE_CHANGED',DETAIL='The original confirmation receipt and immutable unchanged mapping must reciprocally match.';
 END IF;
END $$;
CREATE FUNCTION commerce_mfg_check_existing_posting() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_NARGS=1 AND TG_ARGV[0]='receipt' THEN
  IF TG_OP<>'INSERT' AND (OLD.operation='commerce.manufacturing-posting-setup.confirm-existing' OR jsonb_exists(OLD.result_json::jsonb,'_manufacturingExistingPostingSetup')) THEN PERFORM commerce_mfg_assert_existing_posting(OLD.command_id); END IF;
  IF TG_OP<>'DELETE' AND (NEW.operation='commerce.manufacturing-posting-setup.confirm-existing' OR jsonb_exists(NEW.result_json::jsonb,'_manufacturingExistingPostingSetup')) THEN PERFORM commerce_mfg_assert_existing_posting(NEW.command_id); END IF;
 ELSE
  -- Financial semantics/current settings are a creation-only precondition.
  -- Later legitimate owner changes never erase confirmation/Count history.
  IF NOT EXISTS(SELECT 1 FROM commerce_settings settings JOIN finance_charts chart ON chart.id=settings.chart_id AND chart.company_id=settings.company_id AND chart.base_currency='THB' AND chart.status='active'
   WHERE settings.company_id=NEW.company_id AND to_jsonb(settings)=NEW.mapping_json AND NEW.chart_id=settings.chart_id AND NEW.inventory_account_id=settings.inventory_account_id
   AND NOT EXISTS(SELECT 1 FROM (VALUES
    ('receivable_account_id','asset','receivable'),('payable_account_id','liability','payable'),('revenue_account_id','revenue','none'),('inventory_account_id','asset','none'),('clearing_account_id','liability','none'),('cogs_account_id','expense','none'),('input_tax_account_id','asset','none'),('output_tax_account_id','liability','none'),('cash_account_id','asset','bank')
   ) role(field,category,control_kind) WHERE NOT EXISTS(SELECT 1 FROM finance_accounts account WHERE account.id=NEW.mapping_json->>role.field AND account.company_id=NEW.company_id AND account.chart_id=chart.id AND account.category=role.category AND account.control_kind=role.control_kind AND account.status='active' AND account.currency IN(settings.currency,chart.base_currency)))) THEN
   RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='MANUFACTURING_POSTING_SOURCE_CHANGED',DETAIL='Confirm only the exact current real nine-role Manufacturing mapping.';
  END IF;
  PERFORM commerce_mfg_assert_existing_posting(NEW.command_id);
 END IF;
 RETURN NULL;
END $$;
CREATE CONSTRAINT TRIGGER commerce_mfg_existing_posting_receipt AFTER INSERT OR UPDATE OR DELETE ON command_receipts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION commerce_mfg_check_existing_posting('receipt');
CREATE CONSTRAINT TRIGGER commerce_mfg_existing_posting_source AFTER INSERT ON commerce_mfg_existing_posting_confirmations DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION commerce_mfg_check_existing_posting();

-- Extend only the existing native Manufacturing Count receipt alternative.
-- The original1059 source IDs/private Count proof and initialize branch remain.
CREATE OR REPLACE FUNCTION inventory_assert_mfg_initial_physical_source(target text) RETURNS void LANGUAGE plpgsql AS $$
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
  JOIN command_receipts goods ON goods.command_id=source.manufacturing_setup_command_id AND goods.record_id=c.company_id AND goods.authority_json::jsonb->>'companyId'=c.company_id AND goods.result_json::jsonb->>'id'=c.company_id AND goods.result_json::jsonb->>'version'='1' AND (
   goods.operation='commerce.manufacturing-posting-setup.initialize' AND goods.result_json::jsonb->>'status'='configured'
   OR goods.operation='commerce.manufacturing-posting-setup.confirm-existing' AND goods.result_json::jsonb->>'status'='confirmed-existing' AND commerce_mfg_existing_posting_receipt_valid(goods.command_id)
    AND goods.result_json::jsonb->'_manufacturingExistingPostingSetup'->'mapping'->>'chart_id'=source.chart_id
    AND goods.result_json::jsonb->'_manufacturingExistingPostingSetup'->'mapping'->>'inventory_account_id'=source.inventory_account_id
  )
  WHERE c.id=source.count_id AND c.company_id=source.company_id AND c.branch_id=source.branch_id AND c.warehouse_id=source.warehouse_id AND c.created_by=source.created_by AND c.id='count-'||source.count_command_id AND c.quality_hold_id IS NULL
  AND (SELECT count(*) FROM inventory_count_lines all_lines WHERE all_lines.count_id=c.id)=1
  AND NOT jsonb_exists(receipt.result_json::jsonb,'_initialPhysicalStockCount') AND receipt.result_json::jsonb->>'id'=c.id AND receipt.result_json::jsonb->>'status'='draft' AND receipt.result_json::jsonb->>'version'='1'
  AND receipt.result_json::jsonb->'_manufacturingInitialPhysicalStockCount'=source.proof_json
  AND source.proof_json=jsonb_build_object('kind','manufacturing-initial-physical-stock-count','countId',c.id,'companyId',c.company_id,'warehouseId',c.warehouse_id,'branchId',source.branch_id,'productId',line.product_id,'countCommandId',receipt.command_id,'manufacturingSetupCommandId',goods.command_id,'manufacturingSetupRequestHash',goods.request_hash,'chartId',source.chart_id,'inventoryAccountId',source.inventory_account_id,'varianceAccountId',source.variance_account_id,'currency',source.currency)
 ) THEN RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='MANUFACTURING_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED',DETAIL='The actual Count, immutable original receipt and initial source must reciprocally match.'; END IF;
END $$;
CREATE OR REPLACE FUNCTION inventory_check_mfg_initial_physical_source() RETURNS trigger LANGUAGE plpgsql AS $$
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
   IF OLD.operation IN('commerce.manufacturing-posting-setup.initialize','commerce.manufacturing-posting-setup.confirm-existing') THEN
   FOR goods_source IN SELECT count_id FROM inventory_mfg_initial_physical_sources WHERE manufacturing_setup_command_id=OLD.command_id LOOP PERFORM inventory_assert_mfg_initial_physical_source(goods_source.count_id); END LOOP;
   END IF;
  END IF;
  IF TG_OP<>'DELETE' AND NEW.operation='inventory.count.create' THEN target=NEW.record_id; END IF;
 END IF;
 IF target IS NOT NULL THEN PERFORM inventory_assert_mfg_initial_physical_source(target); END IF;
 RETURN NULL;
END $$;

-- Ordinary old rows are unchanged. Invalid pre-existing private confirmations
-- cannot be grandfathered by an install that otherwise creates only triggers.
DO $$ DECLARE target record; BEGIN
 FOR target IN SELECT command_id FROM command_receipts WHERE operation='commerce.manufacturing-posting-setup.confirm-existing' OR jsonb_exists(result_json::jsonb,'_manufacturingExistingPostingSetup') LOOP
  PERFORM commerce_mfg_assert_existing_posting(target.command_id);
 END LOOP;
 FOR target IN SELECT count_id FROM inventory_mfg_initial_physical_sources LOOP
  PERFORM inventory_assert_mfg_initial_physical_source(target.count_id);
 END LOOP;
END $$;
