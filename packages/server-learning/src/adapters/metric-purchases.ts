import "server-only";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {ApiError} from "@erpx/server-runtime/public/http";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import {hasModuleAccess} from "@erpx/catalogue/module-access";
import {assignmentReadSql} from "@erpx/server-identity_access/public/record-source-authority";
import {learningPackageEnforcementMode,readPurchasedAccesses} from "@erpx/server-commercial/public/purchase-command-boundary";
import {packageClockSql,purchasedCapabilityPredicate} from "@erpx/server-commercial/public/purchased-capability-sql";
import { scheduledFeaturePredicate } from "@erpx/server-pms/public/feature-availability";
const capabilities={'learning-completions':'learn.results','overdue-required-learning':'learn.assignments'} as const;
export async function learningMetricHistoryAccess(database:SqlDatabase,actor:WorkContext){
 const mode=await learningPackageEnforcementMode(),access=await readPurchasedAccesses(database,mode,actor.companyId,['learn.assignments','learn.results']);if(mode!=='enforced')return {'learning-completions':true,'overdue-required-learning':true};
 const current=hasModuleAccess(actor.modules,'learning')&&hasPermission(actor,'records.read')&&!!await database.prepare(`SELECT ${assignmentReadSql('learning_metric_scope','records.read')} allowed FROM (SELECT ?::text company_id,?::text branch_id,?::text department_id) learning_metric_scope`).bind(actor.assignmentId,actor.userId,actor.role,actor.companyId,actor.branchId,actor.departmentId).first('allowed');
 return {'learning-completions':current&&access['learn.assignments'].history&&access['learn.results'].history,'overdue-required-learning':current&&access['learn.assignments'].history};
}
export async function learningMetricSource(database:SqlDatabase,actor:WorkContext,id:string){
 const access=await learningMetricHistoryAccess(database,actor);if(!access[id as keyof typeof capabilities])throw new ApiError(403,'METRIC_SOURCE_UNAVAILABLE','This Learning metric requires its purchased history and current source authority.');if(await learningPackageEnforcementMode()!=='enforced')return {sql:'SELECT * FROM learning_assignments',args:[] as unknown[]};
 const clock='learning_metric_clock.now_ms',capability=capabilities[id as keyof typeof capabilities];return {sql:`WITH learning_metric_clock AS MATERIALIZED (SELECT ${packageClockSql} now_ms) SELECT learning_metric_owned.* FROM learning_assignments learning_metric_owned CROSS JOIN learning_metric_clock WHERE ${assignmentReadSql('learning_metric_owned','records.read')} AND ${purchasedCapabilityPredicate('learning_metric_owned.company_id','learn.assignments','history',clock)} AND ${purchasedCapabilityPredicate('learning_metric_owned.company_id',capability,'history',clock)} AND ${scheduledFeaturePredicate('learning_metric_owned.company_id','learning')}`,args:[actor.assignmentId,actor.userId,actor.role]};
}
