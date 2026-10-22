import { flushFinanceDeferredEffects } from "@erpx/server-finance/public/finance-transactions";
import "server-only";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { iamMutationGuards } from "@erpx/server-identity_access/public/iam-store";
import { workContextGuards } from "@erpx/server-workflow/public/commands";
import type { PostgresDatabase } from "@erpx/server-runtime/public/runtime/postgres";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { companyPurchaseReadLocks, checkPurchasedCapability, readPurchasedAccesses, purchaseCommandBoundary, purchaseCommandFailure,
  posPackageEnforcementMode, type PackageEnforcementMode } from "@erpx/server-commercial/public/purchase-command-boundary";
import { posCommandPurchases, type PosProviderPurchase } from "@erpx/server-pos/domain/package-commands";
import { randomUUID } from "node:crypto";
import { ApiError } from "@erpx/server-runtime/public/http";
import type { CapabilityPurpose } from "@erpx/server-commercial/public/packages";

export async function requirePosPurchased(database: SqlDatabase, actor: WorkContext, capability = "pos.sales", purpose: CapabilityPurpose = "history") {
  await checkPurchasedCapability(database, await posPackageEnforcementMode(), { companyId: actor.companyId, capability, purpose });
}
/** Finance source views may use full accounting or the included POS foundation;
 * their original Finance/report authority is still required by the owner. */
export async function requirePosReportingPurchase(database: SqlDatabase, actor: WorkContext) {
  const mode = await posPackageEnforcementMode();
  if (mode === "disabled") return;
  const access = await readPurchasedAccesses(database, mode, actor.companyId, ["pos.sales", "finance.accounting", "finance.pos-posting"]);
  if (!Object.values(access).some(value => value.history)) throw new ApiError(403, "PACKAGE_REQUIRED", "This package does not permit POS reporting.");
}

/** Derive add-on use from the actual owner record, never a client-supplied
 * service mode on an existing sale/payment/refund/shift. Company remains scoped. */
async function restaurantWork(database: SqlDatabase, companyId: string, operation: string, input: unknown) {
  const payload = input && typeof input === "object" ? input as Record<string, unknown> : {};
  const id = typeof payload.id === "string" ? payload.id : "";
  if (operation === "settings.bootstrap" || operation === "register.save") return payload.mode === "restaurant";
  if (operation === "catalog.save") return payload.kind === "recipe";
  if (operation.startsWith("kitchen.") || operation.startsWith("table.")) return true;
  const shiftId = typeof payload.shiftId === "string" ? payload.shiftId : "";
  const registerId = typeof payload.registerId === "string" ? payload.registerId : "";
  const target = operation === "sale.create" ? database.prepare("SELECT r.mode FROM pos_shifts s JOIN pos_registers r ON r.id=s.register_id WHERE s.id=? AND s.company_id=?").bind(shiftId, companyId)
    : operation === "shift.open" ? database.prepare("SELECT mode FROM pos_registers WHERE id=? AND company_id=?").bind(registerId, companyId)
      : operation === "sale.add" ? database.prepare("SELECT CASE WHEN r.mode='restaurant' OR c.kind='recipe' THEN 'restaurant' ELSE r.mode END mode FROM pos_sales s JOIN pos_registers r ON r.id=s.register_id LEFT JOIN pos_catalog c ON c.product_id=? AND c.company_id=s.company_id WHERE s.id=? AND s.company_id=?").bind(typeof payload.productId === "string" ? payload.productId : "", id, companyId)
        : operation.startsWith("sale.") || operation === "payment.create" || operation === "refund.request" ? database.prepare("SELECT CASE WHEN r.mode='restaurant' OR EXISTS(SELECT 1 FROM pos_lines l JOIN pos_fulfillments f ON f.id=l.fulfillment_id WHERE l.sale_id=s.id AND f.kind='recipe') THEN 'restaurant' ELSE r.mode END mode FROM pos_sales s JOIN pos_registers r ON r.id=s.register_id WHERE s.id=? AND s.company_id=?").bind(id, companyId)
        : operation === "cash.approve" ? database.prepare("SELECT r.mode FROM pos_cash_movements m JOIN pos_shifts s ON s.id=m.shift_id JOIN pos_registers r ON r.id=s.register_id WHERE m.id=? AND s.company_id=?").bind(id, companyId)
          : operation.startsWith("shift.") || operation === "cash.create" ? database.prepare("SELECT r.mode FROM pos_shifts s JOIN pos_registers r ON r.id=s.register_id WHERE s.id=? AND s.company_id=?").bind(id, companyId)
            : operation.startsWith("payment.") ? database.prepare("SELECT r.mode FROM pos_payments p JOIN pos_sales s ON s.id=p.sale_id JOIN pos_registers r ON r.id=s.register_id WHERE p.id=? AND s.company_id=?").bind(id, companyId)
              : operation.startsWith("refund.") ? database.prepare("SELECT r.mode FROM pos_refunds f JOIN pos_sales s ON s.id=f.sale_id JOIN pos_registers r ON r.id=s.register_id WHERE f.id=? AND s.company_id=?").bind(id, companyId)
                : null;
  return target ? (await target.first<{ mode: string }>())?.mode === "restaurant" : false;
}

