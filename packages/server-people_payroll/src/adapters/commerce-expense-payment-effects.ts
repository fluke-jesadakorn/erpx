import { PP_RD24_UNCLAIMED_EXPENSE_GUARD } from './pp-read-tail-r24-sources';
import { nativePpStatement } from '@erpx/server-people_payroll/adapters/native-people-payroll-command';
import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import type {Row} from "@erpx/server-commerce/public/commerce-contracts";
/** Expense reimbursement state belongs to People and is staged in the caller's payment transaction. */
export function commerceExpensePaymentEffects(db:SqlDatabase){return {
 read:(expenseId:string|number|null,invoiceId:string,companyId:string)=>db.prepare("SELECT x.*,p.user_id claimant_user_id FROM finance_expenses x JOIN hr_people p ON p.id=x.employee_id WHERE x.id=? AND x.invoice_id=? AND x.company_id=?").bind(expenseId,invoiceId,companyId).first<Row>(),
 stage:(facts:{remainingMinor:number;stageRemainingMinor:number;reimbursedRemainingMinor:number;reimbursedAt:number;updatedAt:number;id:string|number|null})=>nativePpStatement('pp_commerce_expense_payment_effects_stmt_1').bind(facts.remainingMinor,facts.stageRemainingMinor,facts.reimbursedRemainingMinor,facts.reimbursedAt,facts.updatedAt,facts.id),
 independentClaimant:(key:string,label:string,values:string[])=>assertion(db,key,label,"EXISTS(SELECT 1 FROM hr_people WHERE id=? AND user_id IS DISTINCT FROM ?)",values),
 version:(key:string,label:string,values:(string|number)[],states:string[])=>assertion(db,key,label,`EXISTS(SELECT 1 FROM finance_expenses WHERE id=? AND version=? AND status IN (${states.map(()=>"?").join(",")}))`,values),
 unclaimed:(key:string,label:string,values:(string|number|null)[])=>assertion(db,key,label,PP_RD24_UNCLAIMED_EXPENSE_GUARD,values),
};}
