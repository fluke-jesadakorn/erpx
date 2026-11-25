import {serviceInitialPhysicalStockMarkedRows} from '@erpx/server-inventory/adapters/service-initial-physical-stock-count-source-sql';
import {posInitialPhysicalStockMarkedRows} from '@erpx/server-inventory/adapters/pos-initial-physical-stock-count-source-sql';
import {manufacturingInitialPhysicalStockCountSourceRows,manufacturingInitialPhysicalStockMarkedRows} from '@erpx/server-inventory/adapters/manufacturing-initial-physical-stock-count-source-sql';
import {supplyInventoryTerminalReceipts} from '@erpx/server-inventory/adapters/supply-terminal-receipts';
import {recordAttachmentVersionSourceReadLocks} from '@erpx/server-files/public/record-version-receipts';
import "server-only";
import {createHash} from "node:crypto";
import type {PostgresDatabase} from "@erpx/server-runtime/public/runtime/postgres";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {packageSettlementSource,type PackageSettlementSourceLock} from "@erpx/server-commercial/public/package-settlement-sources";
import {assertManufacturingPackageObligationSources,manufacturingPackageSettlementSources as manufacturing} from "@erpx/server-manufacturing/public/package-obligations";
import {childJsonAggSql,childJoinJsonAggSql,postedInitialPhysicalStockSourceSql,scopeExistsSql,terminalReceiptMatchSql,sourceExistsSql,countScopeRowsSql,markedCountExistsSql,stockObligationHeadSql} from './read-tail-sql';
type Input={companyId:string;packageId:string;packageVersion:number;moduleId:string;now:number};const sources=new WeakMap<SqlDatabase,string>();
export async function lockManufacturingInventoryPackageObligationSources(owner:PostgresDatabase,lock:PackageSettlementSourceLock){const {database,companyId}=packageSettlementSource(owner,lock);await database.batch(recordAttachmentVersionSourceReadLocks(database));await database.prepare("LOCK TABLE command_receipts,inventory_count_lot_lines,inventory_count_decisions,inventory_adjustment_moves,commerce_stock,inventory_lots,inventory_lot_balances,inventory_counts,inventory_count_lines,inventory_transfers,inventory_transfer_lines,inventory_transit,inventory_transit_lines,inventory_transit_events,inventory_landed_costs,inventory_landed_cost_lines IN SHARE MODE").run();sources.set(database,companyId);}
export async function manufacturingInventoryPackageObligations(owner:PostgresDatabase,database:SqlDatabase,input:Input){
 owner.assertActiveTransaction(database);if(sources.get(database)!==input.companyId)throw Error("MANUFACTURING_INVENTORY_PACKAGE_OBLIGATION_SOURCE_REQUIRED");assertManufacturingPackageObligationSources(owner,database,input.companyId);if(input.moduleId!=="inventory"||!["manufacturing","manufacturing-mrp"].includes(input.packageId)||input.packageVersion!==1||!Number.isSafeInteger(input.now)||input.now<0)throw Error("MANUFACTURING_INVENTORY_PACKAGE_OBLIGATION_SCOPE_REQUIRED");
 const facts=(await database.prepare(`WITH scope AS(SELECT ?::text company_id,?::boolean full),orders AS MATERIALIZED(SELECT o.* FROM (${manufacturing.orders}) o,scope WHERE o.company_id=scope.company_id AND (scope.full OR o.advanced OR NOT o.source_valid)),footprint AS MATERIALIZED(SELECT source.* FROM (${manufacturing.stockFootprint}) source JOIN orders o ON o.id=source.order_id AND o.company_id=source.company_id)
 SELECT 'material-reservation' kind,r.id,to_jsonb(r)::text fact,true pending FROM (${manufacturing.materialReservations}) r JOIN orders o ON o.id=r.order_id WHERE r.quantity_milli>r.consumed_milli+r.released_milli
 UNION ALL ${stockObligationHeadSql}to_jsonb(s)::text,true FROM commerce_stock s WHERE (s.quantity_milli<0 OR s.value_minor<0) AND ${scopeExistsSql('footprint','source','warehouse_id','s.warehouse_id','product_id','s.product_id')}
 UNION ALL SELECT 'lot',concat(b.warehouse_id,':',b.lot_id),to_jsonb(b)::text,true FROM inventory_lot_balances b JOIN inventory_lots lot ON lot.id=b.lot_id WHERE (b.quantity_milli<0 OR b.value_minor<0) AND EXISTS(SELECT 1 FROM footprint source WHERE source.company_id=lot.company_id AND source.warehouse_id=b.warehouse_id AND source.product_id=lot.product_id)
 UNION ALL SELECT 'count',c.id,to_jsonb(c)::text,true FROM inventory_counts c,scope WHERE scope.full AND${countScopeRowsSql(`${manufacturingInitialPhysicalStockMarkedRows} UNION ALL ${posInitialPhysicalStockMarkedRows} UNION ALL ${serviceInitialPhysicalStockMarkedRows}`)}
 UNION ALL SELECT 'manufacturing-initial-physical-count',c.id,jsonb_build_array(to_jsonb(c),
 ${childJsonAggSql('inventory_count_lines','line','line.count_id=c.id')},
 ${childJoinJsonAggSql('inventory_count_lot_lines','lot','inventory_count_lines line ON line.id=lot.count_line_id','line.count_id=c.id')},
 ${childJsonAggSql('inventory_count_decisions','event','event.count_id=c.id')},
 ${childJsonAggSql('inventory_adjustment_moves','move','move.count_id=c.id')})::text,
 NOT COALESCE(${sourceExistsSql(manufacturingInitialPhysicalStockCountSourceRows,'source','c.id','c.company_id')} AND ${terminalReceiptMatchSql(supplyInventoryTerminalReceipts,'count','c.id','c.company_id','c.status','c.version')},false)
 FROM inventory_counts c,scope WHERE scope.full AND ${markedCountExistsSql(manufacturingInitialPhysicalStockMarkedRows)}
 UNION ALL SELECT 'manufacturing-initial-stock',concat(stock.warehouse_id,':',stock.product_id),to_jsonb(stock)::text,stock.quantity_milli<0 OR stock.value_minor<0 FROM commerce_stock stock,scope WHERE scope.full AND ${postedInitialPhysicalStockSourceSql(manufacturingInitialPhysicalStockCountSourceRows,'posted')}
 UNION ALL SELECT 'transfer',t.id,to_jsonb(t)::text,true FROM inventory_transfers t,scope WHERE scope.full AND t.company_id=scope.company_id AND t.status NOT IN ('posted','cancelled','rejected')
 UNION ALL SELECT 'transit',t.id,to_jsonb(t)::text,true FROM inventory_transit t,scope WHERE scope.full AND t.company_id=scope.company_id AND t.status NOT IN ('completed','cancelled')
 UNION ALL SELECT 'valuation',l.id,to_jsonb(l)::text,true FROM inventory_landed_costs l,scope WHERE scope.full AND l.company_id=scope.company_id AND l.status NOT IN ('posted','cancelled','rejected') ORDER BY kind,id`).bind(input.companyId,input.packageId==="manufacturing").all<{kind:string;id:string;fact:string;pending:boolean}>()).results;
 return {companyId:input.companyId,packageId:input.packageId,moduleId:"inventory",observedAt:input.now,revision:createHash("sha256").update(JSON.stringify({companyId:input.companyId,packageVersion:input.packageVersion,facts})).digest("hex"),outstanding:facts.filter(fact=>fact.pending!==false).length};
}
