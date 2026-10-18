import { flushFinanceDeferredEffects } from "../../finance/public/finance-transactions";
import "server-only";
import { randomUUID } from "node:crypto";
import type { SqlDatabase, TransactionalSqlDatabase } from "../../runtime/public/runtime/types";
import { companyPurchaseReadLocks, checkPurchasedCapability, purchaseCommandBoundary, purchaseCommandFailure,
  pmsPackageEnforcementMode } from "../../commercial/public/purchase-command-boundary";
import type { PmsCounterPurchase } from "../application/pms-stripe";
import { assertPmsProviderAuthority } from "./provider-authority";

const capabilities = ["pms.guest-accounts", "finance.hotel-posting"] as const;
const scopes = (input: PmsCounterPurchase) => capabilities.map(capability => ({ companyId: input.companyId, capability, purpose: input.purpose ?? "settle-existing" as const }));
/** Shared Stripe transport does not require POS management or a former cashier
 * assignment. Hotel's own package includes its posting foundation. */
export async function requirePmsCounterPurchase(database: SqlDatabase, input: PmsCounterPurchase) {
  const mode = await pmsPackageEnforcementMode();
  if (mode === "disabled") return;
  for (const scope of scopes(input)) await checkPurchasedCapability(database, mode, scope);
  if (input.actor) await assertPmsProviderAuthority(database, input.actor, input.permission);
}
export async function withPmsCounterPurchasedTransaction<Result>(owner: TransactionalSqlDatabase, input: PmsCounterPurchase, run: (transaction: SqlDatabase) => Promise<Result>) {
  const mode = await pmsPackageEnforcementMode();
  if (mode === "disabled") return owner.transaction(run);
  try {
    return await owner.transaction(async transaction => {
      await transaction.batch(companyPurchaseReadLocks(transaction, input.companyId));
      for (const scope of scopes(input)) await checkPurchasedCapability(transaction, mode, scope);
      if (input.actor) await assertPmsProviderAuthority(transaction, input.actor, input.permission);
      const key = `pms-counter:${randomUUID()}`, boundaries = scopes(input).map((scope, index) => purchaseCommandBoundary(transaction, mode, `${key}:${index}`, scope)!);
      await transaction.batch(boundaries.flatMap(boundary => boundary.before));
      const result = await run(transaction);
      await flushFinanceDeferredEffects(transaction);
      if (input.actor) await assertPmsProviderAuthority(transaction, input.actor, input.permission);
      const observed = await transaction.batch([...boundaries.flatMap(boundary => boundary.after),
        transaction.prepare(`DELETE FROM command_assertions WHERE command_id IN (${capabilities.map(() => "?").join(",")})`).bind(...capabilities.map((_, index) => `${key}:${index}`))]);
      boundaries[0]?.observe?.(observed);
      return result;
    });
  } catch (error) { throw purchaseCommandFailure(error) ?? error; }
}
