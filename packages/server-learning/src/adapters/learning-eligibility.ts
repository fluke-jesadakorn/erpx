import {learningTalentSnapshotSources as peoplepayrollSnapshotSources} from '@erpx/server-people_payroll/public/learning-talent-snapshot-sources';
const peoplepayrollSources=peoplepayrollSnapshotSources();
import "server-only";
import { benefitLearningRequirement } from "@erpx/server-benefits/public/eligibility";
import { hasModuleAccess } from "@erpx/catalogue/module-access";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand,replayCommand} from "@erpx/server-workflow/public/commands";
import { sha256 } from "@erpx/server-runtime/public/crypto";
import { ApiError } from "@erpx/server-runtime/public/http";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import type { SqlDatabase,SqlStatement } from "@erpx/server-runtime/public/runtime/types";

import { createLearningBenefitConsumption, createValidateBenefitLearning, createRevokeLearningCertificate } from "@erpx/server-learning/application/eligibility";
import type { LearningCertificate as Certificate, LearningBenefitInput, LearningBenefitElection, LearningRevocationInput } from "@erpx/server-learning/domain/eligibility";
export { certificateRevocationSchema } from "@erpx/server-learning/domain/eligibility";
import {withLearningPurchasedCommand} from "@erpx/server-learning/adapters/purchases";
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

