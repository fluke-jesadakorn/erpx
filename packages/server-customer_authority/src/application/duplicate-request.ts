import {requestSchema,type DuplicateRequestInput} from "@erpx/server-customer_authority/domain/duplicate-request";
import {BusinessRuleError,type Clock} from "@erpx/server-shared/index";
export interface DuplicateRequestPorts {clock:Clock;companyScope:boolean;authorize():Promise<void>;replay(key:string,operation:string,id:string,input:DuplicateRequestInput):Promise<Record<string,unknown>|null>;persist(command:{key:string;operation:string;id:string;input:DuplicateRequestInput;pair:[string,string];now:number}):Promise<Record<string,unknown>>;}
export function createDuplicateRequestApplication(ports:DuplicateRequestPorts){return async(raw:unknown,key:string)=>{
 await ports.authorize();if(!ports.companyScope)throw new BusinessRuleError(403,"CUSTOMER_COMPANY_REVIEW_REQUIRED","Use a Company assignment for identity review.");
 const input=requestSchema.parse(raw),pair=[input.leftCustomerId,input.rightCustomerId].sort() as [string,string],id=`customer-duplicate-${key}`,operation="customer.duplicate.request",prior=await ports.replay(key,operation,id,input);if(prior)return prior;
 return ports.persist({key,operation,id,input,pair,now:ports.clock.now()});
};}
