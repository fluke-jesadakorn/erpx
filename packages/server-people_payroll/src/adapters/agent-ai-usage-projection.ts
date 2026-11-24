import {ppRd32TwoColProjection} from './pp-read-tail-r32-sources';
export const agentExpenseMonthlyCount=()=>ppRd32TwoColProjection("count(*)","expense_ai_usage","company_id","month");
