import {BusinessRuleError,type Clock,type IdentifierSource} from '../../shared';
import type {PublicMarketingForm} from './public-form';
import type {PublicMarketingSubmission,PublicMarketingReceipt} from '../domain/public-submission';
export interface PublicMarketingSubmissionUnit {
 receipt(key:string):Promise<{request_hash:string;candidate_id:string}|null>;
 current(form:PublicMarketingForm):Promise<unknown>;
 presentation(key:string,form:PublicMarketingForm):Promise<unknown>;
 limit(rateKey:string,bucket:number):Promise<{count:number}|null>;
 branch(companyId:string):Promise<{id:string}|null>;
 campaign(id:string,companyId:string):Promise<unknown>;
 candidate(id:string,form:PublicMarketingForm,body:PublicMarketingSubmission,branchId:string,attribution:string,now:number):Promise<unknown>;
 engagement(id:string,form:PublicMarketingForm,now:number):Promise<unknown>;
 retainReceipt(key:string,requestHash:string,id:string,now:number):Promise<unknown>;
 clearAssertions(key:string):Promise<unknown>;
}
export interface PublicMarketingSubmissionPorts {
 clock:Clock;identifiers:IdentifierSource;hash(value:string):string;
 transaction<Result>(run:(unit:PublicMarketingSubmissionUnit)=>Promise<Result>):Promise<Result>;
}
export function createPublicMarketingSubmissionApplication(ports:PublicMarketingSubmissionPorts){return {
 async submit(form:PublicMarketingForm,body:PublicMarketingSubmission,key:string,ip:string):Promise<PublicMarketingReceipt>{
  const requestHash=ports.hash(JSON.stringify({formId:form.id,formVersion:form.version,body})),rateKey=ports.hash(`${form.id}:${ip}`);
  return ports.transaction(async unit=>{
   const prior=await unit.receipt(key);if(prior){if(prior.request_hash!==requestHash)throw new BusinessRuleError(409,'IDEMPOTENCY_CONFLICT','This submission key belongs to a different form.');return {id:prior.candidate_id,status:'pending'};}
   if(!await unit.current(form))throw new BusinessRuleError(409,'FORM_CHANGED','The approved form changed. Reload before submitting.');
   await unit.presentation(key,form);
   const bucket=Math.floor(ports.clock.now()/60000),limit=await unit.limit(rateKey,bucket);if(Number(limit?.count)>5)throw new BusinessRuleError(429,'FORM_RATE_LIMIT','Too many submissions. Try again later.');
   const branch=await unit.branch(form.company_id);if(!branch)throw new BusinessRuleError(409,'FORM_BRANCH_REQUIRED','The form has no active receiving branch.');
   if(body.campaignId&&!await unit.campaign(body.campaignId,form.company_id))throw new BusinessRuleError(422,'CAMPAIGN_UNKNOWN','Campaign ID is unavailable.');
   const now=ports.clock.now(),id=`marketing-candidate-${ports.identifiers.next()}`,attribution={campaignId:body.campaignId??'',source:body.source,capture:'public_form',formId:form.id,formVersion:form.version};
   await unit.candidate(id,form,body,branch.id,JSON.stringify(attribution),now);
   await unit.engagement(id,form,now);
   await unit.retainReceipt(key,requestHash,id,now);
   await unit.clearAssertions(key);
   return {id,status:'pending'};
  });
 }
};}
