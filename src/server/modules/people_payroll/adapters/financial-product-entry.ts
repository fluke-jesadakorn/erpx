import 'server-only';
import {createHash} from 'node:crypto';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
import {ApiError} from '../../runtime/public/http';
import {loadIamAuthority} from '../../identity_access/public/iam-store';
import {currentAssignmentReadScope} from '../../identity_access/public/record-source-authority';
import {effectiveWorkModules} from '../../identity_access/public/rules/work-modules';
import {scheduledFeaturePredicate} from '../../pms/public/hospitality-rollout';
import {packageClockSql,purchasedCapabilityPredicate} from '../../commercial/public/purchased-capability-sql';
import {readPurchasedAccess} from '../../commercial/public/purchase-command-boundary';
import type {StaffActor} from '../../operations/public/staff-policy';
import {expenseVisibleRecordIds} from './expenses';
import {peopleFinancialPresentationAllowed,type PeopleFinancialEntry} from '../domain/financial-product-entry';
export type PeopleFinancialProductScope={companyId:string;branchId:string|null;departmentId:string|null;userId:string;assignmentId:string;assignmentVersion:number;role:string;sessionId:string;beta?:boolean};
/** Read-only candidate proof before Work selection. The original IAM, actual
 * module ceiling and native record reader authorize only these owner pages;
 * this does not construct an acting WorkContext or grant general discovery. */
export async function peopleFinancialProductEntry(database:SqlDatabase,scope:PeopleFinancialProductScope,target:PeopleFinancialEntry){
 if(scope.beta||scope.branchId||scope.departmentId||!scope.sessionId||!Number.isSafeInteger(scope.assignmentVersion)||scope.assignmentVersion<1)return null;
 let iam:Awaited<ReturnType<typeof loadIamAuthority>>;
 try{iam=await loadIamAuthority(database,scope.companyId,scope.userId,scope.role,scope.sessionId);}
 catch(error){if(error instanceof ApiError&&[401,403].includes(error.status))return null;throw error;}
 const capability=target.kind==='expense'?'people.expenses':'people.payroll';
 const purchase=await readPurchasedAccess(database,'enforced',{companyId:scope.companyId,capability});
 if(!purchase.history)return null;
 const actor:StaffActor={userId:scope.userId,assignmentId:scope.assignmentId,role:scope.role,companyId:scope.companyId,branchId:null,departmentId:null,iam};
 const current=currentAssignmentReadScope(actor,'workforce_actor_scope');
 const query=`WITH people_financial_clock AS MATERIALIZED(SELECT ${packageClockSql} now_ms)
 SELECT a.modules_json FROM assignments a CROSS JOIN(SELECT ?::text company_id,NULL::text branch_id,NULL::text department_id)workforce_actor_scope CROSS JOIN people_financial_clock
 WHERE a.id=? AND a.user_id=? AND a.role=? AND a.company_id=workforce_actor_scope.company_id AND a.scope_kind='holding' AND a.version=?
 AND (${current.where})
 AND NOT EXISTS(SELECT 1 FROM iam_revoked_sessions revoked WHERE revoked.session_id=? AND revoked.company_id=workforce_actor_scope.company_id AND revoked.user_id=a.user_id)
 AND ${['people_payroll','finance'].map(feature=>scheduledFeaturePredicate('workforce_actor_scope.company_id',feature)).join(' AND ')}
 AND ${purchasedCapabilityPredicate('workforce_actor_scope.company_id',capability,'history','people_financial_clock.now_ms')}
 AND (?::text IS NULL OR ${target.kind==='payroll'?'EXISTS(SELECT 1 FROM hr_payroll_runs r WHERE r.id=? AND r.company_id=workforce_actor_scope.company_id)':'EXISTS(SELECT 1 FROM finance_expenses e WHERE e.id=? AND e.company_id=workforce_actor_scope.company_id)'})`;
 const bind=()=>database.prepare(query).bind(scope.companyId,scope.assignmentId,scope.userId,scope.role,scope.assignmentVersion,...current.args,scope.sessionId,target.id??null,target.id??null);
 const row=await bind().first<{modules_json:string|string[]}>();if(!row)return null;
 const assigned=typeof row.modules_json==='string'?JSON.parse(row.modules_json) as string[]:row.modules_json;
 const modules=effectiveWorkModules(scope.role,assigned,iam.permissions,false);
 if(!peopleFinancialPresentationAllowed({scopeKind:'holding',beta:false,modules,permissions:iam.permissions},target.kind))return null;
 // Reuse the original Expense visibility+second-stage reader, including
 // cancelled-parent native payment duties. Inspection performs no Flow writes.
 if(target.kind==='expense'&&target.id&&!(await expenseVisibleRecordIds(database,actor,[target.id],true)).has(target.id))return null;
 const final=await bind().first<{modules_json:string|string[]}>();
 if(!final||JSON.stringify(final.modules_json)!==JSON.stringify(row.modules_json))return null;
 return {revision:createHash('sha256').update(JSON.stringify([scope,target,iam.identityVersion,iam.roleRevision,iam.hrAppointmentVersion,modules,purchase.revision])).digest('hex'),purchasePurpose:'history' as const};
}
