-- POS-owned first physical observation at current-period variance. This is not
-- an acquisition, opening balance or grant of general Supply Count management.
CREATE INDEX inventory_pos_initial_receipt_record ON command_receipts(record_id) WHERE operation='inventory.count.create' AND jsonb_exists(result_json::jsonb,'_posInitialPhysicalStockCount');
CREATE TABLE inventory_pos_initial_physical_sources (
 count_id text PRIMARY KEY REFERENCES inventory_counts(id),company_id text NOT NULL,branch_id text NOT NULL,
 warehouse_id text NOT NULL REFERENCES commerce_warehouses(id),product_id text NOT NULL REFERENCES commerce_products(id),
 count_command_id text NOT NULL UNIQUE REFERENCES command_receipts(command_id),pos_setup_command_id text NOT NULL REFERENCES command_receipts(command_id),
 pos_bootstrap_receipt_id text NOT NULL REFERENCES pos_receipts(id),pos_approval_receipt_id text NOT NULL REFERENCES pos_receipts(id),
 chart_id text NOT NULL REFERENCES finance_charts(id),inventory_account_id text NOT NULL REFERENCES finance_accounts(id),variance_account_id text NOT NULL REFERENCES finance_accounts(id),currency text NOT NULL CHECK(currency ~ '^[A-Z]{3}$'),
 proof_json jsonb NOT NULL CHECK(jsonb_typeof(proof_json)='object'),created_by text NOT NULL,created_at bigint NOT NULL
);
CREATE INDEX inventory_pos_initial_source_company ON inventory_pos_initial_physical_sources(company_id,count_id);
CREATE FUNCTION inventory_pos_initial_source_immutable() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='POS_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED',DETAIL='Initial physical Count provenance is immutable.'; END $$;
CREATE TRIGGER inventory_pos_initial_source_immutable BEFORE UPDATE OR DELETE ON inventory_pos_initial_physical_sources FOR EACH ROW EXECUTE FUNCTION inventory_pos_initial_source_immutable();
CREATE FUNCTION inventory_pos_initial_marker_birth() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='UPDATE' AND OLD.result_json::jsonb->'_posInitialPhysicalStockCount' IS DISTINCT FROM NEW.result_json::jsonb->'_posInitialPhysicalStockCount' THEN RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='POS_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED',DETAIL='The private marker is born only with its original receipt INSERT.'; END IF;
 IF jsonb_exists(NEW.result_json::jsonb,'_posInitialPhysicalStockCount') AND (NEW.operation<>'inventory.count.create' OR jsonb_exists(NEW.result_json::jsonb,'_initialPhysicalStockCount') OR jsonb_exists(NEW.result_json::jsonb,'_manufacturingInitialPhysicalStockCount')) THEN RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='POS_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED',DETAIL='Initial Count sources are exclusive.'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER inventory_pos_initial_marker_birth BEFORE INSERT OR UPDATE ON command_receipts FOR EACH ROW EXECUTE FUNCTION inventory_pos_initial_marker_birth();
