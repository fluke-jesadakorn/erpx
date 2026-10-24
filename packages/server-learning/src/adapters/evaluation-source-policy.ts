import "server-only";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";

import {hasModuleAccess} from "@erpx/catalogue/module-access";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import {assignmentReadSql} from "@erpx/server-identity_access/public/record-source-authority";

import {learningPackageEnforcementMode,readPurchasedAccess} from "@erpx/server-commercial/public/purchase-command-boundary";
import {packageClockSql,purchasedCapabilityPredicate} from "@erpx/server-commercial/public/purchased-capability-sql";
import { scheduledFeaturePredicate } from "@erpx/server-pms/public/feature-availability";
export async function evaluationLearningReadScope(database:SqlDatabase,actor:WorkContext){
 const mode=await learningPackageEnforcementMode();if(mode!=='enforced'){if(mode==='shadow')await readPurchasedAccess(database,mode,{companyId:actor.companyId,capability:'learn.results'});return {where:'TRUE',args:[] as unknown[]};}
 const permitted=hasModuleAccess(actor.modules,'learning')&&hasPermission(actor,'records.read');
 return {where:`${permitted?'TRUE':'FALSE'} AND (?::text IS NULL OR a.branch_id=?) AND (?::text IS NULL OR a.department_id=?) AND EXISTS(SELECT 1 FROM(SELECT a.company_id,a.branch_id,a.department_id)learning_actor_scope WHERE ${assignmentReadSql('learning_actor_scope','records.read')}) AND EXISTS(WITH evaluation_learning_clock AS MATERIALIZED(SELECT ${packageClockSql} now_ms) SELECT 1 FROM evaluation_learning_clock WHERE ${purchasedCapabilityPredicate('a.company_id','learn.results','history','evaluation_learning_clock.now_ms')}) AND ${scheduledFeaturePredicate('a.company_id','learning')} AND EXISTS(SELECT 1 FROM assignments learning_module_grant WHERE learning_module_grant.id=? AND learning_module_grant.user_id=? AND learning_module_grant.role=? AND learning_module_grant.company_id=a.company_id AND EXISTS(SELECT 1 FROM erpx_json_each(learning_module_grant.modules_json) module WHERE module.value='learning'))`,args:[actor.branchId,actor.branchId,actor.departmentId,actor.departmentId,actor.assignmentId,actor.userId,actor.role,actor.assignmentId,actor.userId,actor.role]};
}
