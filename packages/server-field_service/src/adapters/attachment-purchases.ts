import "server-only";
import {randomUUID} from "node:crypto";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {assignmentReadSql} from "@erpx/server-identity_access/public/record-source-authority";
import {ApiError} from "@erpx/server-runtime/public/http";
import type {AttachmentOperation,AttachmentPolicy} from "@erpx/server-files/public/attachments/types";
import {servicePackageEnforcementMode,checkPurchasedCapability,purchaseCommandBoundary,observePurchaseResults} from "@erpx/server-commercial/public/purchase-command-boundary";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {workContextGuards} from "@erpx/server-workflow/public/commands";
import {iamMutationGuards} from "@erpx/server-identity_access/public/iam-store";
import {financeDeferredEffectStatements} from "@erpx/server-finance/public/finance-transactions";
import {serviceOwnerSourceImage,serviceOwnerSourceQuery,serviceOwnerSourceReadLocks,requireServiceCurrentAuthority} from "@erpx/server-field_service/adapters/purchases";

export type ServiceAttachmentPurchase={companyId:string;capability:"service.visits";purpose:"settle-existing";actor:WorkContext;targetId:string;sourceImage:string;parts:boolean;terminal:boolean};
/** Existing registered visit target only. Caller labels never establish the
 * order/agreement/customer/asset/case relationship used by this owner. */
export async function serviceAttachmentPurchaseScope(database:SqlDatabase,actor:WorkContext,policy:Pick<AttachmentPolicy,"targetType"|"purposeKey">,targetId:string):Promise<ServiceAttachmentPurchase|null>{
 if(policy.targetType!=="field_service_visit"||await servicePackageEnforcementMode()==="disabled")return null;
 const source=await serviceOwnerSourceImage(database,actor,{kind:"order",id:targetId},true);
 if(!source)throw new ApiError(404,"FIELD_SERVICE_ORDER_NOT_FOUND","The original Service visit is unavailable in this workspace.");
 return {companyId:actor.companyId,capability:"service.visits",purpose:"settle-existing",actor,targetId,sourceImage:source.image,parts:source.asset,terminal:["billed","cancelled"].includes(source.state)};
}
export async function authorizeServiceAttachmentPurchase(database:SqlDatabase,actor:WorkContext,policy:AttachmentPolicy,targetId:string,operation:AttachmentOperation){
 const scope=await serviceAttachmentPurchaseScope(database,actor,policy,targetId);if(!scope)return;
 const read=operation==="read";await requireServiceCurrentAuthority(database,actor,`service-file-read:${randomUUID()}`,!read);
 if(!read&&scope.terminal)throw new ApiError(409,"SERVICE_ATTACHMENT_HISTORY_ONLY","This completed Service source retains its original private evidence as history.");
 const mode=await servicePackageEnforcementMode();for(const capability of [scope.capability,...(scope.parts?["service.parts-assets"]:[])])await checkPurchasedCapability(database,mode,{companyId:scope.companyId,capability,purpose:read?"history":"settle-existing"});
}
export async function serviceAttachmentSourceReadLocks(database:SqlDatabase,scopes:readonly ServiceAttachmentPurchase[]){
 if(!scopes.length||await servicePackageEnforcementMode()==="disabled")return [];
 // Service currently owns one legal Company per Files unit. Sorting the
 // Company fences remains deterministic if an internal unit composes targets.
 return [...new Set(scopes.map(scope=>scope.companyId))].sort().flatMap(company=>serviceOwnerSourceReadLocks(database,company));
}
export async function serviceAttachmentCommandBoundary(database:SqlDatabase,scopes:readonly ServiceAttachmentPurchase[]){
 const mode=await servicePackageEnforcementMode();if(!scopes.length||mode==="disabled")return undefined;
 const unique=[...new Map(scopes.map(scope=>[`${scope.companyId}:${scope.targetId}`,scope])).values()],boundaries=unique.flatMap(scope=>[scope.capability,...(scope.parts?["service.parts-assets"]:[])].map(capability=>{const key=`service-file-purchase:${randomUUID()}`;return {key,boundary:purchaseCommandBoundary(database,mode,key,{companyId:scope.companyId,capability,purpose:"settle-existing"})!};}));
 const guards=(phase:string)=>unique.flatMap(scope=>{const actor=scope.actor,key=`service-file-${phase}:${randomUUID()}`,query=serviceOwnerSourceQuery({kind:"order",id:scope.targetId},true);return [...workContextGuards(database,actor,key),...iamMutationGuards(database,actor,key),
  assertion(database,key,"field-service-file-source",`EXISTS(SELECT 1 FROM (${query}) actual WHERE actual.image=? AND actual.state NOT IN ('billed','cancelled'))`,[scope.targetId,scope.companyId,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId,scope.sourceImage]),
  assertion(database,key,"field-service-file-authority",`(SELECT ${assignmentReadSql("service_file_scope","records.write")} FROM (SELECT ?::text company_id,?::text branch_id,?::text department_id) service_file_scope) AND EXISTS(SELECT 1 FROM assignments selected WHERE selected.id=? AND (jsonb_array_length(modules_json::jsonb)=0 OR modules_json::jsonb @> '["field_service"]'::jsonb))`,[actor.assignmentId,actor.userId,actor.role,actor.companyId,actor.branchId,actor.departmentId,actor.assignmentId]),database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(key)];});
 return {before:[...boundaries.flatMap(item=>item.boundary.before),...guards("before")],after:[...financeDeferredEffectStatements(database),...guards("after"),...boundaries.flatMap(item=>item.boundary.after),...boundaries.map(item=>database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(item.key))],observe:mode==="shadow"?observePurchaseResults:undefined};
}
export function serviceAttachmentFailure(error:unknown){let cause=error;for(let depth=0;depth<5&&cause&&typeof cause==="object";depth++){const value=cause as {code?:string;detail?:string;cause?:unknown};if(value.code==="23514"&&value.detail?.includes("field-service-file-")){const failure=new ApiError(409,"SERVICE_ATTACHMENT_CHANGED","The original Service visit or selected role changed before its evidence committed.");failure.cause=cause;return failure;}cause=value.cause;}return null;}

