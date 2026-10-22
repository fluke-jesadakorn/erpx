import {serviceInitialPhysicalStockMarkedRows} from '@erpx/server-inventory/adapters/service-initial-physical-stock-count-source-sql';
import {posInitialPhysicalStockCountSourceRows,posInitialPhysicalStockMarkedRows} from '@erpx/server-inventory/adapters/pos-initial-physical-stock-count-source-sql';
import {supplyInventoryTerminalReceipts} from '@erpx/server-inventory/adapters/supply-terminal-receipts';
import {recordAttachmentVersionSourceReadLocks} from '@erpx/server-files/public/record-version-receipts';
import "server-only";
import { createHash } from "node:crypto";
import type { PostgresDatabase } from "@erpx/server-runtime/public/runtime/postgres";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { packageSettlementSource, type PackageSettlementSourceLock } from "@erpx/server-commercial/public/package-settlement-sources";
import { assertPosPackageObligationSources, posPackageSettlementSources as pos } from "@erpx/server-pos/public/package-obligations";
type Input = { companyId: string; packageId: string; packageVersion: number; moduleId: string; now: number };
const sources = new WeakMap<SqlDatabase, string>();
export async function lockPosInventoryPackageObligationSources(owner: PostgresDatabase, lock: PackageSettlementSourceLock) {
  const { database, companyId } = packageSettlementSource(owner, lock);
  await database.batch(recordAttachmentVersionSourceReadLocks(database));
  await database.prepare("LOCK TABLE command_receipts,pos_receipts,pos_audit,commerce_stock,inventory_counts,inventory_count_lines,inventory_count_decisions,inventory_adjustment_moves,inventory_transfers,inventory_transfer_lines,inventory_transit,inventory_transit_lines,inventory_transit_events,inventory_landed_costs,inventory_landed_cost_lines IN SHARE MODE").run();
  sources.set(database, companyId);
}
export async function posInventoryPackageObligations(owner: PostgresDatabase, database: SqlDatabase, input: Input) {
  owner.assertActiveTransaction(database);
  if (sources.get(database) !== input.companyId) throw new Error("POS_INVENTORY_PACKAGE_OBLIGATION_SOURCE_REQUIRED");
  assertPosPackageObligationSources(owner, database, input.companyId);
  if (input.moduleId !== "inventory" || input.packageId !== "pos" || input.packageVersion !== 1 || !Number.isSafeInteger(input.now) || input.now < 0)
    throw new Error("POS_INVENTORY_PACKAGE_OBLIGATION_SCOPE_REQUIRED");
  const reservations = (await database.prepare(`SELECT r.fulfillment_id,r.warehouse_id,r.product_id,r.version,r.quantity_milli,
      s.quantity_milli available,s.version stock_version FROM (${pos.reservations}) r
      LEFT JOIN commerce_stock s ON s.warehouse_id=r.warehouse_id AND s.product_id=r.product_id
      WHERE r.company_id=? ORDER BY r.fulfillment_id,r.warehouse_id,r.product_id`).bind(input.companyId).all()).results;
  // A foundation can be the Company's remaining Stock capability. Pending
  // count/transfer/valuation documents must not be orphaned by its withdrawal.
  // Ordinary documents retain their original conservative attribution; the new private source is exact.
  const facts = (await database.prepare(`WITH scope AS (SELECT ?::text company_id)
    SELECT 'count' kind,c.id,c.version,c.status,c.net_value_minor amount,true pending FROM inventory_counts c,scope WHERE c.company_id=scope.company_id AND c.status NOT IN ('posted','cancelled','rejected') AND NOT EXISTS(SELECT 1 FROM (${posInitialPhysicalStockMarkedRows} UNION ALL ${serviceInitialPhysicalStockMarkedRows}) marker WHERE marker.id=c.id)
    UNION ALL SELECT 'transfer',t.id,t.version,t.status,t.total_value_minor,true FROM inventory_transfers t,scope WHERE t.company_id=scope.company_id AND t.status NOT IN ('posted','cancelled','rejected')
    UNION ALL SELECT 'transit',t.id,t.version,t.status,0,true FROM inventory_transit t,scope WHERE t.company_id=scope.company_id AND t.status NOT IN ('completed','cancelled')
    UNION ALL SELECT 'valuation',l.id,l.version,l.status,l.amount_minor,true FROM inventory_landed_costs l,scope WHERE l.company_id=scope.company_id AND l.status NOT IN ('posted','cancelled','rejected')

    UNION ALL SELECT 'pos-initial-count',c.id,c.version,c.status,c.net_value_minor,
      NOT COALESCE(EXISTS(SELECT 1 FROM (${posInitialPhysicalStockCountSourceRows}) source WHERE source.id=c.id AND source.company_id=c.company_id) AND EXISTS(SELECT 1 FROM (${supplyInventoryTerminalReceipts}) terminal WHERE terminal.kind='count' AND terminal.id=c.id AND terminal.company_id=c.company_id AND terminal.status=c.status AND terminal.version=c.version),false)
      FROM inventory_counts c,scope WHERE c.company_id=scope.company_id AND EXISTS(SELECT 1 FROM (${posInitialPhysicalStockMarkedRows}) marker WHERE marker.id=c.id)
    UNION ALL SELECT 'pos-initial-stock',concat(stock.warehouse_id,':',stock.product_id),stock.version,'stock',stock.value_minor,stock.quantity_milli<0 OR stock.value_minor<0 FROM commerce_stock stock,scope WHERE EXISTS(SELECT 1 FROM (${posInitialPhysicalStockCountSourceRows}) source JOIN inventory_counts posted ON posted.id=source.id AND posted.company_id=source.company_id AND posted.status='posted' WHERE source.company_id=scope.company_id AND source.warehouse_id=stock.warehouse_id AND source.product_id=stock.product_id)
    ORDER BY kind,id`).bind(input.companyId).all()).results;
  return { companyId: input.companyId, packageId: input.packageId, moduleId: "inventory", observedAt: input.now,
    revision: createHash("sha256").update(JSON.stringify({ companyId: input.companyId, packageVersion: input.packageVersion, reservations, facts })).digest("hex"), outstanding: reservations.length + facts.filter(fact=>fact.pending!==false).length };
}
