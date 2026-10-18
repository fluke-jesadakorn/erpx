import {learningTalentSnapshotSources as peoplepayrollSnapshotSources} from '../../people_payroll/public/learning-talent-snapshot-sources';
const peoplepayrollSources=peoplepayrollSnapshotSources();
import "server-only";
import { benefitLearningRequirement } from "@/server/modules/benefits/public/eligibility";
import { hasModuleAccess } from "@/data/module-access";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import { assertion,executeCommand,replayCommand } from "@/server/modules/workflow/public/commands";
import { sha256 } from "@/server/modules/runtime/public/crypto";
import { ApiError } from "@/server/modules/runtime/public/http";
import { requirePermission } from "@/server/modules/identity_access/public/iam-permissions";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import type { SqlDatabase,SqlStatement } from "@/server/modules/runtime/public/runtime/types";

import { createLearningBenefitConsumption, createValidateBenefitLearning, createRevokeLearningCertificate } from "../application/eligibility";
import type { LearningCertificate as Certificate, LearningBenefitInput, LearningBenefitElection, LearningRevocationInput } from "../domain/eligibility";
export { certificateRevocationSchema } from "../domain/eligibility";
import {withLearningPurchasedCommand} from "./purchases";
const currentSource=`JOIN learning_eligibility_handoffs h ON h.id=c.handoff_id JOIN learning_assignments a ON a.id=c.assignment_id ${peoplepayrollSources.learning_adapters_learning_eligibility_hr_people_p_join} ON p.id=c.person_id AND p.company_id=c.company_id`;

export function nativeLearningBenefitConsumption(write:SqlDatabase){return createLearningBenefitConsumption({error:violation=>new ApiError(violation.status,violation.code,violation.message),today:()=>new Date().toISOString().slice(0,10),digest:sha256,
 async certificates(actor,input,today){return (await write.prepare(`SELECT c.* FROM learning_certificates c ${currentSource} WHERE c.company_id=? AND c.person_id=? AND c.eligibility_code=? AND c.valid_from<=? AND (c.expires_on IS NULL OR c.expires_on>=?) AND c.valid_from<=? AND (c.expires_on IS NULL OR c.expires_on>=?) AND a.version=c.source_assignment_version AND a.status IN ('completed','waived') AND h.source_assignment_version=a.version AND p.status='active' AND p.merged_into_id IS NULL AND NOT EXISTS(SELECT 1 FROM learning_certificate_revocations r WHERE r.certificate_id=c.id) ORDER BY c.valid_from DESC,c.id`)
    .bind(actor.companyId,input.personId,input.requiredCode,today,today,input.effectiveDate,input.effectiveDate).all<Certificate>()).results;},
 async prepare(actor,input,certificate,digest){const receipt=write.prepare("INSERT INTO learning_benefit_consumption_receipts(id,handoff_id,certificate_id,election_id,company_id,person_id,source_assignment_version,eligibility_code,source_digest,purpose,actor_id,command_id,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)")
    .bind(`learning-benefit-${input.key}`,certificate.handoff_id,certificate.id,input.electionId,actor.companyId,input.personId,certificate.source_assignment_version,certificate.eligibility_code,digest,"Benefits eligibility at election creation",actor.userId,input.key,Date.now());return {guards:learningCertificateGuards(write,actor,input.key,certificate.id,input.personId,input.requiredCode,input.effectiveDate),receipt};},
 });}
