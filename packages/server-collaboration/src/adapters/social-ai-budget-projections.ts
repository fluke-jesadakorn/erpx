import {socialAiMonthlyUsageCount} from './read-tail-sql';
export const socialMonthlyUsageCountSql='('+socialAiMonthlyUsageCount+')';
export const socialRequestRateBudgetSql="(SELECT count(*) FROM social_ai_usage WHERE company_id=? AND state='reserved' AND lease_until>?)<? AND (SELECT count(*) FROM social_ai_usage WHERE actor_id=? AND created_at>?)<?";
