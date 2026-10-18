import "server-only";
import { createHash } from "node:crypto";
import type { PostgresDatabase } from "../../runtime/public/runtime/postgres";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
import { packageSettlementSource, type PackageSettlementSourceLock } from "../../commercial/public/package-settlement-sources";

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
 * readers must hold this POS source lease in the same Holding transaction. */
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
  const facts = (await database.prepare(`WITH scope AS (SELECT ?::text company_id)
    SELECT 'sale' kind,s.id,s.version,s.state FROM pos_sales s,scope WHERE s.company_id=scope.company_id AND s.state NOT IN ('completed','cancelled','merged')
    UNION ALL SELECT 'shift',s.id,s.version,s.state FROM pos_shifts s,scope WHERE s.company_id=scope.company_id AND s.state<>'reviewed'
    UNION ALL SELECT 'cash',m.id,0,'unapproved' FROM pos_cash_movements m JOIN pos_shifts s ON s.id=m.shift_id,scope WHERE s.company_id=scope.company_id AND m.approved_by IS NULL
    UNION ALL SELECT 'kitchen',f.id,f.version,f.state FROM pos_fulfillments f,scope WHERE f.company_id=scope.company_id AND f.kind='recipe' AND f.state NOT IN ('delivered','cancelled')
    UNION ALL SELECT 'payment',p.id,p.version,p.state FROM pos_payments p,scope WHERE p.company_id=scope.company_id AND p.state IN ('requested','pending','exception')
    UNION ALL SELECT 'refund',r.id,r.version,r.state FROM pos_refunds r,scope WHERE r.company_id=scope.company_id AND r.state NOT IN ('completed','rejected')
    UNION ALL SELECT 'refund-payment',rp.id,0,rp.state FROM pos_refund_payments rp JOIN pos_refunds r ON r.id=rp.refund_id,scope WHERE r.company_id=scope.company_id AND rp.state<>'succeeded'
    UNION ALL SELECT 'room-reversal',r.id,r.version,r.state FROM pos_room_charge_reversal_requests r,scope WHERE r.company_id=scope.company_id AND r.state IN ('requested','pos_approved')
    UNION ALL SELECT 'job',j.id,j.attempts,j.state FROM pos_jobs j,scope WHERE j.company_id=scope.company_id AND j.state<>'completed'
    UNION ALL SELECT 'callback',e.id,0,'unapplied' FROM pos_stripe_events e,scope WHERE e.company_id=scope.company_id AND e.applied_at IS NULL
    UNION ALL SELECT 'provider',c.company_id,c.version,c.state FROM pos_connections c,scope WHERE c.company_id=scope.company_id AND c.account_id IS NOT NULL AND c.state<>'disconnected'
    UNION ALL SELECT 'provider-settlement',s.id,0,s.state FROM pos_settlements s,scope WHERE s.company_id=scope.company_id AND (s.state IN ('unmapped','pending','exception','needs_response','under_review','warning_needs_response','warning_under_review') OR s.journal_id IS NULL AND s.amount_minor<>0)
    UNION ALL SELECT 'day',concat(d.branch_id,':',d.business_date),0,'unconfirmed' FROM (
      SELECT company_id,branch_id,business_date FROM pos_sales UNION SELECT company_id,branch_id,business_date FROM pos_shifts
    ) d,scope WHERE d.company_id=scope.company_id AND NOT EXISTS(SELECT 1 FROM pos_day_closes c WHERE c.company_id=d.company_id AND c.branch_id=d.branch_id AND c.business_date=d.business_date AND c.state='confirmed')
    UNION ALL SELECT 'day-preview',c.id,c.version,c.state FROM pos_day_closes c,scope WHERE c.company_id=scope.company_id AND c.state<>'confirmed'
    ORDER BY kind,id`).bind(input.companyId).all()).results;
  return { companyId: input.companyId, packageId: input.packageId, moduleId: "pos", observedAt: input.now,
    revision: createHash("sha256").update(JSON.stringify({ packageVersion: input.packageVersion, companyId: input.companyId, facts })).digest("hex"), outstanding: facts.length };
}
