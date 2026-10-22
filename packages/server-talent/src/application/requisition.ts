import { talentRequisitionSchema, type TalentRequisitionInput } from "@erpx/server-talent/domain/requisition";
import { BusinessRuleError, type Clock } from "@erpx/server-shared/index";
export interface TalentRequisitionInputPorts {
 clock:Clock;branchId:string|null;
 authorize():Promise<void>;
 replay(key:string,operation:string,id:string,input:TalentRequisitionInput):Promise<Record<string,unknown>|null>;
 persist(command:{key:string;operation:string;id:string;input:TalentRequisitionInput;now:number;result:{id:string;status:string;version:number}}):Promise<Record<string,unknown>>;
}
export function createTalentRequisitionApplication(ports:TalentRequisitionInputPorts){return async(raw:unknown,key:string)=>{
 await ports.authorize();
 const input=talentRequisitionSchema.parse(raw);
 if(!ports.branchId)throw new BusinessRuleError(422,"BRANCH_SCOPE_REQUIRED","Choose a branch assignment.");
 const id="talent-req-"+key,operation="talent.requisition.create";
 const prior=await ports.replay(key,operation,id,input);if(prior)return prior;
 
 const result={id,status:"draft",version:1};
 return ports.persist({key,operation,id,input,now:ports.clock.now(),result});
};}
