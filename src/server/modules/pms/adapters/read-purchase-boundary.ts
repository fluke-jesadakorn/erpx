import "server-only";
import type { WorkContext } from "../../identity_access/public/authorization";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
import { checkPurchasedCapability, readPurchasedAccess, pmsPackageEnforcementMode } from "../../commercial/public/purchase-command-boundary";
import { assertPmsProviderAuthority } from "./provider-authority";
import { ApiError } from "../../runtime/public/http";

export async function pmsCompanyHistoryAllowed(database: SqlDatabase, actor: WorkContext) {
  if (actor.departmentId || actor.beta) return false;
  return (await readPurchasedAccess(database, await pmsPackageEnforcementMode(), { companyId: actor.companyId, capability: "pms.front-desk" })).history;
}
export async function requirePmsCompanyHistory(database: SqlDatabase, actor: WorkContext, permission = "pms.read") {
  const mode = await pmsPackageEnforcementMode();
  if (mode === "disabled") return;
  if (actor.departmentId || actor.beta) throw new ApiError(403, "PMS_BRANCH_REQUIRED", "Choose a Company or Branch.");
  await checkPurchasedCapability(database, mode, { companyId: actor.companyId, capability: "pms.front-desk", purpose: "history" });
  await assertPmsProviderAuthority(database, actor, permission);
}
