import {serviceInitialPhysicalStockMarkedRows} from '@erpx/inventory-server/adapters/service-initial-physical-stock-count-source-sql';
import {posInitialPhysicalStockCountSourceRows,posInitialPhysicalStockMarkedRows} from '@erpx/inventory-server/adapters/pos-initial-physical-stock-count-source-sql';
import {supplyInventoryTerminalReceipts} from '@erpx/inventory-server/adapters/supply-terminal-receipts';
import {postedInitialPhysicalStockSourceSql,terminalReceiptMatchSql,sourceExistsSql,packageObligationDocumentRowsSql,packageObligationCountHeadSql,countScopeRowsSql,markedCountExistsSql} from './read-tail-sql';
import {recordAttachmentVersionSourceReadLocks} from '@erpx/files-server/public/record-version-receipts';
import "server-only";
import { createHash } from "node:crypto";
import type { PostgresDatabase } from "@erpx/platform-runtime-server/public/runtime/postgres";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { packageSettlementSource, type PackageSettlementSourceLock } from "@erpx/commercial-server/public/package-settlement-sources";
import { assertPosPackageObligationSources, posPackageSettlementSources as pos } from "@erpx/pos-server/public/package-obligations";
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
  const facts = (await database.prepare(`${packageObligationCountHeadSql},true pending FROM inventory_counts c,scope WHERE${countScopeRowsSql(`${posInitialPhysicalStockMarkedRows} UNION ALL ${serviceInitialPhysicalStockMarkedRows}`)}
${packageObligationDocumentRowsSql(',true')}

    UNION ALL SELECT 'pos-initial-count',c.id,c.version,c.status,c.net_value_minor,
      NOT COALESCE(${sourceExistsSql(posInitialPhysicalStockCountSourceRows,'source','c.id','c.company_id')} AND ${terminalReceiptMatchSql(supplyInventoryTerminalReceipts,'count','c.id','c.company_id','c.status','c.version')},false)
      FROM inventory_counts c,scope WHERE ${markedCountExistsSql(posInitialPhysicalStockMarkedRows)}
    UNION ALL SELECT 'pos-initial-stock',concat(stock.warehouse_id,':',stock.product_id),stock.version,'stock',stock.value_minor,stock.quantity_milli<0 OR stock.value_minor<0 FROM commerce_stock stock,scope WHERE ${postedInitialPhysicalStockSourceSql(posInitialPhysicalStockCountSourceRows,'posted')}
    ORDER BY kind,id`).bind(input.companyId).all()).results;
  return { companyId: input.companyId, packageId: input.packageId, moduleId: "inventory", observedAt: input.now,
    revision: createHash("sha256").update(JSON.stringify({ companyId: input.companyId, packageVersion: input.packageVersion, reservations, facts })).digest("hex"), outstanding: reservations.length + facts.filter(fact=>fact.pending!==false).length };
}
