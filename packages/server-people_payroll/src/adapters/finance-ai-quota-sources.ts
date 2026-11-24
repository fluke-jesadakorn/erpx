import { PP_RD23_EXPENSE_AI_USAGE_MONTHS_SQL } from './pp-read-tail-r23-sources';
export const financeAiExpenseUsageSql=PP_RD23_EXPENSE_AI_USAGE_MONTHS_SQL;
export const financeAiExpenseMonthlyReservationsSql="SELECT company_id,to_char(to_timestamp(created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM') FROM expense_ai_proposals WHERE finance_policy_version IS NOT NULL";
export function financeAiExpenseDatedReservationsSql(timezoneSql:string){return `SELECT company_id,to_char(to_timestamp(created_at/1000.0) AT TIME ZONE ${timezoneSql},'YYYY-MM-DD'),to_char(to_timestamp(created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM') FROM expense_ai_proposals WHERE finance_policy_version IS NOT NULL`;}
export function financeAiExpenseDailyReservationsSql(timezoneSql:string){return `SELECT company_id,to_char(to_timestamp(created_at/1000.0) AT TIME ZONE ${timezoneSql},'YYYY-MM-DD') FROM expense_ai_proposals WHERE finance_policy_version IS NOT NULL`;}
