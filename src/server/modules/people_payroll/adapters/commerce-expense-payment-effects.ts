import "server-only";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
import {assertion} from "../../workflow/public/commands";
import type {Row} from "../../commerce/public/commerce-contracts";
/** Expense reimbursement state belongs to People and is staged in the caller's payment transaction. */
export function commerceExpensePaymentEffects(db:SqlDatabase){return {
 read:(expenseId:string|number|null,invoiceId:string,companyId:string)=>db.prepare("SELECT x.*,p.user_id claimant_user_id FROM finance_expenses x JOIN hr_people p ON p.id=x.employee_id WHERE x.id=? AND x.invoice_id=? AND x.company_id=?").bind(expenseId,invoiceId,companyId).first<Row>(),
 stage:(facts:{remainingMinor:number;stageRemainingMinor:number;reimbursedRemainingMinor:number;reimbursedAt:number;updatedAt:number;id:string|number|null})=>db.prepare("UPDATE finance_expenses SET status=CASE WHEN ?=0 THEN 'reimbursed' ELSE 'posted' END,workflow_stage=CASE WHEN ?=0 THEN 'completed' ELSE 'payment' END,reimbursed_at=CASE WHEN ?=0 THEN ?::bigint ELSE NULL END,version=version+1,updated_at=? WHERE id=?").bind(facts.remainingMinor,facts.stageRemainingMinor,facts.reimbursedRemainingMinor,facts.reimbursedAt,facts.updatedAt,facts.id),
 independentClaimant:(key:string,label:string,values:string[])=>assertion(db,key,label,"EXISTS(SELECT 1 FROM hr_people WHERE id=? AND user_id IS DISTINCT FROM ?)",values),
 version:(key:string,label:string,values:(string|number)[],states:string[])=>assertion(db,key,label,`EXISTS(SELECT 1 FROM finance_expenses WHERE id=? AND version=? AND status IN (${states.map(()=>"?").join(",")}))`,values),
 unclaimed:(key:string,label:string,values:(string|number|null)[])=>assertion(db,key,label,"NOT EXISTS(SELECT 1 FROM hr_payment_batches WHERE source_type='expense' AND source_id=? AND status!='cancelled')",values),
};}
