import "server-only";
import { createHash } from "node:crypto";
import type { PostgresDatabase } from "@erpx/server-runtime/public/runtime/postgres";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { packageSettlementSource, type PackageSettlementSourceLock } from "@erpx/server-commercial/public/package-settlement-sources";

type Input = { companyId: string; packageId: string; packageVersion: number; moduleId: string; now: number };
const sources = new WeakMap<SqlDatabase, string>();
const tables = ["pms_properties", "pms_records", "pms_sources", "pms_postings", "pms_documents", "pms_jobs", "pms_stock_moves",
  "pms_transfer_requests", "pms_charge_corrections", "pms_import_batches", "pms_import_balances", "pms_booking_payments",
  "pms_booking_allocations", "pms_booking_refund_requests", "pms_booking_refund_attempts", "pms_booking_changes",
  "pms_booking_receipts", "pms_booking_tokens", "pms_booking_emails", "pms_stripe_payments", "pms_counter_refund_attempts"];
export async function lockPmsPackageObligationSources(owner: PostgresDatabase, lock: PackageSettlementSourceLock) {
  const { database, companyId } = packageSettlementSource(owner, lock);
  await database.prepare(`LOCK TABLE ${tables.join(",")} IN SHARE MODE`).run();
  sources.set(database, companyId);
}
export function assertPmsPackageObligationSources(owner: PostgresDatabase, database: SqlDatabase, companyId: string) {
  owner.assertActiveTransaction(database);
  if (sources.get(database) !== companyId) throw new Error("PMS_PACKAGE_OBLIGATION_SOURCE_REQUIRED");
}
/** Closed, owner-declared projections for included owners. They must acquire
 * this same source lease, rather than inventing their own Hotel joins. */
