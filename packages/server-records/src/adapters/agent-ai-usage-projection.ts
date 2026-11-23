import { recordsRequestUsageSql, requestAiUsageMonthlyCountSql } from './ai-usage-projection';
export const agentRequestMonthlyCount=()=>requestAiUsageMonthlyCountSql;
export const agentRequestMonthlyUsage=()=>recordsRequestUsageSql;
