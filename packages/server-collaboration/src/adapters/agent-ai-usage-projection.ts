import {socialAiMonthlyUsageCount,socialAiUsageProjection} from './read-tail-sql';
export const socialAiUsageSql=socialAiUsageProjection('company_id,month');
export const agentSocialMonthlyUsage=()=>socialAiUsageSql;
export const agentSocialMonthlyCount=()=>socialAiMonthlyUsageCount('count(*)');