export const pmsPackageSettlementSources = {
  properties: "SELECT id,company_id,branch_id,config_json FROM pms_properties",
  postings: "SELECT p.company_id,pp.property_id,pp.source_key,pp.journal_id FROM pms_postings pp JOIN pms_properties p ON p.id=pp.property_id UNION ALL SELECT company_id,property_id,id,opening_journal_id FROM pms_import_batches WHERE state='applied' AND opening_journal_id IS NOT NULL",
  payments: "SELECT p.company_id,d.finance_id FROM pms_documents d JOIN pms_properties p ON p.id=d.property_id WHERE d.kind IN ('receipt','refund') AND d.finance_id IS NOT NULL UNION SELECT company_id,finance_payment_id FROM pms_transfer_requests WHERE finance_payment_id IS NOT NULL UNION SELECT p.company_id,b.finance_payment_id FROM pms_import_balances b JOIN pms_properties p ON p.id=b.property_id WHERE b.finance_payment_id IS NOT NULL",
  stockMoves: "SELECT p.company_id,m.* FROM pms_stock_moves m JOIN pms_properties p ON p.id=m.property_id",
  roomCharges: "SELECT p.company_id,s.property_id,s.source_key FROM pms_sources s JOIN pms_properties p ON p.id=s.property_id WHERE s.source_key LIKE 'pos:%'",
  clearing: "SELECT p.company_id,c.account_id FROM pms_properties p CROSS JOIN LATERAL (VALUES (p.config_json::jsonb#>>'{accounts,edc}'),(p.config_json::jsonb#>>'{accounts,receivable}'),(p.config_json::jsonb#>>'{accounts,advance}'),(p.config_json::jsonb#>>'{accounts,deposit}'),(p.config_json::jsonb#>>'{onlineAccounting,refundPayable}'),(p.config_json::jsonb#>>'{onlineAccounting,unallocated}')) c(account_id) WHERE c.account_id IS NOT NULL AND c.account_id<>'' UNION SELECT company_id,clearing_account FROM pms_booking_payments WHERE clearing_account IS NOT NULL UNION SELECT company_id,clearing_account FROM pms_stripe_payments WHERE clearing_account IS NOT NULL",
} as const;
export async function pmsPackageObligations(owner: PostgresDatabase, database: SqlDatabase, input: Input) {
  assertPmsPackageObligationSources(owner, database, input.companyId);
  if (input.moduleId !== "pms" || input.packageId !== "pms" || input.packageVersion !== 1 || !Number.isSafeInteger(input.now) || input.now < 0)
    throw new Error("PMS_PACKAGE_OBLIGATION_SCOPE_REQUIRED");
  // Frozen guest receipt/access mail and failed-mail retries retain history
  // rights after inactivation; they do not orphan a settlement obligation.
  const facts = (await database.prepare(`WITH properties AS MATERIALIZED (SELECT * FROM pms_properties WHERE company_id=?),
    records AS MATERIALIZED (SELECT r.id,r.property_id,r.kind,r.version,r.data_json::jsonb data FROM pms_records r JOIN properties p ON p.id=r.property_id)
    SELECT r.kind,r.id,r.version,COALESCE(r.data->>'state','outstanding') state FROM records r WHERE
      (r.kind='reservation' AND r.data->>'state' IN ('payment_pending','confirmed'))
      OR (r.kind='stay' AND r.data->>'state' IN ('reserved','in_house'))
      OR (r.kind='guest_account' AND (r.data->>'state'='open' OR COALESCE((r.data->>'due')::bigint,0)<>0 OR COALESCE((r.data->>'advance')::bigint,0)<>0 OR COALESCE((r.data->>'deposit')::bigint,0)<>0 OR COALESCE((r.data->>'refundReserved')::bigint,0)<>0 OR COALESCE((r.data->>'depositRefundReserved')::bigint,0)<>0))
      OR (r.kind='shift' AND r.data->>'state'<>'reviewed')
      OR (r.kind='refund' AND r.data->>'state' NOT IN ('paid','rejected'))
      OR (r.kind='credit' AND r.data->>'state'='requested')
      OR (r.kind='task' AND r.data->>'state' NOT IN ('done','superseded'))
      OR (r.kind='block' AND r.data->>'state'='active')
      OR (r.kind='night' AND r.data->>'state'<>'completed')
      OR (r.kind='lost' AND r.data->>'state'='stored')
      OR (r.kind='item' AND (COALESCE((r.data->>'issued')::bigint,0)<>0 OR COALESCE((r.data->>'laundry')::bigint,0)<>0))
      OR (r.kind='guest' AND EXISTS(SELECT 1 FROM jsonb_array_elements(COALESCE(r.data->'serviceRequests','[]'::jsonb)) request WHERE request->>'state'='open'))
    UNION ALL SELECT 'property',p.id,p.version,p.state FROM properties p WHERE p.state='closing'
    UNION ALL SELECT 'job',j.id,j.updated_at,j.state FROM pms_jobs j JOIN properties p ON p.id=j.property_id WHERE j.state<>'completed'
    UNION ALL SELECT 'transfer',t.id,t.version,t.state FROM pms_transfer_requests t JOIN properties p ON p.id=t.property_id WHERE t.state='pending'
    UNION ALL SELECT 'correction',c.id,c.version,c.status FROM pms_charge_corrections c JOIN properties p ON p.id=c.property_id WHERE c.status='pending'
    UNION ALL SELECT 'booking-payment',b.id,b.updated_at,b.status FROM pms_booking_payments b JOIN properties p ON p.id=b.property_id WHERE b.status='payment_pending' OR b.status='refund_review' AND b.refund_state IS DISTINCT FROM 'succeeded' OR b.status='expired' AND b.checkout_id IS NOT NULL
    UNION ALL SELECT 'booking-refund',r.id,r.updated_at,r.state FROM pms_booking_refund_requests r JOIN pms_booking_payments b ON b.id=r.payment_id JOIN properties p ON p.id=b.property_id WHERE r.state<>'succeeded'
    UNION ALL SELECT 'booking-change',c.id,c.created_at,c.state FROM pms_booking_changes c JOIN properties p ON p.id=c.property_id WHERE c.state='payment_pending'
    UNION ALL SELECT 'counter-payment',c.id,c.updated_at,c.state FROM pms_stripe_payments c JOIN properties p ON p.id=c.property_id WHERE c.state IN ('requested','pending') OR c.state='refund_review' AND c.refund_state IS DISTINCT FROM 'succeeded' OR c.received_at IS NOT NULL AND c.entry_id IS NULL OR c.refund_state IS NOT NULL AND c.refund_state<>'succeeded'
    UNION ALL SELECT 'counter-refund',concat(a.payment_id,':',a.generation),a.updated_at,a.state FROM pms_counter_refund_attempts a JOIN pms_stripe_payments c ON c.id=a.payment_id JOIN properties p ON p.id=c.property_id WHERE a.state IN ('processing','requires_action')
    ORDER BY kind,id`).bind(input.companyId).all()).results;
  return { companyId: input.companyId, packageId: input.packageId, moduleId: "pms", observedAt: input.now,
    revision: createHash("sha256").update(JSON.stringify({ companyId: input.companyId, packageVersion: input.packageVersion, facts })).digest("hex"), outstanding: facts.length };
}
