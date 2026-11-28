import "server-only";
import type {WorkContext} from "@erpx/identity-access-server/public/authorization";
import type {SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
import {actorScopeExistsProbe,clockedSourceCte,existsRowProbe,moduleGrantAssignmentProbe} from "./learning-read-sql";

import {hasModuleAccess} from "@erpx/platform-catalogue/module-access";
import {hasPermission} from "@erpx/identity-access-ui/lib/permissions";

import {learningPackageEnforcementMode,readPurchasedAccess} from "@erpx/commercial-server/public/purchase-command-boundary";
import {packageClockSql,purchasedCapabilityPredicate} from "@erpx/commercial-server/public/purchased-capability-sql";
import { scheduledFeaturePredicate } from "@erpx/pms-server/public/feature-availability";
export async function evaluationLearningReadScope(database:SqlDatabase,actor:WorkContext){
 const mode=await learningPackageEnforcementMode();if(mode!=='enforced'){if(mode==='shadow')await readPurchasedAccess(database,mode,{companyId:actor.companyId,capability:'learn.results'});return {where:'TRUE',args:[] as unknown[]};}
 const permitted=hasModuleAccess(actor.modules,'learning')&&hasPermission(actor,'records.read');
 return {where:`${permitted?'TRUE':'FALSE'} AND (?::text IS NULL OR a.branch_id=?) AND (?::text IS NULL OR a.department_id=?) AND ${actorScopeExistsProbe('a.company_id,a.branch_id,a.department_id')} AND EXISTS(${clockedSourceCte('evaluation_learning_clock','')} ${existsRowProbe('FROM evaluation_learning_clock', purchasedCapabilityPredicate('a.company_id','learn.results','history','evaluation_learning_clock.now_ms'))}) AND ${scheduledFeaturePredicate('a.company_id','learning')} AND ${moduleGrantAssignmentProbe('learning_module_grant','learning_module_grant.id=? AND learning_module_grant.user_id=? AND learning_module_grant.role=? AND learning_module_grant.company_id=a.company_id')}`,args:[actor.branchId,actor.branchId,actor.departmentId,actor.departmentId,actor.assignmentId,actor.userId,actor.role,actor.assignmentId,actor.userId,actor.role]};
}
