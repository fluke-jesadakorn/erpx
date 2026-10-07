import {agentFinanceDatedUsage} from '../../finance/public/agent-ai-usage-projection';
import {agentSocialMonthlyCount} from '../../collaboration/public/agent-ai-usage-projection';
import {agentExpenseMonthlyCount} from '../../people_payroll/public/agent-ai-usage-projection';
import {agentRequestMonthlyCount} from '../../records/public/agent-ai-usage-projection';
import {agentExecutiveDatedUsage} from '../../runtime/public/agent-ai-usage-projection';
import {financeAiCompanyAllowanceSql} from '../../manage_erpx/public/finance-ai-quota-sources';
import "server-only";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { assertion } from "@/server/modules/workflow/public/commands";
export function GuidedIntakeCollaborationFacts(database: SqlDatabase) { return { guard4(id: string, value: "assistant-quota", companyId: string, value2: number, companyId2: string, month: string, companyId3: string, month2: string, companyId4: string, month3: string, companyId5: string) { return assertion(database, id, value, `(SELECT count(*) FROM (SELECT company_id,created_at FROM ai_workspace_turns UNION ALL ${agentExecutiveDatedUsage()} UNION ALL ${agentFinanceDatedUsage()}) ai_calls WHERE company_id=? AND created_at>=?)+(${agentSocialMonthlyCount()})+(${agentExpenseMonthlyCount()})+(${agentRequestMonthlyCount()})<${financeAiCompanyAllowanceSql}`, [companyId, value2, companyId2, month, companyId3, month2, companyId4, month3, companyId5]); } }; }