export async function prepareLearningBenefitConsumption(write:SqlDatabase,actor:WorkContext,input:LearningBenefitInput){const result=await nativeLearningBenefitConsumption(write)(actor,input);return {certificateId:result.certificateId,...result.native as {guards:SqlStatement[];receipt:SqlStatement}};}
export function learningCertificateGuards(write:SqlDatabase,actor:WorkContext,key:string,certificateId:string,personId:string,code:string,effectiveDate:string):SqlStatement[]{
  const today=new Date().toISOString().slice(0,10);return [
    write.prepare("SELECT c.id FROM learning_certificates c WHERE c.id=? FOR SHARE").bind(certificateId),
    assertion(write,key,"learning-current-certificate",`EXISTS(SELECT 1 FROM learning_certificates c ${currentSource} WHERE c.id=? AND c.company_id=? AND c.person_id=? AND c.eligibility_code=? AND a.version=c.source_assignment_version AND h.source_assignment_version=a.version AND a.status IN ('completed','waived') AND p.status='active' AND p.merged_into_id IS NULL AND c.valid_from<=? AND (c.expires_on IS NULL OR c.expires_on>=?) AND c.valid_from<=? AND (c.expires_on IS NULL OR c.expires_on>=?) AND NOT EXISTS(SELECT 1 FROM learning_certificate_revocations r WHERE r.certificate_id=c.id))`,[certificateId,actor.companyId,personId,code,today,today,effectiveDate,effectiveDate]),
  ];
}
export function nativeValidateBenefitLearning(write:SqlDatabase){return createValidateBenefitLearning({error:violation=>new ApiError(violation.status,violation.code,violation.message),today:()=>new Date().toISOString().slice(0,10),
 requirement:(actor,planId)=>benefitLearningRequirement(write,actor.companyId,planId),
 receipt:async(actor,row)=>await write.prepare("SELECT certificate_id FROM learning_benefit_consumption_receipts WHERE election_id=? AND company_id=? AND person_id=?").bind(row.id,actor.companyId,row.person_id).first<{certificate_id:string}>(),
 guards:(actor,key,certificateId,personId,code,effectiveDate)=>learningCertificateGuards(write,actor,key,certificateId,personId,code,effectiveDate),
 async current(actor,certificateId,today){const row={learning_certificate_id:certificateId};return Boolean(await write.prepare(`SELECT c.id FROM learning_certificates c ${currentSource} WHERE c.id=? AND c.company_id=? AND a.version=c.source_assignment_version AND a.status IN ('completed','waived') AND p.status='active' AND p.merged_into_id IS NULL AND (c.expires_on IS NULL OR c.expires_on>=?) AND NOT EXISTS(SELECT 1 FROM learning_certificate_revocations r WHERE r.certificate_id=c.id)`).bind(row.learning_certificate_id,actor.companyId,today).first());},
 });}
export async function validateBenefitLearning(write:SqlDatabase,actor:WorkContext,row:LearningBenefitElection,key:string){return (await nativeValidateBenefitLearning(write)(actor,row,key)??[]) as SqlStatement[];}
export async function revokeLearningCertificate(actor:WorkContext,id:string,input:LearningRevocationInput,key:string){
 const owner=(await holdingResources(actor.holdingId)).write;
 return withLearningPurchasedCommand(owner,actor,{key,operation:"learning.certificate.revoke",target:{kind:"certificate",id},purpose:"settle-existing",capabilities:["learn.certificates"],permission:"records.review",existing:true,
 sourceWitness:async database=>JSON.stringify(await database.prepare(`SELECT to_jsonb(a)-'updated_at' assignment,to_jsonb(h) handoff,to_jsonb(r) revocation
 FROM learning_certificates c ${currentSource} LEFT JOIN learning_certificate_revocations r ON r.certificate_id=c.id
 WHERE c.id=? AND c.company_id=?`).bind(id,actor.companyId).first())},write=>nativeRevokeLearningCertificate(write,actor,id,input,key));
}
async function nativeRevokeLearningCertificate(write:SqlDatabase,actor:WorkContext,id:string,input:LearningRevocationInput,key:string){
 const database=async()=>write;
 return createRevokeLearningCertificate({
 authorize(actor){actor=requirePermission(actor,"records.review");if(!hasModuleAccess(actor.modules,"learning"))throw new ApiError(403,"MODULE_FORBIDDEN","Use an authorized Learning reviewer assignment.");return actor;},
 error:violation=>new ApiError(violation.status,violation.code,violation.message),now:Date.now,
 replay:async(actor,key,operation,id,input)=>replayCommand(await database(),actor,key,operation,id,input),
 async certificate(actor,id,input){const write=await database();return await write.prepare(`SELECT c.* FROM learning_certificates c ${currentSource} WHERE c.id=? AND c.company_id=? AND (?::text IS NULL OR a.branch_id=?) AND (?::text IS NULL OR a.department_id=?) AND a.version=?`).bind(id,actor.companyId,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId,input.expectedAssignmentVersion).first<Certificate>();},
 async prepare(command,input,c){const write=await database(),{actor,id,key,now}=command;return {native:[write.prepare("SELECT id FROM learning_certificates WHERE id=? FOR UPDATE").bind(id),assertion(write,key,"certificate-source","EXISTS(SELECT 1 FROM learning_assignments WHERE id=? AND company_id=? AND version=?)",[c.assignment_id,actor.companyId,input.expectedAssignmentVersion]),write.prepare("INSERT INTO learning_certificate_revocations(id,certificate_id,company_id,actor_id,reason,command_id,created_at) VALUES(?,?,?,?,?,?,?)").bind(`certificate-revoke-${key}`,id,actor.companyId,actor.userId,input.reason,key,now)]};},
 async commit(command,mutation){return executeCommand(await database(),command.actor,command.key,command.operation,command.input,command.id,command.version,command.result,mutation.native as SqlStatement[]);},
 })(actor,id,input,key);
}
