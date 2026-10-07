import type {SqlDatabase} from '../../runtime/public/runtime/types';
/** Fixed native source clauses preserve the caller's single aggregate/fence statement. */
export function betaManagementPeopleSources(db:SqlDatabase){return {
 memberCompanySource:'JOIN hr_people p ON p.user_id=m.user_id WHERE p.company_id=?',
 memberCompanyPoolSource:'JOIN hr_people p ON p.user_id=m.user_id WHERE p.company_id=$1',
 aiUsageSource:'SELECT company_id,month,input_tokens,output_tokens FROM expense_ai_usage',
 expenseFileOwner:'EXISTS(SELECT 1 FROM finance_expenses e WHERE e.id=a.owner_id AND e.company_id=?)',
 expensePaymentFileOwner:'EXISTS(SELECT 1 FROM expense_payment_drafts e WHERE e.id=a.owner_id AND e.company_id=?)',
 leaveFileOwner:'EXISTS(SELECT 1 FROM leave_requests l WHERE l.id=a.owner_id AND l.company_id=?)',
 async linkedPerson(companyId:string,email:string){return !!await db.prepare('SELECT 1 FROM hr_people WHERE company_id=? AND lower(email)=? AND user_id IS NOT NULL').bind(companyId,email).first();},
};}
