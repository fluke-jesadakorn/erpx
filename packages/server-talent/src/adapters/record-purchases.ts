import "server-only";
import {talentActorScopeCte,talentClockCte} from './talent-scope-sql';
import {hasModuleAccess} from "@erpx/catalogue/module-access";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {assignmentReadSql} from "@erpx/server-identity_access/public/record-source-authority";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {ApiError} from "@erpx/server-runtime/public/http";
import {nativeTalentReadFirst} from "./native-talent-read";
import {talentPackageEnforcementMode,readPurchasedAccesses,checkPurchasedCapability} from "@erpx/server-commercial/public/purchase-command-boundary";
import {purchasedCapabilityPredicate} from "@erpx/server-commercial/public/purchased-capability-sql";
const capabilities=["recruit.positions","recruit.candidates","recruit.applications","recruit.offers"] as const;
/** Owner-local source-row family. The projection and FROM source are
 * parameterized; the emitted SQL is byte-identical to the former five
 * inline copies over the same Recruitment owner rows. */
const sourceRow=(cols:string,from:string)=>`SELECT ${cols} FROM ${from}`;
const sources=`${sourceRow("r.id,r.company_id,r.branch_id,r.department_id,'requisition' kind,'recruit.positions' capability","talent_requisitions r")}
 UNION ALL ${sourceRow("c.id,c.company_id,NULL,NULL,'candidate','recruit.candidates'","talent_candidates c")}
 UNION ALL ${sourceRow("a.id,a.company_id,r.branch_id,r.department_id,'application','recruit.applications'","talent_applications a JOIN talent_requisitions r ON r.id=a.requisition_id AND r.company_id=a.company_id JOIN talent_candidates c ON c.id=a.candidate_id AND c.company_id=a.company_id")}
 UNION ALL ${sourceRow("p.id,a.company_id,r.branch_id,r.department_id,'proposal','recruit.applications'","talent_proposals p JOIN talent_applications a ON a.id=p.application_id JOIN talent_requisitions r ON r.id=a.requisition_id AND r.company_id=a.company_id JOIN talent_candidates c ON c.id=a.candidate_id AND c.company_id=a.company_id")}
 UNION ALL ${sourceRow("o.id,o.company_id,r.branch_id,r.department_id,'offer','recruit.offers'","talent_offers o JOIN talent_applications a ON a.id=o.application_id AND a.company_id=o.company_id JOIN talent_requisitions r ON r.id=a.requisition_id AND r.company_id=a.company_id JOIN talent_candidates c ON c.id=a.candidate_id AND c.company_id=a.company_id")}`;
function currentSource(actor:WorkContext,id:string,company:string,kind?:"application",purchased=true){
 const permission=hasModuleAccess(actor.modules,"talent")&&hasPermission(actor,"records.read")&&hasPermission(actor,"records.review"),purchase=purchased?capabilities.map(capability=>`(talent_source.capability='${capability}' AND ${purchasedCapabilityPredicate("talent_source.company_id",capability,"history","talent_source_clock.now_ms")})`).join(" OR "):"TRUE";
 return {where:`EXISTS(${talentClockCte("talent_source_clock")} SELECT 1 FROM (${sources}) talent_source CROSS JOIN ${talentActorScopeCte} talent_viewer CROSS JOIN talent_source_clock WHERE talent_source.id=${id} AND talent_source.company_id=${company} AND talent_source.company_id=talent_viewer.company_id AND ${permission?"TRUE":"FALSE"} ${kind?"AND talent_source.kind='application'":""} AND (talent_source.kind='candidate' OR (talent_viewer.branch_id IS NULL OR talent_source.branch_id=talent_viewer.branch_id) AND (talent_viewer.department_id IS NULL OR talent_source.department_id=talent_viewer.department_id)) AND ${assignmentReadSql("talent_viewer","records.read")} AND ${assignmentReadSql("talent_viewer","records.review")} AND (${purchase}))`,args:[actor.companyId,actor.branchId,actor.departmentId,actor.assignmentId,actor.userId,actor.role,actor.assignmentId,actor.userId,actor.role]};
}
/** These native identifiers are checked before LIMIT; a private notification
 * label or a copied Records title cannot fabricate a Recruitment source. */
export async function talentSourceReadScope(database:SqlDatabase,actor:WorkContext,surface:"records"|"staff"|"inbox"){
 const mode=await talentPackageEnforcementMode();if(mode!=="enforced"){if(mode==="shadow")await readPurchasedAccesses(database,mode,actor.companyId,capabilities);return {where:"TRUE",args:[] as (string|number|null)[]};}
 const id=surface==="records"?"workspace_records.id":surface==="staff"?"staff_notifications.record_id":"e.source_id",company=surface==="records"?"workspace_records.company_id":surface==="staff"?"staff_notifications.company_id":"e.company_id",scope=currentSource(actor,id,company,surface==="records"?undefined:"application");
 if(surface==="records")return {where:`(NOT(module_id='talent' AND EXISTS(SELECT 1 FROM (${sources}) actual_talent_source WHERE actual_talent_source.id=${id})) OR ${scope.where})`,args:scope.args};
 const type=surface==="staff"?"staff_notifications.record_type":"e.source_type";return {where:`(${type}<>'talent_application' OR ${scope.where})`,args:scope.args};
}
export async function requireTalentWorkspaceRecordRead(database:SqlDatabase,actor:WorkContext,moduleId:string,id:string){
 const mode=await talentPackageEnforcementMode();if(mode==="disabled"||moduleId!=="talent")return;
 const native=await nativeTalentReadFirst<{capability:string;company_id:string}>('talent_rd_workspace_record_source',[id],database);if(!native)return;const scope=currentSource(actor,"workspace_records.id","workspace_records.company_id",undefined,false);
 if(!await database.prepare(`SELECT 1 FROM workspace_records WHERE module_id='talent' AND id=? AND ${scope.where}`).bind(id,...scope.args).first())throw new ApiError(404,"TALENT_SOURCE_NOT_FOUND","The actual Recruitment source is unavailable in this workspace.");
 await checkPurchasedCapability(database,mode,{companyId:actor.companyId,capability:native.capability,purpose:"history"});
}
