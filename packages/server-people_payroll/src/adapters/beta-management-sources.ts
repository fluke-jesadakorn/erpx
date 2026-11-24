import {ppRd42FourColOwnerProjection} from './pp-read-tail-r42-sources';
import {ppRd46OwnerFileProbe} from './pp-read-tail-r46-sources';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
/** Fixed native source clauses preserve the caller's single aggregate/fence statement. */
export function betaManagementPeopleSources(db:SqlDatabase){return {
 memberCompanySource:'JOIN hr_people p ON p.user_id=m.user_id WHERE p.company_id=?',
 memberCompanyPoolSource:'JOIN hr_people p ON p.user_id=m.user_id WHERE p.company_id=$1',
 aiUsageSource:ppRd42FourColOwnerProjection("company_id,month,input_tokens,output_tokens","expense_ai_usage"),
 expenseFileOwner:ppRd46OwnerFileProbe('finance_expenses','e'),
 expensePaymentFileOwner:ppRd46OwnerFileProbe('expense_payment_drafts','e'),
 leaveFileOwner:ppRd46OwnerFileProbe('leave_requests','l'),
 async linkedPerson(companyId:string,email:string){return !!await db.prepare('SELECT 1 FROM hr_people WHERE company_id=? AND lower(email)=? AND user_id IS NOT NULL').bind(companyId,email).first();},
};}
