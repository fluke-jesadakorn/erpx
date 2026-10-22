import {agentFinanceDatedUsage} from '@erpx/server-finance/public/agent-ai-usage-projection';
import {agentSocialMonthlyCount} from '@erpx/server-collaboration/public/agent-ai-usage-projection';
import {agentExpenseMonthlyCount} from '@erpx/server-people_payroll/public/agent-ai-usage-projection';
import {agentRequestMonthlyCount} from '@erpx/server-records/public/agent-ai-usage-projection';
import {agentExecutiveDatedUsage} from '@erpx/server-runtime/public/agent-ai-usage-projection';
import {financeAiCompanyAllowanceSql} from '@erpx/server-manage_erpx/public/finance-ai-quota-sources';
import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
export function GuidedIntakeCollaborationFacts(database: SqlDatabase) { return { guard4(id: string, value: "assistant-quota", companyId: string, value2: number, companyId2: string, month: string, companyId3: string, month2: string, companyId4: string, month3: string, companyId5: string) { return assertion(database, id, value, `(SELECT count(*) FROM (SELECT company_id,created_at FROM ai_workspace_turns UNION ALL ${agentExecutiveDatedUsage()} UNION ALL ${agentFinanceDatedUsage()}) ai_calls WHERE company_id=? AND created_at>=?)+(${agentSocialMonthlyCount()})+(${agentExpenseMonthlyCount()})+(${agentRequestMonthlyCount()})<${financeAiCompanyAllowanceSql}`, [companyId, value2, companyId2, month, companyId3, month2, companyId4, month3, companyId5]); } }; }
