import {recordsFinanceUsageSql} from './records-ai-usage-projection';
export const agentFinanceDatedUsage=()=>"SELECT company_id,created_at FROM finance_ai_usage";
export const agentFinanceMonthlyUsage=()=>recordsFinanceUsageSql;
