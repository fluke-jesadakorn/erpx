import "server-only";
import { randomUUID } from "node:crypto";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { iamMutationGuards } from "@erpx/server-identity_access/public/iam-store";
import {workContextGuards,financialConstraintError} from '@erpx/server-workflow/public/command-guards';
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import { ApiError } from "@erpx/server-runtime/public/http";
import { isTransactionalSqlDatabase, type SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { checkPurchasedCapability, companyPurchaseReadLocks, holdingPurchaseReadLocks, peoplePackageEnforcementMode, purchaseCommandBoundary, purchaseCommandFailure, readPurchasedAccess } from "@erpx/server-commercial/public/purchase-command-boundary";
import type { CapabilityPurpose } from "@erpx/server-commercial/public/rules/packages";
import { flushFinanceDeferredEffects } from "@erpx/server-finance/public/finance-transactions";
import type { FlowView } from "@erpx/feature-shared/lib/flow";
import type { PayrollPurchaseControls } from "@erpx/feature-people_payroll/lib/payroll-purchases";

type Command = { key: string; operation: string; purpose: CapabilityPurpose };
type PayrollActor = Pick<WorkContext,"userId"|"assignmentId"|"role"|"companyId"|"branchId"|"departmentId"|"authority"|"iam">;
/** This is a purchase boundary, never a replacement for payroll's own role,
 * Company/record/version, independent review and source-money policies. */
export async function requirePayrollCurrentAuthority(database: SqlDatabase, actor: PayrollActor, key: string) {
  const now = Date.now();
  const assignment = await database.prepare(`SELECT id FROM assignments WHERE id=? AND user_id=? AND role=? AND company_id=?
    AND valid_from<=? AND valid_until>? AND revoked_at IS NULL
    AND (?::text IS NULL OR branch_id IS NULL OR branch_id=?) FOR SHARE`)
    .bind(actor.assignmentId, actor.userId, actor.role, actor.companyId, now, now, actor.branchId, actor.branchId).first();
  if (!assignment) throw new ApiError(403, "PAYROLL_ACCESS_CHANGED", "Choose a current payroll assignment.");
  await database.batch([...workContextGuards(database, actor, key), ...iamMutationGuards(database, actor, key), database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(key)]);
}
export async function payrollPurchasedAccess(database: SqlDatabase, actor: PayrollActor) {
  const mode = await peoplePackageEnforcementMode();
  if (mode === "disabled") return undefined;
  await requirePayrollCurrentAuthority(database, actor, `payroll-read:${randomUUID()}`);
  return readPurchasedAccess(database, mode, { companyId: actor.companyId, capability: "people.payroll" });
}
export async function payrollPurchaseControls(actor: WorkContext): Promise<PayrollPurchaseControls | undefined> {
  const access = await payrollPurchasedAccess((await holdingResources(actor.holdingId)).write, actor);
  return access ? {newWork:access.newWork,settleExisting:access.settleExisting,history:access.history,revision:access.revision} : undefined;
}
export async function payrollDocumentFlow(database: SqlDatabase, actor: PayrollActor, view: FlowView) {
  const access = await payrollPurchasedAccess(database, actor);
  if (access && !access.history) throw new ApiError(403, "PACKAGE_REQUIRED", "A Payroll purchase or retained history is required.");
  return access && !access.settleExisting ? { ...view, canAct: false, canManage: false } : view;
}
export async function withPayrollPurchasedRead<Result>(actor: WorkContext, run: (database?: SqlDatabase) => Promise<Result>, provided?: SqlDatabase) {
  const mode = await peoplePackageEnforcementMode();
  if (mode === "disabled") return run(provided);
  const database = provided ?? (await holdingResources(actor.holdingId)).write;
  await checkPurchasedCapability(database, mode, { companyId: actor.companyId, capability: "people.payroll", purpose: "history" });
  await requirePayrollCurrentAuthority(database, actor, `payroll-read:${randomUUID()}`);
  return run(database);
}
export async function withPayrollPurchasedCommand<Result>(actor: PayrollActor & Partial<Pick<WorkContext,"holdingId">>, command: Command, run: (database?: SqlDatabase) => Promise<Result>, owned?: SqlDatabase) {
  const mode = await peoplePackageEnforcementMode();
  if (mode === "disabled") return run(owned);
  // Generic Flow callers retain their original minimal actor and trusted
  // supplied Holding database. A transaction client without its owning pool
  // or an actual Holding context cannot manufacture that ownership proof.
  const owner = actor.holdingId ? (await holdingResources(actor.holdingId)).write : owned;
  if (!owner || !isTransactionalSqlDatabase(owner)) throw new ApiError(503, "PAYROLL_TRANSACTION_REQUIRED", "Payroll purchase decisions require the owned Holding PostgreSQL transaction.");
  try {
    const execute = async (transaction: SqlDatabase) => {
      await transaction.batch(companyPurchaseReadLocks(transaction, actor.companyId));
      const replay = await transaction.prepare("SELECT 1 FROM command_receipts WHERE command_id=? AND operation=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?")
        .bind(command.key, command.operation, actor.userId, actor.companyId).first();
      const scope = { companyId: actor.companyId, capability: "people.payroll", purpose: replay ? "history" as const : command.purpose };
      await checkPurchasedCapability(transaction, mode, scope);
      const key = `payroll-purchase:${randomUUID()}`, boundary = purchaseCommandBoundary(transaction, mode, key, scope)!;
      await transaction.batch(boundary.before);
      await requirePayrollCurrentAuthority(transaction, actor, key);
      const result = await run(transaction);
      await flushFinanceDeferredEffects(transaction);
      await requirePayrollCurrentAuthority(transaction, actor, key);
      const observations = await transaction.batch([...boundary.after, transaction.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(key)]);
      boundary.observe?.(observations);
      return result;
    };
    if (owned && owned !== owner) {
      const ownership = owner as SqlDatabase & {assertActiveTransaction?: (database: SqlDatabase) => void};
      if (!ownership.assertActiveTransaction) throw new ApiError(503, "TRANSACTION_OWNERSHIP_REQUIRED", "Payroll requires its owned Holding transaction.");
      ownership.assertActiveTransaction(owned);
      return await execute(owned);
    }
    return await owner.transaction(execute);
  } catch (error) { throw purchaseCommandFailure(error) ?? financialConstraintError(error) ?? error; }
}

/** The payment table also contains contractor obligations. Its physical owner
 * does not make those payments a People Payroll purchase. Resolve the actual
 * persisted source; the original owner still validates every source/version. */
export async function withPayrollPaymentCommand<Result>(actor: WorkContext, batchId: string, command: Command, run: (database?: SqlDatabase) => Promise<Result>) {
  if (await peoplePackageEnforcementMode() === "disabled") return run();
  const owner = (await holdingResources(actor.holdingId)).write;
  if (!isTransactionalSqlDatabase(owner)) throw new ApiError(503, "PAYROLL_TRANSACTION_REQUIRED", "Payment source decisions require PostgreSQL transactions.");
  return owner.transaction(async transaction => {
    await transaction.batch(companyPurchaseReadLocks(transaction, actor.companyId));
    const source = await transaction.prepare("SELECT source_type,source_id FROM hr_payment_batches WHERE id=? AND company_id=? FOR SHARE").bind(batchId, actor.companyId).first<{source_type:string;source_id:string}>();
    const result = source?.source_type === "payroll" ? await withPayrollPurchasedCommand(actor, command, run, transaction) : await run(transaction);
    await flushFinanceDeferredEffects(transaction);
    const current = await transaction.prepare("SELECT source_type,source_id FROM hr_payment_batches WHERE id=? AND company_id=?").bind(batchId, actor.companyId).first();
    if (JSON.stringify(source) !== JSON.stringify(current)) throw new ApiError(409, "PAYMENT_SOURCE_CHANGED", "The payment owner changed. Refresh the original obligation.");
    return result;
  });
}
export async function withPayrollPaymentRead<Result>(actor: WorkContext, batchId: string, run: (database?: SqlDatabase) => Promise<Result>) {
  if (await peoplePackageEnforcementMode() === "disabled") return run();
  const database = (await holdingResources(actor.holdingId)).write;
  const source = await database.prepare("SELECT source_type FROM hr_payment_batches WHERE id=? AND company_id=?").bind(batchId, actor.companyId).first<{ source_type: string }>();
  return source?.source_type === "payroll" ? withPayrollPurchasedRead(actor, run, database) : run();
}

/** A scheduled preview retains its existing system/schedule authority. Each
 * original candidate keeps its own transaction; purchase filtering alone never
 * authorizes a payroll approval, posting, payment or Company assignment. */
export async function withPayrollJobsPurchasedTransaction<Result>(owner: SqlDatabase,
  run: (transaction: SqlDatabase, permits: (companyId: string, purpose: CapabilityPurpose) => Promise<boolean>) => Promise<Result>, finalize?: (transaction: SqlDatabase) => Promise<void>) {
  const mode = await peoplePackageEnforcementMode();
  if (!isTransactionalSqlDatabase(owner)) throw new ApiError(503, "PAYROLL_TRANSACTION_REQUIRED", "Payroll jobs require PostgreSQL transactions.");
  if (mode === "disabled") return owner.transaction(transaction => run(transaction, async () => true));
  try {
    return await owner.transaction(async transaction => {
      await transaction.batch(holdingPurchaseReadLocks(transaction));
      const checked = new Map<string, boolean>(), boundaries: { key: string; boundary: NonNullable<ReturnType<typeof purchaseCommandBoundary>> }[] = [];
      const result = await run(transaction, async (companyId, purpose) => {
        const decisionKey = `${companyId}:${purpose}`;
        if (checked.has(decisionKey)) return checked.get(decisionKey)!;
        const scope = { companyId, capability: "people.payroll", purpose }, access = await readPurchasedAccess(transaction, mode, scope);
        const allowed = purpose === "new-work" ? access.newWork : purpose === "settle-existing" ? access.settleExisting : access.history;
        checked.set(decisionKey, allowed);
        if (allowed) {
          const key = `payroll-job:${randomUUID()}`, boundary = purchaseCommandBoundary(transaction, mode, key, scope)!;
          await transaction.batch(boundary.before); boundaries.push({ key, boundary });
        }
        return allowed;
      });
      await flushFinanceDeferredEffects(transaction);
      await finalize?.(transaction);
      const observations = await transaction.batch(boundaries.flatMap(({ key, boundary }) => [...boundary.after, transaction.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(key)]));
      boundaries[0]?.boundary.observe?.(observations);
      return result;
    });
  } catch (error) { throw purchaseCommandFailure(error) ?? financialConstraintError(error) ?? error; }
}
