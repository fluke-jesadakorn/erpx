import 'server-only';
import type {WorkContext} from '@erpx/identity-access-server/public/authorization';
import {currentAssignmentReadScope} from '@erpx/identity-access-server/public/record-source-authority';
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';

import { appsPackageEnforcementMode, readPurchasedAccess } from '@erpx/commercial-server/public/purchase-command-boundary';
import {purchasedCapabilityPredicate} from '@erpx/commercial-server/public/purchased-capability-sql';
import {OPERATIONS_TASK_SCOPE_SQL,operationsViewerClockHead} from './workforce-actor-scope-source';
import {workTaskSocialPurchasePredicate} from '@erpx/collaboration-server/public/work-task-purchase-source';
import {projectTaskRecordPolicy} from '@erpx/operations-server/adapters/project-task-record-policy';

import { scheduledFeaturePredicate } from "@erpx/pms-server/public/feature-availability";
import {workTaskSocialReadScope} from '@erpx/collaboration-server/public/work-task-read-policy';

export function authority(actor:WorkContext){return currentAssignmentReadScope(actor,'workforce_actor_scope');}
export async function visibleTask(database:SqlDatabase,actor:WorkContext,id:string,company:string){
 const social=await workTaskSocialReadScope(database,actor),current=authority(actor),viewer='work_record_viewer';
 return {where:`${operationsViewerClockHead(viewer)}staff_tasks t CROSS JOIN ${viewer} CROSS JOIN LATERAL${OPERATIONS_TASK_SCOPE_SQL}workforce_actor_scope
 WHERE t.id=${id} AND t.company_id=${company} AND t.project_id IS NULL AND ${current.where} AND ${scheduledFeaturePredicate('t.company_id','operations')}
 AND ${projectTaskRecordPolicy('t.id','t.company_id',viewer)}
 AND ${purchasedCapabilityPredicate('t.company_id','work.actions','history',`${viewer}.now_ms`)}
 AND (t.template_id IS NULL OR ${purchasedCapabilityPredicate('t.company_id','work.advanced-routines','history',`${viewer}.now_ms`)})
 AND (NOT ${workTaskSocialPurchasePredicate()} OR ${purchasedCapabilityPredicate('t.company_id','work.social','history',`${viewer}.now_ms`)}) AND (${social.where}))`,args:[actor.userId,actor.assignmentId,...current.args,...social.args]};
}
export async function evaluationOrdinaryTaskReadScope(database:SqlDatabase,actor:WorkContext){
 const mode=await appsPackageEnforcementMode();if(mode!=='enforced'){if(mode==='shadow')await readPurchasedAccess(database,mode,{companyId:actor.companyId,capability:'work.actions'});return {where:'TRUE',args:[] as unknown[]};}
 return visibleTask(database,actor,'evaluation_task.id','evaluation_task.company_id');
}
