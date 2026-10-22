import "server-only";
import {randomUUID} from "node:crypto";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {iamMutationGuards} from "@erpx/server-identity_access/public/iam-store";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {workContextGuards} from "@erpx/server-workflow/public/commands";
import {learningPackageEnforcementMode,checkPurchasedCapability,purchaseCommandBoundary,observePurchaseResults,companyPurchaseReadLocks} from "@erpx/server-commercial/public/purchase-command-boundary";
import type {CapabilityPurpose} from "@erpx/server-commercial/public/rules/packages";
import {financeDeferredEffectStatements} from "@erpx/server-finance/public/finance-transactions";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {ApiError} from "@erpx/server-runtime/public/http";
import type {AttachmentOperation} from "@erpx/server-files/public/attachments/types";
import {learningOwnsAttachmentAction} from "@erpx/server-learning/adapters/purchases";

type Target="learning_course"|"learning_assignment";
export type LearningAttachmentPurchase={companyId:string;capability:"learn.courses"|"learn.certificates";purpose:CapabilityPurpose;actor:WorkContext;targetType:Target;targetId:string;witness:string;ownedCompletion:boolean};
function source(type:Target,ownedCompletion=false){
 if(type==="learning_course")return "SELECT c.*,NULL::text branch_id,NULL::text department_id,(to_jsonb(c)-'version'-'updated_at')::text image FROM learning_courses c WHERE c.id=? AND c.company_id=?";
 return `SELECT a.*,jsonb_build_object('assignment',to_jsonb(a)-'version'-'updated_at'${ownedCompletion?"-'status'-'attendance'-'score_bps'-'result_evidence'-'completion_reason'":""},'course',to_jsonb(c)-'version'-'updated_at')::text image
 FROM learning_assignments a JOIN learning_courses c ON c.id=a.course_id AND c.company_id=a.company_id WHERE a.id=? AND a.company_id=?`;
}
export async function learningAttachmentPurchaseScope(database:SqlDatabase,actor:WorkContext,type:string,id:string):Promise<LearningAttachmentPurchase|null>{
 if(!["learning_course","learning_assignment"].includes(type)||await learningPackageEnforcementMode()==="disabled")return null;
 const targetType=type as Target,ownedCompletion=targetType==="learning_assignment"&&learningOwnsAttachmentAction(database,actor,id);
 const row=await database.prepare(source(targetType,ownedCompletion)).bind(id,actor.companyId).first<{image:string}>();
 if(!row)throw new ApiError(404,"ATTACHMENT_TARGET_NOT_FOUND","The original Learning file source is unavailable.");
 return {companyId:actor.companyId,capability:targetType==="learning_course"?"learn.courses":"learn.certificates",purpose:targetType==="learning_course"?"new-work":"settle-existing",actor,targetType,targetId:id,witness:row.image,ownedCompletion};
}
/** Original sensitive Files policy decides read/manage/self first. Purchasing
 * never replaces that policy or imposes a full People management dependency. */
export async function authorizeLearningAttachmentPurchase(database:SqlDatabase,actor:WorkContext,type:string,id:string,operation:AttachmentOperation){
 const scope=await learningAttachmentPurchaseScope(database,actor,type,id);if(!scope)return;
 const key=`learning-file-read:${randomUUID()}`;
 await database.batch([...workContextGuards(database,actor,key),...iamMutationGuards(database,actor,key),database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(key)]);
 await checkPurchasedCapability(database,await learningPackageEnforcementMode(),{companyId:scope.companyId,capability:scope.capability,purpose:operation==="read"?"history":scope.purpose});
}
export async function learningAttachmentSourceReadLocks(database:SqlDatabase,scopes:readonly LearningAttachmentPurchase[]){
 if(!scopes.length||await learningPackageEnforcementMode()==="disabled")return [];
 const locks=[...new Set(scopes.map(scope=>scope.companyId))].sort().map(companyId=>companyPurchaseReadLocks(database,companyId));
 // File binding advances the original course/assignment version. Acquire
 // the writer lease before any snapshot instead of upgrading two concurrent
 // Company SHARE leases after both have reached their owner effects.
 return [...locks.map(items=>items[0]),database.prepare("LOCK TABLE learning_courses,learning_assignments IN SHARE ROW EXCLUSIVE MODE"),...locks.flatMap(items=>items.slice(1))];
}
export async function learningAttachmentCommandBoundary(database:SqlDatabase,scopes:readonly LearningAttachmentPurchase[]){
 const mode=await learningPackageEnforcementMode();if(!scopes.length||mode==="disabled")return;
 const unique=[...new Map(scopes.map(scope=>[`${scope.companyId}:${scope.targetType}:${scope.targetId}:${scope.purpose}`,scope])).values()];
 const boundaries=unique.map(scope=>{const key=`learning-file-purchase:${randomUUID()}`;return {key,boundary:purchaseCommandBoundary(database,mode,key,scope)!};});
 const guards=(phase:string)=>unique.flatMap(scope=>{
  const actor=scope.actor,key=`learning-file-${phase}:${randomUUID()}`;
  // These are the original sensitive Files writers. Completion may change
  // its own phase fields only inside the private complete-command marker;
  // the outer owner unit compares the full final assignment and certificate.
  const writer=scope.targetType==="learning_course"?"iam_role_allows(?,?,'learning.documents.manage')":
   "(iam_role_allows(?,?,'learning.documents.manage') OR (iam_same_person(learning_file_source.learner_id,?,learning_file_source.company_id) AND learning_file_source.status IN ('in_progress','completed')))";
  const writerArgs=scope.targetType==="learning_course"?[actor.role,actor.companyId]:[actor.role,actor.companyId,actor.userId];
  const recordScope=scope.targetType==="learning_course"?"TRUE":"(?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id=?)";
  const scopeArgs=scope.targetType==="learning_course"?[]:[actor.branchId,actor.branchId,actor.departmentId,actor.departmentId];
  return [...workContextGuards(database,actor,key),...iamMutationGuards(database,actor,key),assertion(database,key,"learning-file-source",`EXISTS(SELECT 1 FROM (${source(scope.targetType,scope.ownedCompletion)}) learning_file_source WHERE image=? AND ${recordScope} AND ${writer})`,[scope.targetId,scope.companyId,scope.witness,...scopeArgs,...writerArgs]),database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(key)];
 });
 return {before:[...boundaries.flatMap(item=>item.boundary.before),...guards("before")],after:[...financeDeferredEffectStatements(database),...guards("after"),...boundaries.flatMap(item=>item.boundary.after),...boundaries.map(item=>database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(item.key))],observe:mode==="shadow"?observePurchaseResults:undefined};
}
export function learningAttachmentFailure(error:unknown){let current=error;for(let depth=0;depth<5&&current&&typeof current==="object";depth++){const item=current as {code?:string;detail?:string;cause?:unknown};if(item.code==="23514"&&item.detail?.includes("learning-file-"))return new ApiError(409,"LEARNING_ATTACHMENT_CHANGED","The Learning file source or current authority changed.");current=item.cause;}return null;}
