import 'server-only';
import {servicePartsPostingSourceRows} from '@erpx/server-commerce/public/service-parts-posting-proof';
export const serviceInitialPhysicalStockCapabilities=['service.orders','service.parts-assets','finance.service-invoice'] as const;
export const serviceInitialPhysicalStockMarker='_serviceInitialPhysicalStockCount';
export type ServiceInitialPhysicalStockProof={kind:'service-initial-physical-stock-count';countId:string;companyId:string;warehouseId:string;branchId:string;productId:string;countCommandId:string;serviceSetupCommandId:string;serviceSetupRequestHash:string;chartId:string;inventoryAccountId:string;varianceAccountId:string;currency:string};
/** Existing-table proof keeps pre-1062 ordinary readers schema compatible.
 * 1062 guarantees every committed private Count marker has its native source;
 * the same native Service proof supports distinct initialize and existing confirmation receipts. Invalid markers are never ordinary Supply records. */
export const serviceInitialPhysicalStockCountSourceRows=`SELECT c.id,c.company_id,c.branch_id,c.warehouse_id,line.product_id,receipt.command_id source_command_id,receipt.result_json::jsonb->'${serviceInitialPhysicalStockMarker}' source_proof
 FROM inventory_counts c JOIN command_receipts receipt ON receipt.operation='inventory.count.create' AND receipt.record_id=c.id AND c.id='count-'||receipt.command_id AND receipt.actor_id=c.created_by AND receipt.authority_json::jsonb->>'companyId'=c.company_id
 JOIN inventory_count_lines line ON line.count_id=c.id
 JOIN (${servicePartsPostingSourceRows}) goods ON goods.command_id=receipt.result_json::jsonb->'${serviceInitialPhysicalStockMarker}'->>'serviceSetupCommandId' AND goods.company_id=c.company_id AND goods.request_hash=receipt.result_json::jsonb->'${serviceInitialPhysicalStockMarker}'->>'serviceSetupRequestHash'
 AND goods.mapping->>'chart_id'=receipt.result_json::jsonb->'${serviceInitialPhysicalStockMarker}'->>'chartId' AND goods.mapping->>'inventory_account_id'=receipt.result_json::jsonb->'${serviceInitialPhysicalStockMarker}'->>'inventoryAccountId' AND goods.mapping->>'currency'=receipt.result_json::jsonb->'${serviceInitialPhysicalStockMarker}'->>'currency' 
 WHERE to_regclass('inventory_service_initial_physical_sources') IS NOT NULL AND NOT jsonb_exists(receipt.result_json::jsonb,'_initialPhysicalStockCount') AND NOT jsonb_exists(receipt.result_json::jsonb,'_manufacturingInitialPhysicalStockCount') AND NOT jsonb_exists(receipt.result_json::jsonb,'_posInitialPhysicalStockCount') AND jsonb_exists(receipt.result_json::jsonb,'${serviceInitialPhysicalStockMarker}') AND receipt.result_json::jsonb->'${serviceInitialPhysicalStockMarker}'->>'kind'='service-initial-physical-stock-count'
 AND receipt.result_json::jsonb->'${serviceInitialPhysicalStockMarker}'->>'countId'=c.id AND receipt.result_json::jsonb->'${serviceInitialPhysicalStockMarker}'->>'companyId'=c.company_id
 AND receipt.result_json::jsonb->'${serviceInitialPhysicalStockMarker}'->>'branchId'=c.branch_id AND receipt.result_json::jsonb->'${serviceInitialPhysicalStockMarker}'->>'warehouseId'=c.warehouse_id AND receipt.result_json::jsonb->'${serviceInitialPhysicalStockMarker}'->>'productId'=line.product_id AND receipt.result_json::jsonb->'${serviceInitialPhysicalStockMarker}'->>'countCommandId'=receipt.command_id
 AND receipt.result_json::jsonb->>'id'=c.id AND receipt.result_json::jsonb->>'status'='draft' AND receipt.result_json::jsonb->>'version'='1'
 AND line.snapshot_quantity_milli=0 AND line.snapshot_value_minor=0 AND line.snapshot_version=0 AND c.quality_hold_id IS NULL
 AND (SELECT count(*) FROM inventory_count_lines actual WHERE actual.count_id=c.id)=1`;

export const serviceInitialPhysicalStockMarkedRows=`SELECT record_id id,authority_json::jsonb->>'companyId' company_id FROM command_receipts WHERE operation='inventory.count.create' AND jsonb_exists(result_json::jsonb,'${serviceInitialPhysicalStockMarker}')`;
/** Full private receipt/native lineage, with absence witnesses. No1062 relation is queried. */
export const serviceInitialPhysicalStockSourceImages=`SELECT c.id,c.company_id,c.journal_id,
 jsonb_build_object('count',to_jsonb(c),'lines',(SELECT jsonb_agg(to_jsonb(l) ORDER BY l.id) FROM inventory_count_lines l WHERE l.count_id=c.id),
 'lots',(SELECT jsonb_agg(to_jsonb(lot) ORDER BY lot.id) FROM inventory_count_lot_lines lot JOIN inventory_count_lines l ON l.id=lot.count_line_id WHERE l.count_id=c.id),
 'decisions',(SELECT jsonb_agg(to_jsonb(d) ORDER BY d.id) FROM inventory_count_decisions d WHERE d.count_id=c.id),
 'moves',(SELECT jsonb_agg(to_jsonb(m) ORDER BY m.id) FROM inventory_adjustment_moves m WHERE m.count_id=c.id),
 'receipt',to_jsonb(receipt),'mappingReceipt',to_jsonb(mapping),'proof',source.source_proof)::text image
 FROM inventory_counts c JOIN (${serviceInitialPhysicalStockMarkedRows}) marker ON marker.id=c.id
 LEFT JOIN (${serviceInitialPhysicalStockCountSourceRows}) source ON source.id=c.id AND source.company_id=c.company_id
 LEFT JOIN command_receipts receipt ON receipt.operation='inventory.count.create' AND receipt.record_id=c.id AND c.id='count-'||receipt.command_id
 LEFT JOIN command_receipts mapping ON mapping.command_id=receipt.result_json::jsonb->'${serviceInitialPhysicalStockMarker}'->>'serviceSetupCommandId'`;
