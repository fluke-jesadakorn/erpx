WITH orders AS MATERIALIZED(
 SELECT * FROM jsonb_to_recordset($2::jsonb->'orders') AS o(id text,company_id text,branch_id text,department_id text,version bigint)
),reservations AS MATERIALIZED(
 SELECT * FROM jsonb_to_recordset($2::jsonb->'reservations') AS p(id text,order_id text,warehouse_id text,product_id text,quantity_milli bigint,consumed_milli bigint,released_milli bigint,digest text)
),issue_events AS MATERIALIZED(
 SELECT * FROM jsonb_to_recordset($2::jsonb->'events') AS e(order_id text,version bigint,action text,details_json jsonb)
),scope AS(SELECT $1::text company_id),parts AS MATERIALIZED(
 SELECT p.* FROM operational_inventory_consumptions p WHERE p.source_kind='field_service'
 AND(p.company_id=$1 OR EXISTS(SELECT 1 FROM orders o WHERE o.id=p.source_id))
),initial_markers AS MATERIALIZED(
 SELECT r.record_id id FROM command_receipts r WHERE r.operation='inventory.count.create'
 AND jsonb_exists(r.result_json::jsonb,'_serviceInitialPhysicalStockCount')
 UNION SELECT count_id FROM inventory_service_initial_physical_sources
),initial_sources AS MATERIALIZED(
 SELECT c.id,c.company_id,c.branch_id,c.warehouse_id,line.product_id FROM inventory_counts c
 JOIN inventory_service_initial_physical_sources s ON s.count_id=c.id AND s.company_id=c.company_id
  AND s.branch_id=c.branch_id AND s.warehouse_id=c.warehouse_id
 JOIN inventory_count_lines line ON line.count_id=c.id AND line.product_id=s.product_id
 JOIN command_receipts r ON r.command_id=s.count_command_id AND r.record_id=c.id
  AND r.operation='inventory.count.create' AND r.actor_id=c.created_by AND r.authority_json::jsonb->>'companyId'=c.company_id
 JOIN command_receipts goods ON goods.command_id=s.service_setup_command_id
  AND goods.operation IN('commerce.service-parts-posting-setup.initialize','commerce.service-parts-posting-setup.confirm-existing')
  AND goods.record_id=c.company_id AND goods.authority_json::jsonb->>'companyId'=c.company_id
 WHERE c.id='count-'||r.command_id AND c.quality_hold_id IS NULL
  AND line.snapshot_quantity_milli=0 AND line.snapshot_value_minor=0 AND line.snapshot_version=0
  AND(SELECT count(*) FROM inventory_count_lines all_lines WHERE all_lines.count_id=c.id)=1
  AND r.result_json::jsonb->>'id'=c.id AND r.result_json::jsonb->>'status'='draft' AND r.result_json::jsonb->>'version'='1'
  AND NOT jsonb_exists(r.result_json::jsonb,'_initialPhysicalStockCount')
  AND NOT jsonb_exists(r.result_json::jsonb,'_manufacturingInitialPhysicalStockCount')
  AND NOT jsonb_exists(r.result_json::jsonb,'_posInitialPhysicalStockCount')
  AND r.result_json::jsonb->'_serviceInitialPhysicalStockCount'=s.proof_json
  AND s.created_by=c.created_by
  AND s.proof_json=jsonb_build_object('kind','service-initial-physical-stock-count','countId',c.id,
   'companyId',c.company_id,'warehouseId',c.warehouse_id,'branchId',c.branch_id,'productId',line.product_id,
   'countCommandId',r.command_id,'serviceSetupCommandId',goods.command_id,'serviceSetupRequestHash',goods.request_hash,
   'chartId',s.chart_id,'inventoryAccountId',s.inventory_account_id,'varianceAccountId',s.variance_account_id,'currency',s.currency)
  AND commerce_service_parts_posting_receipt_valid(goods.command_id)
  AND goods.request_hash=s.proof_json->>'serviceSetupRequestHash'
  AND goods.result_json::jsonb->>'version'='1' AND goods.result_json::jsonb->>'id'=c.company_id
  AND goods.result_json::jsonb->'_servicePartsPostingSetup'->'mapping'->>'chart_id'=s.chart_id
  AND goods.result_json::jsonb->'_servicePartsPostingSetup'->'mapping'->>'inventory_account_id'=s.inventory_account_id
  AND goods.result_json::jsonb->'_servicePartsPostingSetup'->'mapping'->>'currency'=s.currency
),terminal_receipts AS(
 WITH terminals AS(
  SELECT 'count'::text kind,c.id,c.company_id,c.status,c.version,d.reviewed_version+1 terminal_version,r.command_id
  FROM inventory_counts c JOIN inventory_count_decisions d ON d.count_id=c.id
  JOIN command_receipts r ON r.command_id=d.command_id AND r.record_id=c.id AND r.actor_id=d.actor_id
   AND r.operation='inventory.count.'||d.action AND r.authority_json::jsonb->>'companyId'=c.company_id
   AND r.result_json::jsonb->>'status'=c.status AND r.result_json::jsonb->>'version'=(d.reviewed_version+1)::text
  WHERE c.status IN('posted','cancelled','rejected') AND d.action=CASE c.status WHEN 'posted' THEN 'post' WHEN 'rejected' THEN 'reject' ELSE 'cancel' END
 ) SELECT terminal.* FROM terminals terminal WHERE terminal.terminal_version<=terminal.version AND EXISTS(
  WITH RECURSIVE versions(version) AS(SELECT terminal.terminal_version UNION
   SELECT binding.next_version FROM versions JOIN (__RECORD_VERSION_STEPS__) binding ON binding.previous_version=versions.version
    AND binding.company_id=terminal.company_id AND binding.target_id=terminal.id AND binding.target_type IN('stock_count','inventory_stock_count')
    WHERE binding.next_version<=terminal.version)
  SELECT 1 FROM versions WHERE version=terminal.version
 )
),facts AS(
 SELECT 'reservation' kind,part.id,part.digest fact,true pending FROM reservations part JOIN orders source ON source.id=part.order_id LEFT JOIN commerce_warehouses warehouse ON warehouse.id=part.warehouse_id AND warehouse.company_id=source.company_id AND warehouse.branch_id=source.branch_id LEFT JOIN commerce_products product ON product.id=part.product_id AND product.company_id=source.company_id WHERE warehouse.id IS NULL OR product.id IS NULL OR part.quantity_milli>part.consumed_milli+part.released_milli OR part.consumed_milli<>COALESCE((SELECT sum(issue.quantity_milli) FROM parts issue JOIN issue_events event ON event.order_id=source.id AND event.version=issue.source_version AND event.action='issue-part' AND event.details_json->>'consumptionId'=issue.id AND event.details_json->>'reservationId'=part.id WHERE issue.source_id=source.id AND issue.company_id=source.company_id AND issue.warehouse_id=part.warehouse_id AND issue.product_id=part.product_id),0)
 UNION ALL SELECT 'issue',part.id,to_jsonb(part)::text,true FROM parts part LEFT JOIN orders source ON source.id=part.source_id AND source.company_id=part.company_id AND source.branch_id=part.branch_id AND source.department_id IS NOT DISTINCT FROM part.department_id WHERE source.id IS NULL OR part.source_version>source.version OR NOT EXISTS(SELECT 1 FROM issue_events event JOIN reservations reservation ON reservation.id=event.details_json->>'reservationId' AND reservation.order_id=source.id AND reservation.warehouse_id=part.warehouse_id AND reservation.product_id=part.product_id WHERE event.order_id=source.id AND event.version=part.source_version AND event.action='issue-part' AND event.details_json->>'consumptionId'=part.id AND event.details_json->>'journalId'=part.journal_id AND event.details_json->>'quantityMilli'=part.quantity_milli::text AND event.details_json->>'costMinor'=part.value_minor::text)
 UNION ALL SELECT 'lot-source',part.id,to_jsonb(part)::text,true FROM parts part LEFT JOIN commerce_products product ON product.id=part.product_id AND product.company_id=part.company_id WHERE product.id IS NULL OR product.tracking_mode='none' AND EXISTS(SELECT 1 FROM operational_inventory_consumption_lots allocation WHERE allocation.consumption_id=part.id) OR product.tracking_mode<>'none' AND (NOT EXISTS(SELECT 1 FROM operational_inventory_consumption_lots allocation WHERE allocation.consumption_id=part.id HAVING sum(allocation.quantity_milli)=part.quantity_milli AND sum(allocation.value_minor)=part.value_minor) OR EXISTS(SELECT 1 FROM operational_inventory_consumption_lots allocation LEFT JOIN inventory_lots lot ON lot.id=allocation.lot_id AND lot.company_id=part.company_id AND lot.product_id=part.product_id WHERE allocation.consumption_id=part.id AND lot.id IS NULL))
 UNION ALL SELECT 'stock',warehouse.id||':'||stock.product_id,to_jsonb(stock)::text,true FROM commerce_stock stock JOIN commerce_warehouses warehouse ON warehouse.id=stock.warehouse_id,scope WHERE warehouse.company_id=scope.company_id AND (stock.quantity_milli<0 OR stock.value_minor<0) AND (EXISTS(SELECT 1 FROM parts issue WHERE issue.warehouse_id=stock.warehouse_id AND issue.product_id=stock.product_id) OR EXISTS(SELECT 1 FROM reservations reservation JOIN orders source ON source.id=reservation.order_id WHERE reservation.warehouse_id=stock.warehouse_id AND reservation.product_id=stock.product_id))

 UNION ALL SELECT 'service-initial-count',c.id,jsonb_build_array(to_jsonb(c),
 (SELECT jsonb_agg(to_jsonb(line) ORDER BY line.id) FROM inventory_count_lines line WHERE line.count_id=c.id),
 (SELECT jsonb_agg(to_jsonb(lot) ORDER BY lot.id) FROM inventory_count_lot_lines lot JOIN inventory_count_lines line ON line.id=lot.count_line_id WHERE line.count_id=c.id),
 (SELECT jsonb_agg(to_jsonb(event) ORDER BY event.id) FROM inventory_count_decisions event WHERE event.count_id=c.id),
 (SELECT jsonb_agg(to_jsonb(move) ORDER BY move.id) FROM inventory_adjustment_moves move WHERE move.count_id=c.id))::text,
 NOT COALESCE(EXISTS(SELECT 1 FROM initial_sources source WHERE source.id=c.id AND source.company_id=c.company_id) AND EXISTS(SELECT 1 FROM terminal_receipts terminal WHERE terminal.kind='count' AND terminal.id=c.id AND terminal.company_id=c.company_id AND terminal.status=c.status AND terminal.version=c.version),false)
 FROM inventory_counts c,scope WHERE c.company_id=scope.company_id AND EXISTS(SELECT 1 FROM initial_markers marker WHERE marker.id=c.id)
 UNION ALL SELECT 'service-initial-stock',concat(stock.warehouse_id,':',stock.product_id),to_jsonb(stock)::text,stock.quantity_milli<0 OR stock.value_minor<0 FROM commerce_stock stock,scope WHERE EXISTS(SELECT 1 FROM initial_sources source JOIN inventory_counts posted ON posted.id=source.id AND posted.company_id=source.company_id AND posted.status='posted' WHERE source.company_id=scope.company_id AND source.warehouse_id=stock.warehouse_id AND source.product_id=stock.product_id)

) SELECT count(*) FILTER(WHERE pending)::bigint outstanding,encode(sha256(convert_to(coalesce(jsonb_agg(to_jsonb(f) ORDER BY kind,id)::text,'[]'),'UTF8')),'hex') revision FROM facts f
