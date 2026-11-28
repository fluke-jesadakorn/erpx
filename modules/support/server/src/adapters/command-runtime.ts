import{nativeSupportReadRuntime}from '@erpx/support-server/adapters/read-runtime';
import "server-only";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import type { SqlDatabase, SqlStatement } from "@erpx/platform-runtime-server/public/runtime/types";

import { ApiError } from "@erpx/platform-runtime-server/public/http";
import { requirePermission } from "@erpx/identity-access-server/public/iam-permissions";
import { executeCommand, replayCommand } from "@erpx/workflow-server/public/commands";
import { checkPurchasedCapability, purchaseCommandBoundary, purchaseCommandFailure, supportPackageEnforcementMode } from "@erpx/commercial-server/public/purchase-command-boundary";

import { supportCommandPurchase } from "@erpx/support-server/domain/package-commands";
import type { SupportCommandPorts } from "@erpx/support-server/application/command";
export function nativeSupportRuntime(actor:WorkContext,supplied?:SqlDatabase){const {database,purchase,requirePurchase}=nativeSupportReadRuntime(actor,supplied);const ports: SupportCommandPorts = {
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
return { database, purchase, requirePurchase, ...ports };}
