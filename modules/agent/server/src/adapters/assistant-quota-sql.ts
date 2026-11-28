import 'server-only';
import {agentFinanceDatedUsage} from '@erpx/finance-server/public/agent-ai-usage-projection';
import {agentSocialMonthlyCount} from '@erpx/collaboration-server/public/agent-ai-usage-projection';
import {agentExpenseMonthlyCount} from '@erpx/people-payroll-server/public/agent-ai-usage-projection';
import {agentRequestMonthlyCount} from '@erpx/records-server/public/agent-ai-usage-projection';
import {agentExecutiveDatedUsage} from '@erpx/platform-runtime-server/public/agent-ai-usage-projection';
import {financeAiCompanyAllowanceSql} from '@erpx/manage-erpx-server/public/finance-ai-quota-sources';

/** Owner-local fixed assistant quota expression. Guided-intake, workspace
 * assistant and translation guards share one byte-identical text and keep their
 * own binds; the native quota owner remains the authority. The alias is the
 * only textual difference between callers. */
export const assistantQuotaSql = (alias: 'ai_calls' | 'calls'): string =>
  `(SELECT count(*) FROM (SELECT company_id,created_at FROM ai_workspace_turns UNION ALL ${agentExecutiveDatedUsage()} UNION ALL ${agentFinanceDatedUsage()}) ${alias} WHERE company_id=? AND created_at>=?)+(${agentSocialMonthlyCount()})+(${agentExpenseMonthlyCount()})+(${agentRequestMonthlyCount()})<${financeAiCompanyAllowanceSql}`;