CREATE FUNCTION inventory_assert_pos_initial_source(target text) RETURNS void LANGUAGE plpgsql AS $$
DECLARE source inventory_pos_initial_physical_sources%ROWTYPE;marked boolean;
BEGIN
 SELECT * INTO source FROM inventory_pos_initial_physical_sources WHERE count_id=target;
 SELECT EXISTS(SELECT 1 FROM command_receipts receipt WHERE receipt.record_id=target AND receipt.operation='inventory.count.create' AND jsonb_exists(receipt.result_json::jsonb,'_posInitialPhysicalStockCount')) INTO marked;
 IF source.count_id IS NULL AND NOT marked THEN RETURN; END IF;
 IF source.count_id IS NULL OR NOT marked OR NOT EXISTS(
 SELECT 1 FROM (SELECT c.id,c.company_id,c.branch_id,c.warehouse_id,line.product_id,receipt.command_id source_command_id,receipt.result_json::jsonb->'_posInitialPhysicalStockCount' source_proof
 FROM inventory_counts c JOIN command_receipts receipt ON receipt.operation='inventory.count.create' AND receipt.record_id=c.id AND c.id='count-'||receipt.command_id AND receipt.actor_id=c.created_by AND receipt.authority_json::jsonb->>'companyId'=c.company_id
 JOIN inventory_count_lines line ON line.count_id=c.id
 JOIN command_receipts goods ON goods.command_id=receipt.result_json::jsonb->'_posInitialPhysicalStockCount'->>'posSetupCommandId' AND goods.record_id=c.company_id AND goods.operation='commerce.pos-posting-setup.initialize' AND goods.authority_json::jsonb->>'companyId'=c.company_id AND goods.result_json::jsonb->>'id'=c.company_id AND goods.result_json::jsonb->>'status'='configured' AND goods.result_json::jsonb->>'version'='1' AND goods.request_hash=receipt.result_json::jsonb->'_posInitialPhysicalStockCount'->>'posSetupRequestHash'
 JOIN pos_receipts bootstrap ON bootstrap.id=receipt.result_json::jsonb->'_posInitialPhysicalStockCount'->>'posBootstrapReceiptId' AND bootstrap.company_id=c.company_id AND bootstrap.request_hash=receipt.result_json::jsonb->'_posInitialPhysicalStockCount'->>'posBootstrapRequestHash'
 JOIN pos_audit bootstrap_audit ON bootstrap_audit.id=receipt.result_json::jsonb->'_posInitialPhysicalStockCount'->>'posBootstrapAuditId' AND bootstrap_audit.company_id=c.company_id AND bootstrap_audit.record_id=c.company_id AND bootstrap_audit.operation='settings.bootstrap' AND bootstrap_audit.branch_id IS NULL AND bootstrap_audit.details_json::jsonb->'result'=bootstrap.result_json::jsonb AND bootstrap.result_json::jsonb->>'id'=c.company_id AND bootstrap.result_json::jsonb->>'warehouseId'=c.warehouse_id
 JOIN pos_receipts approval ON approval.id=receipt.result_json::jsonb->'_posInitialPhysicalStockCount'->>'posApprovalReceiptId' AND approval.company_id=c.company_id AND approval.request_hash=receipt.result_json::jsonb->'_posInitialPhysicalStockCount'->>'posApprovalRequestHash'
 JOIN pos_audit approval_audit ON approval_audit.id=receipt.result_json::jsonb->'_posInitialPhysicalStockCount'->>'posApprovalAuditId' AND approval_audit.company_id=c.company_id AND approval_audit.record_id=c.company_id AND approval_audit.operation='accounting.approve' AND approval_audit.branch_id IS NULL AND approval_audit.actor_id<>bootstrap_audit.actor_id AND approval_audit.details_json::jsonb->'result'=approval.result_json::jsonb AND approval.result_json::jsonb->>'id'=c.company_id AND approval.result_json::jsonb->>'state'='approved'
 WHERE to_regclass('inventory_pos_initial_physical_sources') IS NOT NULL AND jsonb_exists(receipt.result_json::jsonb,'_posInitialPhysicalStockCount') AND NOT jsonb_exists(receipt.result_json::jsonb,'_initialPhysicalStockCount') AND NOT jsonb_exists(receipt.result_json::jsonb,'_manufacturingInitialPhysicalStockCount')
 AND receipt.result_json::jsonb->'_posInitialPhysicalStockCount'->>'kind'='pos-initial-physical-stock-count' AND receipt.result_json::jsonb->'_posInitialPhysicalStockCount'->>'countId'=c.id AND receipt.result_json::jsonb->'_posInitialPhysicalStockCount'->>'companyId'=c.company_id AND receipt.result_json::jsonb->'_posInitialPhysicalStockCount'->>'branchId'=c.branch_id AND receipt.result_json::jsonb->'_posInitialPhysicalStockCount'->>'warehouseId'=c.warehouse_id AND receipt.result_json::jsonb->'_posInitialPhysicalStockCount'->>'productId'=line.product_id AND receipt.result_json::jsonb->'_posInitialPhysicalStockCount'->>'countCommandId'=receipt.command_id
 AND receipt.result_json::jsonb->>'id'=c.id AND receipt.result_json::jsonb->>'status'='draft' AND receipt.result_json::jsonb->>'version'='1' AND line.snapshot_quantity_milli=0 AND line.snapshot_value_minor=0 AND line.snapshot_version=0 AND c.quality_hold_id IS NULL AND (SELECT count(*) FROM inventory_count_lines actual WHERE actual.count_id=c.id)=1) actual
 JOIN inventory_counts c ON c.id=actual.id AND c.company_id=actual.company_id
 JOIN commerce_warehouses warehouse ON warehouse.id=c.warehouse_id AND warehouse.company_id=c.company_id AND warehouse.branch_id=c.branch_id
 JOIN commerce_products product ON product.id=actual.product_id AND product.company_id=c.company_id
 JOIN finance_charts chart ON chart.id=source.chart_id AND chart.company_id=c.company_id
 JOIN finance_accounts inventory ON inventory.id=source.inventory_account_id AND inventory.company_id=c.company_id AND inventory.chart_id=chart.id
 JOIN finance_accounts variance ON variance.id=source.variance_account_id AND variance.company_id=c.company_id AND variance.chart_id=chart.id
 WHERE actual.id=source.count_id AND actual.company_id=source.company_id AND actual.branch_id=source.branch_id AND actual.warehouse_id=source.warehouse_id AND actual.product_id=source.product_id AND c.created_by=source.created_by AND actual.source_command_id=source.count_command_id AND actual.source_proof=source.proof_json
 AND source.proof_json->>'posSetupCommandId'=source.pos_setup_command_id AND source.proof_json->>'posBootstrapReceiptId'=source.pos_bootstrap_receipt_id AND source.proof_json->>'posApprovalReceiptId'=source.pos_approval_receipt_id AND source.proof_json->>'chartId'=source.chart_id AND source.proof_json->>'inventoryAccountId'=source.inventory_account_id AND source.proof_json->>'varianceAccountId'=source.variance_account_id AND source.proof_json->>'currency'=source.currency
 AND source.proof_json=jsonb_build_object('kind',source.proof_json->'kind','countId',source.proof_json->'countId','companyId',source.proof_json->'companyId','warehouseId',source.proof_json->'warehouseId','branchId',source.proof_json->'branchId','productId',source.proof_json->'productId','countCommandId',source.proof_json->'countCommandId','posSetupCommandId',source.proof_json->'posSetupCommandId','posSetupRequestHash',source.proof_json->'posSetupRequestHash','posBootstrapReceiptId',source.proof_json->'posBootstrapReceiptId','posBootstrapRequestHash',source.proof_json->'posBootstrapRequestHash','posBootstrapAuditId',source.proof_json->'posBootstrapAuditId','posApprovalReceiptId',source.proof_json->'posApprovalReceiptId','posApprovalRequestHash',source.proof_json->'posApprovalRequestHash','posApprovalAuditId',source.proof_json->'posApprovalAuditId','posAccountingImageHash',source.proof_json->'posAccountingImageHash','chartId',source.proof_json->'chartId','inventoryAccountId',source.proof_json->'inventoryAccountId','varianceAccountId',source.proof_json->'varianceAccountId','currency',source.proof_json->'currency')
 ) THEN RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='POS_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED',DETAIL='Count, mapping, reviewed POS evidence and private receipt must reciprocally match.'; END IF;
