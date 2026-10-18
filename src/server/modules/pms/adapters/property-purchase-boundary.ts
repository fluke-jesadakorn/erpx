import { flushFinanceDeferredEffects } from "../../finance/public/finance-transactions";
import "server-only";
import { randomUUID } from "node:crypto";
import type { SqlDatabase, TransactionalSqlDatabase } from "../../runtime/public/runtime/types";
import { ApiError } from "../../runtime/public/http";
import { companyPurchaseReadLocks, readPurchasedAccess, readPurchasedAccesses, purchaseCommandBoundary, purchaseCommandFailure,
  pmsPackageEnforcementMode } from "../../commercial/public/purchase-command-boundary";
import type { CapabilityPurpose } from "../../commercial/public/rules/packages";
import type { PmsPropertyPurchase, PmsPropertyPurchaseRequest } from "../domain/property-purchases";
import { assertPmsProviderAuthority } from "./provider-authority";

const capabilities = ["pms.reservations", "pms.rooms", "pms.guest-accounts", "finance.hotel-posting"] as const;
async function checkScopes(database: SqlDatabase, mode: Awaited<ReturnType<typeof pmsPackageEnforcementMode>>, companyId: string, purpose: CapabilityPurpose) {
  const access = await readPurchasedAccesses(database, mode, companyId, capabilities);
  for (const capability of capabilities) {
    const decision = access[capability], allowed = purpose === "new-work" ? decision.newWork : purpose === "settle-existing" ? decision.settleExisting : decision.history;
    if (!allowed) throw new ApiError(403, decision.codes[purpose], "This package does not permit the requested work.");
  }
}
async function propertyScope(database: SqlDatabase, propertyId: string) {
  const property = await database.prepare("SELECT company_id,branch_id FROM pms_properties WHERE id=?").bind(propertyId).first<{ company_id: string; branch_id: string }>();
  if (!property) throw new ApiError(404, "PMS_NOT_FOUND", "Hotel not found.");
  return property;
}
export async function pmsPropertyPurchaseFlags(database: SqlDatabase, propertyId: string) {
  const mode = await pmsPackageEnforcementMode();
  if (mode === "disabled") return { newWork: true, settleExisting: true, history: true };
  const access = await readPurchasedAccess(database, mode, { companyId: (await propertyScope(database, propertyId)).company_id, capability: "pms.reservations" });
  return { newWork: access.newWork, settleExisting: access.settleExisting, history: access.history };
}
export async function pmsPropertyPurchaseAllowed(database: SqlDatabase, propertyId: string, purpose: CapabilityPurpose) {
  const access = await pmsPropertyPurchaseFlags(database, propertyId);
  return purpose === "new-work" ? access.newWork : purpose === "settle-existing" ? access.settleExisting : access.history;
}
export async function requirePmsPropertyPurchase(database: SqlDatabase, input: PmsPropertyPurchase) {
  const mode = await pmsPackageEnforcementMode();
  if (mode === "disabled") return;
  const property = await propertyScope(database, input.propertyId), companyId = property.company_id;
  try {
    await checkScopes(database, mode, companyId, input.purpose);
    if (input.actor) {
      if (input.actor.companyId !== companyId || input.actor.branchId && input.actor.branchId !== property.branch_id || input.actor.departmentId || input.actor.beta) throw new ApiError(404, "PMS_NOT_FOUND", "Hotel not found.");
      await assertPmsProviderAuthority(database, input.actor, input.permission ?? "pms.read");
    }
  } catch (error) { if (input.hideUnavailable && error instanceof ApiError && error.status === 403) throw new ApiError(404, "PMS_NOT_FOUND", "Hotel not found."); throw error; }
}
/** Purchase gates surround the same Property/receipt/money transaction. The
 * preflight identifies only the immutable Company; its binding and all normal
 * token, publication, version, actor and record guards are checked in the unit. */
