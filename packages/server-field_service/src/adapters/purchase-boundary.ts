import { assignmentReadSql } from "@erpx/server-identity_access/public/record-source-authority";
import "server-only";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase, SqlStatement } from "@erpx/server-runtime/public/runtime/types";
import { executeCommand, replayCommand } from "@erpx/server-workflow/public/commands";
import { ApiError } from "@erpx/server-runtime/public/http";
import { servicePackageEnforcementMode, readPurchasedAccess, checkPurchasedCapability, purchaseCommandBoundary,
  purchaseCommandFailure, observePurchaseResults, type PurchasedAccess } from "@erpx/server-commercial/public/purchase-command-boundary";
import type { CapabilityPurpose } from "@erpx/server-commercial/public/packages";
import { serviceCommandPurchases } from "@erpx/server-field_service/domain/package-commands";
import {serviceFinalPurchaseCheck} from "@erpx/server-field_service/adapters/purchases";
export function servicePurchaseRuntime(actor: WorkContext, database: () => Promise<SqlDatabase>) {
  const cache = new Map<string, Promise<PurchasedAccess>>();
  let current: Promise<boolean> | undefined;
  const currentRead = () => current ??= (async () => {
    const db = await database(); return Boolean(await db.prepare(`SELECT ${assignmentReadSql("s", "records.read")} allowed
      FROM (SELECT ?::text company_id,?::text branch_id,?::text department_id) s`)
      .bind(actor.assignmentId, actor.userId, actor.role, actor.companyId, actor.branchId, actor.departmentId).first("allowed"));
  })();
  const requireCurrentRead = async () => { if (!await currentRead()) throw new ApiError(403, "WORK_ROLE_UNAVAILABLE", "The selected work role is no longer available."); };
  const purchase = (capability: string) => { let value = cache.get(capability); if (!value) {
    value = (async () => readPurchasedAccess(database, await servicePackageEnforcementMode(), { companyId: actor.companyId, capability }))(); cache.set(capability, value);
  } return value; };
  const requirePurchase = async (capability: string, purpose: CapabilityPurpose = "history") => {
    const value = await purchase(capability), allowed = purpose === "history" ? value.history : purpose === "new-work" ? value.newWork : value.settleExisting;
    if (!allowed) throw new ApiError(403, value.codes[purpose], "This package does not permit the requested work."); return value;
  };
  const replay = async (key: string, operation: string, id: string, input: unknown) => {
    const db = await database(), prior = await replayCommand(db, actor, key, operation, id, input);
    if (prior) for (const scope of serviceCommandPurchases(operation)) await checkPurchasedCapability(db, await servicePackageEnforcementMode(), { companyId: actor.companyId, ...scope, purpose: "history" });
    return prior;
  };
  return { purchase, requirePurchase, replay, currentRead, requireCurrentRead };
}
/** Every included stock/invoice effect stays in the original Holding batch. */
export async function executeServiceCommand<Result>(db: SqlDatabase, actor: WorkContext, key: string, operation: string,
  input: unknown, id: string, version: number, result: Result, effects: SqlStatement[], usesAsset: boolean | (() => Promise<boolean>) = false) {
  const mode = await servicePackageEnforcementMode(), asset = mode === "disabled" ? false : typeof usesAsset === "function" ? await usesAsset() : usesAsset, scopes = serviceCommandPurchases(operation, asset);
  await serviceFinalPurchaseCheck(db,actor,key,operation,asset);
  for (const scope of scopes) await checkPurchasedCapability(db, mode, { companyId: actor.companyId, ...scope, purpose: "history" });
  const boundaries = scopes.map((scope, index) => purchaseCommandBoundary(db, mode, `${key}:service-purchase:${index}`, { companyId: actor.companyId, ...scope }));
  const boundary = mode === "disabled" ? undefined : {
    before: boundaries.flatMap(value => value?.before ?? []),
    after: [...boundaries.flatMap(value => value?.after ?? []), ...scopes.map((_, index) => db.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(`${key}:service-purchase:${index}`))],
    observe: mode === "shadow" ? observePurchaseResults : undefined,
  };
  try { return await executeCommand(db, actor, key, operation, input, id, version, result, effects, actor, boundary); }
  catch (error) { throw purchaseCommandFailure(error) ?? error; }
}
