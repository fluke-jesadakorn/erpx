import { flushFinanceDeferredEffects } from "@erpx/finance-server/public/finance-transactions";
import { nativeClearCommandAssertionsMany } from '@erpx/collaboration-server/adapters/native-command-assertions';
import "server-only";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import { iamMutationGuards } from "@erpx/identity-access-server/public/iam-store";
import {workContextGuards} from '@erpx/workflow-server/public/command-guards';
import type { PostgresDatabase } from "@erpx/platform-runtime-server/public/runtime/postgres";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { companyPurchaseReadLocks, checkPurchasedCapability, readPurchasedAccesses, purchaseCommandBoundary, purchaseCommandFailure,
  posPackageEnforcementMode, type PackageEnforcementMode } from "@erpx/commercial-server/public/purchase-command-boundary";
import { posCommandPurchases, type PosProviderPurchase } from "@erpx/pos-server/domain/package-commands";
import { nativePosOne } from "./native-owner-write";
import { randomUUID } from "node:crypto";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import type { CapabilityPurpose } from "@erpx/commercial-server/public/packages";

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

/** Owner-issued purchase-boundary reads. The SQL for every kind is selected by
 * the native POS owner registry (`erpx-server-pos::adapters::boundary_reads`) on
 * the caller's transaction; only a typed kind and positional values cross this
 * boundary. There is no TypeScript SQL, mirror or fallback here. */
async function nativePosMode(database: SqlDatabase, companyId: string, kind: string, values: unknown[]) {
  return (await nativePosOne<{ mode: string }>(database, companyId, kind, values))?.mode ?? null;
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
  const target = operation === "sale.create" ? nativePosMode(database, companyId, 'pos_boundary_sale_create_mode', [shiftId, companyId])
    : operation === "shift.open" ? nativePosMode(database, companyId, 'pos_boundary_shift_open_mode', [registerId, companyId])
      : operation === "sale.add" ? nativePosMode(database, companyId, 'pos_boundary_sale_add_mode', [typeof payload.productId === "string" ? payload.productId : "", id, companyId])
        : operation.startsWith("sale.") || operation === "payment.create" || operation === "refund.request" ? nativePosMode(database, companyId, 'pos_boundary_sale_mode', [id, companyId])
        : operation === "cash.approve" ? nativePosMode(database, companyId, 'pos_boundary_cash_approve_mode', [id, companyId])
          : operation.startsWith("shift.") || operation === "cash.create" ? nativePosMode(database, companyId, 'pos_boundary_shift_mode', [id, companyId])
            : operation.startsWith("payment.") ? nativePosMode(database, companyId, 'pos_boundary_payment_mode', [id, companyId])
              : operation.startsWith("refund.") ? nativePosMode(database, companyId, 'pos_boundary_refund_mode', [id, companyId])
                : null;
  return target ? (await target) === "restaurant" : false;
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
      const replay = !!await nativePosOne(transaction, actor.companyId, 'pos_boundary_receipt_replay', [key, actor.companyId]);
      const scopes = posCommandPurchases(operation, await restaurantWork(transaction, actor.companyId, operation, input))
        .map(scope => ({ companyId: actor.companyId, ...scope, ...(replay ? { purpose: "history" as const } : {}) }));
      const boundaries = scopes.map((scope, index) => purchaseCommandBoundary(transaction, mode, `${key}:purchase:${index}`, scope)!);
      for (const scope of scopes) await checkPurchasedCapability(transaction, mode, scope);
      await transaction.batch(boundaries.flatMap(boundary => boundary.before));
      const result = await run(transaction);
      await flushFinanceDeferredEffects(transaction);
      const assertions = [key, ...scopes.map((_, index) => `${key}:purchase:${index}`)];
      const observations = await transaction.batch([...workContextGuards(transaction, actor, key), ...iamMutationGuards(transaction, actor, key),
        ...boundaries.flatMap(boundary => boundary.after), await nativeClearCommandAssertionsMany(transaction, actor.companyId, assertions)]);
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
        await nativeClearCommandAssertionsMany(transaction, input.companyId, scopes.map((_, index) => `${key}:${index}`))]);
      boundaries[0]?.observe?.(observations);
      return result;
    });
  } catch (error) { throw purchaseCommandFailure(error) ?? error; }
}
