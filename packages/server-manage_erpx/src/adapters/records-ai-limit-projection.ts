/** Canonical owner-local projection for the Company monthly AI allowance.
 * Records and Finance quota sources share this one fixed admitted projection;
 * callers keep their own export names and bind the Company id. */
import {betaPolicyLimitSql} from './beta-policy-limit-sql';
export const recordsAiMonthlyLimitSql = `COALESCE((${betaPolicyLimitSql('ai_monthly_limit')}),100)`;
