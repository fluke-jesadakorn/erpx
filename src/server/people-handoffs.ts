import "server-only";
import { z } from "zod";
import { samePerson } from "@/lib/iam";
import type { WorkContext } from "./authorization";
import { assertion,executeCommand,replayCommand } from "./commands";
import { snapshotMoney } from "./company-money";
import { sha256 } from "./crypto";
import { ApiError } from "./http";
import { requirePermission } from "./iam-permissions";
import { holdingResources } from "./runtime";
import type { SqlDatabase } from "./runtime/types";

export const hireConsumptionSchema=z.object({handoffId:z.string().min(1),expectedOfferVersion:z.number().int().positive(),expectedCandidateVersion:z.number().int().positive(),countryCode:z.string().length(2).transform(v=>v.toUpperCase()).default("TH"),payGroupId:z.string().min(1),reconcilePersonId:z.string().min(1).optional(),expectedPersonVersion:z.number().int().positive().optional(),evidenceRef:z.string().trim().min(8).max(2000)}).strict();
export const candidatePrivacySchema=z.object({action:z.enum(["hold","release","withdraw","erase"]),expectedVersion:z.number().int().positive(),reason:z.string().trim().min(8).max(2000)}).strict();
type Source={id:string;offer_id:string;company_id:string;branch_id:string;department_id:string|null;candidate_id:string;source_offer_version:number;offer_version:number;offer_status:string;start_date:string;currency:string;base_amount_minor:number;job_title:string;candidate_version:number;candidate_status:string;retention_until:string;legal_name:string;email:string;phone:string|null;privacy_hold:boolean;accepted_evidence:string;consent_evidence:string};
function companyScope(actor:WorkContext){if(actor.branchId||actor.departmentId)throw new ApiError(403,"COMPANY_HR_SCOPE_REQUIRED","Use a Company HR or Legal assignment for the handoff.");}
async function hireSource(write:SqlDatabase,actor:WorkContext,id:string){
  const row=await write.prepare(`SELECT h.*,o.version offer_version,o.status offer_status,o.start_date,o.currency,o.base_amount_minor,o.accepted_evidence,r.job_title,c.version candidate_version,c.status candidate_status,c.retention_until,c.legal_name,c.email,c.phone,c.privacy_hold,c.consent_evidence
   FROM talent_hire_handoffs h JOIN talent_offers o ON o.id=h.offer_id JOIN talent_applications a ON a.id=o.application_id JOIN talent_requisitions r ON r.id=a.requisition_id JOIN talent_candidates c ON c.id=h.candidate_id WHERE h.id=? AND h.company_id=?`)
    .bind(id,actor.companyId).first<Source>();if(!row)throw new ApiError(404,"HIRE_HANDOFF_NOT_FOUND","Hire handoff not found.");return row;
}
export async function hireHandoffQueue(actor:WorkContext){
  actor=requirePermission(actor,"people.manage");companyScope(actor);const {write}=await holdingResources(actor.holdingId);
  return (await write.prepare("SELECT h.*,o.application_id,o.currency,o.base_amount_minor,o.start_date,o.version offer_version,c.legal_name,c.version candidate_version,c.status candidate_status,r.job_title,x.id receipt_id,x.person_id,x.engagement_id FROM talent_hire_handoffs h JOIN talent_offers o ON o.id=h.offer_id JOIN talent_candidates c ON c.id=h.candidate_id JOIN talent_applications a ON a.id=o.application_id JOIN talent_requisitions r ON r.id=a.requisition_id LEFT JOIN talent_hire_consumption_receipts x ON x.handoff_id=h.id WHERE h.company_id=? ORDER BY h.created_at,h.id").bind(actor.companyId).all()).results;
}
export async function consumeHireHandoff(actor:WorkContext,input:z.infer<typeof hireConsumptionSchema>,key:string){
  actor=requirePermission(actor,"people.manage");companyScope(actor);const {write}=await holdingResources(actor.holdingId),id=`hire-consumption-${key}`,operation="people.hire-handoff.consume",prior=await replayCommand(write,actor,key,operation,id,input);if(prior)return prior;
  const source=await hireSource(write,actor,input.handoffId),today=new Date().toISOString().slice(0,10);
  const receipt=await write.prepare("SELECT id FROM talent_hire_consumption_receipts WHERE handoff_id=? AND company_id=?").bind(source.id,actor.companyId).first();if(receipt)throw new ApiError(409,"HIRE_HANDOFF_ALREADY_CONSUMED","This accepted offer already has an immutable HR consumption receipt.");
  if(source.offer_status!=="accepted"||source.offer_version!==source.source_offer_version||source.offer_version!==input.expectedOfferVersion||source.candidate_version!==input.expectedCandidateVersion)throw new ApiError(409,"HIRE_HANDOFF_SOURCE_CHANGED","Review the current accepted offer and candidate revision before creating HR records.");
  if(source.candidate_status!=="active"||source.retention_until<today||!source.consent_evidence.trim()||!source.accepted_evidence?.trim())throw new ApiError(409,"HIRE_HANDOFF_PRIVACY_BLOCKED","Active recruitment consent, retention and recorded offer acceptance are required.");
  const group=await write.prepare("SELECT id,currency,frequency FROM hr_pay_groups WHERE id=? AND company_id=? AND status='active'").bind(input.payGroupId,actor.companyId).first<{id:string;currency:string;frequency:string}>();if(!group||group.currency!==source.currency)throw new ApiError(422,"HIRE_PAY_GROUP_INVALID","Choose an active same-currency Company pay group.");
  const matches=(await write.prepare("SELECT id,version,status,merged_into_id FROM hr_people WHERE company_id=? AND lower(email)=lower(?)").bind(actor.companyId,source.email).all<{id:string;version:number;status:string;merged_into_id:string|null}>()).results;
  const person=input.reconcilePersonId?await write.prepare("SELECT id,version,status,merged_into_id FROM hr_people WHERE id=? AND company_id=?").bind(input.reconcilePersonId,actor.companyId).first<{id:string;version:number;status:string;merged_into_id:string|null}>():null;
  if(input.reconcilePersonId&&(!person||person.version!==input.expectedPersonVersion||person.status!=="active"||person.merged_into_id))throw new ApiError(409,"HIRE_PERSON_CHANGED","Review the current active unmerged HR identity revision.");
  if(matches.length&&(!person||matches.some(row=>row.id!==person.id)))throw new ApiError(409,"HIRE_IDENTITY_RECONCILIATION_REQUIRED","Reconcile the existing Company person explicitly before consuming this offer.");
  const personId=person?.id??`person-hire-${key}`,engagementId=`engagement-hire-${key}`,compensationId=`compensation-hire-${key}`,digest=await sha256(JSON.stringify(source)),now=Date.now(),money=await snapshotMoney(write,actor,{sourceType:"talent_hire_compensation",sourceId:compensationId,amountKind:"base",transactionDate:source.start_date,currency:source.currency,currencyScale:2,amountUnits:String(source.base_amount_minor)});
  const result={id,handoffId:source.id,personId,engagementId,compensationId,status:"consumed",version:1,reconciled:!!person};
  return executeCommand(write,actor,key,operation,input,id,1,result,[
    write.prepare("SELECT pg_advisory_xact_lock(hashtextextended(?,0))").bind(`hire-person:${actor.companyId}:${source.email.toLowerCase()}`),
    write.prepare("SELECT id FROM talent_candidates WHERE id=? FOR SHARE").bind(source.candidate_id),
    assertion(write,key,"hire-source","EXISTS(SELECT 1 FROM talent_hire_handoffs h JOIN talent_offers o ON o.id=h.offer_id JOIN talent_candidates c ON c.id=h.candidate_id WHERE h.id=? AND h.company_id=? AND o.status='accepted' AND o.version=? AND h.source_offer_version=o.version AND c.version=? AND c.status='active' AND c.retention_until>=?)",[source.id,actor.companyId,input.expectedOfferVersion,input.expectedCandidateVersion,today]),
    assertion(write,key,"hire-not-consumed","NOT EXISTS(SELECT 1 FROM talent_hire_consumption_receipts WHERE handoff_id=?)",[source.id]),
    assertion(write,key,"hire-email-reconcile","NOT EXISTS(SELECT 1 FROM hr_people WHERE company_id=? AND lower(email)=lower(?) AND id<>?)",[actor.companyId,source.email,personId]),
    assertion(write,key,"hire-group","EXISTS(SELECT 1 FROM hr_pay_groups WHERE id=? AND company_id=? AND currency=? AND status='active')",[input.payGroupId,actor.companyId,source.currency]),
    assertion(write,key,"hire-placement","EXISTS(SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND status='active') AND (?::text IS NULL OR EXISTS(SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND branch_id=? AND status='active'))",[source.branch_id,actor.companyId,source.department_id,source.department_id,actor.companyId,source.branch_id]),
    ...(person?[assertion(write,key,"hire-person","EXISTS(SELECT 1 FROM hr_people WHERE id=? AND company_id=? AND version=? AND status='active' AND merged_into_id IS NULL)",[person.id,actor.companyId,person.version])]:[write.prepare("INSERT INTO hr_people(id,company_id,legal_name,email,phone,country_code,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(personId,actor.companyId,source.legal_name,source.email,source.phone,input.countryCode,actor.userId,now,now)]),
    write.prepare("INSERT INTO hr_engagements(id,person_id,company_id,branch_id,department_id,worker_type,employment_type,status,start_date,job_title,currency,pay_group_id,created_by,created_at,updated_at) VALUES(?,?,?,?,?,'employee','salaried','onboarding',?,?,?,?,?,?,?)").bind(engagementId,personId,actor.companyId,source.branch_id,source.department_id,source.start_date,source.job_title,source.currency,input.payGroupId,actor.userId,now,now),
    write.prepare("INSERT INTO hr_compensation_revisions(id,engagement_id,effective_from,pay_frequency,base_amount_minor,currency,status,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,'draft',?,?,?)").bind(compensationId,engagementId,source.start_date,group.frequency,source.base_amount_minor,source.currency,actor.userId,now,now),
    money.statement,write.prepare("INSERT INTO talent_hire_consumption_receipts(id,handoff_id,company_id,person_id,engagement_id,compensation_id,source_offer_version,source_candidate_version,source_digest,purpose,actor_id,command_id,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(id,source.id,actor.companyId,personId,engagementId,compensationId,source.offer_version,source.candidate_version,digest,input.evidenceRef,actor.userId,key,now),
  ]);
}
export async function changeCandidatePrivacy(actor:WorkContext,candidateId:string,input:z.infer<typeof candidatePrivacySchema>,key:string){
  actor=requirePermission(actor,input.action==="hold"?"legal.hold":input.action==="release"?"legal.hold.release":input.action==="erase"?"legal.destroy":"people.manage");companyScope(actor);
  const {write}=await holdingResources(actor.holdingId),operation=`talent.privacy.${input.action}`,prior=await replayCommand(write,actor,key,operation,candidateId,input);if(prior)return prior;
  const row=await write.prepare("SELECT id,status,privacy_hold,privacy_requested_by,version,retention_until FROM talent_candidates WHERE id=? AND company_id=?").bind(candidateId,actor.companyId).first<{id:string;status:string;privacy_hold:boolean;privacy_requested_by:string|null;version:number;retention_until:string}>();if(!row)throw new ApiError(404,"CANDIDATE_NOT_FOUND","Candidate not found.");
  if(row.version!==input.expectedVersion||row.status==="erased")throw new ApiError(409,"CANDIDATE_CHANGED","Review the current candidate revision.");
  if(input.action==="hold"&&row.privacy_hold||input.action==="release"&&!row.privacy_hold||input.action==="withdraw"&&row.status!=="active")throw new ApiError(409,"CANDIDATE_PRIVACY_STATE","This privacy action is unavailable in the current state.");
  if(input.action==="erase"&&(row.status!=="withdrawn"||row.privacy_hold))throw new ApiError(409,"CANDIDATE_ERASURE_BLOCKED","Withdraw recruitment processing and release active preservation holds before erasure.");
  if(input.action==="erase"&&samePerson(actor,row.privacy_requested_by))throw new ApiError(403,"SELF_APPROVAL","Another Legal destruction reviewer must authorize recruitment contact erasure.");
  const status=input.action==="erase"?"erased":input.action==="withdraw"?"withdrawn":row.status,version=row.version+1,result={id:candidateId,status,version,privacyHold:input.action==="hold"?true:input.action==="release"?false:row.privacy_hold};
  return executeCommand(write,actor,key,operation,input,candidateId,version,result,[
    write.prepare("SELECT id FROM talent_candidates WHERE id=? FOR UPDATE").bind(candidateId),assertion(write,key,"candidate-privacy","EXISTS(SELECT 1 FROM talent_candidates WHERE id=? AND company_id=? AND version=? AND status=? AND privacy_hold::int=?)",[candidateId,actor.companyId,row.version,row.status,row.privacy_hold?1:0]),
    ...(input.action==="erase"?[write.prepare("UPDATE talent_candidates SET legal_name='Erased recruitment contact',email=?,phone=NULL,consent_evidence='Recruitment contact erased after independent Legal review',status='erased',version=?,updated_at=? WHERE id=?").bind(`${candidateId}@erased.invalid`,version,Date.now(),candidateId)]:[write.prepare("UPDATE talent_candidates SET status=?,privacy_hold=?,privacy_requested_by=CASE WHEN ?='withdraw' THEN ? ELSE privacy_requested_by END,version=?,updated_at=? WHERE id=?").bind(status,result.privacyHold,input.action,actor.userId,version,Date.now(),candidateId)]),
    write.prepare("INSERT INTO talent_candidate_privacy_events(id,candidate_id,company_id,action,candidate_version,actor_id,reason,command_id,created_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(`candidate-privacy-${key}`,candidateId,actor.companyId,input.action,version,actor.userId,input.reason,key,Date.now()),
  ]);
}
