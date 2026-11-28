import "server-only";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { checkPurchasedCapability, readPurchasedAccess, pmsPackageEnforcementMode } from "@erpx/commercial-server/public/purchase-command-boundary";
import { assertPmsProviderAuthority } from "@erpx/pms-server/adapters/provider-authority";
import { ApiError } from "@erpx/platform-runtime-server/public/http";

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
