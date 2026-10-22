import 'server-only';
export const initialPhysicalStockCapabilities=['sales.orders','inventory.stock-core','finance.sales-posting'] as const;
export const initialPhysicalStockMarker='_initialPhysicalStockCount';
export type InitialPhysicalStockProof={kind:'initial-physical-stock-count';countId:string;companyId:string;warehouseId:string;branchId:string;productId:string;countCommandId:string;goodsSetupCommandId:string;goodsSetupRequestHash:string;chartId:string;inventoryAccountId:string;varianceAccountId:string;currency:string};
/** Existing-table proof keeps pre-1058 ordinary readers schema compatible.
 * 1058 reciprocally guarantees every committed private marker has its native
 * immutable source row. Invalid markers are never ordinary Supply records. */
export const initialPhysicalStockCountSourceRows=`SELECT c.id,c.company_id,c.branch_id,c.warehouse_id,line.product_id,receipt.command_id source_command_id,receipt.result_json::jsonb->'${initialPhysicalStockMarker}' source_proof
 FROM inventory_counts c JOIN command_receipts receipt ON receipt.operation='inventory.count.create' AND receipt.record_id=c.id AND c.id='count-'||receipt.command_id AND receipt.actor_id=c.created_by AND receipt.authority_json::jsonb->>'companyId'=c.company_id
 JOIN inventory_count_lines line ON line.count_id=c.id
 JOIN command_receipts goods ON goods.command_id=receipt.result_json::jsonb->'${initialPhysicalStockMarker}'->>'goodsSetupCommandId' AND goods.operation='commerce.sales-goods-posting-setup.initialize' AND goods.record_id=c.company_id AND goods.authority_json::jsonb->>'companyId'=c.company_id AND goods.result_json::jsonb->>'id'=c.company_id AND goods.result_json::jsonb->>'status'='configured' AND goods.result_json::jsonb->>'version'='1' AND goods.request_hash=receipt.result_json::jsonb->'${initialPhysicalStockMarker}'->>'goodsSetupRequestHash'
 WHERE jsonb_exists(receipt.result_json::jsonb,'${initialPhysicalStockMarker}') AND receipt.result_json::jsonb->'${initialPhysicalStockMarker}'->>'kind'='initial-physical-stock-count'
 AND receipt.result_json::jsonb->'${initialPhysicalStockMarker}'->>'countId'=c.id AND receipt.result_json::jsonb->'${initialPhysicalStockMarker}'->>'companyId'=c.company_id
 AND receipt.result_json::jsonb->'${initialPhysicalStockMarker}'->>'branchId'=c.branch_id AND receipt.result_json::jsonb->'${initialPhysicalStockMarker}'->>'warehouseId'=c.warehouse_id AND receipt.result_json::jsonb->'${initialPhysicalStockMarker}'->>'productId'=line.product_id AND receipt.result_json::jsonb->'${initialPhysicalStockMarker}'->>'countCommandId'=receipt.command_id
 AND receipt.result_json::jsonb->>'id'=c.id AND receipt.result_json::jsonb->>'status'='draft' AND receipt.result_json::jsonb->>'version'='1'
 AND line.snapshot_quantity_milli=0 AND line.snapshot_value_minor=0 AND line.snapshot_version=0 AND c.quality_hold_id IS NULL
 AND (SELECT count(*) FROM inventory_count_lines actual WHERE actual.count_id=c.id)=1`;

export const initialPhysicalStockMarkedRows=`SELECT record_id id,authority_json::jsonb->>'companyId' company_id FROM command_receipts WHERE operation='inventory.count.create' AND jsonb_exists(result_json::jsonb,'${initialPhysicalStockMarker}')`;
