export const agentSocialMonthlyUsage=()=>"SELECT company_id,month FROM social_ai_usage";
export const agentSocialMonthlyCount=()=>"SELECT count(*) FROM social_ai_usage WHERE company_id=? AND month=?";
