import {socialAiMonthlyUsageCount} from './read-tail-sql';
export const recordsSocialUsageCountSql = '('+socialAiMonthlyUsageCount('COUNT(*)')+')';
