import { PP_RD23_EXPENSE_AI_USAGE_MONTHS_SQL } from './pp-read-tail-r23-sources';
import { ppRd43AiReservationProjection } from './pp-read-tail-r43-sources';
export const financeAiExpenseUsageSql=PP_RD23_EXPENSE_AI_USAGE_MONTHS_SQL;
export const financeAiExpenseMonthlyReservationsSql=ppRd43AiReservationProjection("company_id,to_char(to_timestamp(created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')");
export function financeAiExpenseDatedReservationsSql(timezoneSql:string){return ppRd43AiReservationProjection(`company_id,to_char(to_timestamp(created_at/1000.0) AT TIME ZONE ${timezoneSql},'YYYY-MM-DD'),to_char(to_timestamp(created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')`);}
export function financeAiExpenseDailyReservationsSql(timezoneSql:string){return ppRd43AiReservationProjection(`company_id,to_char(to_timestamp(created_at/1000.0) AT TIME ZONE ${timezoneSql},'YYYY-MM-DD')`);}
