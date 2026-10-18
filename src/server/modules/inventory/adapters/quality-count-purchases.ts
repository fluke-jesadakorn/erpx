import "server-only";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
import {qualityPurchaseSourceRows} from "../../quality/public/purchase-sources";
/** Existing controlled disposal only: actual reciprocal source FKs, immutable
 * request event and original Quality command receipt establish Count identity.
 * Manual Count creation/amendment never enters through this reader. */
export const qualityDisposalCountRows=`SELECT c.id,c.company_id,c.branch_id,c.warehouse_id,c.quality_hold_id,source.kind,source.stock_capability,source.finance_capability,source.witness source_witness,receipt.command_id source_command_id
 FROM inventory_counts c JOIN inventory_quality_holds h ON h.id=c.quality_hold_id AND h.company_id=c.company_id AND h.branch_id=c.branch_id AND h.warehouse_id=c.warehouse_id
 JOIN inventory_quality_disposition_events e ON e.hold_id=h.id AND e.count_id=c.id AND e.action='request' AND e.actor_id=c.created_by
 JOIN command_receipts receipt ON receipt.operation='quality.hold.disposition.scrap' AND receipt.record_id=h.id AND receipt.actor_id=c.created_by AND receipt.authority_json::jsonb->>'companyId'=c.company_id AND receipt.result_json::jsonb->>'countId'=c.id AND e.id='quality-disposition-'||receipt.command_id AND c.id='quality-scrap-'||receipt.command_id
 JOIN (${qualityPurchaseSourceRows}) source ON source.id=h.id AND source.company_id=h.company_id
 WHERE c.created_by=c.prepared_by AND EXISTS(SELECT 1 FROM inventory_count_lines line WHERE line.count_id=c.id AND line.product_id=h.product_id AND line.snapshot_quantity_milli>line.counted_quantity_milli)
 AND NOT EXISTS(SELECT 1 FROM inventory_count_lines line WHERE line.count_id=c.id AND line.product_id<>h.product_id)
 AND (c.status IN ('rejected','cancelled') OR (h.disposition_count_id=c.id AND h.disposition_kind='scrap' AND h.disposition_quantity_milli>0 AND h.last_result='fail' AND h.status IN ('disposition_pending','disposed')))`;
export async function qualityDisposalCountSource(database:SqlDatabase,scope:{companyId:string;branchId:string|null},id:string){
 const source=await database.prepare(`SELECT id,quality_hold_id,stock_capability,finance_capability,source_witness::text source_witness,source_command_id FROM (${qualityDisposalCountRows}) source WHERE id=? AND company_id=? AND (?::text IS NULL OR branch_id=?) AND kind='customer-return'`).bind(id,scope.companyId,scope.branchId,scope.branchId).first<{id:string;quality_hold_id:string;stock_capability:'inventory.stock-core';finance_capability:'finance.sales-posting';source_witness:string;source_command_id:string}>();
 return source?{capabilities:["sales.orders","inventory.stock-core"] as const,stockCapability:source.stock_capability,financeCapability:source.finance_capability,witness:JSON.stringify(source)}:null;
}