export type FieldServiceAttachmentVersionContext={actor:WorkContext;commandId:string};
/** The original Files receipt supplies the binder. A Service order version may
 * advance only with its matching immutable owner event, so the original
 * deferred Support case projection sees the same source version and actor.
 * Terminal billing/cancellation history never becomes a new business event. */
export function fieldServiceAttachmentRecordVersionStep(database:SqlDatabase,targetType:string,targetId:string,expectedVersion:number,now:number,context?:FieldServiceAttachmentVersionContext){
 if(targetType!=="field_service_visit")return null;
 if(!context)throw new ApiError(503,"SERVICE_ATTACHMENT_CONTEXT_REQUIRED","Service evidence requires the original Files command context.");
 const {actor,commandId}=context;
 if(!actor.modules.includes("field_service"))throw new ApiError(403,"MODULE_FORBIDDEN","Choose an original Field Service assignment.");
 return database.prepare(`WITH changed AS (
  UPDATE field_service_orders service SET version=version+1,updated_at=?
  WHERE service.id=? AND service.company_id=? AND service.version=? AND service.status NOT IN ('billed','cancelled')
   AND (?::text IS NULL OR service.branch_id=?) AND (?::text IS NULL OR service.department_id=?)
   AND ${assignmentReadSql("service","records.write")}
   AND EXISTS(SELECT 1 FROM command_receipts receipt WHERE receipt.command_id=? AND receipt.operation IN ('attachment.record.bind','attachment.compatibility.bind') AND receipt.record_id=service.id AND receipt.actor_id=? AND receipt.authority_json::jsonb->>'companyId'=service.company_id)
  RETURNING service.*
 ), evidence AS (
  INSERT INTO field_service_order_events(id,order_id,version,action,actor_id,reason,status,technician_id,scheduled_at,details_json,created_at)
  SELECT ?,changed.id,changed.version,'attach-evidence',?,'Attached private Service visit evidence',changed.status,changed.technician_id,changed.scheduled_at,
   jsonb_build_object('attachmentCommandId',?::text),? FROM changed RETURNING id
 ) INSERT INTO command_assertions(command_id,step,ok) SELECT ?,'field-service-file-version',CASE WHEN (SELECT count(*) FROM evidence)=1 THEN 1 ELSE 0 END`)
  .bind(now,targetId,actor.companyId,expectedVersion,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId,actor.assignmentId,actor.userId,actor.role,commandId,actor.userId,`service-file-event-${commandId}`,actor.userId,commandId,now,commandId);
}
