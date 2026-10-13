/** Compatibility entry; its legacy implementation is tracked in ownership.json. */
export { readFinanceAiPolicy, financeAiPolicyView, saveFinanceAiPolicy, financeAiQuotaSql, queueFinanceAi, runFinanceAi, drainFinanceAi, financeAiWorkspace, dismissFinanceAi, financeAiDraftApplication, applyFinanceAiDraft, scanFinanceAi } from "@/server/modules/finance/adapters/finance-ai";
export type { FinanceAiPolicy, FinanceAiProposal } from "@/server/modules/finance/adapters/finance-ai";
