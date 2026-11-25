import "server-only";
import {samePerson} from "@erpx/feature-identity_access/lib/iam";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {holdingResources} from "../../../../packages/server-runtime/src/public/runtime";
import {ApiError} from "@erpx/server-runtime/public/http";
import {readPurchasedAccesses,talentPackageEnforcementMode} from "@erpx/server-commercial/public/purchase-command-boundary";
import {withTalentPurchasedRead} from "@erpx/server-talent/adapters/purchases";
import {nativeTalentReadAll,nativeTalentReadFirst} from "./native-talent-read";
import {beginTalentProductSourceRead} from "@erpx/server-talent/adapters/product-entry";
import type {TalentProductPresentationOptions} from "@erpx/server-talent/domain/product-entry";
const permissions=["legal.hold","legal.hold.release","legal.destroy","people.manage"] as const;
/** Privacy reviewers receive the preservation/withdrawal state and immutable
 * action metadata, never recruiting contacts, consent or interview material. */
export async function candidatePrivacyWorkspace(actor:WorkContext,id:string,options:TalentProductPresentationOptions={}){
 if(actor.branchId||actor.departmentId)throw new ApiError(403,"COMPANY_HR_SCOPE_REQUIRED","Use a Company HR or Legal assignment for candidate privacy.");
 const permission=permissions.find(value=>hasPermission(actor,value));if(!permission)throw new ApiError(403,"CANDIDATE_PRIVACY_FORBIDDEN","A Company HR or Legal privacy permission is required.");
 const strict=options.strictProduct?await beginTalentProductSourceRead(actor,{kind:"candidate-privacy",id}):null;
 return withTalentPurchasedRead(actor,["recruit.candidates"],async provided=>{
  const database=provided??(await holdingResources(actor.holdingId)).write,row=await nativeTalentReadFirst<{id:string;status:string;version:number;retention_until:string;privacy_hold:boolean;privacy_requested_by:string|null;can_hold:boolean;can_release:boolean;can_erase:boolean;can_withdraw:boolean}>('talent_rd_candidate_privacy_workspace',[actor.companyId,...permissions.flatMap(()=>[actor.assignmentId,actor.userId,actor.role]),id],database);
  if(!row)throw new ApiError(404,"CANDIDATE_NOT_FOUND","Candidate privacy source not found.");if(!row.can_hold&&!row.can_release&&!row.can_erase&&!row.can_withdraw)throw new ApiError(403,"CANDIDATE_PRIVACY_FORBIDDEN","Choose a current Company privacy work role.");
  const history=await nativeTalentReadAll<{id:string;action:string;candidate_version:number;created_at:number}>('talent_rd_privacy_history',[id,actor.companyId],database),mode=await talentPackageEnforcementMode(),access=strict?await strict():mode==="disabled"?undefined:(await readPurchasedAccesses(database,mode,actor.companyId,["recruit.candidates"]))["recruit.candidates"];
  return {candidate:{id:row.id,status:row.status,version:row.version,retentionUntil:row.retention_until,privacyHold:row.privacy_hold},actions:{hold:row.can_hold&&!row.privacy_hold&&row.status!=="erased",release:row.can_release&&row.privacy_hold&&row.status!=="erased",withdraw:row.can_withdraw&&row.status==="active",erase:row.can_erase&&row.status==="withdrawn"&&!row.privacy_hold&&!samePerson(actor,row.privacy_requested_by)},history,purchase:access?{newWork:access.newWork,settleExisting:access.settleExisting,history:access.history,revision:access.revision}:undefined};
 },undefined,[permission]);
}