END $$;
CREATE FUNCTION inventory_check_pos_initial_source() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE target text;affected record;
BEGIN
 IF TG_TABLE_NAME='inventory_pos_initial_physical_sources' THEN
  target=NEW.count_id;
  IF NOT EXISTS(SELECT 1 FROM commerce_settings settings JOIN (SELECT settings.company_id,register.warehouse_id,register.branch_id,settings.currency,settings.accounts_json,
 bootstrap.id bootstrap_receipt_id,bootstrap.request_hash bootstrap_request_hash,bootstrap_audit.id bootstrap_audit_id,
 approval.id approval_receipt_id,approval.request_hash approval_request_hash,approval_audit.id approval_audit_id,
 encode(sha256(convert_to(jsonb_build_object('currency',settings.currency,'accounts',settings.accounts_json::jsonb,'preparedBy',settings.accounting_prepared_by,'approvedBy',settings.accounting_approved_by,'register',to_jsonb(register))::text,'UTF8')),'hex') accounting_image_hash,
 jsonb_build_object('settings',to_jsonb(settings),'register',to_jsonb(register),'bootstrap',to_jsonb(bootstrap),'bootstrapAudit',to_jsonb(bootstrap_audit),'approval',to_jsonb(approval),'approvalAudit',to_jsonb(approval_audit))::text accounting_image
 FROM pos_settings settings JOIN pos_registers register ON register.company_id=settings.company_id AND register.active
 JOIN pos_audit bootstrap_audit ON bootstrap_audit.company_id=settings.company_id AND bootstrap_audit.record_id=settings.company_id AND bootstrap_audit.operation='settings.bootstrap' AND bootstrap_audit.branch_id IS NULL AND bootstrap_audit.actor_id=settings.accounting_prepared_by
 JOIN pos_receipts bootstrap ON bootstrap.company_id=settings.company_id AND bootstrap.result_json::jsonb=bootstrap_audit.details_json::jsonb->'result' AND bootstrap.result_json::jsonb->>'id'=settings.company_id AND bootstrap.result_json::jsonb->>'registerId'=register.id AND bootstrap.result_json::jsonb->>'warehouseId'=register.warehouse_id
 JOIN pos_audit approval_audit ON approval_audit.company_id=settings.company_id AND approval_audit.record_id=settings.company_id AND approval_audit.operation='accounting.approve' AND approval_audit.branch_id IS NULL AND approval_audit.actor_id=settings.accounting_approved_by AND approval_audit.actor_id<>bootstrap_audit.actor_id
 JOIN pos_receipts approval ON approval.company_id=settings.company_id AND approval.result_json::jsonb=approval_audit.details_json::jsonb->'result' AND approval.result_json::jsonb->>'id'=settings.company_id AND approval.result_json::jsonb->>'state'='approved'
 WHERE settings.accounting_prepared_by IS NOT NULL AND settings.accounting_approved_by IS NOT NULL) accounting ON accounting.company_id=settings.company_id AND accounting.warehouse_id=NEW.warehouse_id AND accounting.branch_id=NEW.branch_id AND accounting.currency=settings.currency AND accounting.accounts_json::jsonb->>'chartId'=settings.chart_id AND accounting.accounts_json::jsonb->>'inventory'=settings.inventory_account_id
   JOIN commerce_warehouses warehouse ON warehouse.id=NEW.warehouse_id AND warehouse.company_id=NEW.company_id AND warehouse.branch_id=NEW.branch_id AND warehouse.status='active'
   JOIN commerce_products product ON product.id=NEW.product_id AND product.company_id=NEW.company_id AND product.item_type='stock' AND product.status='active'
   JOIN finance_charts chart ON chart.id=NEW.chart_id AND chart.company_id=NEW.company_id AND chart.status='active' AND chart.base_currency=NEW.currency
   JOIN finance_accounts inventory ON inventory.id=NEW.inventory_account_id AND inventory.company_id=NEW.company_id AND inventory.chart_id=chart.id AND inventory.currency=NEW.currency AND inventory.category='asset' AND inventory.control_kind='none' AND inventory.status='active'
   JOIN finance_accounts variance ON variance.id=NEW.variance_account_id AND variance.company_id=NEW.company_id AND variance.chart_id=chart.id AND variance.currency=NEW.currency AND variance.category='expense' AND variance.code='5400' AND variance.status='active'
   WHERE settings.company_id=NEW.company_id AND settings.chart_id=NEW.chart_id AND settings.inventory_account_id=NEW.inventory_account_id AND accounting.bootstrap_receipt_id=NEW.pos_bootstrap_receipt_id AND accounting.approval_receipt_id=NEW.pos_approval_receipt_id AND accounting.bootstrap_request_hash=NEW.proof_json->>'posBootstrapRequestHash' AND accounting.bootstrap_audit_id=NEW.proof_json->>'posBootstrapAuditId' AND accounting.approval_request_hash=NEW.proof_json->>'posApprovalRequestHash' AND accounting.approval_audit_id=NEW.proof_json->>'posApprovalAuditId' AND accounting.accounting_image_hash=NEW.proof_json->>'posAccountingImageHash') THEN
   RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='POS_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED',DETAIL='Creation must capture the actual independently reviewed POS mapping and register.';
  END IF;
 ELSIF TG_TABLE_NAME='inventory_count_lines' THEN
  IF TG_OP<>'INSERT' THEN PERFORM inventory_assert_pos_initial_source(OLD.count_id); END IF;
  IF TG_OP<>'DELETE' THEN target=NEW.count_id; END IF;
 ELSIF TG_TABLE_NAME='inventory_counts' THEN target=CASE WHEN TG_OP='DELETE' THEN OLD.id ELSE NEW.id END;
 ELSIF TG_NARGS=1 AND TG_ARGV[0]='receipt' THEN
  IF TG_OP<>'INSERT' THEN
   IF OLD.operation='inventory.count.create' THEN PERFORM inventory_assert_pos_initial_source(OLD.record_id); END IF;
   IF OLD.operation='commerce.pos-posting-setup.initialize' THEN FOR affected IN SELECT count_id FROM inventory_pos_initial_physical_sources WHERE pos_setup_command_id=OLD.command_id LOOP PERFORM inventory_assert_pos_initial_source(affected.count_id); END LOOP; END IF;
  END IF;
  IF TG_OP<>'DELETE' AND NEW.operation='inventory.count.create' THEN target=NEW.record_id; END IF;
 ELSIF TG_NARGS=1 AND TG_ARGV[0]='pos-receipt' THEN
  IF TG_OP<>'INSERT' THEN FOR affected IN SELECT count_id FROM inventory_pos_initial_physical_sources WHERE pos_bootstrap_receipt_id=OLD.id OR pos_approval_receipt_id=OLD.id LOOP PERFORM inventory_assert_pos_initial_source(affected.count_id); END LOOP; END IF;
 ELSIF TG_NARGS=1 AND TG_ARGV[0]='pos-audit' THEN
  IF TG_OP<>'INSERT' THEN FOR affected IN SELECT count_id FROM inventory_pos_initial_physical_sources WHERE proof_json->>'posBootstrapAuditId'=OLD.id OR proof_json->>'posApprovalAuditId'=OLD.id LOOP PERFORM inventory_assert_pos_initial_source(affected.count_id); END LOOP; END IF;
 END IF;
 IF target IS NOT NULL THEN PERFORM inventory_assert_pos_initial_source(target); END IF;RETURN NULL;
