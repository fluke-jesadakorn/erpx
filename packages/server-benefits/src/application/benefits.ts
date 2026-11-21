import { BusinessRuleError } from "@erpx/server-shared/index";
import {benefitWorkItems,type BenefitRow} from "@erpx/server-benefits/domain/benefits";

export interface BenefitsPorts{
 actorId:string;canRead:boolean;canReview:boolean;canWrite:boolean;canManageDocuments:boolean;
 authorize():Promise<void>;
 person():Promise<BenefitRow|null>;
 plans():Promise<BenefitRow[]>;windows():Promise<BenefitRow[]>;dependents(personId:string):Promise<BenefitRow[]>;
 elections(personId:string,reviewer:boolean,activeOnly?:boolean):Promise<BenefitRow[]>;
 election(id:string,personId:string,reviewer:boolean):Promise<BenefitRow|null>;
 history(id:string):Promise<BenefitRow[]>;payroll(id:string):Promise<BenefitRow|null>;
}
export function createBenefitsApplication(ports:BenefitsPorts){
 return {
  async workspace(personal=false){
   await ports.authorize();const person=await ports.person();
   const [plans,windows,dependents,elections]=await Promise.all([ports.plans(),ports.windows(),person?ports.dependents(String(person.id)):Promise.resolve([]),ports.elections(String(person?.id??""),!personal&&ports.canReview)]);
   return {plans,windows,dependents,elections,actorId:ports.actorId,canWrite:ports.canWrite,canReview:ports.canReview,canManageDocuments:ports.canManageDocuments};
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
