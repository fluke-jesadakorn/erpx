import 'server-only';
import type{SqlDatabase}from '@erpx/server-runtime/public/runtime/types';
import{peoplePackageEnforcementMode,readPurchasedAccess}from '@erpx/server-commercial/public/purchase-command-boundary';
import{packageClockSql,purchasedCapabilityPredicate}from '@erpx/server-commercial/public/purchased-capability-sql';
/** Code-owned Company input, clock alias and capability; no SQL identifiers from callers. */
export async function expenseCompanyHistoryScope(database:SqlDatabase,companyId:string,alias:"finance_expenses"|"e"="finance_expenses"){
 const mode=await peoplePackageEnforcementMode();if(mode!=="enforced"){if(mode==="shadow")await readPurchasedAccess(database,mode,{companyId,capability:"people.expenses"});return "TRUE";}
 return `EXISTS(WITH expense_company_clock AS MATERIALIZED(SELECT ${packageClockSql} now_ms) SELECT 1 FROM expense_company_clock WHERE ${purchasedCapabilityPredicate(`${alias}.company_id`,"people.expenses","history","expense_company_clock.now_ms")})`;
}
