import { flushFinanceDeferredEffects } from "@erpx/finance-server/public/finance-transactions";
import { nativeClearCommandAssertionsMany } from '@erpx/collaboration-server/adapters/native-command-assertions';
import "server-only";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import { iamMutationGuards } from "@erpx/identity-access-server/public/iam-store";
import {workContextGuards} from '@erpx/workflow-server/public/command-guards';
import type { SqlDatabase, TransactionalSqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { companyPurchaseReadLocks, checkPurchasedCapability, purchaseCommandBoundary, purchaseCommandFailure,
  pmsPackageEnforcementMode } from "@erpx/commercial-server/public/purchase-command-boundary";
import { pmsCommandPurchases } from "@erpx/pms-server/domain/package-commands";

export type PmsPurchaseCommand = { actor: WorkContext; operation: string; input: unknown; key: string };
/** Staff and durable Night Audit share this adapter. Keep the original Property
 * session lock, indexed reads and pipelined transaction options. */
export async function withPmsPurchasedCommand<Result>(owner: TransactionalSqlDatabase, command: PmsPurchaseCommand,
  run: (transaction: SqlDatabase) => Promise<Result>, options?: Parameters<TransactionalSqlDatabase["transaction"]>[1]) {
  const mode = await pmsPackageEnforcementMode();
  if (mode === "disabled") return owner.transaction(run, options);
  const { actor, operation, input, key } = command;
  try {
    return await owner.transaction(async transaction => {
      await transaction.batch(companyPurchaseReadLocks(transaction, actor.companyId));
      const replay = !!await transaction.prepare("SELECT 1 FROM command_receipts WHERE command_id=? AND operation=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?")
        .bind(key, `pms.${operation}`, actor.userId, actor.companyId).first();
      const scopes = pmsCommandPurchases(operation, input).map(scope => ({ companyId: actor.companyId, ...scope, ...(replay ? { purpose: "history" as const } : {}) }));
      const boundaries = scopes.map((scope, index) => purchaseCommandBoundary(transaction, mode, `${key}:pms-purchase:${index}`, scope)!);
      for (const scope of scopes) await checkPurchasedCapability(transaction, mode, scope);
      await transaction.batch(boundaries.flatMap(boundary => boundary.before));
      const result = await run(transaction);
      await flushFinanceDeferredEffects(transaction);
      const assertions = [key, ...scopes.map((_, index) => `${key}:pms-purchase:${index}`)];
      const observed = await transaction.batch([...workContextGuards(transaction, actor, key), ...iamMutationGuards(transaction, actor, key), ...boundaries.flatMap(boundary => boundary.after),
        await nativeClearCommandAssertionsMany(transaction, actor.companyId, assertions)]);
      boundaries[0]?.observe?.(observed);
      return result;
    }, options);
  } catch (error) { throw purchaseCommandFailure(error) ?? error; }
}
