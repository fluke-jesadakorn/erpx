import "server-only";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { SqlDatabase, SqlStatement } from "@/server/modules/runtime/public/runtime/types";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import { ApiError } from "@/server/modules/runtime/public/http";
import { requirePermission } from "@/server/modules/identity_access/public/iam-permissions";
import { executeCommand, replayCommand } from "@/server/modules/workflow/public/commands";
import { checkPurchasedCapability, readPurchasedAccess, purchaseCommandBoundary, purchaseCommandFailure, supportPackageEnforcementMode, type PurchasedAccess } from "../../commercial/public/purchase-command-boundary";
import type { CapabilityPurpose } from "../../commercial/public/packages";
import { supportCommandPurchase } from "../domain/package-commands";
import type { SupportCommandPorts } from "../application/command";
export function nativeSupportRuntime(actor: WorkContext, supplied?: SqlDatabase) {
  let db = supplied;
  const database = async () => db ??= (await holdingResources(actor.holdingId)).write;
  const access = new Map<string, Promise<PurchasedAccess>>();
  const purchase = (capability: string) => {
    let pending = access.get(capability);
    if (!pending) { pending = (async () => readPurchasedAccess(database, await supportPackageEnforcementMode(), { companyId: actor.companyId, capability }))(); access.set(capability, pending); }
    return pending;
  };
  const requirePurchase = async (capability: string, purpose: CapabilityPurpose = "history") => {
    const value = await purchase(capability), allowed = purpose === "new-work" ? value.newWork : purpose === "settle-existing" ? value.settleExisting : value.history;
    if (!allowed) throw new ApiError(403, value.codes[purpose], "This package does not permit the requested work.");
    return value;
  };
  const ports: SupportCommandPorts = {
    authorize: requirePermission, error: (status, code, message) => new ApiError(status, code, message), now: Date.now,
    replay: async (a, key, operation, id, input) => {
      const write = await database(), prior = await replayCommand(write, a, key, operation, id, input);
      if (prior) await checkPurchasedCapability(write, await supportPackageEnforcementMode(), {
        companyId: a.companyId, ...supportCommandPurchase(operation), purpose: "history",
      });
      return prior;
    },
    commit: async (command, mutation) => {
      const write = await database(), mode = await supportPackageEnforcementMode();
      const scope = { companyId: command.actor.companyId, ...supportCommandPurchase(command.operation) };
      // A receipt may win the race inside executeCommand. Require history here;
      // new effects alone execute the protected purchase guards in that batch.
      await checkPurchasedCapability(write, mode, { ...scope, purpose: "history" });
      try {
        return await executeCommand(write, command.actor, command.key, command.operation, command.input, command.id,
          command.version, command.result, mutation.native as SqlStatement[], command.scope,
          purchaseCommandBoundary(write, mode, command.key, scope));
      } catch (error) { throw purchaseCommandFailure(error) ?? error; }
    },
  };
  return { database, purchase, requirePurchase, ...ports };
}
