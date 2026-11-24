/** Canonical owner-local projection for the Company monthly AI allowance.
 * Records and Finance quota sources share this one fixed admitted projection;
 * callers keep their own export names and bind the Company id. */
export const recordsAiMonthlyLimitSql = 'COALESCE((SELECT ai_monthly_limit FROM beta_policy WHERE company_id=?),100)';
