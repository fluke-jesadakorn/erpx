import { recordsRequestUsageSql } from './ai-usage-projection';
export const agentRequestMonthlyCount=()=>"SELECT count(*) FROM request_ai_usage WHERE company_id=? AND month=?";
export const agentRequestMonthlyUsage=()=>recordsRequestUsageSql;
