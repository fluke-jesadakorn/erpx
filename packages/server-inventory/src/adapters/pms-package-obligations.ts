import "server-only";
import { createHash } from "node:crypto";
import type { PostgresDatabase } from "@erpx/server-runtime/public/runtime/postgres";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { packageSettlementSource, type PackageSettlementSourceLock } from "@erpx/server-commercial/public/package-settlement-sources";
import { assertPmsPackageObligationSources, pmsPackageSettlementSources as pms } from "@erpx/server-pms/public/package-obligations";
type Input = { companyId: string; packageId: string; packageVersion: number; moduleId: string; now: number };
const sources = new WeakMap<SqlDatabase, string>();
export async function lockPmsInventoryPackageObligationSources(owner: PostgresDatabase, lock: PackageSettlementSourceLock) {
  const { database, companyId } = packageSettlementSource(owner, lock);
  await database.prepare("LOCK TABLE commerce_stock,inventory_counts,inventory_count_lines,inventory_transfers,inventory_transfer_lines,inventory_transit,inventory_transit_lines,inventory_transit_events,inventory_landed_costs,inventory_landed_cost_lines IN SHARE MODE").run();
  sources.set(database, companyId);
}
export async function pmsInventoryPackageObligations(owner: PostgresDatabase, database: SqlDatabase, input: Input) {
  owner.assertActiveTransaction(database);
  if (sources.get(database) !== input.companyId) throw new Error("PMS_INVENTORY_PACKAGE_OBLIGATION_SOURCE_REQUIRED");
  assertPmsPackageObligationSources(owner, database, input.companyId);
  if (input.moduleId !== "inventory" || input.packageId !== "pms" || input.packageVersion !== 1 || !Number.isSafeInteger(input.now) || input.now < 0)
    throw new Error("PMS_INVENTORY_PACKAGE_OBLIGATION_SCOPE_REQUIRED");
  // Keep Company stock foundation work reachable, including work from another
  // owner using the same foundation. Shared attribution is conservative here.
  const facts = (await database.prepare(`WITH scope AS (SELECT ?::text company_id)
    SELECT 'count' kind,c.id,c.version,c.status,c.net_value_minor amount FROM inventory_counts c,scope WHERE c.company_id=scope.company_id AND c.status NOT IN ('posted','cancelled','rejected')
    UNION ALL SELECT 'transfer',t.id,t.version,t.status,t.total_value_minor FROM inventory_transfers t,scope WHERE t.company_id=scope.company_id AND t.status NOT IN ('posted','cancelled','rejected')
    UNION ALL SELECT 'transit',t.id,t.version,t.status,0 FROM inventory_transit t,scope WHERE t.company_id=scope.company_id AND t.status NOT IN ('completed','cancelled')
    UNION ALL SELECT 'valuation',l.id,l.version,l.status,l.amount_minor FROM inventory_landed_costs l,scope WHERE l.company_id=scope.company_id AND l.status NOT IN ('posted','cancelled','rejected')
    UNION ALL SELECT 'stock',concat(s.warehouse_id,':',s.product_id),s.version,'invalid',s.quantity_milli FROM commerce_stock s,scope
      WHERE (s.quantity_milli<0 OR s.value_minor<0) AND EXISTS(SELECT 1 FROM (${pms.stockMoves}) m WHERE m.company_id=scope.company_id AND m.warehouse_id=s.warehouse_id AND m.product_id=s.product_id)
    ORDER BY kind,id`).bind(input.companyId).all()).results;
  return { companyId: input.companyId, packageId: input.packageId, moduleId: "inventory", observedAt: input.now,
    revision: createHash("sha256").update(JSON.stringify({ companyId: input.companyId, packageVersion: input.packageVersion, facts })).digest("hex"), outstanding: facts.length };
}
