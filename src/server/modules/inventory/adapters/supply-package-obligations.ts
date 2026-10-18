import {serviceInitialPhysicalStockMarkedRows} from './service-initial-physical-stock-count-source-sql';
import {posInitialPhysicalStockMarkedRows} from './pos-initial-physical-stock-count-source-sql';
import {manufacturingInitialPhysicalStockMarkedRows} from './manufacturing-initial-physical-stock-count-source-sql';
import {initialPhysicalStockMarkedRows} from './initial-physical-stock-count-source-sql';
import {supplyInventoryTerminalReceipts} from "./supply-terminal-receipts";
import {recordAttachmentVersionSourceReadLocks} from "../../files/public/record-version-receipts";
import "server-only";
import {createHash} from "node:crypto";
import type {PostgresDatabase} from "../../runtime/public/runtime/postgres";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
import {packageSettlementSource,type PackageSettlementSourceLock} from "../../commercial/public/package-settlement-sources";
import {qualityPurchaseSourceRows,qualityPurchaseSourceReadLocks} from "../../quality/public/purchase-sources";

type Input={companyId:string;packageId:string;packageVersion:number;moduleId:string;now:number};
const sources=new WeakMap<SqlDatabase,string>();
export async function lockSupplyInventoryPackageObligationSources(owner:PostgresDatabase,lock:PackageSettlementSourceLock){
 const {database,companyId}=packageSettlementSource(owner,lock);
 await database.batch(qualityPurchaseSourceReadLocks(database));
 await database.batch(recordAttachmentVersionSourceReadLocks(database));
 await database.prepare(`LOCK TABLE commerce_stock,commerce_warehouses,inventory_lots,inventory_lot_balances,
 inventory_counts,inventory_count_lines,inventory_count_lot_lines,inventory_count_decisions,inventory_adjustment_moves,
 inventory_transfers,inventory_transfer_lines,inventory_transfer_decisions,inventory_transfer_moves,inventory_transfer_lot_allocations,
 inventory_transit,inventory_transit_lines,inventory_transit_events,inventory_transit_lots,inventory_transit_lot_events,
 inventory_landed_costs,inventory_landed_cost_lines,inventory_landed_cost_lots,inventory_landed_cost_events,inventory_landed_cost_moves,command_receipts IN SHARE MODE`).run();
 sources.set(database,companyId);
}
export function assertSupplyInventoryPackageObligationSources(owner:PostgresDatabase,database:SqlDatabase,companyId:string){
 owner.assertActiveTransaction(database);
 if(sources.get(database)!==companyId)throw Error("SUPPLY_INVENTORY_PACKAGE_OBLIGATION_SOURCE_REQUIRED");
}
export const supplyInventorySettlementSources={
 counts:"SELECT * FROM inventory_counts",countLines:"SELECT * FROM inventory_count_lines",countDecisions:"SELECT * FROM inventory_count_decisions",
 transfers:"SELECT * FROM inventory_transfers",transferLines:"SELECT * FROM inventory_transfer_lines",transferDecisions:"SELECT * FROM inventory_transfer_decisions",
 transit:"SELECT * FROM inventory_transit",transitLines:"SELECT * FROM inventory_transit_lines",transitEvents:"SELECT * FROM inventory_transit_events",
 landedCosts:"SELECT * FROM inventory_landed_costs",landedCostLines:"SELECT * FROM inventory_landed_cost_lines",landedCostEvents:"SELECT * FROM inventory_landed_cost_events",
} as const;
/** Native Supply documents remain obligations until their original decisions
 * and quantity/value effects agree. Retained positive Stock is not unfinished
 * work. Only proven Sales-return disposal is delegated to the Sales owner. */