END $$;
CREATE CONSTRAINT TRIGGER inventory_pos_initial_source_receipt AFTER INSERT OR UPDATE OR DELETE ON command_receipts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION inventory_check_pos_initial_source('receipt');
CREATE CONSTRAINT TRIGGER inventory_pos_initial_source_count AFTER INSERT OR UPDATE OR DELETE ON inventory_counts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION inventory_check_pos_initial_source();
CREATE CONSTRAINT TRIGGER inventory_pos_initial_source_line AFTER INSERT OR UPDATE OR DELETE ON inventory_count_lines DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION inventory_check_pos_initial_source();
CREATE CONSTRAINT TRIGGER inventory_pos_initial_source_relation AFTER INSERT ON inventory_pos_initial_physical_sources DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION inventory_check_pos_initial_source();
CREATE CONSTRAINT TRIGGER inventory_pos_initial_source_pos_receipt AFTER UPDATE OR DELETE ON pos_receipts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION inventory_check_pos_initial_source('pos-receipt');
CREATE CONSTRAINT TRIGGER inventory_pos_initial_source_pos_audit AFTER UPDATE OR DELETE ON pos_audit DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION inventory_check_pos_initial_source('pos-audit');
DO $$ DECLARE target record; BEGIN FOR target IN SELECT record_id FROM command_receipts WHERE operation='inventory.count.create' AND jsonb_exists(result_json::jsonb,'_posInitialPhysicalStockCount') LOOP PERFORM inventory_assert_pos_initial_source(target.record_id); END LOOP; END $$;
