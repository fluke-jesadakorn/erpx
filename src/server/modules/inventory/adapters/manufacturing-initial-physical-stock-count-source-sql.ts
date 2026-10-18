import 'server-only';
import {manufacturingPostingSetupReceiptRows,manufacturingExistingPostingMarker} from '../../commerce/public/manufacturing-existing-posting-proof';
export const manufacturingInitialPhysicalStockCapabilities=['manufacturing.materials','manufacturing.costs','inventory.stock-core','finance.production-costing'] as const;
export const manufacturingInitialPhysicalStockMarker='_manufacturingInitialPhysicalStockCount';
export type ManufacturingInitialPhysicalStockProof={kind:'manufacturing-initial-physical-stock-count';countId:string;companyId:string;warehouseId:string;branchId:string;productId:string;countCommandId:string;manufacturingSetupCommandId:string;manufacturingSetupRequestHash:string;chartId:string;inventoryAccountId:string;varianceAccountId:string;currency:string};
/** Existing-table proof keeps pre-1059 ordinary readers schema compatible.
 * 1059 guarantees every committed private Count marker has its native source;
 * additive1060 guarantees the distinct existing-mapping receipt alternative. Invalid markers are never ordinary Supply records. */
export const manufacturingInitialPhysicalStockCountSourceRows=`SELECT c.id,c.company_id,c.branch_id,c.warehouse_id,line.product_id,receipt.command_id source_command_id,receipt.result_json::jsonb->'${manufacturingInitialPhysicalStockMarker}' source_proof
 FROM inventory_counts c JOIN command_receipts receipt ON receipt.operation='inventory.count.create' AND receipt.record_id=c.id AND c.id='count-'||receipt.command_id AND receipt.actor_id=c.created_by AND receipt.authority_json::jsonb->>'companyId'=c.company_id
 JOIN inventory_count_lines line ON line.count_id=c.id
 JOIN (${manufacturingPostingSetupReceiptRows}) goods ON goods.command_id=receipt.result_json::jsonb->'${manufacturingInitialPhysicalStockMarker}'->>'manufacturingSetupCommandId' AND goods.record_id=c.company_id AND goods.authority_json::jsonb->>'companyId'=c.company_id AND goods.result_json::jsonb->>'id'=c.company_id AND goods.result_json::jsonb->>'version'='1' AND goods.request_hash=receipt.result_json::jsonb->'${manufacturingInitialPhysicalStockMarker}'->>'manufacturingSetupRequestHash'
 AND (goods.operation='commerce.manufacturing-posting-setup.initialize' OR goods.result_json::jsonb->'${manufacturingExistingPostingMarker}'->'mapping'->>'chart_id'=receipt.result_json::jsonb->'${manufacturingInitialPhysicalStockMarker}'->>'chartId' AND goods.result_json::jsonb->'${manufacturingExistingPostingMarker}'->'mapping'->>'inventory_account_id'=receipt.result_json::jsonb->'${manufacturingInitialPhysicalStockMarker}'->>'inventoryAccountId')
 WHERE NOT jsonb_exists(receipt.result_json::jsonb,'_initialPhysicalStockCount') AND jsonb_exists(receipt.result_json::jsonb,'${manufacturingInitialPhysicalStockMarker}') AND receipt.result_json::jsonb->'${manufacturingInitialPhysicalStockMarker}'->>'kind'='manufacturing-initial-physical-stock-count'
 AND receipt.result_json::jsonb->'${manufacturingInitialPhysicalStockMarker}'->>'countId'=c.id AND receipt.result_json::jsonb->'${manufacturingInitialPhysicalStockMarker}'->>'companyId'=c.company_id
 AND receipt.result_json::jsonb->'${manufacturingInitialPhysicalStockMarker}'->>'branchId'=c.branch_id AND receipt.result_json::jsonb->'${manufacturingInitialPhysicalStockMarker}'->>'warehouseId'=c.warehouse_id AND receipt.result_json::jsonb->'${manufacturingInitialPhysicalStockMarker}'->>'productId'=line.product_id AND receipt.result_json::jsonb->'${manufacturingInitialPhysicalStockMarker}'->>'countCommandId'=receipt.command_id
 AND receipt.result_json::jsonb->>'id'=c.id AND receipt.result_json::jsonb->>'status'='draft' AND receipt.result_json::jsonb->>'version'='1'
 AND line.snapshot_quantity_milli=0 AND line.snapshot_value_minor=0 AND line.snapshot_version=0 AND c.quality_hold_id IS NULL
 AND (SELECT count(*) FROM inventory_count_lines actual WHERE actual.count_id=c.id)=1`;

export const manufacturingInitialPhysicalStockMarkedRows=`SELECT record_id id,authority_json::jsonb->>'companyId' company_id FROM command_receipts WHERE operation='inventory.count.create' AND jsonb_exists(result_json::jsonb,'${manufacturingInitialPhysicalStockMarker}')`;

/** Full private receipt/native lineage, with absence witnesses. No1059 relation is queried. */
export const manufacturingInitialPhysicalStockSourceImages=`SELECT c.id,c.company_id,c.journal_id,
 jsonb_build_object('count',to_jsonb(c),'lines',(SELECT jsonb_agg(to_jsonb(l) ORDER BY l.id) FROM inventory_count_lines l WHERE l.count_id=c.id),
 'lots',(SELECT jsonb_agg(to_jsonb(lot) ORDER BY lot.id) FROM inventory_count_lot_lines lot JOIN inventory_count_lines l ON l.id=lot.count_line_id WHERE l.count_id=c.id),
 'decisions',(SELECT jsonb_agg(to_jsonb(d) ORDER BY d.id) FROM inventory_count_decisions d WHERE d.count_id=c.id),
 'moves',(SELECT jsonb_agg(to_jsonb(m) ORDER BY m.id) FROM inventory_adjustment_moves m WHERE m.count_id=c.id),
 'receipt',to_jsonb(receipt),'mappingReceipt',to_jsonb(mapping),'proof',source.source_proof)::text image
 FROM inventory_counts c JOIN (${manufacturingInitialPhysicalStockMarkedRows}) marker ON marker.id=c.id AND marker.company_id=c.company_id
 LEFT JOIN (${manufacturingInitialPhysicalStockCountSourceRows}) source ON source.id=c.id AND source.company_id=c.company_id
 LEFT JOIN command_receipts receipt ON receipt.operation='inventory.count.create' AND receipt.record_id=c.id AND c.id='count-'||receipt.command_id
 LEFT JOIN command_receipts mapping ON mapping.command_id=receipt.result_json::jsonb->'${manufacturingInitialPhysicalStockMarker}'->>'manufacturingSetupCommandId'`;
