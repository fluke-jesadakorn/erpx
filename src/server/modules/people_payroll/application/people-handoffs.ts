import {z} from 'zod';import {samePerson} from "@/lib/identity_access/iam";import type {WorkContext} from '../../identity_access/public/authorization';import type {snapshotMoney} from '../../finance/public/company-money';import {BusinessRuleError,type Clock} from '../../shared';import type {PayrollStatutoryCommands} from './payroll-statutory';import {hireConsumptionSchema,candidatePrivacySchema,type Source,companyScope} from '../domain/people-handoffs';export interface PeopleHandoffRepository<Effect>{readHireSourceRow(id:string,companyId:string):Promise<Source|null>;
readHireHandoffQueueEffect(companyId:string):Promise<{results:(Record<string,unknown>)[]}>;
readConsumeHireHandoffReceipt(id:string,companyId:string):Promise<Record<string,unknown>|null>;
readConsumeHireHandoffGroup(payGroupId:string,companyId:string):Promise<{id:string;currency:string;frequency:string}|null>;
readConsumeHireHandoffMatches(companyId:string,email:string):Promise<{results:({id:string;version:number;status:string;merged_into_id:string|null})[]}>;
readConsumeHireHandoffPerson(reconcilePersonId:string,companyId:string):Promise<{id:string;version:number;status:string;merged_into_id:string|null}|null>;
stageConsumeHireHandoffEffect(value:string):Effect;
stageConsumeHireHandoffEffect2(candidate_id:string):Effect;
guard9(key:string,value:"hire-source",id:string,companyId:string,expectedOfferVersion:number,expectedCandidateVersion:number,today:string):Effect;
guard10(key:string,value:"hire-not-consumed",id:string):Effect;
guard11(key:string,value:"hire-email-reconcile",companyId:string,email:string,personId:string):Effect;
guard12(key:string,value:"hire-group",payGroupId:string,companyId:string,currency:string):Effect;
guard13(key:string,value:"hire-placement",branch_id:string,companyId:string,department_id:string | null,department_id2:string | null,companyId2:string,branch_id2:string):Effect;
guard14(key:string,value:"hire-person",id:string,companyId:string,version:number):Effect;
stageConsumeHireHandoffEffect3(personId:string,companyId:string,legal_name:string,email:string,phone:string | null,countryCode:string,userId:string,now:number,now2:number):Effect;
stageConsumeHireHandoffEffect4(engagementId:string,personId:string,companyId:string,branch_id:string,department_id:string | null,start_date:string,job_title:string,currency:string,payGroupId:string,userId:string,now:number,now2:number):Effect;
stageConsumeHireHandoffEffect5(compensationId:string,engagementId:string,start_date:string,frequency:string,base_amount_minor:number,currency:string,userId:string,now:number,now2:number):Effect;
stageConsumeHireHandoffEffect6(id:string,id2:string,companyId:string,personId:string,engagementId:string,compensationId:string,offer_version:number,candidate_version:number,digest:string,evidenceRef:string,userId:string,key:string,now:number):Effect;
readChangeCandidatePrivacyRow(candidateId:string,companyId:string):Promise<{id:string;status:string;privacy_hold:boolean;privacy_requested_by:string|null;version:number;retention_until:string}|null>;
stageChangeCandidatePrivacyEffect(candidateId:string):Effect;
guard21(key:string,value:"candidate-privacy",candidateId:string,companyId:string,version:number,status:string,value2:0 | 1):Effect;
stageChangeCandidatePrivacyEffect2(value:string,version:number,now:number,candidateId:string):Effect;
stageChangeCandidatePrivacyEffect3(status:string,privacyHold:boolean,action:"hold" | "release" | "withdraw",userId:string,version:number,now:number,candidateId:string):Effect;
stageChangeCandidatePrivacyEffect4(value:string,candidateId:string,companyId:string,action:"hold" | "release" | "withdraw" | "erase",version:number,userId:string,reason:string,key:string,now:number):Effect;}export interface PeopleHandoffPorts<Effect>{repository:PeopleHandoffRepository<Effect>;clock:Clock;hash(value:string):Promise<string>;authorize(actor:WorkContext,permission:string):WorkContext;prepare(holdingId:string):Promise<void>;commands:PayrollStatutoryCommands<Effect>;money(actor:WorkContext,input:Parameters<typeof snapshotMoney>[2]):Promise<{statement:Effect}>;}export function createPeopleHandoffApplication<Effect>(ports:PeopleHandoffPorts<Effect>){const repository=ports.repository;async function hireSource(actor:WorkContext,id:string){
  const row=await repository.readHireSourceRow(id,actor.companyId);if(!row)throw new BusinessRuleError(404,"HIRE_HANDOFF_NOT_FOUND","Hire handoff not found.");return row;
}
async function hireHandoffQueue(actor:WorkContext){
  actor=ports.authorize(actor,"people.manage");companyScope(actor);await ports.prepare(actor.holdingId);
  return (await repository.readHireHandoffQueueEffect(actor.companyId)).results;
}
async function consumeHireHandoff(actor:WorkContext,input:z.infer<typeof hireConsumptionSchema>,key:string){
  actor=ports.authorize(actor,"people.manage");companyScope(actor);await ports.prepare(actor.holdingId);const id=`hire-consumption-${key}`,operation="people.hire-handoff.consume",prior=await ports.commands.replay(actor,key,operation,id,input);if(prior)return prior;
  const source=await hireSource(actor,input.handoffId),today=new Date(ports.clock.now()).toISOString().slice(0,10);
  const receipt=await repository.readConsumeHireHandoffReceipt(source.id,actor.companyId);if(receipt)throw new BusinessRuleError(409,"HIRE_HANDOFF_ALREADY_CONSUMED","This accepted offer already has an immutable HR consumption receipt.");
  if(source.offer_status!=="accepted"||source.offer_version!==source.source_offer_version||source.offer_version!==input.expectedOfferVersion||source.candidate_version!==input.expectedCandidateVersion)throw new BusinessRuleError(409,"HIRE_HANDOFF_SOURCE_CHANGED","Review the current accepted offer and candidate revision before creating HR records.");
  if(source.candidate_status!=="active"||source.retention_until<today||!source.consent_evidence.trim()||!source.accepted_evidence?.trim())throw new BusinessRuleError(409,"HIRE_HANDOFF_PRIVACY_BLOCKED","Active recruitment consent, retention and recorded offer acceptance are required.");
  const group=await repository.readConsumeHireHandoffGroup(input.payGroupId,actor.companyId);if(!group||group.currency!==source.currency)throw new BusinessRuleError(422,"HIRE_PAY_GROUP_INVALID","Choose an active same-currency Company pay group.");
  const matches=(await repository.readConsumeHireHandoffMatches(actor.companyId,source.email)).results;
  const person=input.reconcilePersonId?await repository.readConsumeHireHandoffPerson(input.reconcilePersonId,actor.companyId):null;
  if(input.reconcilePersonId&&(!person||person.version!==input.expectedPersonVersion||person.status!=="active"||person.merged_into_id))throw new BusinessRuleError(409,"HIRE_PERSON_CHANGED","Review the current active unmerged HR identity revision.");
  if(matches.length&&(!person||matches.some(row=>row.id!==person.id)))throw new BusinessRuleError(409,"HIRE_IDENTITY_RECONCILIATION_REQUIRED","Reconcile the existing Company person explicitly before consuming this offer.");
  const personId=person?.id??`person-hire-${key}`,engagementId=`engagement-hire-${key}`,compensationId=`compensation-hire-${key}`,digest=await ports.hash(JSON.stringify(source)),now=ports.clock.now(),money=await ports.money(actor,{sourceType:"talent_hire_compensation",sourceId:compensationId,amountKind:"base",transactionDate:source.start_date,currency:source.currency,currencyScale:2,amountUnits:String(source.base_amount_minor)});
  const result={id,handoffId:source.id,personId,engagementId,compensationId,status:"consumed",version:1,reconciled:!!person};
  return ports.commands.commit(actor,key,operation,input,id,1,result,[
    repository.stageConsumeHireHandoffEffect(`hire-person:${actor.companyId}:${source.email.toLowerCase()}`),
    repository.stageConsumeHireHandoffEffect2(source.candidate_id),
    repository.guard9(key,"hire-source",source.id,actor.companyId,input.expectedOfferVersion,input.expectedCandidateVersion,today),
    repository.guard10(key,"hire-not-consumed",source.id),
    repository.guard11(key,"hire-email-reconcile",actor.companyId,source.email,personId),
    repository.guard12(key,"hire-group",input.payGroupId,actor.companyId,source.currency),
    repository.guard13(key,"hire-placement",source.branch_id,actor.companyId,source.department_id,source.department_id,actor.companyId,source.branch_id),
    ...(person?[repository.guard14(key,"hire-person",person.id,actor.companyId,person.version)]:[repository.stageConsumeHireHandoffEffect3(personId,actor.companyId,source.legal_name,source.email,source.phone,input.countryCode,actor.userId,now,now)]),
    repository.stageConsumeHireHandoffEffect4(engagementId,personId,actor.companyId,source.branch_id,source.department_id,source.start_date,source.job_title,source.currency,input.payGroupId,actor.userId,now,now),
    repository.stageConsumeHireHandoffEffect5(compensationId,engagementId,source.start_date,group.frequency,source.base_amount_minor,source.currency,actor.userId,now,now),
    money.statement,repository.stageConsumeHireHandoffEffect6(id,source.id,actor.companyId,personId,engagementId,compensationId,source.offer_version,source.candidate_version,digest,input.evidenceRef,actor.userId,key,now),
  ]);
}
async function changeCandidatePrivacy(actor:WorkContext,candidateId:string,input:z.infer<typeof candidatePrivacySchema>,key:string){
  actor=ports.authorize(actor,input.action==="hold"?"legal.hold":input.action==="release"?"legal.hold.release":input.action==="erase"?"legal.destroy":"people.manage");companyScope(actor);
  await ports.prepare(actor.holdingId);const operation=`talent.privacy.${input.action}`,prior=await ports.commands.replay(actor,key,operation,candidateId,input);if(prior)return prior;
  const row=await repository.readChangeCandidatePrivacyRow(candidateId,actor.companyId);if(!row)throw new BusinessRuleError(404,"CANDIDATE_NOT_FOUND","Candidate not found.");
  if(row.version!==input.expectedVersion||row.status==="erased")throw new BusinessRuleError(409,"CANDIDATE_CHANGED","Review the current candidate revision.");
  if(input.action==="hold"&&row.privacy_hold||input.action==="release"&&!row.privacy_hold||input.action==="withdraw"&&row.status!=="active")throw new BusinessRuleError(409,"CANDIDATE_PRIVACY_STATE","This privacy action is unavailable in the current state.");
  if(input.action==="erase"&&(row.status!=="withdrawn"||row.privacy_hold))throw new BusinessRuleError(409,"CANDIDATE_ERASURE_BLOCKED","Withdraw recruitment processing and release active preservation holds before erasure.");
  if(input.action==="erase"&&samePerson(actor,row.privacy_requested_by))throw new BusinessRuleError(403,"SELF_APPROVAL","Another Legal destruction reviewer must authorize recruitment contact erasure.");
  const status=input.action==="erase"?"erased":input.action==="withdraw"?"withdrawn":row.status,version=row.version+1,result={id:candidateId,status,version,privacyHold:input.action==="hold"?true:input.action==="release"?false:row.privacy_hold};
  return ports.commands.commit(actor,key,operation,input,candidateId,version,result,[
    repository.stageChangeCandidatePrivacyEffect(candidateId),repository.guard21(key,"candidate-privacy",candidateId,actor.companyId,row.version,row.status,row.privacy_hold?1:0),
    ...(input.action==="erase"?[repository.stageChangeCandidatePrivacyEffect2(`${candidateId}@erased.invalid`,version,ports.clock.now(),candidateId)]:[repository.stageChangeCandidatePrivacyEffect3(status,result.privacyHold,input.action,actor.userId,version,ports.clock.now(),candidateId)]),
    repository.stageChangeCandidatePrivacyEffect4(`candidate-privacy-${key}`,candidateId,actor.companyId,input.action,version,actor.userId,input.reason,key,ports.clock.now()),
  ]);
}return {hireHandoffQueue,consumeHireHandoff,changeCandidatePrivacy};}
