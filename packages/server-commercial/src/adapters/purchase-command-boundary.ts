import "server-only";
import { createHash } from "node:crypto";
import type { SqlDatabase, SqlResult } from "@erpx/server-runtime/public/runtime/types";
import { ApiError } from "@erpx/server-runtime/public/http";
import { capabilityRevisionSource } from "@erpx/server-commercial/domain/capability-decision";
import { purchasedCapability, type CompanyPackageGrant, type CompanyPackageHistoryRight, type CapabilityPurpose } from "@erpx/server-commercial/domain/packages";
import { readCompanyPurchaseFacts } from "@erpx/server-commercial/adapters/package-grants";
import { packageWriterFence } from "@erpx/server-commercial/adapters/package-locks";
import { packageClockSql, purchasedCapabilityPredicate } from "@erpx/server-commercial/adapters/purchased-capability-sql";
import type { PackageEnforcementMode } from "@erpx/server-commercial/adapters/development-package-enforcement";

type Scope = { companyId: string; capability: string; purpose: CapabilityPurpose };
export type PurchasedAccess = { newWork: boolean; settleExisting: boolean; history: boolean; revision: string;
  codes: Record<CapabilityPurpose, ReturnType<typeof purchasedCapability>["code"]> };
type Observation = { commercial_purchase_observation: true; company_id: string; capability: string; purpose: CapabilityPurpose;
  allowed: boolean; observed_at: number; grants: CompanyPackageGrant[] | string; history: CompanyPackageHistoryRight[] | string };
function observe(input: Scope, grants: CompanyPackageGrant[], history: CompanyPackageHistoryRight[], now: number, phase: "read" | "checked") {
  const decision = purchasedCapability(grants, input.companyId, input.capability, input.purpose, now, history);
  const purchaseRevision = createHash("sha256").update(capabilityRevisionSource(input.companyId, grants, now, history)).digest("hex");
  // Scoped decision telemetry only. No record, user, evidence or grant bodies.
  try { console.info(JSON.stringify({ event: "commercial.purchase.shadow", phase, ...input, code: decision.code, purchaseRevision })); }
  catch { /* Telemetry failure never changes owner command behavior. */ }
  return decision;
}

/** Purchase is one gate. Existing release, IAM, record and transaction guards
 * remain owner responsibilities; this reader never fabricates those gates. */
export async function checkPurchasedCapability(database: SqlDatabase, mode: PackageEnforcementMode, input: Scope) {
  if (mode === "disabled") return;
  const { grants, history } = await readCompanyPurchaseFacts(database, input.companyId), now = Date.now();
  const decision = mode === "shadow" ? observe(input, grants, history, now, "read") : purchasedCapability(grants, input.companyId, input.capability, input.purpose, now, history);
  if (mode === "enforced" && !decision.allowed) throw new ApiError(403, decision.code, "This package does not permit the requested work.");
}

/** One request-local presentation observation, without private grant facts.
 * Owner authority/release/record policy must still intersect these flags. */
export async function readPurchasedAccess(database: SqlDatabase | (() => Promise<SqlDatabase>), mode: PackageEnforcementMode, input: Omit<Scope, "purpose">): Promise<PurchasedAccess> {
  return (await readPurchasedAccesses(database, mode, input.companyId, [input.capability]))[input.capability];
}

/** Related owner controls share one Company purchase/history snapshot. */
export async function readPurchasedAccesses(database: SqlDatabase | (() => Promise<SqlDatabase>), mode: PackageEnforcementMode, companyId: string, capabilities: readonly string[]): Promise<Record<string, PurchasedAccess>> {
  const purposes = ["new-work", "settle-existing", "history"] as const;
  if (mode === "disabled") return Object.fromEntries(capabilities.map(capability => [capability, { newWork: true, settleExisting: true, history: true, revision: "disabled",
    codes: { "new-work": "GRANTED", "settle-existing": "GRANTED", history: "GRANTED" } }]));
  const { grants, history } = await readCompanyPurchaseFacts(typeof database === "function" ? await database() : database, companyId), now = Date.now();
  const revision = createHash("sha256").update(capabilityRevisionSource(companyId, grants, now, history)).digest("hex");
  return Object.fromEntries([...new Set(capabilities)].map(capability => {
    const decisions = purposes.map(purpose => mode === "shadow" ? observe({ companyId, capability, purpose }, grants, history, now, "read")
      : purchasedCapability(grants, companyId, capability, purpose, now, history));
    const allowed = (index: number) => mode === "shadow" || decisions[index].allowed;
    return [capability, { newWork: allowed(0), settleExisting: allowed(1), history: allowed(2), revision,
      codes: { "new-work": decisions[0].code, "settle-existing": decisions[1].code, history: decisions[2].code } }];
  }));
}

/** Attach to an owner's existing atomic batch, before aggregate/IAM locks and
 * after receipts/audit/outbox. The last clock is sampled after all owner writes. */