/** The original POS application still owns permissions, record versions,
 * receipt hashes, Finance/stock and audit. Add the purchase gates around that
 * same Holding transaction, before its locks and after its final owner effects. */
export async function withPosPurchasedCommand<Result>(owner: PostgresDatabase, actor: WorkContext, operation: string, input: unknown, key: string,
  mode: PackageEnforcementMode, run: (transaction: SqlDatabase) => Promise<Result>) {
  if (mode === "disabled") return owner.transaction(async transaction => {
    // Rare strict startup constructors lease package sources before POS and
    // Finance references. Keep the original disabled policy/results, while
    // waiting at that same query-free fence before the first POS snapshot.
    await transaction.batch([companyPurchaseReadLocks(transaction, actor.companyId)[0]]);
    return run(transaction);
  });
  try {
    return await owner.transaction(async transaction => {
      await transaction.batch(companyPurchaseReadLocks(transaction, actor.companyId));
      const replay = !!await transaction.prepare("SELECT 1 FROM pos_receipts WHERE id=? AND company_id=?").bind(key, actor.companyId).first();
      const scopes = posCommandPurchases(operation, await restaurantWork(transaction, actor.companyId, operation, input))
        .map(scope => ({ companyId: actor.companyId, ...scope, ...(replay ? { purpose: "history" as const } : {}) }));
      const boundaries = scopes.map((scope, index) => purchaseCommandBoundary(transaction, mode, `${key}:purchase:${index}`, scope)!);
      for (const scope of scopes) await checkPurchasedCapability(transaction, mode, scope);
      await transaction.batch(boundaries.flatMap(boundary => boundary.before));
      const result = await run(transaction);
      await flushFinanceDeferredEffects(transaction);
      const assertions = [key, ...scopes.map((_, index) => `${key}:purchase:${index}`)];
      const observations = await transaction.batch([...workContextGuards(transaction, actor, key), ...iamMutationGuards(transaction, actor, key),
        ...boundaries.flatMap(boundary => boundary.after), transaction.prepare(`DELETE FROM command_assertions WHERE command_id IN (${assertions.map(() => "?").join(",")})`).bind(...assertions)]);
      boundaries[0]?.observe?.(observations);
      return result;
    });
  } catch (error) { throw purchaseCommandFailure(error) ?? error; }
}
export { posPackageEnforcementMode };

async function providerScopes(database: SqlDatabase, input: PosProviderPurchase) {
  return posCommandPurchases(input.operation, await restaurantWork(database, input.companyId, input.operation, { id: input.recordId }))
    .map(scope => ({ companyId: input.companyId, ...scope, purpose: "settle-existing" as const }));
}
export async function requirePosProviderPurchase(database: SqlDatabase, input: PosProviderPurchase) {
  const mode = await posPackageEnforcementMode();
  if (mode !== "disabled") for (const scope of await providerScopes(database, input)) await checkPurchasedCapability(database, mode, scope);
}
/** Provider reconciliation retains its independent system authority and exact
 * provider identity. Purchase guards share each original financial transaction;
 * signed callbacks can still be durably ingested before settlement is possible. */
export async function withPosProviderPurchasedTransaction<Result>(owner: PostgresDatabase, input: PosProviderPurchase, run: (transaction: SqlDatabase) => Promise<Result>) {
  const mode = await posPackageEnforcementMode();
  if (mode === "disabled") return owner.transaction(async transaction => {
    // Preserve provider policy/results while sharing the query-free startup
    // exclusion before the provider's first original business snapshot.
    await transaction.batch([companyPurchaseReadLocks(transaction, input.companyId)[0]]);
    return run(transaction);
  });
  try {
    return await owner.transaction(async transaction => {
      await transaction.batch(companyPurchaseReadLocks(transaction, input.companyId));
      const scopes = await providerScopes(transaction, input), key = `pos-provider:${randomUUID()}`;
      for (const scope of scopes) await checkPurchasedCapability(transaction, mode, scope);
      const boundaries = scopes.map((scope, index) => purchaseCommandBoundary(transaction, mode, `${key}:${index}`, scope)!);
      await transaction.batch(boundaries.flatMap(boundary => boundary.before));
      const result = await run(transaction);
      await flushFinanceDeferredEffects(transaction);
      const observations = await transaction.batch([...boundaries.flatMap(boundary => boundary.after),
        transaction.prepare(`DELETE FROM command_assertions WHERE command_id IN (${scopes.map(() => "?").join(",")})`).bind(...scopes.map((_, index) => `${key}:${index}`))]);
      boundaries[0]?.observe?.(observations);
      return result;
    });
  } catch (error) { throw purchaseCommandFailure(error) ?? error; }
}
