import type { SqlDatabase } from '../../runtime/public/runtime/types';
import { assertion } from '../../workflow/public/commands';
import { recordsSocialUsageCountSql } from '../../collaboration/public/records-ai-usage-projection';
import { recordsExpenseUsageCountSql } from '../../people_payroll/public/records-ai-usage-projection';
import { recordsFinanceUsageSql } from '../../finance/public/records-ai-usage-projection';
import { recordsRequestUsageSql } from '../../records/public/ai-usage-projection';
import { recordsExecutiveUsageSql } from '../../platform_administration/public/records-ai-usage-projection';
import { recordsAiMonthlyLimitSql } from '../../manage_erpx/public/records-ai-limit-projection';
const workspaceUsageSql = "SELECT company_id,to_char(to_timestamp(created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM') AS month FROM ai_workspace_turns";
const quotaSql = `${recordsSocialUsageCountSql}+${recordsExpenseUsageCountSql}+(SELECT COUNT(*) FROM (${recordsRequestUsageSql} UNION ALL ${recordsFinanceUsageSql} UNION ALL ${workspaceUsageSql} UNION ALL ${recordsExecutiveUsageSql}) WHERE company_id=? AND month=?)`;
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
