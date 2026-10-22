import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { AttachmentOperation } from "@erpx/server-files/public/attachments/types";
import { observePurchaseResults, purchaseCommandBoundary, supportPackageEnforcementMode } from "@erpx/server-commercial/public/purchase-command-boundary";
import { nativeSupportRuntime } from "@erpx/server-support/adapters/command-runtime";

export type SupportAttachmentPurchase = { companyId: string; capability: string };
export function supportAttachmentPurchaseScope(actor: WorkContext, targetType: string): SupportAttachmentPurchase | null {
  const capability = targetType === "support_ticket" ? "support.evidence" : targetType === "support_incident" ? "support.incidents" : null;
  return capability ? { companyId: actor.companyId, capability } : null;
}
/** A purchase is additional to the Files owner's current IAM/record policy. */
export async function authorizeSupportAttachmentPurchase(database: SqlDatabase, actor: WorkContext, targetType: string, operation: AttachmentOperation): Promise<SupportAttachmentPurchase | null> {
  const scope = supportAttachmentPurchaseScope(actor, targetType);
  if (!scope) return null;
  await nativeSupportRuntime(actor, database).requirePurchase(scope.capability, operation === "read" ? "history" : "settle-existing");
  return scope;
}

/** Protect every generic file batch/command that changed an authorized Support
 * target. Unique guard keys coexist with normal owner/receipt assertion keys. */
export async function supportAttachmentCommandBoundary(database: SqlDatabase, scopes: readonly SupportAttachmentPurchase[]) {
  if (!scopes.length) return undefined;
  const mode = await supportPackageEnforcementMode();
  if (mode === "disabled") return undefined;
  const unique = [...new Map(scopes.map(scope => [`${scope.companyId}:${scope.capability}`, scope])).values()]
    .sort((a, b) => a.companyId.localeCompare(b.companyId) || a.capability.localeCompare(b.capability));
  const boundaries = unique.map(scope => {
    const key = `support-file-purchase:${crypto.randomUUID()}`;
    return { key, boundary: purchaseCommandBoundary(database, mode, key, { ...scope, purpose: "settle-existing" })! };
  });
  return { before: boundaries.flatMap(item => item.boundary.before),
    after: [...boundaries.flatMap(item => item.boundary.after), ...boundaries.map(item => database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(item.key))],
    observe: mode === "shadow" ? observePurchaseResults : undefined };
}