export async function supplyInventoryPackageObligations(owner:PostgresDatabase,database:SqlDatabase,input:Input){
 assertSupplyInventoryPackageObligationSources(owner,database,input.companyId);
 if(input.moduleId!=="inventory"||!["supply","supply-quality","supply-advanced"].includes(input.packageId)||input.packageVersion!==1||!Number.isSafeInteger(input.now)||input.now<0)throw Error("SUPPLY_INVENTORY_PACKAGE_OBLIGATION_SCOPE_REQUIRED");
 const facts=(await database.prepare(`WITH scope AS(SELECT ?::text company_id,?::boolean full,?::boolean quality_addon,?::boolean advanced_addon),counts AS MATERIALIZED(
 SELECT count.* FROM inventory_counts count,scope WHERE count.company_id=scope.company_id AND NOT EXISTS(SELECT 1 FROM (${initialPhysicalStockMarkedRows} UNION ALL ${manufacturingInitialPhysicalStockMarkedRows} UNION ALL ${posInitialPhysicalStockMarkedRows} UNION ALL ${serviceInitialPhysicalStockMarkedRows}) initial_marker WHERE initial_marker.id=count.id) AND (scope.full OR scope.quality_addon AND EXISTS(SELECT 1 FROM (${qualityPurchaseSourceRows}) quality_source WHERE quality_source.id=count.quality_hold_id AND quality_source.company_id=count.company_id AND quality_source.kind='advanced')) AND NOT EXISTS(
 SELECT 1 FROM (${qualityPurchaseSourceRows}) source WHERE source.id=count.quality_hold_id AND source.company_id=count.company_id AND source.kind='customer-return'))
 SELECT 'count' kind,c.id,jsonb_build_array(to_jsonb(c),
 (SELECT jsonb_agg(to_jsonb(line) ORDER BY line.id) FROM inventory_count_lines line WHERE line.count_id=c.id),
 (SELECT jsonb_agg(to_jsonb(lot) ORDER BY lot.id) FROM inventory_count_lot_lines lot JOIN inventory_count_lines line ON line.id=lot.count_line_id WHERE line.count_id=c.id),
 (SELECT jsonb_agg(to_jsonb(event) ORDER BY event.id) FROM inventory_count_decisions event WHERE event.count_id=c.id),
 (SELECT jsonb_agg(to_jsonb(move) ORDER BY move.id) FROM inventory_adjustment_moves move WHERE move.count_id=c.id)) fact,
 NOT COALESCE(EXISTS(SELECT 1 FROM (${supplyInventoryTerminalReceipts}) terminal WHERE terminal.kind='count' AND terminal.id=c.id AND terminal.company_id=c.company_id AND terminal.status=c.status AND terminal.version=c.version),false) pending
 FROM counts c
 UNION ALL SELECT 'transfer',t.id,jsonb_build_array(to_jsonb(t),
 (SELECT jsonb_agg(to_jsonb(line) ORDER BY line.id) FROM inventory_transfer_lines line WHERE line.transfer_id=t.id),
 (SELECT jsonb_agg(to_jsonb(event) ORDER BY event.id) FROM inventory_transfer_decisions event WHERE event.transfer_id=t.id),
 (SELECT jsonb_agg(to_jsonb(move) ORDER BY move.id) FROM inventory_transfer_moves move WHERE move.transfer_id=t.id)),
 NOT COALESCE(t.status IN ('posted','cancelled','rejected') AND EXISTS(SELECT 1 FROM inventory_transfer_decisions event JOIN command_receipts receipt
 ON receipt.command_id=event.command_id AND receipt.record_id=t.id AND receipt.actor_id=event.actor_id AND receipt.operation='inventory.transfer.'||event.action
 AND receipt.authority_json::jsonb->>'companyId'=t.company_id AND receipt.result_json::jsonb->>'status'=t.status
 WHERE event.transfer_id=t.id AND event.reviewed_version<t.version AND event.action=CASE t.status WHEN 'posted' THEN 'post' WHEN 'rejected' THEN 'reject' ELSE 'cancel' END)
 AND (t.status<>'posted' OR EXISTS(SELECT 1 FROM inventory_transfer_lines line WHERE line.transfer_id=t.id))
 AND (t.status<>'posted' OR t.delivery_mode<>'transit' OR EXISTS(SELECT 1 FROM inventory_transit transit WHERE transit.transfer_id=t.id AND transit.company_id=t.company_id)),false)
 FROM inventory_transfers t,scope WHERE scope.full AND t.company_id=scope.company_id
 UNION ALL SELECT 'transit',t.id,jsonb_build_array(to_jsonb(t),
 (SELECT jsonb_agg(to_jsonb(line) ORDER BY line.id) FROM inventory_transit_lines line WHERE line.transit_id=t.id),
 (SELECT jsonb_agg(to_jsonb(event) ORDER BY event.id) FROM inventory_transit_events event WHERE event.transit_id=t.id),
 (SELECT jsonb_agg(to_jsonb(lot) ORDER BY lot.id) FROM inventory_transit_lots lot JOIN inventory_transit_lines line ON line.id=lot.transit_line_id WHERE line.transit_id=t.id)),
 NOT COALESCE((t.status='cancelled' AND EXISTS(SELECT 1 FROM command_receipts receipt WHERE receipt.record_id=t.id AND receipt.operation='inventory.transit.cancel'
 AND receipt.authority_json::jsonb->>'companyId'=t.company_id AND receipt.result_json::jsonb->>'version'=t.version::text)
 AND EXISTS(SELECT 1 FROM inventory_transit_events event WHERE event.transit_id=t.id AND event.kind='cancel'))
 OR(t.status='completed' AND EXISTS(SELECT 1 FROM inventory_transit_lines line WHERE line.transit_id=t.id)
 AND NOT EXISTS(SELECT 1 FROM inventory_transit_lines line WHERE line.transit_id=t.id AND (line.received_milli+line.lost_milli<>line.quantity_milli OR line.settled_value_minor<>line.value_minor
 OR line.received_milli<>COALESCE((SELECT sum(event.quantity_milli) FROM inventory_transit_events event WHERE event.line_id=line.id AND event.kind='receipt'),0)
 OR line.lost_milli<>COALESCE((SELECT sum(event.quantity_milli) FROM inventory_transit_events event WHERE event.line_id=line.id AND event.kind='loss_approved'),0)))
 AND NOT EXISTS(SELECT 1 FROM inventory_transit_events proposal WHERE proposal.transit_id=t.id AND proposal.kind='loss_requested'
 AND NOT EXISTS(SELECT 1 FROM inventory_transit_events decision WHERE decision.proposal_id=proposal.id))),false)
 FROM inventory_transit t,scope WHERE scope.full AND t.company_id=scope.company_id
 UNION ALL SELECT 'landed-cost',cost.id,jsonb_build_array(to_jsonb(cost),
 (SELECT jsonb_agg(to_jsonb(line) ORDER BY line.id) FROM inventory_landed_cost_lines line WHERE line.landed_cost_id=cost.id),
 (SELECT jsonb_agg(to_jsonb(event) ORDER BY event.id) FROM inventory_landed_cost_events event WHERE event.landed_cost_id=cost.id),
 (SELECT jsonb_agg(to_jsonb(move) ORDER BY move.id) FROM inventory_landed_cost_moves move WHERE move.landed_cost_id=cost.id)),
 NOT COALESCE(EXISTS(SELECT 1 FROM (${supplyInventoryTerminalReceipts}) terminal WHERE terminal.kind='landed-cost' AND terminal.id=cost.id AND terminal.company_id=cost.company_id AND terminal.status=cost.status AND terminal.version=cost.version)
 AND (cost.status<>'posted' OR cost.amount_minor=COALESCE((SELECT sum(move.value_minor) FROM inventory_landed_cost_moves move WHERE move.landed_cost_id=cost.id),0)),false)
 FROM inventory_landed_costs cost,scope WHERE (scope.full OR scope.advanced_addon) AND cost.company_id=scope.company_id
 UNION ALL SELECT 'invalid-stock',concat(stock.warehouse_id,':',stock.product_id),to_jsonb(stock),stock.quantity_milli<0 OR stock.value_minor<0
 FROM commerce_stock stock JOIN commerce_warehouses warehouse ON warehouse.id=stock.warehouse_id CROSS JOIN scope WHERE warehouse.company_id=scope.company_id AND scope.full
 ORDER BY kind,id`).bind(input.companyId,input.packageId==="supply",input.packageId==="supply-quality",input.packageId==="supply-advanced").all<{kind:string;id:string;fact:unknown;pending:boolean}>()).results;
 return {companyId:input.companyId,packageId:input.packageId,moduleId:"inventory",observedAt:input.now,
 revision:createHash("sha256").update(JSON.stringify({companyId:input.companyId,packageVersion:input.packageVersion,facts})).digest("hex"),outstanding:facts.filter(fact=>fact.pending!==false).length};
}
