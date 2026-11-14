(
  ((e.aggregate_type = 'inventory_count' AND e.event_type IN ('inventory.count.create','inventory.count.update','inventory.count.submit','inventory.count.reject','inventory.count.approve','inventory.count.post','inventory.count.cancel'))
   OR (e.aggregate_type = 'flow_assign' AND e.event_type = 'flow.assign' AND (SELECT count(*) FROM flow_events all_events WHERE all_events.command_id=e.command_id)=1 AND EXISTS (
     SELECT 1 FROM flow_events fe JOIN command_receipts fr ON fr.command_id=fe.command_id WHERE fe.command_id=e.command_id AND fe.company_id=e.payload_json::jsonb->>'companyId' AND fe.kind='stock_count' AND fe.record_id=e.aggregate_id AND fe.record_version=e.aggregate_version AND fe.action='assign' AND fe.actor_user_id=fr.actor_id AND fr.operation='flow.assign' AND fr.record_id=e.aggregate_id AND (CASE WHEN fr.authority_json IS JSON OBJECT THEN fr.authority_json::jsonb ELSE '{}'::jsonb END)->>'companyId'=fe.company_id
   )))
  AND e.payload_json::jsonb->>'id'=e.aggregate_id
  AND e.payload_json::jsonb->'version'=to_jsonb(e.aggregate_version)
  AND EXISTS (SELECT 1 FROM command_receipts event_receipt WHERE event_receipt.command_id=e.command_id AND event_receipt.operation=e.event_type AND event_receipt.record_id=e.aggregate_id AND (CASE WHEN event_receipt.authority_json IS JSON OBJECT THEN event_receipt.authority_json::jsonb ELSE '{}'::jsonb END)->>'companyId'=e.payload_json::jsonb->>'companyId')
  AND EXISTS (
    SELECT 1 FROM inventory_service_initial_physical_sources s
    JOIN inventory_counts c ON c.id=s.count_id AND c.company_id=s.company_id AND c.branch_id=s.branch_id AND c.warehouse_id=s.warehouse_id AND c.created_by=s.created_by AND c.id='count-'||s.count_command_id AND c.quality_hold_id IS NULL
    JOIN inventory_count_lines l ON l.count_id=c.id AND l.product_id=s.product_id AND l.snapshot_quantity_milli=0 AND l.snapshot_value_minor=0 AND l.snapshot_version=0
    JOIN command_receipts r ON r.command_id=s.count_command_id AND r.record_id=c.id AND r.operation='inventory.count.create' AND r.actor_id=c.created_by AND (CASE WHEN r.authority_json IS JSON OBJECT THEN r.authority_json::jsonb ELSE '{}'::jsonb END)->>'companyId'=c.company_id
    JOIN command_receipts g ON g.command_id=s.service_setup_command_id AND g.record_id=c.company_id AND g.operation IN ('commerce.service-parts-posting-setup.initialize','commerce.service-parts-posting-setup.confirm-existing') AND (CASE WHEN g.authority_json IS JSON OBJECT THEN g.authority_json::jsonb ELSE '{}'::jsonb END)->>'companyId'=c.company_id AND CASE WHEN g.result_json IS JSON OBJECT AND g.authority_json IS JSON OBJECT THEN commerce_service_parts_posting_receipt_valid(g.command_id) ELSE false END
    JOIN commerce_warehouses w ON w.id=c.warehouse_id AND w.company_id=c.company_id
    JOIN commerce_products p ON p.id=l.product_id AND p.company_id=c.company_id
    JOIN finance_charts chart ON chart.id=s.chart_id AND chart.company_id=c.company_id
    JOIN finance_accounts inventory ON inventory.id=s.inventory_account_id AND inventory.company_id=c.company_id AND inventory.chart_id=chart.id
    JOIN finance_accounts variance ON variance.id=s.variance_account_id AND variance.company_id=c.company_id AND variance.chart_id=chart.id
    WHERE s.count_id=e.aggregate_id AND s.company_id=e.payload_json::jsonb->>'companyId'
      AND (SELECT count(*) FROM inventory_count_lines all_lines WHERE all_lines.count_id=c.id)=1
      AND (CASE WHEN r.result_json IS JSON OBJECT THEN r.result_json::jsonb ELSE '{}'::jsonb END)->>'id'=c.id AND (CASE WHEN r.result_json IS JSON OBJECT THEN r.result_json::jsonb ELSE '{}'::jsonb END)->>'status'='draft' AND (CASE WHEN r.result_json IS JSON OBJECT THEN r.result_json::jsonb ELSE '{}'::jsonb END)->>'version'='1'
      AND NOT jsonb_exists((CASE WHEN r.result_json IS JSON OBJECT THEN r.result_json::jsonb ELSE '{}'::jsonb END),'_initialPhysicalStockCount') AND NOT jsonb_exists((CASE WHEN r.result_json IS JSON OBJECT THEN r.result_json::jsonb ELSE '{}'::jsonb END),'_manufacturingInitialPhysicalStockCount') AND NOT jsonb_exists((CASE WHEN r.result_json IS JSON OBJECT THEN r.result_json::jsonb ELSE '{}'::jsonb END),'_posInitialPhysicalStockCount')
      AND (CASE WHEN r.result_json IS JSON OBJECT THEN r.result_json::jsonb ELSE '{}'::jsonb END)->'_serviceInitialPhysicalStockCount'=s.proof_json
      AND s.proof_json=jsonb_build_object('kind','service-initial-physical-stock-count','countId',c.id,'companyId',c.company_id,'warehouseId',c.warehouse_id,'branchId',s.branch_id,'productId',l.product_id,'countCommandId',r.command_id,'serviceSetupCommandId',g.command_id,'serviceSetupRequestHash',g.request_hash,'chartId',s.chart_id,'inventoryAccountId',s.inventory_account_id,'varianceAccountId',s.variance_account_id,'currency',s.currency)
      AND (CASE WHEN g.result_json IS JSON OBJECT THEN g.result_json::jsonb ELSE '{}'::jsonb END)->>'id'=c.company_id AND (CASE WHEN g.result_json IS JSON OBJECT THEN g.result_json::jsonb ELSE '{}'::jsonb END)->>'version'='1'
      AND (CASE WHEN g.result_json IS JSON OBJECT THEN g.result_json::jsonb ELSE '{}'::jsonb END)->'_servicePartsPostingSetup'->'mapping'->>'chart_id'=s.chart_id
      AND (CASE WHEN g.result_json IS JSON OBJECT THEN g.result_json::jsonb ELSE '{}'::jsonb END)->'_servicePartsPostingSetup'->'mapping'->>'inventory_account_id'=s.inventory_account_id
      AND (CASE WHEN g.result_json IS JSON OBJECT THEN g.result_json::jsonb ELSE '{}'::jsonb END)->'_servicePartsPostingSetup'->'mapping'->>'currency'=s.currency
  )
)
