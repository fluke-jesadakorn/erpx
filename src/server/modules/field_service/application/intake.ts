import { BusinessRuleError,type Clock } from "../../shared";
import { serviceRequestSchema,type ServiceRequestInput } from "../domain/service";
import type { ServiceIntakeAgreement } from "../public/intake-sources";
export function createServiceIntakeApplication(ports:{clock:Clock;authorize():Promise<void>;
 replay(key:string,operation:string,id:string,input:ServiceRequestInput):Promise<Record<string,unknown>|null>;
 source(id:string):Promise<ServiceIntakeAgreement|null>;
 commit(command:{key:string;operation:string;id:string;input:ServiceRequestInput;source:ServiceIntakeAgreement;now:number}):Promise<Record<string,unknown>>}) {
 return async(raw:unknown,key:string)=>{
  await ports.authorize();const input=serviceRequestSchema.parse(raw),id=`service-order-${key}`,operation="field.order.request";
  const prior=await ports.replay(key,operation,id,input);if(prior)return prior;
  const source=await ports.source(input.agreementId);if(!source)throw new BusinessRuleError(404,"FIELD_SERVICE_AGREEMENT_NOT_FOUND","Agreement not found in this work scope.");
  if(source.version!==input.agreementVersion)throw new BusinessRuleError(409,"FIELD_SERVICE_AGREEMENT_CHANGED","Refresh the agreement before requesting service.");
  return ports.commit({key,operation,id,input,source,now:ports.clock.now()});
 };
}
