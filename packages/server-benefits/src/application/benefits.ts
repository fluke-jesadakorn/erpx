import type { Clock } from "@erpx/server-shared/index";
import { BusinessRuleError } from "@erpx/server-shared/index";
import {assertBenefitElectionSources,assertBenefitIndependentReview,assertBenefitVerifiedDependents,benefitDependentSchema,benefitElectionSchema,benefitEvidence,benefitTransition,benefitTransitionChanges,benefitTransitionSchema,benefitWindowSchema,benefitWorkItems,type BenefitRow,type BenefitRecordKind,type BenefitWindowInput,type BenefitDependentInput,type BenefitElectionInput,type BenefitTransitionInput} from "@erpx/server-benefits/domain/benefits";

type Command = {key:string;operation:string;id:string;now:number};
export type BenefitMutation<Learning,Evidence> = Command & (
 | {action:"window";input:BenefitWindowInput}
 | {action:"dependent";input:BenefitDependentInput;person:BenefitRow}
 | {action:"election";input:BenefitElectionInput;person:BenefitRow;plan:BenefitRow;window:BenefitRow;learning:Learning|null}
 | {action:"transition";input:BenefitTransitionInput;kind:BenefitRecordKind;record:BenefitRow;from:string;to:string;changes:ReturnType<typeof benefitTransitionChanges>;releasePayroll:boolean;learning:Learning|null;evidence:Evidence|null}
);
export interface BenefitsPorts<Learning,Evidence>{
 actorId:string;clock:Clock;canRead:boolean;canReview:boolean;canWrite:boolean;canManageDocuments:boolean;
 authorize(write?:boolean):Promise<void>;authorizeReview():Promise<void>;
 replay(key:string,operation:string,id:string,input:unknown):Promise<Record<string,unknown>|null>;
 person():Promise<BenefitRow|null>;
 plans():Promise<BenefitRow[]>;windows():Promise<BenefitRow[]>;dependents(personId:string):Promise<BenefitRow[]>;
 elections(personId:string,reviewer:boolean,activeOnly?:boolean):Promise<BenefitRow[]>;
 plan(id:string,version:number):Promise<BenefitRow|null>;window(id:string,version:number):Promise<BenefitRow|null>;
 verifiedDependents(personId:string):Promise<string[]>;
 record(kind:BenefitRecordKind,id:string):Promise<BenefitRow|null>;
 election(id:string,personId:string,reviewer:boolean):Promise<BenefitRow|null>;
 history(id:string):Promise<BenefitRow[]>;payroll(id:string):Promise<BenefitRow|null>;
 prepareLearning(input:{personId:string;requiredCode:string;effectiveDate:string;electionId:string;key:string}):Promise<Learning>;
 validateLearning(record:BenefitRow,key:string):Promise<Learning>;
 prepareEvidence(id:string,kind:BenefitRecordKind,input:BenefitTransitionInput,evidence:NonNullable<ReturnType<typeof benefitEvidence>>,key:string):Promise<Evidence>;
 commit<Result>(mutation:BenefitMutation<Learning,Evidence>,result:Result):Promise<Result>;
}
export function createBenefitsApplication<Learning,Evidence>(ports:BenefitsPorts<Learning,Evidence>){
 return {
  async workspace(personal=false){
   await ports.authorize();const person=await ports.person();
   const [plans,windows,dependents,elections]=await Promise.all([ports.plans(),ports.windows(),person?ports.dependents(String(person.id)):Promise.resolve([]),ports.elections(String(person?.id??""),!personal&&ports.canReview)]);
   return {plans,windows,dependents,elections,actorId:ports.actorId,canWrite:ports.canWrite,canReview:ports.canReview,canManageDocuments:ports.canManageDocuments};
  },
  async createWindow(raw:unknown,key:string){
   await ports.authorize(true);await ports.authorizeReview();const input=benefitWindowSchema.parse(raw),id=`benefit-window-${key}`,operation="benefits.window.create";
   const prior=await ports.replay(key,operation,id,input);if(prior)return prior;
   if(input.closesOn<input.opensOn)throw new BusinessRuleError(422,"INVALID_WINDOW","Close date must follow open date.");
   return ports.commit({action:"window",key,operation,id,input,now:ports.clock.now()},{id,status:"draft",version:1});
  },
  async createDependent(raw:unknown,key:string){
   await ports.authorize(true);const input=benefitDependentSchema.parse(raw),person=await ports.person();
   if(!person)throw new BusinessRuleError(422,"BENEFIT_WORKER_REQUIRED","An active worker record is required.");
   const id=`benefit-dependent-${key}`,operation="benefits.dependent.create",prior=await ports.replay(key,operation,id,input);if(prior)return prior;
   return ports.commit({action:"dependent",key,operation,id,input,person,now:ports.clock.now()},{id,status:"pending",version:1});
  },
  async createElection(raw:unknown,key:string){
   await ports.authorize(true);const input=benefitElectionSchema.parse(raw),id=`benefit-election-${key}`,operation="benefits.election.create",prior=await ports.replay(key,operation,id,input);if(prior)return prior;
   const person=await ports.person();if(!person)throw new BusinessRuleError(422,"BENEFIT_WORKER_REQUIRED","An active worker record is required.");
   const plan=await ports.plan(input.planId,input.planVersion),window=await ports.window(input.windowId,input.windowVersion);
   // The original source-presence guard precedes the clock/window and dependent checks.
   if(!plan||!window)throw new BusinessRuleError(409,"BENEFIT_SOURCE_CHANGED","Choose current published plan and open window versions.");
   const sources=assertBenefitElectionSources(plan,window,new Date(ports.clock.now()).toISOString().slice(0,10),input);
   if(input.dependentIds.length)assertBenefitVerifiedDependents(input,await ports.verifiedDependents(String(person.id)));
   const now=ports.clock.now();
   const learning=plan.required_eligibility_code?await ports.prepareLearning({personId:String(person.id),requiredCode:String(plan.required_eligibility_code),effectiveDate:String(window.effective_on),electionId:id,key}):null;
   return ports.commit({action:"election",key,operation,id,input,person,plan:sources.plan,window:sources.window,now,learning},{id,status:"elected",version:1});
  },
  async change(kind:BenefitRecordKind,id:string,raw:unknown,key:string){
   await ports.authorize(true);const input=benefitTransitionSchema.parse(raw),operation=`benefits.${kind}.${input.action}`,prior=await ports.replay(key,operation,id,input),review=kind!=="election"||input.action!=="submit";
   if(review)await ports.authorizeReview();if(prior)return prior;
   const record=await ports.record(kind,id);if(!record)throw new BusinessRuleError(404,"BENEFIT_RECORD_NOT_FOUND","Benefits record not found.");
   const state=benefitTransition(kind,record,input);
   if(!review){const person=await ports.person();if(!person||String(person.id)!==String(record.person_id))throw new BusinessRuleError(403,"BENEFIT_ELECTION_OWNER_REQUIRED","Only the employee who owns this election can submit it.");}
   if(review)await ports.authorizeReview();
   assertBenefitIndependentReview(record,input,ports.actorId);
   const policy=benefitEvidence(kind,input),evidence=policy?await ports.prepareEvidence(id,kind,input,policy,key):null;
   const now=ports.clock.now(),learning=kind==="election"&&input.action!=="cancel"?await ports.validateLearning(record,key):null;
   return ports.commit({action:"transition",kind,key,operation,id,input,record,from:state.from,to:state.to,changes:benefitTransitionChanges(input,ports.actorId),releasePayroll:kind==="election"&&input.action==="release",now,learning,evidence},{id,status:state.to,version:input.expectedVersion+1});
  },
  async detail(id:string){
   await ports.authorize();const person=await ports.person(),election=await ports.election(id,String(person?.id??""),ports.canReview);
   if(!election)throw new BusinessRuleError(404,"BENEFIT_ELECTION_NOT_FOUND","Election not found.");
   const history=await ports.history(id),payroll=await ports.payroll(id);
   return {election,history,payroll,canReview:ports.canReview,canManageDocuments:ports.canManageDocuments,isOwner:Boolean(person&&String(person.id)===String(election.person_id)),actorId:ports.actorId};
  },
  async work(){
   if(!ports.canRead)return [];
   const person=await ports.person(),rows=await ports.elections(String(person?.id??""),ports.canReview,true);
   return benefitWorkItems(rows,person,ports.actorId,ports.canReview);
  },
 };
}
