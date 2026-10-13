import { financeAiDateAt } from "@/server/modules/finance/domain/ai-contracts";
export { financeAiKinds, financeAiLabels, financeAiPolicySchema, financeAiPrepareSchema, financeAiResultSchema, financeAiDecisionSchema, financeAiApplySchema } from "@/server/modules/finance/domain/ai-contracts";
export type { FinanceAiKind, FinanceAiResult } from "@/server/modules/finance/domain/ai-contracts";
/** UI/native compatibility: pure services receive dates from their injected clock. */
export function financeAiDate(timezone: string, now = new Date()) { return financeAiDateAt(timezone, now); }
