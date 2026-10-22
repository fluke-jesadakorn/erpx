export const agentRequestMonthlyCount=()=>"SELECT count(*) FROM request_ai_usage WHERE company_id=? AND month=?";
export const agentRequestMonthlyUsage=()=>"SELECT company_id,month FROM request_ai_usage";
