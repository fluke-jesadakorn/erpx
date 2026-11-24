import { socialMonthlyUsageCountSql, socialRequestRateBudgetSql } from '@erpx/server-collaboration/public/social-ai-budget-projections';
import { recordsExpenseUsageCountSql } from '@erpx/server-people_payroll/public/records-ai-usage-projection';
import { recordsRequestUsageSql } from '@erpx/server-records/public/ai-usage-projection';
import { recordsFinanceUsageSql } from '@erpx/server-finance/public/records-ai-usage-projection';
import { recordsExecutiveUsageSql } from '@erpx/server-platform_administration/public/records-ai-usage-projection';
import { recordsAiMonthlyLimitSql } from '@erpx/server-manage_erpx/public/records-ai-limit-projection';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import { agentHomeAiNameSql } from './collaboration-home-sources';
import { agentWorkspaceMonthlyUsageSql } from './records-ai-quota';
/** Byte-identical Agent records quota count shared by the quotaAllowed read and the guardQuota assertion.
 * Each call site keeps its own binds; the native quota owner remains the authority. */
const agentRecordsQuotaCountSql = `(SELECT count(*) FROM (${recordsRequestUsageSql} UNION ALL ${recordsFinanceUsageSql} UNION ALL ${agentWorkspaceMonthlyUsageSql} UNION ALL ${recordsExecutiveUsageSql}) calls WHERE company_id=? AND month=?)`;
export function collaborationAgentSources(db: SqlDatabase) {
    return { async quotaAllowed(companyId: string, month: string, companyId2: string, month2: string, companyId3: string, month3: string, companyId4: string, companyId5: string, now: number, userId: string, value: number, maxConcurrent: number, maxPerMinute: number) {
            return db.prepare(`SELECT CASE WHEN
      ${socialMonthlyUsageCountSql} +
      ${recordsExpenseUsageCountSql} +
      ${agentRecordsQuotaCountSql} < ${recordsAiMonthlyLimitSql}
      AND ${socialRequestRateBudgetSql} THEN 1 ELSE 0 END AS allowed`)
                .bind(companyId, month, companyId2, month2, companyId3, month3, companyId4, companyId5, now, maxConcurrent, userId, value, maxPerMinute).first<{
                allowed: number;
            }>();
        }, guardQuota(usageId: string, value: "quota", companyId: string, value2: string, companyId2: string, value3: string, companyId3: string, value4: string, companyId4: string, companyId5: string, now: number, userId: string, value5: number, maxConcurrent: number, maxPerMinute: number) { return assertion(db, usageId, value, `${socialMonthlyUsageCountSql} + ${recordsExpenseUsageCountSql} + ${agentRecordsQuotaCountSql} < ${recordsAiMonthlyLimitSql} AND ${socialRequestRateBudgetSql}`, [companyId, value2, companyId2, value3, companyId3, value4, companyId4, companyId5, now, maxConcurrent, userId, value5, maxPerMinute]); }, async assistantName(companyId: string) { return db.prepare(agentHomeAiNameSql).bind(companyId).first<{
            home_ai_name: string;
        }>(); } };
}
