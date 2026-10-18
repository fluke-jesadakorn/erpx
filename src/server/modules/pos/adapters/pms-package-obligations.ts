import "server-only";
import { createHash } from "node:crypto";
import type { PostgresDatabase } from "../../runtime/public/runtime/postgres";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
import { assertPmsPackageObligationSources, pmsPackageSettlementSources as pms } from "../../pms/public/package-obligations";
import { assertPosPackageObligationSources } from "./package-obligations";

type Input = { companyId: string; packageId: string; packageVersion: number; moduleId: string; now: number };
/** The shared provider transport remains POS-owned. This reader is scoped to
 * Hotel v1, independently of POS base product withdrawal. */
export async function pmsPosPackageObligations(owner: PostgresDatabase, database: SqlDatabase, input: Input) {
  assertPosPackageObligationSources(owner, database, input.companyId);
  assertPmsPackageObligationSources(owner, database, input.companyId);
  if (input.moduleId !== "pos" || input.packageId !== "pms" || input.packageVersion !== 1 || !Number.isSafeInteger(input.now) || input.now < 0)
    throw new Error("PMS_POS_PACKAGE_OBLIGATION_SCOPE_REQUIRED");
  const facts = (await database.prepare(`WITH scope AS (SELECT ?::text company_id),
    hotels AS MATERIALIZED (SELECT p.id FROM (${pms.properties}) p,scope WHERE p.company_id=scope.company_id),
    linked_sales AS MATERIALIZED (SELECT s.* FROM pos_sales s JOIN pos_registers r ON r.id=s.register_id,scope WHERE s.company_id=scope.company_id
      AND (r.property_id IN (SELECT id FROM hotels) OR EXISTS(SELECT 1 FROM (${pms.roomCharges}) c WHERE c.company_id=s.company_id AND c.source_key LIKE 'pos:'||s.id||':%')))
    SELECT 'sale' kind,s.id,s.version,s.state FROM linked_sales s WHERE s.state NOT IN ('completed','cancelled','merged')
    UNION ALL SELECT 'shift',s.id,s.version,s.state FROM pos_shifts s JOIN pos_registers r ON r.id=s.register_id,scope WHERE s.company_id=scope.company_id AND r.property_id IN (SELECT id FROM hotels) AND s.state<>'reviewed'
    UNION ALL SELECT 'kitchen',f.id,f.version,f.state FROM pos_fulfillments f JOIN pos_lines l ON l.fulfillment_id=f.id JOIN linked_sales s ON s.id=l.sale_id WHERE f.state NOT IN ('delivered','cancelled') AND f.kind='recipe'
    UNION ALL SELECT 'payment',p.id,p.version,p.state FROM pos_payments p JOIN linked_sales s ON s.id=p.sale_id WHERE p.state IN ('requested','pending','exception')
    UNION ALL SELECT 'refund',r.id,r.version,r.state FROM pos_refunds r JOIN linked_sales s ON s.id=r.sale_id WHERE r.state NOT IN ('completed','rejected')
    UNION ALL SELECT 'room-reversal',r.id,r.version,r.state FROM pos_room_charge_reversal_requests r,scope WHERE r.company_id=scope.company_id AND r.property_id IN (SELECT id FROM hotels) AND r.state IN ('requested','pos_approved')
    UNION ALL SELECT 'provider',c.company_id,c.version,c.state FROM pos_connections c,scope WHERE c.company_id=scope.company_id AND c.account_id IS NOT NULL AND c.state<>'disconnected'
    UNION ALL SELECT 'job',j.id,j.attempts,j.state FROM pos_jobs j,scope WHERE j.company_id=scope.company_id AND j.state<>'completed'
    UNION ALL SELECT 'callback',e.id,0,'unapplied' FROM pos_stripe_events e,scope WHERE e.company_id=scope.company_id AND e.applied_at IS NULL
    UNION ALL SELECT 'provider-settlement',s.id,0,s.state FROM pos_settlements s,scope WHERE s.company_id=scope.company_id AND (s.state IN ('unmapped','pending','exception','needs_response','under_review','warning_needs_response','warning_under_review') OR s.journal_id IS NULL AND s.amount_minor<>0)
    ORDER BY kind,id`).bind(input.companyId).all()).results;
  return { companyId: input.companyId, packageId: input.packageId, moduleId: "pos", observedAt: input.now,
    revision: createHash("sha256").update(JSON.stringify({ companyId: input.companyId, packageVersion: input.packageVersion, facts })).digest("hex"), outstanding: facts.length };
}
