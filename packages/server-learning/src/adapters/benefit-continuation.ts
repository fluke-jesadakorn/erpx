import 'server-only';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {hasModuleAccess} from '@erpx/catalogue/module-access';
import {hasPermission} from '@erpx/feature-identity_access/lib/permissions';
import {ApiError} from '@erpx/server-runtime/public/http';
import {benefitPurchaseControls} from '@erpx/server-benefits/public/purchases';
import type {LearningBenefitContinuation,LearningBenefitSource} from '@erpx/server-learning/domain/benefit-continuation';
/** Navigation only, using the existing Benefits authority/purchase reader.
 * The actual Benefits page and command recheck person/window/certificate; this
 * does not read Benefits business rows or select/consume a certificate. */
export async function learningBenefitContinuation(actor:WorkContext,source:LearningBenefitSource):Promise<LearningBenefitContinuation|null>{
 if(!hasModuleAccess(actor.modules,'benefits')||!hasPermission(actor,'records.read')||source.purchases&&source.purchases['learn.certificates']?.history!==true)return null;
 const {assignment,handoff,certificate}=source;
 if(!handoff||!certificate||assignment.learner_id!==actor.userId||!['completed','waived'].includes(String(assignment.status)))return null;
 const id=certificate.id,personId=certificate.person_id,code=certificate.eligibility_code,version=Number(certificate.source_assignment_version);
 if(typeof id!=='string'||!id||typeof personId!=='string'||!personId||typeof code!=='string'||!code||!Number.isSafeInteger(version)||version<1
  ||assignment.company_id!==actor.companyId||certificate.company_id!==actor.companyId||handoff.company_id!==actor.companyId
  ||certificate.assignment_id!==assignment.id||handoff.assignment_id!==assignment.id||certificate.handoff_id!==handoff.id
  ||handoff.person_id!==personId||handoff.eligibility_code!==code||Number(assignment.version)!==version||Number(handoff.source_assignment_version)!==version)return null;
 try{const purchase=await benefitPurchaseControls(actor);return !purchase||purchase.history?{eligibilityCode:code}:null;}
 catch(error){if(error instanceof ApiError&&[403,404].includes(error.status))return null;throw error;}
}
