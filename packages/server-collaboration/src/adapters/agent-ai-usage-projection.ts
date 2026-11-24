export const socialAiUsageSql="SELECT company_id,month FROM social_ai_usage";
export const agentSocialMonthlyUsage=()=>socialAiUsageSql;
import {socialAiMonthlyUsageCount} from './read-tail-sql';
export const agentSocialMonthlyCount=()=>socialAiMonthlyUsageCount('count(*)');
