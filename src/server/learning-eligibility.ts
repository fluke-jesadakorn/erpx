import "server-only";
import { z } from "zod";
import { hasModuleAccess } from "@/data/module-access";
import type { WorkContext } from "./authorization";
import { assertion,executeCommand,replayCommand } from "./commands";
import { sha256 } from "./crypto";
import { ApiError } from "./http";
import { requirePermission } from "./iam-permissions";
import { holdingResources } from "./runtime";
import type { SqlDatabase,SqlStatement } from "./runtime/types";

type Certificate={id:string;handoff_id:string;company_id:string;person_id:string;assignment_id:string;source_assignment_version:number;eligibility_code:string;valid_from:string;expires_on:string|null;evidence_ref:string};
const currentSource="JOIN learning_eligibility_handoffs h ON h.id=c.handoff_id JOIN learning_assignments a ON a.id=c.assignment_id JOIN hr_people p ON p.id=c.person_id AND p.company_id=c.company_id";
export async function prepareLearningBenefitConsumption(write:SqlDatabase,actor:WorkContext,input:{personId:string;requiredCode:string;effectiveDate:string;electionId:string;key:string}){
  const today=new Date().toISOString().slice(0,10),rows=(await write.prepare(`SELECT c.* FROM learning_certificates c ${currentSource} WHERE c.company_id=? AND c.person_id=? AND c.eligibility_code=? AND c.valid_from<=? AND (c.expires_on IS NULL OR c.expires_on>=?) AND c.valid_from<=? AND (c.expires_on IS NULL OR c.expires_on>=?) AND a.version=c.source_assignment_version AND a.status IN ('completed','waived') AND h.source_assignment_version=a.version AND p.status='active' AND p.merged_into_id IS NULL AND NOT EXISTS(SELECT 1 FROM learning_certificate_revocations r WHERE r.certificate_id=c.id) ORDER BY c.valid_from DESC,c.id`)
    .bind(actor.companyId,input.personId,input.requiredCode,today,today,input.effectiveDate,input.effectiveDate).all<Certificate>()).results;
  if(!rows.length)throw new ApiError(409,"BENEFIT_LEARNING_ELIGIBILITY_REQUIRED","Complete the required learning or reviewed waiver with a current certificate covering the benefit start date.");
  const certificate=rows[0],digest=await sha256(JSON.stringify(certificate)),receipt=write.prepare("INSERT INTO learning_benefit_consumption_receipts(id,handoff_id,certificate_id,election_id,company_id,person_id,source_assignment_version,eligibility_code,source_digest,purpose,actor_id,command_id,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)")
    .bind(`learning-benefit-${input.key}`,certificate.handoff_id,certificate.id,input.electionId,actor.companyId,input.personId,certificate.source_assignment_version,certificate.eligibility_code,digest,"Benefits eligibility at election creation",actor.userId,input.key,Date.now());
  return {certificateId:certificate.id,guards:learningCertificateGuards(write,actor,input.key,certificate.id,input.personId,input.requiredCode,input.effectiveDate),receipt};
}
export function learningCertificateGuards(write:SqlDatabase,actor:WorkContext,key:string,certificateId:string,personId:string,code:string,effectiveDate:string):SqlStatement[]{
  const today=new Date().toISOString().slice(0,10);return [
    write.prepare("SELECT c.id FROM learning_certificates c WHERE c.id=? FOR SHARE").bind(certificateId),
    assertion(write,key,"learning-current-certificate",`EXISTS(SELECT 1 FROM learning_certificates c ${currentSource} WHERE c.id=? AND c.company_id=? AND c.person_id=? AND c.eligibility_code=? AND a.version=c.source_assignment_version AND h.source_assignment_version=a.version AND a.status IN ('completed','waived') AND p.status='active' AND p.merged_into_id IS NULL AND c.valid_from<=? AND (c.expires_on IS NULL OR c.expires_on>=?) AND c.valid_from<=? AND (c.expires_on IS NULL OR c.expires_on>=?) AND NOT EXISTS(SELECT 1 FROM learning_certificate_revocations r WHERE r.certificate_id=c.id))`,[certificateId,actor.companyId,personId,code,today,today,effectiveDate,effectiveDate]),
  ];
}
export async function validateBenefitLearning(write:SqlDatabase,actor:WorkContext,row:{id:string;person_id:string;learning_certificate_id?:string|null;effective_from:string;plan_id:string},key:string){
  const plan=await write.prepare("SELECT required_eligibility_code FROM benefit_plans WHERE id=? AND company_id=?").bind(row.plan_id,actor.companyId).first<{required_eligibility_code:string|null}>();
  if(!plan?.required_eligibility_code)return [];
  const receipt=await write.prepare("SELECT certificate_id FROM learning_benefit_consumption_receipts WHERE election_id=? AND company_id=? AND person_id=?").bind(row.id,actor.companyId,row.person_id).first<{certificate_id:string}>();
  if(!row.learning_certificate_id||receipt?.certificate_id!==row.learning_certificate_id)throw new ApiError(409,"BENEFIT_LEARNING_ELIGIBILITY_REQUIRED","The election must retain its original learning consumption receipt.");
  const guards=learningCertificateGuards(write,actor,key,row.learning_certificate_id,row.person_id,plan.required_eligibility_code,row.effective_from);
  const c=await write.prepare(`SELECT c.id FROM learning_certificates c ${currentSource} WHERE c.id=? AND c.company_id=? AND a.version=c.source_assignment_version AND a.status IN ('completed','waived') AND p.status='active' AND p.merged_into_id IS NULL AND (c.expires_on IS NULL OR c.expires_on>=?) AND NOT EXISTS(SELECT 1 FROM learning_certificate_revocations r WHERE r.certificate_id=c.id)`).bind(row.learning_certificate_id,actor.companyId,new Date().toISOString().slice(0,10)).first();
  if(!c)throw new ApiError(409,"BENEFIT_LEARNING_CERTIFICATE_EXPIRED","The retained learning certificate expired, was revoked, or its source changed. Renew eligibility before continuing the election.");return guards;
}
export const certificateRevocationSchema=z.object({expectedAssignmentVersion:z.number().int().positive(),reason:z.string().trim().min(8).max(2000)}).strict();
export async function revokeLearningCertificate(actor:WorkContext,id:string,input:z.infer<typeof certificateRevocationSchema>,key:string){
  actor=requirePermission(actor,"records.review");if(!hasModuleAccess(actor.modules,"learning"))throw new ApiError(403,"MODULE_FORBIDDEN","Use an authorized Learning reviewer assignment.");const {write}=await holdingResources(actor.holdingId),operation="learning.certificate.revoke",prior=await replayCommand(write,actor,key,operation,id,input);if(prior)return prior;
  const c=await write.prepare(`SELECT c.* FROM learning_certificates c ${currentSource} WHERE c.id=? AND c.company_id=? AND (?::text IS NULL OR a.branch_id=?) AND (?::text IS NULL OR a.department_id=?) AND a.version=?`).bind(id,actor.companyId,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId,input.expectedAssignmentVersion).first<Certificate>();
  if(!c)throw new ApiError(404,"LEARNING_CERTIFICATE_NOT_FOUND","Current in-scope learning certificate not found.");
  const result={id,status:"revoked",version:input.expectedAssignmentVersion};return executeCommand(write,actor,key,operation,input,id,result.version,result,[write.prepare("SELECT id FROM learning_certificates WHERE id=? FOR UPDATE").bind(id),assertion(write,key,"certificate-source","EXISTS(SELECT 1 FROM learning_assignments WHERE id=? AND company_id=? AND version=?)",[c.assignment_id,actor.companyId,input.expectedAssignmentVersion]),write.prepare("INSERT INTO learning_certificate_revocations(id,certificate_id,company_id,actor_id,reason,command_id,created_at) VALUES(?,?,?,?,?,?,?)").bind(`certificate-revoke-${key}`,id,actor.companyId,actor.userId,input.reason,key,Date.now())]);
}
