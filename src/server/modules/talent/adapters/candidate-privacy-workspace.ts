import "server-only";
import {samePerson} from "@/lib/identity_access/iam";
import {hasPermission} from "@/lib/identity_access/permissions";
import type {WorkContext} from "../../identity_access/public/authorization";
import {assignmentReadSql} from "../../identity_access/public/record-source-authority";
import {holdingResources} from "../../runtime/public/runtime";
import {ApiError} from "../../runtime/public/http";
import {readPurchasedAccesses,talentPackageEnforcementMode} from "../../commercial/public/purchase-command-boundary";
import {withTalentPurchasedRead} from "./purchases";
import {beginTalentProductSourceRead} from "./product-entry";
import type {TalentProductPresentationOptions} from "../domain/product-entry";
const permissions=["legal.hold","legal.hold.release","legal.destroy","people.manage"] as const;
/** Privacy reviewers receive the preservation/withdrawal state and immutable
 * action metadata, never recruiting contacts, consent or interview material. */
export async function candidatePrivacyWorkspace(actor:WorkContext,id:string,options:TalentProductPresentationOptions={}){
 if(actor.branchId||actor.departmentId)throw new ApiError(403,"COMPANY_HR_SCOPE_REQUIRED","Use a Company HR or Legal assignment for candidate privacy.");
 const permission=permissions.find(value=>hasPermission(actor,value));if(!permission)throw new ApiError(403,"CANDIDATE_PRIVACY_FORBIDDEN","A Company HR or Legal privacy permission is required.");
 const strict=options.strictProduct?await beginTalentProductSourceRead(actor,{kind:"candidate-privacy",id}):null;
 return withTalentPurchasedRead(actor,["recruit.candidates"],async provided=>{
  const database=provided??(await holdingResources(actor.holdingId)).write,row=await database.prepare(`WITH privacy_viewer AS (SELECT ?::text company_id,NULL::text branch_id,NULL::text department_id)
 SELECT c.id,c.status,c.version,c.retention_until,c.privacy_hold,c.privacy_requested_by,${assignmentReadSql("privacy_viewer","legal.hold")} can_hold,${assignmentReadSql("privacy_viewer","legal.hold.release")} can_release,${assignmentReadSql("privacy_viewer","legal.destroy")} can_erase,${assignmentReadSql("privacy_viewer","people.manage")} can_withdraw
 FROM talent_candidates c CROSS JOIN privacy_viewer WHERE c.id=? AND c.company_id=privacy_viewer.company_id`).bind(actor.companyId,...permissions.flatMap(()=>[actor.assignmentId,actor.userId,actor.role]),id).first<{id:string;status:string;version:number;retention_until:string;privacy_hold:boolean;privacy_requested_by:string|null;can_hold:boolean;can_release:boolean;can_erase:boolean;can_withdraw:boolean}>();
  if(!row)throw new ApiError(404,"CANDIDATE_NOT_FOUND","Candidate privacy source not found.");if(!row.can_hold&&!row.can_release&&!row.can_erase&&!row.can_withdraw)throw new ApiError(403,"CANDIDATE_PRIVACY_FORBIDDEN","Choose a current Company privacy work role.");
  const history=(await database.prepare("SELECT id,action,candidate_version,created_at FROM talent_candidate_privacy_events WHERE candidate_id=? AND company_id=? ORDER BY candidate_version,id").bind(id,actor.companyId).all<{id:string;action:string;candidate_version:number;created_at:number}>()).results,mode=await talentPackageEnforcementMode(),access=strict?await strict():mode==="disabled"?undefined:(await readPurchasedAccesses(database,mode,actor.companyId,["recruit.candidates"]))["recruit.candidates"];
  return {candidate:{id:row.id,status:row.status,version:row.version,retentionUntil:row.retention_until,privacyHold:row.privacy_hold},actions:{hold:row.can_hold&&!row.privacy_hold&&row.status!=="erased",release:row.can_release&&row.privacy_hold&&row.status!=="erased",withdraw:row.can_withdraw&&row.status==="active",erase:row.can_erase&&row.status==="withdrawn"&&!row.privacy_hold&&!samePerson(actor,row.privacy_requested_by)},history,purchase:access?{newWork:access.newWork,settleExisting:access.settleExisting,history:access.history,revision:access.revision}:undefined};
 },undefined,[permission]);
}
