import { socialMonthlyUsageCountSql, socialRequestRateBudgetSql } from '@erpx/server-collaboration/public/social-ai-budget-projections';
import { socialExpenseUsageSql } from '@erpx/server-people_payroll/public/collaboration-ai-usage';
import { recordsRequestUsageSql } from '@erpx/server-records/public/ai-usage-projection';
import { recordsFinanceUsageSql } from '@erpx/server-finance/public/records-ai-usage-projection';
import { recordsExecutiveUsageSql } from '@erpx/server-platform_administration/public/records-ai-usage-projection';
import { recordsAiMonthlyLimitSql } from '@erpx/server-manage_erpx/public/records-ai-limit-projection';
import { agentWorkspaceMonthlyUsageSql } from './records-ai-quota';
import { agentMonthlyUnionCountSql } from './read-tail-r25-source-sql';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import {assertion} from "@erpx/server-workflow/public/command-assertions";
/** Byte-identical five-branch records quota union count, now composed from the
 * one Agent monthly-union count head. Kept local so this adapter owns its binds. */
const collaborationQuotaUnionCountSql = agentMonthlyUnionCountSql(
    `${socialExpenseUsageSql} UNION ALL ${recordsRequestUsageSql} UNION ALL ${recordsFinanceUsageSql} UNION ALL ${agentWorkspaceMonthlyUsageSql} UNION ALL ${recordsExecutiveUsageSql}`,
);
export function collaborationAiSources(db: SqlDatabase) {
    return { guardQuota(id: string, value: "quota", companyId: string, month: string, companyId2: string, month2: string, companyId3: string, companyId4: string, now: number, userId: string, value2: number, maxConcurrent: number, maxPerMinute: number) {
            return assertion(db, id, value, `${socialMonthlyUsageCountSql} + ${collaborationQuotaUnionCountSql} < ${recordsAiMonthlyLimitSql} AND ${socialRequestRateBudgetSql}`, [
                companyId,
                month,
                companyId2,
                month2,
                companyId3,
                companyId4,
                now,
                maxConcurrent,
                userId,
                value2,
                maxPerMinute,
            ]);
        } };
}
