import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
export const financeWorkspacePendingPeopleSql="(SELECT count(*) FROM hr_payment_batches b WHERE b.company_id=? AND b.status IN ('draft','approved','exported','partially_settled'))";
export const financeWorkspaceReadyExpenseAiSql="(SELECT count(*) FROM expense_ai_proposals WHERE company_id=? AND state='ready')";
export async function financeWorkspacePeoplePayments(write:SqlDatabase,companyId:string){return (await write.prepare(`SELECT b.id,b.source_type,b.source_id,b.currency,b.status,r.pay_date due_date,
          COALESCE(SUM(CASE WHEN i.status IN ('outstanding','failed') THEN i.amount_minor ELSE 0 END),0) amount_minor
          FROM hr_payment_batches b LEFT JOIN hr_payment_batch_items i ON i.batch_id=b.id
          LEFT JOIN hr_payroll_runs r ON r.id=b.source_id AND b.source_type='payroll' AND r.company_id=b.company_id
          WHERE b.company_id=? AND b.status IN ('draft','approved','exported','partially_settled')
          GROUP BY b.id,r.pay_date ORDER BY r.pay_date NULLS LAST,b.id`).bind(companyId).all<Record<string,string|number|null>>()).results;}
