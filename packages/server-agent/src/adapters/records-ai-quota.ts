import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import { recordsSocialUsageCountSql } from '@erpx/server-collaboration/public/records-ai-usage-projection';
import { recordsExpenseUsageCountSql } from '@erpx/server-people_payroll/public/records-ai-usage-projection';
import { recordsFinanceUsageSql } from '@erpx/server-finance/public/records-ai-usage-projection';
import { recordsRequestUsageSql } from '@erpx/server-records/public/ai-usage-projection';
import { recordsExecutiveUsageSql } from '@erpx/server-platform_administration/public/records-ai-usage-projection';
import { recordsAiMonthlyLimitSql } from '@erpx/server-manage_erpx/public/records-ai-limit-projection';
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
