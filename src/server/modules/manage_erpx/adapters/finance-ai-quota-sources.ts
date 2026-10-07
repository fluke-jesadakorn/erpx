export {financeAiSocialUsageSql} from '../../collaboration/public/finance-ai-quota-sources';
export const financeAiCompanyAllowanceSql="COALESCE((SELECT ai_monthly_limit FROM beta_policy WHERE company_id=?),100)";
