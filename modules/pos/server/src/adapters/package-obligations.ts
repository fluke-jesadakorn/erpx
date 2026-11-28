import "server-only";
import { createHash } from "node:crypto";
import type { PostgresDatabase } from "@erpx/platform-runtime-server/public/runtime/postgres";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { packageSettlementSource, type PackageSettlementSourceLock } from "@erpx/commercial-server/public/package-settlement-sources";
import { nativePosPackageObligationsStatement } from "./native-pos-obligation-statement";

type Input = { companyId: string; packageId: string; packageVersion: number; moduleId: string; now: number };
const sources = new WeakMap<SqlDatabase, string>();
const tables = ["pos_sales", "pos_registers", "pos_settings", "pos_shifts", "pos_cash_movements", "pos_lines", "pos_fulfillments",
  "pos_reservations", "pos_stock_moves", "pos_payments", "pos_refunds", "pos_refund_payments", "pos_jobs", "pos_stripe_events",
  "pos_settlements", "pos_connections", "pos_day_closes", "pos_room_charge_reversals", "pos_room_charge_reversal_requests", "pos_postings"];
export async function lockPosPackageObligationSources(owner: PostgresDatabase, lock: PackageSettlementSourceLock) {
  const { database, companyId } = packageSettlementSource(owner, lock);
  await database.prepare(`LOCK TABLE ${tables.join(",")} IN SHARE MODE`).run();
  sources.set(database, companyId);
}
export function assertPosPackageObligationSources(owner: PostgresDatabase, database: SqlDatabase, companyId: string) {
  owner.assertActiveTransaction(database);
  if (sources.get(database) !== companyId) throw new Error("POS_PACKAGE_OBLIGATION_SOURCE_REQUIRED");
}
/** Closed source projections for the included Finance/Stock owners. All three
 * readers must hold this POS source lease in the same Holding transaction.
 *
 * CROSS-OWNER CONSUMERS (retained TypeScript, removal documented): consumed by
 * `modules/finance/server/src/adapters/pos-package-obligations.ts` and
 * `modules/inventory/server/src/adapters/pos-package-obligations.ts`, which
 * embed these fragments in their own settlement readers. `lockPosPackageObligationSources`
 * is likewise held by `modules/commercial/server/src/adapters/owner-package-obligations.ts`.
 * Each consumer needs its own owner-side settlement source port and removal
 * task before this projection is retired; the owner SQL is mirrored in
 * `erpx_server_pos::public::obligation_reads`. */
export const posPackageSettlementSources = {
  postings: "SELECT source_key,company_id,journal_id FROM pos_postings",
  payments: "SELECT id,sale_id,company_id,finance_payment_id,state FROM pos_payments",
  settings: "SELECT company_id,accounts_json FROM pos_settings",
  reservations: "SELECT r.fulfillment_id,r.warehouse_id,r.product_id,r.quantity_milli,f.company_id,f.version FROM pos_reservations r JOIN pos_fulfillments f ON f.id=r.fulfillment_id",
} as const;
export async function posPackageObligations(owner: PostgresDatabase, database: SqlDatabase, input: Input) {
  assertPosPackageObligationSources(owner, database, input.companyId);
  if (input.moduleId !== "pos" || input.packageId !== "pos" || input.packageVersion !== 1 || !Number.isSafeInteger(input.now) || input.now < 0)
    throw new Error("POS_PACKAGE_OBLIGATION_SCOPE_REQUIRED");
  const facts = await nativePosPackageObligationsStatement(database, input.companyId);
  return { companyId: input.companyId, packageId: input.packageId, moduleId: "pos", observedAt: input.now,
    revision: createHash("sha256").update(JSON.stringify({ packageVersion: input.packageVersion, companyId: input.companyId, facts })).digest("hex"), outstanding: facts.length };
}
