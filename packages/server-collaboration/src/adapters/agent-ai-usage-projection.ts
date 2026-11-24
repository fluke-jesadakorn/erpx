export const socialAiUsageSql="SELECT company_id,month FROM social_ai_usage";
export const agentSocialMonthlyUsage=()=>socialAiUsageSql;
export const agentSocialMonthlyCount=()=>"SELECT count(*) FROM social_ai_usage WHERE company_id=? AND month=?";
