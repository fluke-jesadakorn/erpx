export const expenseFinancialLanding='finance/documents/expenses';
export const payrollFinancialLanding='people-organization/people/payroll/dashboard';
export type PeopleFinancialEntry={kind:'expense'|'payroll';id?:string};
/** Only the original daily collection and native record overview. Preparation,
 * Directory, general Finance and arbitrary record subpages remain separate. */
export function peopleFinancialEntry(destination:string):PeopleFinancialEntry|null{
 if(destination===expenseFinancialLanding)return {kind:'expense'};
 if(destination===payrollFinancialLanding)return {kind:'payroll'};
 const expense=/^finance\/documents\/expenses\/([^/]+)(?:\/overview)?$/.exec(destination);
 const payroll=/^people-organization\/people\/payroll\/runs\/([^/]+)(?:\/overview)?$/.exec(destination);
 const id=expense?.[1]??payroll?.[1];
 if(!id||/[\\/%?#\u0000-\u001f\u007f]/.test(id)||[".","..",'new','settings'].includes(id))return null;
 return {kind:expense?'expense':'payroll',id};
}
export function peopleFinancialPresentationAllowed(context:{scopeKind:string;beta:boolean;modules:readonly string[];permissions?:readonly string[]},kind:PeopleFinancialEntry['kind']){
 return !context.beta&&context.scopeKind==='holding'&&context.modules.includes('finance')&&
  (kind==='expense'?(context.permissions?.includes('expense.pay')===true||context.permissions?.includes('expense.read')===true&&['expense.prepare','expense.approve'].some(permission=>context.permissions?.includes(permission))):
   context.permissions?.includes('finance.pay')===true&&context.permissions.includes('payroll.payment.read'));
}