export function purchaseCommandBoundary(database: SqlDatabase, mode: PackageEnforcementMode, key: string, input: Scope) {
  if (mode === "disabled") return undefined;
  const predicate = purchasedCapabilityPredicate("purchase_scope.company_id", input.capability, input.purpose, "purchase_scope.now");
  const clock = "WITH purchase_scope AS MATERIALIZED (SELECT ?::text company_id," + packageClockSql + " now) ";
  const guard = (phase: string) => database.prepare(clock + `INSERT INTO command_assertions(command_id,step,ok) SELECT ?,?,CASE WHEN ${predicate} THEN 1 ELSE 0 END FROM purchase_scope`)
    .bind(input.companyId, key, `commercial-purchase-${phase}`);
  const before = [...companyPurchaseReadLocks(database, input.companyId),
    ...(mode === "enforced" ? [guard("before")] : [])];
  const after = mode === "enforced" ? [guard("after")] : [database.prepare(clock + `SELECT true commercial_purchase_observation,purchase_scope.company_id,?::text capability,?::text purpose,${predicate} allowed,purchase_scope.now observed_at,
    COALESCE((SELECT jsonb_agg(jsonb_build_object('companyId',g.company_id,'packageId',g.package_id,'packageVersion',g.package_version,'state',g.state,'startsAt',g.starts_at,'endsAt',g.ends_at,'version',g.version)) FROM company_package_grants g WHERE g.company_id=purchase_scope.company_id),'[]'::jsonb) grants,
    ${historyObservationSql("purchase_scope.company_id")} history FROM purchase_scope`)
    .bind(input.companyId, input.capability, input.purpose)];
  return { before, after, observe: mode === "shadow" ? observePurchaseResults : undefined };
}

/** Company owner commands take these before aggregate/IAM locks, including
 * when deciding whether a receipt is a replay under retained history. */
export function companyPurchaseReadLocks(database: SqlDatabase, companyId: string) {
  return [packageWriterFence(database),
    database.prepare("SELECT pg_advisory_xact_lock(hashtextextended(?,0))").bind(`commercial.package:company:${companyId}`),
    database.prepare("SELECT package_id FROM company_package_grants WHERE company_id=? ORDER BY package_id FOR SHARE").bind(companyId)];
}

/** Observational only: may run after a pool batch has committed. */
export function observePurchaseResults(results: SqlResult[]): undefined {
  try {
    for (const result of results) for (const row of result.results) {
      if (row.commercial_purchase_observation !== true) continue;
      const item = row as unknown as Observation;
      const grants = typeof item.grants === "string" ? JSON.parse(item.grants) as CompanyPackageGrant[] : item.grants;
      const history = typeof item.history === "string" ? JSON.parse(item.history) as CompanyPackageHistoryRight[] : item.history;
      observe({ companyId: item.company_id, capability: item.capability, purpose: item.purpose }, grants, history, Number(item.observed_at), "checked");
    }
  } catch { /* Never turn telemetry failure into an owner command failure. */ }
  return undefined;
}

/** A Holding-wide owner sweep uses the same order as normal Company writers.
 * Follow with locks on the owner's source rows before evaluating effects. */
export function holdingPurchaseReadLocks(database: SqlDatabase) {
  return [packageWriterFence(database),
    database.prepare("SELECT pg_advisory_xact_lock(hashtextextended('commercial.package:company:'||company_id,0)) FROM (SELECT DISTINCT company_id FROM company_package_grants ORDER BY company_id) companies"),
    database.prepare("SELECT company_id,package_id FROM company_package_grants ORDER BY company_id,package_id FOR SHARE")];
}

/** Select-list fragment for an owner's actual shadow batch. Company/time are
 * code-owned identifiers; all private grant facts stay on the server. */
export function purchaseObservationProjection(company: string, capability: string, purpose: CapabilityPurpose, now: string) {
  const predicate = purchasedCapabilityPredicate(company, capability, purpose, now), literal = (value: string) => `'${value.replaceAll("'", "''")}'`;
  return `true commercial_purchase_observation,${company} company_id,${literal(capability)}::text capability,${literal(purpose)}::text purpose,${predicate} allowed,${now} observed_at,
    COALESCE((SELECT jsonb_agg(jsonb_build_object('companyId',g.company_id,'packageId',g.package_id,'packageVersion',g.package_version,'state',g.state,'startsAt',g.starts_at,'endsAt',g.ends_at,'version',g.version)) FROM company_package_grants g WHERE g.company_id=${company}),'[]'::jsonb) grants,
    ${historyObservationSql(company)} history`;
}

function historyObservationSql(company: string) {
  return `COALESCE((SELECT jsonb_agg(jsonb_build_object('companyId',h.company_id,'packageId',h.package_id,'packageVersion',h.package_version,'sourceGrantVersion',h.source_grant_version,'sourceEventId',h.source_event_id,'retainedAt',h.retained_at)) FROM company_package_history_rights h WHERE h.company_id=${company}),'[]'::jsonb)`;
}

export function purchaseCommandFailure(error: unknown) {
  let current = error;
  for (let depth = 0; depth < 5 && current && typeof current === "object"; depth++) {
    const failure = current as { code?: string; detail?: string; cause?: unknown };
    if (failure.code === "23514" && failure.detail?.includes(", commercial-purchase-"))
      return new ApiError(409, "PACKAGE_CAPABILITY_CHANGED", "Package access changed. Reload and review before retrying.");
    current = failure.cause;
  }
  return null;
}
