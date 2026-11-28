export {financeAiSocialUsageSql} from '@erpx/collaboration-server/public/finance-ai-quota-sources';
/** Owner-local Company monthly AI allowance. Records and Finance share the
 * one admitted projection held by the records adapter; callers keep their
 * export names and bind the Company id. */
export {recordsAiMonthlyLimitSql as financeAiCompanyAllowanceSql} from './records-ai-limit-projection';
