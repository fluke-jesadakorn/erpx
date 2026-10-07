import { benefitPlanSchema, type BenefitPlanInput } from "../domain/plan";
import { BusinessRuleError, type Clock } from "../../shared";
export interface BenefitPlanInputPorts {
 clock:Clock;
 authorize():Promise<void>;
 replay(key:string,operation:string,id:string,input:BenefitPlanInput):Promise<Record<string,unknown>|null>;
 persist(command:{key:string;operation:string;id:string;input:BenefitPlanInput;now:number;result:{id:string;status:string;version:number}}):Promise<Record<string,unknown>>;
}
export function createBenefitPlanApplication(ports:BenefitPlanInputPorts){return async(raw:unknown,key:string)=>{
 await ports.authorize();
 const input=benefitPlanSchema.parse(raw);
 
 const id="benefit-plan-"+key,operation="benefits.plan.create";
 const prior=await ports.replay(key,operation,id,input);if(prior)return prior;
 if(input.effectiveTo && input.effectiveTo<input.effectiveFrom)throw new BusinessRuleError(422,"INVALID_EFFECTIVE_RANGE","Effective end must follow start.");
 const result={id,status:"draft",version:1};
 return ports.persist({key,operation,id,input,now:ports.clock.now(),result});
};}
