import "server-only";
import { randomUUID } from "node:crypto";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import { iamMutationGuards } from "@erpx/server-identity_access/public/iam-store";
import { workContextGuards, financialConstraintError } from "@erpx/server-workflow/public/commands";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { isTransactionalSqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import { ApiError } from "@erpx/server-runtime/public/http";
import { companyPurchaseReadLocks, checkPurchasedCapability, purchaseCommandBoundary, purchaseCommandFailure, financePackageEnforcementMode, readPurchasedAccesses } from "@erpx/server-commercial/public/purchase-command-boundary";
import { commercialPackages } from "@erpx/server-commercial/public/packages";
import type { CapabilityPurpose } from "@erpx/server-commercial/public/rules/packages";
import type { FinancePurchaseControls } from "@erpx/server-finance/domain/purchase-controls";
import { flushFinanceDeferredEffects } from "@erpx/server-finance/adapters/finance-transactions";
import {inFinanceSignatureParent,isFinanceSignatureOperation,requireFinanceSignatureParent} from '@erpx/server-finance/adapters/tax-signature-read-parent';

type CompanyPurchase = { companyId: string; capability: string; purpose: CapabilityPurpose };
type Command = { key: string; operation: string; purpose: CapabilityPurpose; capability?: string; sharedConfiguration?: boolean; alternativeCapabilities?: readonly string[];
  strictPurchase?:true;
  signatureRecordId?:string;
  companyPurchases?: (database: SqlDatabase) => Promise<CompanyPurchase[]> };
export const financeReferenceCapabilities = () => [...new Set(["finance.accounting", ...commercialPackages.flatMap(definition => definition.includedCapabilities.filter(capability => capability.startsWith("finance.")))])];
export async function requireFinanceCurrentAuthority(database: SqlDatabase, actor: WorkContext, key: string) {
  requirePermission(actor, "finance.read");
  const row = await database.prepare(`SELECT id FROM assignments WHERE id=? AND user_id=? AND role=? AND company_id=?
    AND valid_from<=? AND valid_until>? AND revoked_at IS NULL AND (?::text IS NULL OR branch_id IS NULL OR branch_id=?) FOR SHARE`)
    .bind(actor.assignmentId, actor.userId, actor.role, actor.companyId, Date.now(), Date.now(), actor.branchId, actor.branchId).first();
  if (!row) throw new ApiError(403, "FINANCE_ACCESS_CHANGED", "Choose a current Finance assignment.");
  await database.batch([...workContextGuards(database, actor, key), ...iamMutationGuards(database, actor, key), database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(key)]);
}
export async function requireFinancePurchasedRead(database: SqlDatabase, actor: WorkContext, capability = "finance.accounting") {
  const mode = await financePackageEnforcementMode();
  if (mode === "disabled") return;
  await checkPurchasedCapability(database, mode, { companyId: actor.companyId, capability, purpose: "history" });
  await requireFinanceCurrentAuthority(database, actor, `finance-read:${randomUUID()}`);
}
/** Shared accounting setup references are available to an authorized included
 * foundation consumer. This does not authorize general GL/reporting commands. */
export async function requireFinanceReferenceRead(database: SqlDatabase, actor: WorkContext) {
  const mode = await financePackageEnforcementMode();
  if (mode === "disabled") return;
  const capabilities = financeReferenceCapabilities();
  const access = await readPurchasedAccesses(database, mode, actor.companyId, capabilities);
  if (!Object.values(access).some(value => value.history)) throw new ApiError(403, "PACKAGE_REQUIRED", "An accounting product or included foundation is required.");
  await requireFinanceCurrentAuthority(database, actor, `finance-reference:${randomUUID()}`);
}
export async function withFinancePurchasedRead<Result>(actor: WorkContext, run: (database?: SqlDatabase) => Promise<Result>, capability = "finance.accounting", sharedConfiguration = false, alternativeCapabilities: readonly string[] = []) {
  const mode=await financePackageEnforcementMode();
  if (mode === "disabled") return run();
  const database = (await holdingResources(actor.holdingId)).write;
  if (sharedConfiguration) await requireFinanceReferenceRead(database, actor);
  else if (alternativeCapabilities.length) {
    const access = await readPurchasedAccesses(database, mode, actor.companyId, [capability, ...alternativeCapabilities]);
    if (!Object.values(access).some(value => value.history)) throw new ApiError(403, "PACKAGE_REQUIRED", "An accounting product or the actual included owner foundation is required.");
    await requireFinanceCurrentAuthority(database, actor, `finance-owner-read:${randomUUID()}`);
  } else await requireFinancePurchasedRead(database, actor, capability);
  return run(database);
}
/** Shared chart, currency and reviewed Company policy setup. Never selects a
 * posting foundation from a user-supplied source label. Original Finance IAM,
 * independence and record policies remain inside the owner command. */
export function withFinanceConfigurationCommand<Result>(actor: WorkContext, command: Omit<Command, "sharedConfiguration" | "capability">, run: (transaction?: SqlDatabase) => Promise<Result>) {
  return withFinancePurchasedCommand(actor, {...command, sharedConfiguration: true}, run);
}
export async function financePurchaseControls(actor: WorkContext, capabilities:readonly string[]=["finance.accounting", "finance.banking", "finance.close", "finance.advanced", "finance.receivables", "finance.tax"]): Promise<FinancePurchaseControls | undefined> {
  const mode = await financePackageEnforcementMode();
  if (mode === "disabled") return undefined;
  const database = (await holdingResources(actor.holdingId)).write;
  await requireFinanceCurrentAuthority(database, actor, `finance-controls:${randomUUID()}`);
  const access = await readPurchasedAccesses(database, mode, actor.companyId, capabilities);
  return Object.fromEntries(Object.entries(access).map(([capability, value]) => [capability, { newWork: value.newWork, settleExisting: value.settleExisting, history: value.history, revision: value.revision }]));
}
/** Only Finance-owned public operations enter here. Low-level prepared journals
 * and an owner's supplied transaction keep their named foundation contract. */
export async function withFinancePurchasedCommand<Result>(actor: WorkContext, command: Command, run: (transaction?: SqlDatabase) => Promise<Result>, ownedTransaction?: SqlDatabase) {
  const mode = command.strictPurchase ? "enforced" as const : await financePackageEnforcementMode();
  const signature=isFinanceSignatureOperation(command.operation)?{key:command.key,operation:command.operation,signatureRecordId:command.signatureRecordId}:null;
  if (mode === "disabled"&&!signature) return run(ownedTransaction);
  const owner = (await holdingResources(actor.holdingId)).write;
  if (!isTransactionalSqlDatabase(owner)) throw new ApiError(503, "TRANSACTION_REQUIRED", "Finance purchase decisions require a Holding transaction.");
  if(mode==='disabled'&&signature){
    if(ownedTransaction){requireFinanceSignatureParent(ownedTransaction,actor,signature);return run(ownedTransaction);}
    return inFinanceSignatureParent(owner,actor,signature,run);
  }
  try {
    const execute = async (transaction: SqlDatabase) => {
      // The generic payment writer must wait before its first Company snapshot.
      // A source reader takes payment SHARE before receipt writer leases; taking
      // payment RX only after inserting its receipt would invert that order.
      if (!ownedTransaction && command.operation === "finance.payment.create") {
        await transaction.batch([companyPurchaseReadLocks(transaction, actor.companyId)[0], transaction.prepare("LOCK TABLE finance_payments IN ROW EXCLUSIVE MODE")]);
      }
      // Chart/account writers must wait before the Company advisory SELECT.
      // Commerce takes its reference SHARE leases before that same advisory;
      // taking the writer lease afterwards would invert the two lock orders.
      if (command.sharedConfiguration && ["finance.chart.create", "finance.account.create", "finance.account.import", "finance.account.active", "finance.account.inactive"].includes(command.operation)) {
        await transaction.batch([companyPurchaseReadLocks(transaction, actor.companyId)[0], transaction.prepare("LOCK TABLE finance_charts,finance_accounts IN ROW EXCLUSIVE MODE")]);
      }
      if (command.sharedConfiguration && ["finance.period.create", "money.rate.confirm"].includes(command.operation)) {
        await transaction.batch([companyPurchaseReadLocks(transaction, actor.companyId)[0], transaction.prepare(command.operation==="finance.period.create"?"LOCK TABLE finance_periods IN ROW EXCLUSIVE MODE":"LOCK TABLE company_fx_rates IN ROW EXCLUSIVE MODE")]);
      }
      // Source identities are read before locks; the original owner retains its
      // version/source guards. Acquire every Company fence in one stable order.
      const sourcePurchases = await command.companyPurchases?.(transaction) ?? [];
      const companies = [...new Set([actor.companyId, ...sourcePurchases.map(scope => scope.companyId)])].sort();
      await transaction.batch(companies.flatMap(company => companyPurchaseReadLocks(transaction, company)));
      const receipt = await transaction.prepare("SELECT 1 FROM command_receipts WHERE command_id=? AND operation=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?")
        .bind(command.key, command.operation, actor.userId, actor.companyId).first();
      const purpose = receipt ? "history" as const : command.purpose;
      let capability = command.capability ?? "finance.accounting";
      if (command.sharedConfiguration || command.alternativeCapabilities?.length) {
        const access = await readPurchasedAccesses(transaction, mode, actor.companyId, command.sharedConfiguration ? financeReferenceCapabilities() : [capability, ...command.alternativeCapabilities!]);
        capability = Object.keys(access).find(key => access[key].codes[purpose] === "GRANTED")
          ?? Object.keys(access).find(key => access[key].codes.history === "GRANTED") ?? capability;
      }
      const scopes = [{ companyId: actor.companyId, capability, purpose }, ...sourcePurchases.map(scope => ({...scope, purpose: receipt ? "history" as const : scope.purpose}))];
      for (const scope of scopes) await checkPurchasedCapability(transaction, mode, scope);
      const key = `finance-purchase:${randomUUID()}`;
      const boundaries = scopes.map((scope,index) => ({ key: `${key}:${index}`, boundary: purchaseCommandBoundary(transaction, mode, `${key}:${index}`, scope)! }));
      await transaction.batch(boundaries.flatMap(item => item.boundary.before));
      await requireFinanceCurrentAuthority(transaction, actor, key);
      const result = await run(transaction);
      await flushFinanceDeferredEffects(transaction);
      await requireFinanceCurrentAuthority(transaction, actor, key);
      const observed = await transaction.batch(boundaries.flatMap(item => [...item.boundary.after, transaction.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(item.key)]));
      boundaries[0]?.boundary.observe?.(observed);
      return result;
    };
    if (ownedTransaction) {
      const ownership = owner as SqlDatabase & {assertActiveTransaction?: (database: SqlDatabase) => void};
      if (!ownership.assertActiveTransaction) throw new ApiError(503,"TRANSACTION_OWNERSHIP_REQUIRED","Finance requires its owned Holding transaction.");
      ownership.assertActiveTransaction(ownedTransaction);
      if(signature)requireFinanceSignatureParent(ownedTransaction,actor,signature);
      return await execute(ownedTransaction);
    }
    if(signature)return await inFinanceSignatureParent(owner,actor,signature,execute);
    return await owner.transaction(execute);
  } catch (error) { throw purchaseCommandFailure(error) ?? financialConstraintError(error) ?? error; }
}
