import {peoplePackageEnforcementMode} from "@erpx/server-commercial/public/purchase-command-boundary";
import {withExpensePurchasedRead,expensePurchaseControls} from "@erpx/server-people_payroll/adapters/expense-purchases";
import { runtimeAppSources } from "@erpx/server-runtime/public/app-native-sources";
import "server-only";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { holdingResources, runtimeEnv } from "../../../../packages/server-runtime/src/public/runtime";
import {expenseDraftSchema} from '@erpx/server-people_payroll/adapters/expense-drafts';
import {expenseFromDatabase,getExpense} from '@erpx/server-people_payroll/adapters/expense-read';
import {expenseOptions} from '@erpx/server-people_payroll/adapters/expenses';
import { expenseRoute } from "@erpx/server-people_payroll/adapters/expense-routing";
import { activeStaff } from "@erpx/server-operations/public/staff-policy";
import { staffSettings } from "@erpx/server-operations/public/staff-settings";
import { namedRoute } from "@erpx/server-records/public/request-preview";
import { expenseAiStatus, queueExpenseAi } from "@erpx/server-people_payroll/adapters/expense-ai";
import { ApiError } from "@erpx/server-runtime/public/http";
import { createExpensePreview } from "@erpx/server-people_payroll/application/expense-preview";
import { createExpensePostingPreview } from "@erpx/server-people_payroll/application/expense-posting-preview";
import { expenseJournalProjection, expensePostingAccounts } from "@erpx/server-finance/public/expense-posting-projection";
import { expensePostingConfiguration } from "@erpx/server-commerce/public/expense-posting-configuration";
import {nativeExpenseOptions,type ExpenseRow} from '@erpx/server-people_payroll/adapters/expenses';

export async function expensePostingPreview(actor: WorkContext, expense: ExpenseRow) {
  const {write}=await holdingResources(actor.holdingId);
  return withExpensePurchasedRead(write,actor,async database=>{
    const current=await peoplePackageEnforcementMode()==='disabled'?expense:await expenseFromDatabase(database,actor,String(expense.id));
    return createExpensePostingPreview({repository:async()=>({posted:journalId=>expenseJournalProjection(database,actor.companyId,journalId),configuration:()=>expensePostingConfiguration(database,actor.companyId),accounts:()=>expensePostingAccounts(database,actor.companyId)})})(current);
  });
}

const preview = createExpensePreview<WorkContext, Awaited<ReturnType<typeof namedRoute>>>({
  options: expenseOptions,
  expense: getExpense,
  async staff(actor, claimant) {
    const { write } = await holdingResources(actor.holdingId);
    return activeStaff(write, actor.companyId, claimant);
  },
  async route(actor, claimant, preparers, requestKind) {
    const { write } = await holdingResources(actor.holdingId);
    return namedRoute(write, actor, await expenseRoute(write, actor.companyId, claimant, preparers, false, requestKind));
  },
  error: violation => new ApiError(violation.status, violation.code, violation.message),
  blocker: error => error instanceof ApiError && [409, 422].includes(error.status) ? error.message : null,
});

export async function previewExpense(actor: WorkContext, raw: unknown) {
  const input = expenseDraftSchema.parse(raw);
  const id = raw && typeof raw === "object" && "id" in raw && typeof raw.id === "string" ? raw.id : undefined;
  if(await peoplePackageEnforcementMode()==='disabled')return preview(actor,input,id);
  const {write}=await holdingResources(actor.holdingId);
  return withExpensePurchasedRead(write,actor,async database=>createExpensePreview<WorkContext,Awaited<ReturnType<typeof namedRoute>>>({
    options:context=>nativeExpenseOptions(database)(context),
    expense:(context,expenseId)=>expenseFromDatabase(database,context,expenseId),
    staff:(context,claimant)=>activeStaff(database,context.companyId,claimant),
    route:async(context,claimant,preparers,requestKind)=>namedRoute(database,context,await expenseRoute(database,context.companyId,claimant,preparers,false,requestKind)),
    error:violation=>new ApiError(violation.status,violation.code,violation.message),
    blocker:error=>error instanceof ApiError&&[409,422].includes(error.status)?error.message:null,
  })(actor,input,id));
}

export async function readExpenseAiStatus(actor: WorkContext, id: string, payment: string | null) {
  await getExpense(actor, id);
  const { write } = await holdingResources(actor.holdingId), env = await runtimeEnv();
  const health = await runtimeAppSources(env.CONTROL_DB).readyHeartbeat();
  return { ...await expenseAiStatus(write, actor, env, id, payment), workerReady: !!health && Date.now() - Number(health.heartbeat_at) < 45000 };
}

export async function expensePageCapabilities(actor: WorkContext) {
  const { write } = await holdingResources(actor.holdingId);
  return { canRepair: (await staffSettings(write, actor)).canManage && (await expensePurchaseControls(actor,write))?.settleExisting !== false };
}

export async function requestExpenseAi(actor: WorkContext, id: string, input: Parameters<typeof queueExpenseAi>[4], key: string) {
  await getExpense(actor, id);
  const { write } = await holdingResources(actor.holdingId), env = await runtimeEnv();
  return queueExpenseAi(write, actor, env, id, input, key);
}
