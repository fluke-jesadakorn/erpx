import { socialMonthlyUsageCountSql, socialRequestRateBudgetSql } from '@erpx/server-collaboration/public/social-ai-budget-projections';
import { socialExpenseUsageSql } from '@erpx/server-people_payroll/public/collaboration-ai-usage';
import { recordsRequestUsageSql } from '@erpx/server-records/public/ai-usage-projection';
import { recordsFinanceUsageSql } from '@erpx/server-finance/public/records-ai-usage-projection';
import { recordsExecutiveUsageSql } from '@erpx/server-platform_administration/public/records-ai-usage-projection';
import { recordsAiMonthlyLimitSql } from '@erpx/server-manage_erpx/public/records-ai-limit-projection';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import {assertion} from "@erpx/server-workflow/public/command-assertions";
export function collaborationAiSources(db: SqlDatabase) {
    return { guardQuota(id: string, value: "quota", companyId: string, month: string, companyId2: string, month2: string, companyId3: string, companyId4: string, now: number, userId: string, value2: number, maxConcurrent: number, maxPerMinute: number) {
            return assertion(db, id, value, `${socialMonthlyUsageCountSql} + (SELECT count(*) FROM (${socialExpenseUsageSql} UNION ALL ${recordsRequestUsageSql} UNION ALL ${recordsFinanceUsageSql} UNION ALL SELECT company_id,to_char(to_timestamp(created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM') AS month FROM ai_workspace_turns UNION ALL ${recordsExecutiveUsageSql}) WHERE company_id=? AND month=?) < ${recordsAiMonthlyLimitSql} AND ${socialRequestRateBudgetSql}`, [
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
