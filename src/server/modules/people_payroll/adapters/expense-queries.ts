import { runtimeAppSources } from "../../runtime/public/app-native-sources";
import "server-only";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import { holdingResources, runtimeEnv } from "@/server/modules/runtime/public/runtime";
import { expenseDraftSchema } from "@/server/expense-drafts";
import { expenseOptions, getExpense } from "@/server/expenses";
import { expenseRoute } from "@/server/expense-routing";
import { activeStaff } from "@/server/modules/operations/public/staff-policy";
import { staffSettings } from "@/server/modules/operations/public/staff-settings";
import { namedRoute } from "@/server/modules/records/public/request-preview";
import { expenseAiStatus, queueExpenseAi } from "@/server/expense-ai";
import { ApiError } from "@/server/modules/runtime/public/http";
import { createExpensePreview } from "../application/expense-preview";
import { createExpensePostingPreview } from "../application/expense-posting-preview";
import { expenseJournalProjection, expensePostingAccounts } from "@/server/modules/finance/public/expense-posting-projection";
import { expensePostingConfiguration } from "@/server/modules/commerce/public/expense-posting-configuration";
import type { ExpenseRow } from "@/server/expenses";

export async function expensePostingPreview(actor: WorkContext, expense: ExpenseRow) {
  return createExpensePostingPreview({ repository: async () => {
    const { write } = await holdingResources(actor.holdingId);
    return { posted: journalId => expenseJournalProjection(write, actor.companyId, journalId), configuration: () => expensePostingConfiguration(write, actor.companyId), accounts: () => expensePostingAccounts(write, actor.companyId) };
  } })(expense);
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
  return preview(actor, input, id);
}

export async function readExpenseAiStatus(actor: WorkContext, id: string, payment: string | null) {
  await getExpense(actor, id);
  const { write } = await holdingResources(actor.holdingId), env = await runtimeEnv();
  const health = await runtimeAppSources(env.CONTROL_DB).readyHeartbeat();
  return { ...await expenseAiStatus(write, actor, env, id, payment), workerReady: !!health && Date.now() - Number(health.heartbeat_at) < 45000 };
}

export async function expensePageCapabilities(actor: WorkContext) {
  const { write } = await holdingResources(actor.holdingId);
  return { canRepair: (await staffSettings(write, actor)).canManage };
}

export async function requestExpenseAi(actor: WorkContext, id: string, input: Parameters<typeof queueExpenseAi>[4], key: string) {
  await getExpense(actor, id);
  const { write } = await holdingResources(actor.holdingId), env = await runtimeEnv();
  return queueExpenseAi(write, actor, env, id, input, key);
}
