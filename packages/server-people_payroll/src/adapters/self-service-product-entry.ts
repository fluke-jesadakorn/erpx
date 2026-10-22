import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
import {loadIamAuthority} from '@erpx/server-identity_access/public/iam-store';
import {currentAssignmentReadScope} from '@erpx/server-identity_access/public/record-source-authority';
import {activeStaff,type StaffActor} from '@erpx/server-operations/public/staff-policy';
import {purchasedCapability} from '@erpx/server-commercial/public/rules/packages';
import {readCompanyPurchaseFacts} from '@erpx/server-commercial/public/package-grants';
import {hasWorkforceProductRecordHistory} from '@erpx/server-people_payroll/adapters/workforce-record-purchases';
import type {WorkforceSelfServiceEntry,WorkforceSelfServiceEntryScope} from '@erpx/contracts/workforce-self-service-entry';
/** Read-only role candidate discovery, never an acting WorkContext or role
 * selection. Original active employment and exact record privacy still apply. */
export async function workforceProductSelfServiceEntry(database:SqlDatabase,scope:WorkforceSelfServiceEntryScope,target:WorkforceSelfServiceEntry){
 if(!Number.isSafeInteger(scope.assignmentVersion)||scope.assignmentVersion<1)return null;
 try{
  const iam=await loadIamAuthority(database,scope.companyId,scope.userId,scope.role,scope.sessionId),actor:StaffActor={userId:scope.userId,assignmentId:scope.assignmentId,role:scope.role,companyId:scope.companyId,branchId:scope.branchId,departmentId:scope.departmentId,iam};
  const current=currentAssignmentReadScope(actor,'workforce_actor_scope');
  if(!await database.prepare(`SELECT 1 FROM(SELECT ?::text company_id,?::text branch_id,?::text department_id)workforce_actor_scope WHERE (${current.where}) AND EXISTS(SELECT 1 FROM assignments WHERE id=? AND version=?) AND NOT EXISTS(SELECT 1 FROM iam_revoked_sessions WHERE session_id=? AND company_id=? AND user_id=?)`).bind(scope.companyId,scope.branchId,scope.departmentId,...current.args,scope.assignmentId,scope.assignmentVersion,scope.sessionId??null,scope.companyId,scope.userId).first())return null;
  if(target.kind==='leave-record'||target.kind==='overtime-record'){
   if(!await hasWorkforceProductRecordHistory(database,actor,target.kind==='leave-record'?'leave':'overtime',target.id))return null;
  }else{
   // This is the same activeStaff owner used by the original module-free
   // Leave summary/create and Overtime self-service, without a directory grant.
   await activeStaff(database,scope.companyId,scope.userId);
   const facts=await readCompanyPurchaseFacts(database,scope.companyId);
   if(!purchasedCapability(facts.grants,scope.companyId,'people.workforce','history',Date.now(),facts.history).allowed)return null;
  }
  return {capability:'people.workforce',purchasePurpose:'history' as const};
 }catch(error){if(error instanceof ApiError&&([403,404].includes(error.status)||(error.status===409&&error.code==='STAFF_LINK_REQUIRED')))return null;throw error;}
}
