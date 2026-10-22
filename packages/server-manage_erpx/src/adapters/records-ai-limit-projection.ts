export const recordsAiMonthlyLimitSql = 'COALESCE((SELECT ai_monthly_limit FROM beta_policy WHERE company_id=?),100)';
