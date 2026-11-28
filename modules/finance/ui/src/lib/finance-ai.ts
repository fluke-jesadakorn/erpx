import { financeAiDateAt } from "@erpx/finance-server/public/rules/ai-contracts";
export { financeAiKinds, financeAiLabels, financeAiPolicySchema, financeAiPrepareSchema, financeAiResultSchema, financeAiDecisionSchema, financeAiApplySchema } from "@erpx/finance-server/public/rules/ai-contracts";
export type { FinanceAiKind, FinanceAiResult } from "@erpx/finance-server/public/rules/ai-contracts";
/** UI/native compatibility: pure services receive dates from their injected clock. */
export function financeAiDate(timezone: string, now = new Date()) { return financeAiDateAt(timezone, now); }
