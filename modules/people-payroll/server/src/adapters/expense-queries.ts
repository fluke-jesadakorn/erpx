import {peoplePackageEnforcementMode} from "@erpx/commercial-server/public/purchase-command-boundary";
import {withExpensePurchasedRead,expensePurchaseControls} from "@erpx/people-payroll-server/adapters/expense-purchases";
import { runtimeAppSources } from "@erpx/platform-runtime-server/public/app-native-sources";
import "server-only";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import { holdingResources, runtimeEnv } from "../../../../../platform/runtime/server/src/public/runtime";
import {expenseDraftSchema} from '@erpx/people-payroll-server/adapters/expense-drafts';
import {expenseFromDatabase,getExpense} from '@erpx/people-payroll-server/adapters/expense-read';
import {expenseOptions} from '@erpx/people-payroll-server/adapters/expenses';
import { expenseRoute } from "@erpx/people-payroll-server/adapters/expense-routing";
import { activeStaff } from "@erpx/operations-server/public/staff-policy";
import { staffSettings } from "@erpx/operations-server/public/staff-settings";
import { namedRoute } from "@erpx/records-server/public/request-preview";
import { expenseAiStatus, queueExpenseAi } from "@erpx/people-payroll-server/adapters/expense-ai";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import { createExpensePreview } from "@erpx/people-payroll-server/application/expense-preview";
import { createExpensePostingPreview } from "@erpx/people-payroll-server/application/expense-posting-preview";
import { expenseJournalProjection, expensePostingAccounts } from "@erpx/finance-server/public/expense-posting-projection";
import { expensePostingConfiguration } from "@erpx/commerce-server/public/expense-posting-configuration";
import {nativeExpenseOptions,type ExpenseRow} from '@erpx/people-payroll-server/adapters/expenses';

export async function expensePostingPreview(actor: WorkContext, expense: ExpenseRow) {
  const {write}=await holdingResources(actor.holdingId);
  return withExpensePurchasedRead(write,actor,async database=>{
    const current=await peoplePackageEnforcementMode()==='disabled'?expense:await expenseFromDatabase(database,actor,String(expense.id));
    return createExpensePostingPreview({repository:async()=>({posted:journalId=>expenseJournalProjection(database,actor,journalId),configuration:()=>expensePostingConfiguration(database,actor.companyId),accounts:()=>expensePostingAccounts(database,actor)})})(current);
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
