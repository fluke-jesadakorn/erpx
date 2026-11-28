import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';
import {assertion} from "@erpx/workflow-server/public/command-assertions";
import { recordsSocialUsageCountSql } from '@erpx/collaboration-server/public/records-ai-usage-projection';
import { recordsExpenseUsageCountSql } from '@erpx/people-payroll-server/public/records-ai-usage-projection';
import { recordsFinanceUsageSql } from '@erpx/finance-server/public/records-ai-usage-projection';
import { recordsRequestUsageSql } from '@erpx/records-server/public/ai-usage-projection';
import { recordsExecutiveUsageSql } from '@erpx/platform-administration-server/public/records-ai-usage-projection';
import { recordsAiMonthlyLimitSql } from '@erpx/manage-erpx-server/public/records-ai-limit-projection';
import { agentMonthlyUsageColsSql } from './agent-monthly-usage-source';
/** The one owner-authored fixed SQL for the Agent workspace-turns monthly
 * usage projection. Every quota caller reuses the byte-identical shared core;
 * no adapter authors its own literal. */
export const agentWorkspaceMonthlyUsageSql = `${agentMonthlyUsageColsSql} AS month FROM ai_workspace_turns`;
/** One owner-authored count over the records quota union. The count casing and
 * the optional alias are the only differences between the quota read and the
 * collaboration guard; both call sites keep their own binds. */
export const recordsQuotaCountSql = (count: 'count' | 'COUNT', alias: 'calls' | '' = ''): string =>
  `(SELECT ${count}(*) FROM (${recordsRequestUsageSql} UNION ALL ${recordsFinanceUsageSql} UNION ALL ${agentWorkspaceMonthlyUsageSql} UNION ALL ${recordsExecutiveUsageSql})${alias ? ` ${alias}` : ''} WHERE company_id=? AND month=?)`;
const quotaSql = `${recordsSocialUsageCountSql}+${recordsExpenseUsageCountSql}+${recordsQuotaCountSql('COUNT')}`;
export function recordsAiQuota(database: SqlDatabase) {
    return {
        read(args: string[], companyId: string) {
            return database.prepare(`SELECT ${quotaSql} used,${recordsAiMonthlyLimitSql} allowance`).bind(...args, companyId).first<{
                used: number;
                allowance: number;
            }>();
        },
        guard(key: string, step: string, args: string[], companyId: string) { return assertion(database, key, step, `${quotaSql}<${recordsAiMonthlyLimitSql}`, [...args, companyId]); }
    };
}
