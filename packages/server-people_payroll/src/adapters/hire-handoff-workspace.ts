import {canCreateHireHandoffPayGroup} from '@erpx/server-people_payroll/adapters/hire-handoff-pay-group';
import "server-only";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {requirePermission} from "@erpx/server-identity_access/public/iam-permissions";
import {holdingResources} from "../../../../packages/server-runtime/src/public/runtime";
import {ApiError} from "@erpx/server-runtime/public/http";
import {withTalentPurchasedRead} from "@erpx/server-talent/public/purchases";
import {peopleHireHandoffFacts} from "@erpx/server-talent/public/people-hire-handoff-facts";
import {readPurchasedAccesses,talentPackageEnforcementMode} from "@erpx/server-commercial/public/purchase-command-boundary";
import {companyScope} from "@erpx/server-people_payroll/domain/people-handoffs";
import {beginTalentProductSourceRead,type TalentProductPresentationOptions} from "@erpx/server-talent/public/product-entry";
/** One accepted source, same-currency groups and exact-email reconciliation
 * identities. It does not expose the Company People directory or Payroll. */
export async function hireHandoffWorkspace(actor:WorkContext,handoffId:string,options:TalentProductPresentationOptions={}){
 requirePermission(actor,"people.manage");try{companyScope(actor);}catch{throw new ApiError(403,"COMPANY_HR_SCOPE_REQUIRED","Use a Company HR assignment for the handoff.");}
 const strict=options.strictProduct?await beginTalentProductSourceRead(actor,{kind:"hire",id:handoffId}):null;
 return withTalentPurchasedRead(actor,["recruit.offers"],async provided=>{
  const database=provided??(await holdingResources(actor.holdingId)).write,source=await peopleHireHandoffFacts(database).readHireSourceRow(handoffId,actor.companyId);
  if(!source||source.offer_status!=="accepted"||source.source_offer_version>source.offer_version)throw new ApiError(404,"HIRE_HANDOFF_NOT_FOUND","Accepted hire handoff not found.");
  const receipt=await database.prepare("SELECT id,person_id,engagement_id,compensation_id,source_offer_version,source_candidate_version,created_at FROM talent_hire_consumption_receipts WHERE handoff_id=? AND company_id=?").bind(handoffId,actor.companyId).first<{id:string;person_id:string;engagement_id:string;compensation_id:string;source_offer_version:number;source_candidate_version:number;created_at:number}>(),today=new Date().toISOString().slice(0,10),eligible=source.offer_version===source.source_offer_version&&source.candidate_status==="active"&&source.retention_until>=today&&!!source.consent_evidence.trim()&&!!source.accepted_evidence?.trim();
  const payGroups=receipt?[]:(await database.prepare("SELECT id,name,currency,frequency FROM hr_pay_groups WHERE company_id=? AND currency=? AND status='active' ORDER BY name,id").bind(actor.companyId,source.currency).all<{id:string;name:string;currency:string;frequency:string}>()).results,people=receipt?[]:(await database.prepare("SELECT id,legal_name,version,status,merged_into_id FROM hr_people WHERE company_id=? AND lower(email)=lower(?) ORDER BY id").bind(actor.companyId,source.email).all<{id:string;legal_name:string;version:number;status:string;merged_into_id:string|null}>()).results,mode=await talentPackageEnforcementMode(),access=strict?await strict():mode==="disabled"?undefined:(await readPurchasedAccesses(database,mode,actor.companyId,["recruit.offers"]))["recruit.offers"];
  const canCreatePayGroup=!receipt&&eligible&&!payGroups.length&&await canCreateHireHandoffPayGroup(actor,handoffId);
  return {companyId:actor.companyId,...(canCreatePayGroup?{canCreatePayGroup}:{}),handoff:{id:source.id,applicationId:(await database.prepare("SELECT application_id FROM talent_offers WHERE id=? AND company_id=?").bind(source.offer_id,actor.companyId).first<string>("application_id"))!,offerVersion:source.offer_version,candidateVersion:source.candidate_version,legalName:source.legal_name,email:source.email,jobTitle:source.job_title,currency:source.currency,baseAmountMinor:source.base_amount_minor,startDate:source.start_date},receipt,payGroups,people,canConsume:!receipt&&eligible,purchase:access?{newWork:access.newWork,settleExisting:access.settleExisting,history:access.history,revision:access.revision}:undefined};
 },undefined,["people.manage"]);
}
