import { z } from "zod";
import { BusinessRuleError } from "../../shared";
import type { WorkItem } from "@/lib/work_projection/work-item";

export type BenefitRow = Record<string, string | number | null>;
export type BenefitRecordKind = "plan" | "window" | "dependent" | "election";
const reason = z.string().trim().min(3).max(3000);
const version = z.number().int().positive();
const date = z.string().regex(/^\d{4}-\d{2}-\d{2}$/);
export const benefitWindowSchema = z.object({name:z.string().trim().min(2).max(160),opensOn:date,closesOn:date,effectiveOn:date});
export const benefitDependentSchema = z.object({name:z.string().trim().min(2).max(160),relationship:z.string().trim().min(2).max(80),birthDate:date});
export const benefitElectionSchema = z.object({planId:z.string().min(1),planVersion:version,windowId:z.string().min(1),windowVersion:version,dependentIds:z.array(z.string().min(1)).max(20).default([])});
export const benefitTransitionSchema = z.object({
 action:z.enum(["publish","reject","retire","open","close","cancel","verify","remove","submit","acknowledge","release"]),
 expectedVersion:version,reason,providerReference:z.string().trim().min(2).max(160).optional(),
 attachmentSetIds:z.array(z.string().min(1)).max(20).default([]),
});
export type BenefitWindowInput = z.infer<typeof benefitWindowSchema>;
export type BenefitDependentInput = z.infer<typeof benefitDependentSchema>;
export type BenefitElectionInput = z.infer<typeof benefitElectionSchema>;
export type BenefitTransitionInput = z.infer<typeof benefitTransitionSchema>;
export function benefitTransitionChanges(input:BenefitTransitionInput,actorId:string){
 return {
  ...(input.action==="submit"?{submittedBy:actorId}:{}),
  ...(input.action==="acknowledge"?{providerReference:input.providerReference??null,acknowledgedBy:actorId}:{}),
  ...(input.action==="release"?{releasedBy:actorId}:{}),
  ...(["publish","reject","retire","open","close","verify","remove"].includes(input.action)?{reviewedBy:actorId}:{}),
 };
}
const transitions:Record<BenefitRecordKind, Partial<Record<BenefitTransitionInput["action"], readonly [string,string]>>> = {
 plan:{publish:["draft","published"],reject:["draft","rejected"],retire:["published","retired"]},
 window:{open:["draft","open"],close:["open","closed"],cancel:["draft","cancelled"]},
 dependent:{verify:["pending","verified"],reject:["pending","rejected"],remove:["verified","removed"]},
 election:{submit:["elected","submitted"],acknowledge:["submitted","acknowledged"],release:["acknowledged","released"],cancel:["elected","cancelled"]},
};
export function benefitTransition(kind:BenefitRecordKind,row:BenefitRow,input:BenefitTransitionInput){
 const rule=transitions[kind][input.action];
 if(!rule||row.status!==rule[0])throw new BusinessRuleError(409,"INVALID_BENEFIT_TRANSITION","This transition is not valid for the current status.");
 if(Number(row.version)!==input.expectedVersion)throw new BusinessRuleError(409,"CONFLICT","The record changed. Reload and retry.");
 return {from:rule[0],to:rule[1]};
}
export function assertBenefitIndependentReview(row:BenefitRow,input:BenefitTransitionInput,actorId:string){
 if(["publish","reject","open","verify"].includes(input.action)&&row.created_by===actorId)throw new BusinessRuleError(409,"CONFLICT","Independent review is required.");
 if(input.action==="acknowledge"&&!input.providerReference)throw new BusinessRuleError(422,"PROVIDER_REFERENCE_REQUIRED","Record the provider acknowledgement reference.");
 if(input.action==="release"&&row.acknowledged_by===actorId)throw new BusinessRuleError(409,"CONFLICT","Payroll release must be independent from provider acknowledgement.");
}
export function assertBenefitElectionSources(plan:BenefitRow|null,window:BenefitRow|null,today:string,input:BenefitElectionInput){
 if(!plan||!window)throw new BusinessRuleError(409,"BENEFIT_SOURCE_CHANGED","Choose current published plan and open window versions.");
 if(today<String(window.opens_on)||today>String(window.closes_on))throw new BusinessRuleError(409,"ENROLLMENT_WINDOW_CLOSED","The enrollment window is not active today.");
 if(String(plan.coverage_type)==="family"&&!input.dependentIds.length)throw new BusinessRuleError(422,"VERIFIED_DEPENDENT_REQUIRED","Family coverage requires a verified dependent.");
 return {plan,window};
}
export function assertBenefitVerifiedDependents(input:BenefitElectionInput,verifiedIds:readonly string[]){
 const verified=new Set(verifiedIds);
 if(new Set(input.dependentIds).size!==input.dependentIds.length||input.dependentIds.some(id=>!verified.has(id)))throw new BusinessRuleError(422,"VERIFIED_DEPENDENT_REQUIRED","Every selected dependent must be verified.");
}
export function benefitEvidence(kind:BenefitRecordKind,input:BenefitTransitionInput){
 const purpose=kind==="dependent"&&input.action==="verify"?"dependent_proof":kind==="election"&&input.action==="submit"?"election_document":kind==="election"&&input.action==="acknowledge"?"provider_proof":null;
 const targetType=kind==="dependent"?"benefit_dependent":kind==="election"?"benefit_election":null;
 return purpose&&targetType&&input.attachmentSetIds.length?{targetType,purpose,bindingType:`benefits_${kind}_${input.action}`,componentRef:kind==="dependent"?"verification":input.action==="submit"?"employee-submit":"provider-acknowledgement"}:null;
}
export function benefitWorkItems(rows:BenefitRow[],person:BenefitRow|null,actorId:string,canReview:boolean):WorkItem[]{
 return rows.map(x=>{
  const review=["submitted","acknowledged"].includes(String(x.status));
  const canAct=review?canReview&&(x.status!=="acknowledged"||x.acknowledged_by!==actorId):x.person_id===person?.id;
  return {id:String(x.id),type:"benefit_enrollment",title:`Benefits · ${x.plan_name}`,detail:`${x.employee_name} · ${x.status}`,href:`/benefits/${x.id}`,status:String(x.status),updatedAt:Number(x.updated_at),queue:canAct?"todo":"waiting",canAct,nextAction:String(x.status)==="elected"?"Submit election":String(x.status)==="submitted"?"Record provider acknowledgement":"Release payroll input",relationships:[x.person_id===person?.id?"requested":"team",...(review?["approval" as const]:[])]};
 });
}