export async function withPmsPropertyPurchasedTransaction<Result>(owner: TransactionalSqlDatabase,
  run: (transaction: SqlDatabase) => Promise<Result>, request?: PmsPropertyPurchaseRequest) {
  const mode = await pmsPackageEnforcementMode();
  if (mode === "disabled" || !request) return owner.transaction(run);
  const input = typeof request === "function" ? await request() : request;
  if (!input) return owner.transaction(run);
  const companyId = (await propertyScope(owner, input.propertyId)).company_id;
  try {
    return await owner.transaction(async transaction => {
      await transaction.batch(companyPurchaseReadLocks(transaction, companyId));
      const identity = async () => {
        const current = await transaction.prepare("SELECT company_id,branch_id FROM pms_properties WHERE id=? FOR KEY SHARE").bind(input.propertyId).first<{ company_id: string; branch_id: string }>();
        if (current?.company_id !== companyId || input.actor && (input.actor.companyId !== companyId || input.actor.branchId && input.actor.branchId !== current.branch_id || input.actor.departmentId || input.actor.beta)) throw new ApiError(409, "PACKAGE_CAPABILITY_CHANGED", "Hotel scope changed. Review this request again.");
      };
      await identity();
      const replay = input.receipt && !!await transaction.prepare(input.receipt.kind === "reservation"
        ? "SELECT 1 FROM pms_booking_receipts WHERE id=? AND property_id=?" : "SELECT 1 FROM pms_booking_changes WHERE id=? AND property_id=?")
        .bind(input.receipt.id, input.propertyId).first();
      const scopes = capabilities.map(capability => ({ companyId, capability, purpose: replay ? "history" as const : input.purpose }));
      await checkScopes(transaction, mode, companyId, replay ? "history" : input.purpose);
      if (input.actor) await assertPmsProviderAuthority(transaction, input.actor, input.permission ?? "pms.read");
      const key = `pms-property:${randomUUID()}`, boundaries = input.readOnly ? [] : scopes.map((scope, index) => purchaseCommandBoundary(transaction, mode, `${key}:${index}`, scope)!);
      if (boundaries.length) await transaction.batch(boundaries.flatMap(boundary => boundary.before));
      const result = await run(transaction);
      if (!input.readOnly) await flushFinanceDeferredEffects(transaction);
      await identity();
      if (input.actor) await assertPmsProviderAuthority(transaction, input.actor, input.permission ?? "pms.read");
      if (boundaries.length) {
        const observed = await transaction.batch([...boundaries.flatMap(boundary => boundary.after),
          transaction.prepare(`DELETE FROM command_assertions WHERE command_id IN (${scopes.map(() => "?").join(",")})`).bind(...scopes.map((_, index) => `${key}:${index}`))]);
        boundaries[0]?.observe?.(observed);
      } else await checkScopes(transaction, mode, companyId, replay ? "history" : input.purpose);
      return result;
    });
  } catch (error) {
    const failure = purchaseCommandFailure(error) ?? error;
    if (input.hideUnavailable && failure instanceof ApiError && failure.status === 403) throw new ApiError(404, "PMS_NOT_FOUND", "Hotel not found.");
    throw failure;
  }
}

/** A Finance consumer may use full accounting or Hotel's included posting
 * foundation. Purchase never supplies the original Finance/report permission. */
export async function requirePmsFinancialRead(database: SqlDatabase, actor: NonNullable<PmsPropertyPurchase["actor"]>, reports = false) {
  const mode = await pmsPackageEnforcementMode();
  if (mode === "disabled") return;
  const access = await readPurchasedAccesses(database, mode, actor.companyId, ["pms.guest-accounts", "finance.hotel-posting", "finance.accounting"]);
  if (!Object.values(access).some(value => value.history)) throw new ApiError(403, "PACKAGE_REQUIRED", "This package does not permit Hotel financial reporting.");
  await assertPmsProviderAuthority(database, actor, "finance.read");
  if (reports) await assertPmsProviderAuthority(database, actor, "pms.reports");
}
